//! Opt-in fixed-cohort investigations, independent of Build state and canonical Audit publication.
pub mod aggregation;
pub mod config;
pub mod evidence;
mod storage;

use anyhow::{Context, Result, ensure};
use config::{AuditInputs, Config};
use orchestrate_contracts::{
    Adoption, ArtifactKind, Implementation, ReconciledDiscovery, decode, digest_bytes, encode,
    investigation::*,
};
use orchestrate_core::{
    Store, now_ms, prepare_source_checkout,
    provider::{
        InvocationApi, InvocationOutcome, ProcessInvocationApi, ProviderConfig, prepare_invocation,
    },
    verify_git_checkout, write_bytes_sync,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

type Sources = BTreeMap<String, (PathBuf, BTreeMap<String, String>)>;
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RepositoryIdentity {
    commit: String,
    tree: String,
    files: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    submodules: BTreeMap<String, String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct InputIdentity {
    schema_version: u32,
    config_digest: String,
    request_digest: String,
    question_kind: QuestionKind,
    repository: Option<RepositoryIdentity>,
    files: BTreeMap<String, String>,
    audit: Option<AuditInputs>,
}
struct Frozen {
    identity: InputIdentity,
    digest: String,
    request: String,
    files: Vec<String>,
    root: PathBuf,
    reconciled: Option<ReconciledDiscovery>,
    implementation: Option<Implementation>,
}
#[derive(Serialize)]
struct LaneInput<'a> {
    schema_version: u32,
    lane_id: &'a str,
    input_digest: &'a str,
    question_kind: &'a QuestionKind,
    repository: &'a Option<RepositoryIdentity>,
    sources: &'a BTreeMap<String, String>,
    audit: &'a Option<AuditInputs>,
}

pub fn run(root: &Path, config_path: &Path) -> Result<InvestigationResult> {
    run_with_invoker(root, config_path, &ProcessInvocationApi)
}
pub fn run_with_invoker(
    root: &Path,
    config_path: &Path,
    invoker: &dyn InvocationApi,
) -> Result<InvestigationResult> {
    let (config, raw_config) = config::load(config_path)?;
    ensure!(root.is_absolute(), "orchestration root must be absolute");
    // Check the root boundary before creating any product-local files.
    if let Some(target) = &config.target {
        let target = fs::canonicalize(target)?;
        let mut ancestor = root.to_owned();
        let mut suffix = Vec::new();
        while !ancestor.exists() {
            suffix.push(
                ancestor
                    .file_name()
                    .context("invalid store root")?
                    .to_owned(),
            );
            ancestor.pop();
        }
        let mut resolved = fs::canonicalize(ancestor)?;
        for part in suffix.into_iter().rev() {
            resolved.push(part);
        }
        ensure!(
            !resolved.starts_with(&target),
            "investigation root must be outside the target repository"
        );
    }
    let store = Store::open(root)?;
    // Resolve and verify all authority and input bytes before any agent runs.
    let prepared = prepare_inputs(&store, &config)?;
    let run_id = format!(
        "investigation-{}-{}",
        now_ms(),
        SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
    );
    let run_dir = store.root().join("investigations").join(&run_id);
    fs::create_dir_all(run_dir.parent().context("run has no parent")?)?;
    fs::create_dir(&run_dir)?;
    let frozen = freeze(&run_dir, &config, &raw_config, prepared)?;
    let parents = config
        .audit
        .as_ref()
        .map(|a| {
            vec![
                a.reconciled.clone(),
                a.adoption.clone(),
                a.implementation.clone(),
            ]
        })
        .unwrap_or_default();
    let input_manifest = storage::seal(
        &frozen.root,
        "inputs",
        &frozen.digest,
        &frozen.files,
        &parents,
    )?;
    storage::json(
        &run_dir.join("run.json"),
        &serde_json::json!({"schema_version":1,"run_id":run_id,"input_digest":frozen.digest,"input_manifest_digest":input_manifest}),
    )?;
    let providers: Vec<_> = config
        .lanes
        .iter()
        .flat_map(|l| std::iter::repeat_n(l.provider(), l.count))
        .collect();
    let parallel = config
        .max_parallel
        .unwrap_or(providers.len())
        .min(providers.len());
    let mut lanes = Vec::new();
    let mut graphs = Vec::new();
    let mut limitations = Vec::new();
    if let Some(repository) = &frozen.identity.repository {
        for (path, commit) in &repository.submodules {
            limitations.push(format!("Submodule {path} is pinned at {commit}; its contents were not frozen or made available for inspection."));
        }
    }
    for (batch, group) in providers.chunks(parallel).enumerate() {
        let outcomes = std::thread::scope(|scope| {
            let handles: Vec<_> = group
                .iter()
                .enumerate()
                .map(|(index, provider)| {
                    let index = batch * parallel + index;
                    let lane_id = format!("lane-{:04}", index + 1);
                    let lane_dir = run_dir.join("lanes").join(&lane_id);
                    let frozen = &frozen;
                    let config = &config;
                    let parents = &parents;
                    (
                        index,
                        scope.spawn(move || {
                            assess_lane(
                                &lane_dir, &lane_id, provider, frozen, config, parents, invoker,
                            )
                        }),
                    )
                })
                .collect();
            handles
                .into_iter()
                .map(|(index, h)| {
                    (
                        index,
                        h.join()
                            .unwrap_or_else(|_| Err(anyhow::anyhow!("lane thread panicked"))),
                    )
                })
                .collect::<Vec<_>>()
        });
        for (index, outcome) in outcomes {
            match outcome {
                Ok((record, graph, notes)) => {
                    lanes.push(record);
                    if let Some(graph) = graph {
                        graphs.push(graph);
                    }
                    limitations.extend(notes);
                }
                Err(error) => lanes.push(LaneRecord {
                    lane_id: format!("lane-{:04}", index + 1),
                    state: LaneState::Failed,
                    detail: format!("controller could not finalize lane artifacts: {error:#}"),
                    position: None,
                    manifest_digest: String::new(),
                }),
            }
        }
    }
    // Unrecorded attempts are failed, never silently omitted from the cohort.
    for index in 0..providers.len() {
        let id = format!("lane-{:04}", index + 1);
        if !lanes.iter().any(|l| l.lane_id == id) {
            lanes.push(LaneRecord {
                lane_id: id,
                state: LaneState::Failed,
                detail: "controller interrupted this lane; inspect any retained raw files".into(),
                position: None,
                manifest_digest: String::new(),
            });
        }
    }
    lanes.sort_by(|a, b| a.lane_id.cmp(&b.lane_id));
    limitations.extend(require_fresh_sessions(&run_dir, &mut lanes, &mut graphs)?);
    storage::json(&run_dir.join("cohort-validation.json"), &lanes)?;
    // Re-read digest-protected input/lane records at the barrier, before any synthesis.
    let barrier = (|| -> Result<()> {
        storage::verify(&frozen.root, Some(&input_manifest))?;
        for lane in &lanes {
            if !lane.manifest_digest.is_empty() {
                storage::verify(
                    &run_dir.join("lanes").join(&lane.lane_id),
                    Some(&lane.manifest_digest),
                )?;
            }
        }
        Ok(())
    })();
    let reconciliation = if let Err(error) = barrier {
        limitations.push(format!("integrity barrier rejected aggregation: {error:#}"));
        None
    } else if graphs.is_empty() {
        limitations.push("No eligible lane graph was available for reconciliation.".into());
        None
    } else {
        match reconcile(&run_dir, &config, &frozen, &graphs, &parents, invoker) {
            Ok(value) => Some(value),
            Err(error) => {
                limitations.push(format!(
                    "reconciliation did not produce an eligible result: {error:#}"
                ));
                None
            }
        }
    };
    let result = aggregation::result(
        &config,
        &run_id,
        &frozen.digest,
        lanes,
        reconciliation,
        limitations,
        &graphs,
    );
    storage::json(&run_dir.join("result.json"), &result)?;
    write_bytes_sync(
        &run_dir.join("report.md"),
        aggregation::render(&result).as_bytes(),
    )?;
    let mut final_files = vec![
        "run.json".into(),
        "cohort-validation.json".into(),
        "result.json".into(),
        "report.md".into(),
        "frozen/manifest.json".into(),
    ];
    for lane in &result.lanes {
        if !lane.manifest_digest.is_empty() {
            final_files.push(format!("lanes/{}/manifest.json", lane.lane_id));
        }
    }
    if run_dir.join("reconciler/manifest.json").exists() {
        final_files.push("reconciler/manifest.json".into());
    }
    storage::seal(&run_dir, &run_id, &frozen.digest, &final_files, &parents)?;
    Ok(result)
}

struct Prepared {
    request: String,
    repository: Option<(PathBuf, String, String)>,
    inputs: Vec<Vec<u8>>,
    authority: Vec<(String, Vec<u8>)>,
    reconciled: Option<ReconciledDiscovery>,
    implementation: Option<Implementation>,
}
fn prepare_inputs(store: &Store, config: &Config) -> Result<Prepared> {
    let request = match &config.request {
        Some(path) => String::from_utf8(fs::read(path).context("cannot read request")?)?,
        None => "Determine whether this exact registered Implementation satisfies every binding requirement of this exact adopted Reconciled Discovery. Assess the full text, acceptance and conditions; technical suggestions are advisory. Use inspection only; executable verification requires authorization in a supplied request.".into(),
    };
    ensure!(!request.trim().is_empty(), "request cannot be empty");
    let inputs = config
        .inputs
        .iter()
        .map(fs::read)
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut authority = Vec::new();
    let mut reconciled = None;
    let mut implementation = None;
    let mut target = config.target.clone();
    let mut revision = config.revision.clone();
    if let Some(audit) = &config.audit {
        ensure!(
            audit.reconciled.kind == ArtifactKind::ReconciledDiscovery
                && audit.adoption.kind == ArtifactKind::Adoption
                && audit.implementation.kind == ArtifactKind::Implementation,
            "invalid conformance reference kinds"
        );
        let effort = store.load_effort(&audit.effort)?;
        let (contract_envelope, contract): (_, ReconciledDiscovery) =
            store.load_json(&effort, &audit.reconciled, "reconciled-discovery.json")?;
        let (adoption_envelope, adoption): (_, Adoption) =
            store.load_json(&effort, &audit.adoption, "adoption.json")?;
        let (implementation_envelope, candidate): (_, Implementation) =
            store.load_json(&effort, &audit.implementation, "implementation.json")?;
        ensure!(
            contract_envelope.outcome == "IMPLEMENTATION_READY"
                && contract.context_id == effort.context.id
                && contract.baseline_commit == effort.baseline_commit
                && contract.baseline_tree == effort.baseline_tree
                && adoption_envelope.outcome == "ADOPTED"
                && adoption_envelope.parents == vec![audit.reconciled.clone()]
                && implementation_envelope.parents == vec![audit.adoption.clone()]
                && candidate.discovery_baseline_commit == contract.baseline_commit
                && candidate.discovery_baseline_tree == contract.baseline_tree
                && adoption.reconciled == audit.reconciled
                && candidate.reconciled == audit.reconciled
                && candidate.adoption == audit.adoption,
            "mismatched conformance authority chain"
        );
        let repo = store.project_for(&effort)?.canonical_locator;
        if let Some(t) = &target {
            ensure!(
                fs::canonicalize(t)? == fs::canonicalize(&repo)?,
                "conformance target differs from registered project"
            );
        }
        if let Some(r) = &revision {
            ensure!(
                *r == candidate.target_commit,
                "conformance revision differs from registered Implementation"
            );
        }
        ensure!(
            !store.root().starts_with(fs::canonicalize(&repo)?),
            "investigation root must be outside the target repository"
        );
        target = Some(repo);
        revision = Some(candidate.target_commit.clone());
        authority.push((
            "authority/reconciled-discovery.json".into(),
            encode(&contract)?,
        ));
        authority.push(("authority/adoption.json".into(), encode(&adoption)?));
        authority.push(("authority/implementation.json".into(), encode(&candidate)?));
        reconciled = Some(contract);
        implementation = Some(candidate);
    }
    let repository = if let Some(target) = target {
        let target = fs::canonicalize(target)?;
        let revision = revision.context("repository requires exact revision")?;
        let commit = storage::git(
            &target,
            &["rev-parse", "--verify", &format!("{revision}^{{commit}}")],
        )?;
        ensure!(
            commit.eq_ignore_ascii_case(&revision),
            "revision must identify the exact commit"
        );
        let tree = storage::git(
            &target,
            &["rev-parse", "--verify", &format!("{commit}^{{tree}}")],
        )?;
        if let Some(candidate) = &implementation {
            ensure!(
                tree == candidate.target_tree,
                "registered Implementation tree mismatch"
            );
        }
        Some((target, commit, tree))
    } else {
        None
    };
    Ok(Prepared {
        request,
        repository,
        inputs,
        authority,
        reconciled,
        implementation,
    })
}
fn freeze(
    run_dir: &Path,
    config: &Config,
    raw_config: &[u8],
    prepared: Prepared,
) -> Result<Frozen> {
    let root = run_dir.join("frozen");
    fs::create_dir(&root)?;
    write_bytes_sync(&root.join("request.md"), prepared.request.as_bytes())?;
    write_bytes_sync(&root.join("config.toml"), raw_config)?;
    storage::json(&root.join("resolved-config.json"), config)?;
    let mut files = vec![
        "request.md".into(),
        "config.toml".into(),
        "resolved-config.json".into(),
    ];
    let mut input_files = BTreeMap::new();
    input_files.insert(
        "request.md".into(),
        digest_bytes(prepared.request.as_bytes()),
    );
    for (index, bytes) in prepared.inputs.iter().enumerate() {
        let path = format!("inputs/input-{:04}", index + 1);
        write_bytes_sync(&root.join(&path), bytes)?;
        input_files.insert(path.clone(), digest_bytes(bytes));
        files.push(path);
    }
    for (path, bytes) in &prepared.authority {
        write_bytes_sync(&root.join(path), bytes)?;
        input_files.insert(path.clone(), digest_bytes(bytes));
        files.push(path.clone());
    }
    let repository = if let Some((repo, commit, tree)) = &prepared.repository {
        let source = root.join("source");
        prepare_source_checkout(repo, &source, commit, tree)?;
        let inventory = storage::inventory(&source, commit)?;
        files.extend(inventory.files.keys().map(|p| format!("source/{p}")));
        Some(RepositoryIdentity {
            commit: commit.clone(),
            tree: tree.clone(),
            files: inventory.files,
            submodules: inventory.submodules,
        })
    } else {
        None
    };
    let identity = InputIdentity {
        schema_version: 1,
        config_digest: digest_bytes(&encode(config)?),
        request_digest: digest_bytes(prepared.request.as_bytes()),
        question_kind: config.question_kind(),
        repository,
        files: input_files,
        audit: config.audit.clone(),
    };
    let digest = digest_bytes(&encode(&identity)?);
    storage::json(&root.join("identity.json"), &identity)?;
    files.push("identity.json".into());
    Ok(Frozen {
        identity,
        digest,
        request: prepared.request,
        files,
        root,
        reconciled: prepared.reconciled,
        implementation: prepared.implementation,
    })
}
fn lane_workspace(root: &Path, lane_id: &str, frozen: &Frozen) -> Result<Sources> {
    fs::create_dir_all(root.join("scratch"))?;
    let mut sources = Sources::new();
    for (path, digest) in &frozen.identity.files {
        let bytes = storage::read(&frozen.root, path)?;
        write_bytes_sync(&root.join(path), &bytes)?;
        let id = if path == "request.md" {
            "request".into()
        } else {
            path.clone()
        };
        sources.insert(
            id,
            (
                root.to_owned(),
                BTreeMap::from([(path.clone(), digest.clone())]),
            ),
        );
    }
    if let Some(repo) = &frozen.identity.repository {
        prepare_source_checkout(
            &frozen.root.join("source"),
            &root.join("source"),
            &repo.commit,
            &repo.tree,
        )?;
        sources.insert(
            "repository".into(),
            (root.join("source"), repo.files.clone()),
        );
    }
    storage::json(
        &root.join("lane-input.json"),
        &LaneInput {
            schema_version: 1,
            lane_id,
            input_digest: &frozen.digest,
            question_kind: &frozen.identity.question_kind,
            repository: &frozen.identity.repository,
            sources: &frozen.identity.files,
            audit: &frozen.identity.audit,
        },
    )?;
    Ok(sources)
}
fn assess_lane(
    root: &Path,
    lane_id: &str,
    provider: &ProviderConfig,
    frozen: &Frozen,
    config: &Config,
    parents: &[orchestrate_contracts::ArtifactRef],
    invoker: &dyn InvocationApi,
) -> Result<(LaneRecord, Option<LaneGraph>, Vec<String>)> {
    let sources = lane_workspace(root, lane_id, frozen)?;
    let prompt = format!(
        "{}\n\nLane identity: {lane_id}. Input digest: {}. Question kind: {:?}.\nRead lane-input.json and request.md as written. Frozen request:\n{}\n",
        orchestrate_guides::INVESTIGATOR,
        frozen.digest,
        frozen.identity.question_kind,
        frozen.request
    );
    write_bytes_sync(&root.join("prompt.md"), prompt.as_bytes())?;
    let policy = if provider.adapter == "codex" {
        vec!["--skip-git-repo-check".into()]
    } else {
        vec![]
    };
    let plan = prepare_invocation(provider, &prompt, root, None, &policy)?
        .with_stdin_prompt()?
        .with_timeout(Duration::from_secs(config.provider_timeout_seconds));
    storage::json(&root.join("invocation.json"), &plan.record)?;
    let mut state = LaneState::Failed;
    let detail;
    let mut graph = None;
    let mut notes = Vec::new();
    match invoker.invoke(&plan, root, root) {
        Err(error) => {
            state = if error.chain().any(|e| {
                e.downcast_ref::<std::io::Error>()
                    .is_some_and(|e| e.kind() == std::io::ErrorKind::NotFound)
            }) {
                LaneState::Unavailable
            } else {
                LaneState::Failed
            };
            detail = format!("provider invocation failed: {error:#}");
        }
        Ok(outcome) => {
            persist_outcome(root, &outcome)?;
            let captured = evidence::receipts(&outcome.stdout);
            storage::json(&root.join("receipts.json"), &captured)?;
            if !outcome.success {
                detail = if outcome.timed_out {
                    format!(
                        "provider timed out after {} seconds; retained output is partial and contributes no vote",
                        config.provider_timeout_seconds
                    )
                } else {
                    format!("provider failed with exit code {:?}", outcome.exit_code)
                };
            } else {
                let validation = (|| -> Result<LaneGraph> {
                    let response = outcome
                        .final_response
                        .as_deref()
                        .context("provider produced no final response")?;
                    let mut candidate: LaneGraph =
                        decode(response.as_bytes()).context("malformed lane graph")?;
                    for (source, files) in sources.values() {
                        storage::verify_inventory(source, files)?;
                    }
                    if let Some(repo) = &frozen.identity.repository {
                        verify_git_checkout(&root.join("source"), &repo.commit, &repo.tree)?;
                    }
                    let audit = match (&config.audit, &frozen.reconciled, &frozen.implementation) {
                        (Some(refs), Some(reconciled), Some(implementation)) => {
                            Some(evidence::AuditContext {
                                refs,
                                reconciled,
                                implementation,
                            })
                        }
                        _ => None,
                    };
                    notes = evidence::validate_graph(
                        &mut candidate,
                        lane_id,
                        &frozen.digest,
                        &frozen.identity.question_kind,
                        &sources,
                        &captured,
                        audit,
                    )?;
                    Ok(candidate)
                })();
                match validation {
                    Ok(candidate) => {
                        state = LaneState::Completed;
                        detail = "validated and frozen evidence graph".into();
                        storage::json(&root.join("graph.json"), &candidate)?;
                        graph = Some(candidate);
                    }
                    Err(error) => {
                        state = LaneState::Invalid;
                        detail = format!("lane ineligible: {error:#}");
                    }
                }
            }
        }
    }
    storage::json(
        &root.join("validation.json"),
        &serde_json::json!({"stage":"lane_graph","graph_valid":state==LaneState::Completed,"state":state,"detail":detail,"limitations":notes}),
    )?;
    let mut files = vec![
        "lane-input.json".into(),
        "prompt.md".into(),
        "invocation.json".into(),
        "validation.json".into(),
    ];
    files.extend(frozen.identity.files.keys().cloned());
    for path in [
        "transport.jsonl",
        "stderr.txt",
        "outcome.json",
        "provider-metadata.json",
        "response.txt",
        "receipts.json",
        "graph.json",
    ] {
        if root.join(path).is_file() {
            files.push(path.into());
        }
    }
    let manifest_digest = storage::seal(root, lane_id, &frozen.digest, &files, parents)?;
    let position = graph.as_ref().and_then(|g| g.conclusion.position.clone());
    Ok((
        LaneRecord {
            lane_id: lane_id.into(),
            state,
            detail,
            position,
            manifest_digest,
        },
        graph,
        notes,
    ))
}
fn persist_outcome(root: &Path, outcome: &InvocationOutcome) -> Result<()> {
    // Also persist fake/injected transport, through the same artifact boundary as real processes.
    write_bytes_sync(&root.join("transport.jsonl"), outcome.stdout.as_bytes())?;
    write_bytes_sync(&root.join("stderr.txt"), outcome.stderr.as_bytes())?;
    storage::json(&root.join("outcome.json"), outcome)?;
    storage::json(
        &root.join("provider-metadata.json"),
        &evidence::provider_metadata(outcome),
    )?;
    if let Some(response) = &outcome.final_response {
        write_bytes_sync(&root.join("response.txt"), response.as_bytes())?;
    }
    Ok(())
}
fn reconcile(
    run_dir: &Path,
    config: &Config,
    frozen: &Frozen,
    graphs: &[LaneGraph],
    parents: &[orchestrate_contracts::ArtifactRef],
    invoker: &dyn InvocationApi,
) -> Result<Reconciliation> {
    let root = run_dir.join("reconciler");
    fs::create_dir(&root)?;
    let mut lane_evidence = Vec::new();
    for graph in graphs {
        let lane = run_dir.join("lanes").join(&graph.lane_id);
        let mut receipts: Vec<CommandReceipt> = decode(&storage::read(&lane, "receipts.json")?)?;
        for receipt in &mut receipts {
            if receipt.output.chars().count() > 8000 {
                receipt.output = format!(
                    "{}\n[bounded excerpt; full output retained in lane transport]",
                    receipt.output.chars().take(8000).collect::<String>()
                );
            }
        }
        let mut observations: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        for observation in &graph.observations {
            if let Some(location) = &observation.source {
                let source = if location.source_id == "repository" {
                    frozen.root.join("source")
                } else {
                    frozen.root.clone()
                };
                let bytes = storage::read(&source, &location.path)?;
                let text = String::from_utf8_lossy(&bytes);
                let excerpt = if let Some(line) = location.line {
                    text.lines()
                        .skip(line.saturating_sub(11))
                        .take(31)
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    text.chars().take(4000).collect::<String>()
                };
                observations.insert(observation.id.clone(),serde_json::json!({"excerpt":excerpt,"sha256":digest_bytes(&bytes),"line":location.line,"bounded_excerpt":true}));
            }
        }
        lane_evidence.push(serde_json::json!({"graph":graph,"receipts":receipts,"source_observations":observations}));
    }
    let authority = frozen
        .identity
        .files
        .keys()
        .filter(|p| p.starts_with("authority/"))
        .map(|p| {
            Ok((
                p.clone(),
                String::from_utf8(storage::read(&frozen.root, p)?)?,
            ))
        })
        .collect::<Result<BTreeMap<_, _>>>()?;
    let repository = frozen.identity.repository.as_ref().map(|repository| {
        serde_json::json!({"commit":repository.commit,"tree":repository.tree,"submodules":repository.submodules})
    });
    let evidence = serde_json::json!({"input_digest":frozen.digest,"request":frozen.request,"repository":repository,"authority":authority,"lanes":lane_evidence});
    storage::json(&root.join("evidence.json"), &evidence)?;
    let prompt = format!(
        "{}\n\nInput digest: {}. Frozen request:\n{}\n\nSealed evidence:\n{}\n",
        orchestrate_guides::INVESTIGATION_RECONCILER,
        frozen.digest,
        frozen.request,
        serde_json::to_string_pretty(&evidence)?
    );
    write_bytes_sync(&root.join("prompt.md"), prompt.as_bytes())?;
    let policy = if config.reconciler.adapter == "codex" {
        vec!["--skip-git-repo-check".into()]
    } else {
        vec![]
    };
    let plan = prepare_invocation(&config.reconciler, &prompt, &root, None, &policy)?
        .with_stdin_prompt()?
        .with_timeout(Duration::from_secs(config.provider_timeout_seconds));
    storage::json(&root.join("invocation.json"), &plan.record)?;
    let validation = (|| -> Result<Reconciliation> {
        let outcome = invoker.invoke(&plan, &root, &root)?;
        persist_outcome(&root, &outcome)?;
        ensure!(
            !outcome.timed_out,
            "reconciler provider timed out after {} seconds; retained output is partial",
            config.provider_timeout_seconds
        );
        ensure!(outcome.success, "reconciler provider failed");
        // Synthesis may read the bounded evidence, but may not execute a new investigation.
        ensure!(
            !evidence::has_tool_activity(&outcome.stdout),
            "reconciler attempted shell investigation"
        );
        let value: Reconciliation = decode(
            outcome
                .final_response
                .as_deref()
                .context("reconciler response missing")?
                .as_bytes(),
        )?;
        aggregation::validate(&value, &frozen.digest, graphs)?;
        storage::json(&root.join("reconciliation.json"), &value)?;
        Ok(value)
    })();
    storage::json(
        &root.join("validation.json"),
        &serde_json::json!({"eligible":validation.is_ok(),"error":validation.as_ref().err().map(|e|format!("{e:#}"))}),
    )?;
    let mut files = vec![
        "evidence.json".into(),
        "prompt.md".into(),
        "invocation.json".into(),
        "validation.json".into(),
    ];
    for path in [
        "transport.jsonl",
        "stderr.txt",
        "outcome.json",
        "provider-metadata.json",
        "response.txt",
        "reconciliation.json",
    ] {
        if root.join(path).is_file() {
            files.push(path.into());
        }
    }
    storage::seal(&root, "reconciler", &frozen.digest, &files, parents)?;
    validation
}

fn require_fresh_sessions(
    run_dir: &Path,
    lanes: &mut [LaneRecord],
    graphs: &mut Vec<LaneGraph>,
) -> Result<Vec<String>> {
    let mut sessions: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (index, lane) in lanes
        .iter()
        .enumerate()
        .filter(|(_, l)| l.state == LaneState::Completed)
    {
        let root = run_dir.join("lanes").join(&lane.lane_id);
        let outcome: InvocationOutcome = decode(&storage::read(&root, "outcome.json")?)?;
        let invocation: orchestrate_core::provider::InvocationRecord =
            decode(&storage::read(&root, "invocation.json")?)?;
        ensure!(
            invocation.session_in.is_none(),
            "investigation lane was resumed"
        );
        if let Some(session) = outcome.observed_session.filter(|s| !s.is_empty()) {
            sessions
                .entry((invocation.adapter, session))
                .or_default()
                .push(index);
        }
    }
    let mut notes = Vec::new();
    for ((adapter, session), indices) in sessions
        .into_iter()
        .filter(|(_, indices)| indices.len() > 1)
    {
        let detail = format!(
            "lane ineligible: repeated observed {adapter} session {session}; distinct fresh assessments were not established"
        );
        notes.push(detail.clone());
        for index in indices {
            lanes[index].state = LaneState::Invalid;
            lanes[index].position = None;
            lanes[index].detail = detail.clone();
        }
    }
    graphs.retain(|g| {
        lanes
            .iter()
            .any(|l| l.lane_id == g.lane_id && l.state == LaneState::Completed)
    });
    Ok(notes)
}

/// Offline inspection never opens/initializes the store or calls a provider.
pub fn inspect(root: &Path, run_id: &str) -> Result<InvestigationResult> {
    ensure!(
        storage::safe(run_id) && !run_id.contains('/'),
        "invalid investigation run ID"
    );
    let dir = root.join("investigations").join(run_id);
    let metadata: serde_json::Value = decode(&storage::read(&dir, "run.json")?)?;
    ensure!(
        metadata["schema_version"] == 1 && metadata["run_id"] == run_id,
        "run metadata identity/version mismatch"
    );
    let input_digest = metadata["input_digest"]
        .as_str()
        .context("missing input digest")?;
    let input_manifest = metadata["input_manifest_digest"]
        .as_str()
        .context("missing input manifest identity")?;
    let frozen = dir.join("frozen");
    let inputs = storage::verify(&frozen, Some(input_manifest))?;
    ensure!(
        inputs.identity == "inputs" && inputs.input_digest == input_digest,
        "frozen input identity mismatch"
    );
    let identity: InputIdentity = decode(&storage::read(&frozen, "identity.json")?)?;
    ensure!(
        digest_bytes(&encode(&identity)?) == input_digest,
        "frozen identity digest mismatch"
    );
    let config: Config = decode(&storage::read(&frozen, "resolved-config.json")?)?;
    config.validate()?;
    ensure!(
        identity.config_digest == digest_bytes(&encode(&config)?),
        "resolved config identity mismatch"
    );
    let mut sources = Sources::new();
    for (path, digest) in &identity.files {
        let id = if path == "request.md" {
            "request".into()
        } else {
            path.clone()
        };
        sources.insert(
            id,
            (
                frozen.clone(),
                BTreeMap::from([(path.clone(), digest.clone())]),
            ),
        );
    }
    if let Some(repo) = &identity.repository {
        sources.insert(
            "repository".into(),
            (frozen.join("source"), repo.files.clone()),
        );
    }
    let authority = if let Some(refs) = &config.audit {
        Some((
            refs,
            decode::<ReconciledDiscovery>(&storage::read(
                &frozen,
                "authority/reconciled-discovery.json",
            )?)?,
            decode::<Implementation>(&storage::read(&frozen, "authority/implementation.json")?)?,
        ))
    } else {
        None
    };
    let read_lane = |id: &str, expected: Option<&str>| -> Result<(LaneRecord, Option<LaneGraph>)> {
        let lane = dir.join("lanes").join(id);
        let manifest = storage::verify(&lane, expected)?;
        ensure!(
            manifest.identity == id && manifest.input_digest == input_digest,
            "lane identity mismatch"
        );
        ensure!(
            manifest
                .payloads
                .iter()
                .any(|p| p.path == "validation.json"),
            "lane lacks sealed validation"
        );
        let validation: serde_json::Value = decode(&storage::read(&lane, "validation.json")?)?;
        let state: LaneState = serde_json::from_value(validation["state"].clone())?;
        ensure!(
            validation
                .get("graph_valid")
                .or_else(|| validation.get("eligible"))
                .and_then(|v| v.as_bool())
                == Some(state == LaneState::Completed)
                && validation.get("stage").is_none_or(|v| v == "lane_graph"),
            "contradictory lane graph validation"
        );
        let graph = if state == LaneState::Completed {
            ensure!(
                manifest.payloads.iter().any(|p| p.path == "graph.json")
                    && manifest.payloads.iter().any(|p| p.path == "receipts.json"),
                "completed lane lacks sealed graph/receipts"
            );
            let mut graph: LaneGraph = decode(&storage::read(&lane, "graph.json")?)?;
            let outcome: InvocationOutcome = decode(&storage::read(&lane, "outcome.json")?)?;
            ensure!(outcome.success, "completed graph came from failed provider");
            let receipts: Vec<CommandReceipt> = decode(&storage::read(&lane, "receipts.json")?)?;
            ensure!(
                encode(&receipts)? == encode(&evidence::receipts(&outcome.stdout))?,
                "receipts do not match retained native transport"
            );
            let audit = authority
                .as_ref()
                .map(
                    |(refs, reconciled, implementation)| evidence::AuditContext {
                        refs,
                        reconciled,
                        implementation,
                    },
                );
            evidence::validate_graph(
                &mut graph,
                id,
                input_digest,
                &identity.question_kind,
                &sources,
                &receipts,
                audit,
            )?;
            Some(graph)
        } else {
            None
        };
        let record = LaneRecord {
            lane_id: id.into(),
            state,
            detail: validation["detail"]
                .as_str()
                .context("missing lane validation detail")?
                .into(),
            position: graph.as_ref().and_then(|g| g.conclusion.position.clone()),
            manifest_digest: digest_bytes(&storage::read(&lane, "manifest.json")?),
        };
        Ok((record, graph))
    };
    if !dir.join("manifest.json").exists() {
        let mut lanes = Vec::new();
        let mut graphs = Vec::new();
        for index in 0..config.cohort_size() {
            let id = format!("lane-{:04}", index + 1);
            if dir.join("lanes").join(&id).join("manifest.json").exists() {
                let (record, graph) = read_lane(&id, None)?;
                lanes.push(record);
                if let Some(graph) = graph {
                    graphs.push(graph);
                }
            } else {
                lanes.push(LaneRecord{lane_id:id,state:LaneState::Failed,detail:"run stopped without a frozen lane record; retained raw files remain available".into(),position:None,manifest_digest:String::new()});
            }
        }
        let mut notes = require_fresh_sessions(&dir, &mut lanes, &mut graphs)?;
        notes.push("Run has no final manifest; inspection does not resume or aggregate it.".into());
        return Ok(aggregation::result(
            &config,
            run_id,
            input_digest,
            lanes,
            None,
            notes,
            &graphs,
        ));
    }
    let manifest = storage::verify(&dir, None)?;
    ensure!(
        manifest.identity == run_id && manifest.input_digest == input_digest,
        "final manifest identity mismatch"
    );
    let result: InvestigationResult = decode(&storage::read(&dir, "result.json")?)?;
    ensure!(
        result.run_id == run_id
            && result.input_digest == input_digest
            && result.lanes.len() == config.cohort_size(),
        "result identity/cohort mismatch"
    );
    let mut graphs = Vec::new();
    let mut records = Vec::new();
    for (index, lane) in result.lanes.iter().enumerate() {
        ensure!(
            lane.lane_id == format!("lane-{:04}", index + 1),
            "result contains missing, duplicated or reordered cohort identities"
        );
        if lane.manifest_digest.is_empty() {
            ensure!(
                lane.state == LaneState::Failed && lane.position.is_none(),
                "unsealed lane cannot supply a vote"
            );
            records.push(lane.clone());
        } else {
            let (record, graph) = read_lane(&lane.lane_id, Some(&lane.manifest_digest))?;
            records.push(record);
            if let Some(graph) = graph {
                graphs.push(graph);
            }
        }
    }
    require_fresh_sessions(&dir, &mut records, &mut graphs)?;
    ensure!(
        encode(&records)? == encode(&result.lanes)?,
        "cohort eligibility differs from frozen evidence"
    );
    let cohort: Vec<LaneRecord> = decode(&storage::read(&dir, "cohort-validation.json")?)?;
    ensure!(
        encode(&cohort)? == encode(&result.lanes)?,
        "cohort validation differs from result"
    );
    if dir.join("reconciler/manifest.json").exists() {
        storage::verify(&dir.join("reconciler"), None)?;
    }
    if let Some(reconciliation) = &result.reconciliation {
        aggregation::validate(reconciliation, input_digest, &graphs)?;
        let sealed: Reconciliation = decode(&storage::read(
            &dir.join("reconciler"),
            "reconciliation.json",
        )?)?;
        ensure!(
            encode(&sealed)? == encode(reconciliation)?,
            "result reconciliation differs from frozen record"
        );
    }
    let derived = aggregation::result(
        &config,
        run_id,
        input_digest,
        result.lanes.clone(),
        result.reconciliation.clone(),
        vec![],
        &graphs,
    );
    let mut expected = serde_json::to_value(derived)?;
    let mut retained = serde_json::to_value(&result)?;
    expected
        .as_object_mut()
        .context("result is not an object")?
        .remove("limitations");
    retained
        .as_object_mut()
        .context("result is not an object")?
        .remove("limitations");
    ensure!(
        expected == retained,
        "result totals/recommendation differ from deterministic evidence derivation"
    );
    Ok(result)
}

#[cfg(test)]
mod tests;
