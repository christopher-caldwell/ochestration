//! Import one collaborative chat as ordinary Discovery and Reconciled artifacts.

use anyhow::{Context, Result, ensure};
use orchestrate_contracts::{
    ArtifactKind, DiscoveryAttribution, DiscoverySourceRef, EvidenceSynthesis, Independence,
    Provenance, ReconciledDiscovery, ReconciledRequirement, RejectedAlternative, RequestKind,
    Requirement, TechnicalSuggestion, encode, validate_reconciled_discovery,
};
use orchestrate_core::{Store, write_bytes_sync};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, HashSet},
    fs::File,
    io::Read,
    path::Path,
};
use zip::ZipArchive;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChatDiscovery {
    effort: String,
    request: String,
    #[serde(default)]
    constraints: Vec<String>,
    problem: String,
    selected_direction: String,
    evidence_summary: String,
    #[serde(default)]
    findings: Vec<String>,
    #[serde(default)]
    decisions: Vec<String>,
    #[serde(default)]
    disagreements: Vec<String>,
    #[serde(default)]
    product_behavior_changed: Vec<String>,
    #[serde(default)]
    product_behavior_unchanged: Vec<String>,
    #[serde(default)]
    technical_behavior_changed: Vec<String>,
    #[serde(default)]
    technical_behavior_unchanged: Vec<String>,
    requirements: Vec<ChatRequirement>,
    #[serde(default)]
    rejected_alternatives: Vec<ChatAlternative>,
    #[serde(default)]
    implementation_risks: Vec<String>,
    #[serde(default)]
    compatibility_concerns: Vec<String>,
    #[serde(default)]
    caveats: Vec<String>,
    #[serde(default)]
    advisory_technical_suggestions: Vec<String>,
    #[serde(default)]
    blockers: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChatRequirement {
    id: String,
    text: String,
    acceptance: String,
    authority: ChatAuthority,
    #[serde(default)]
    user_statement: Option<String>,
    #[serde(default)]
    condition: Option<String>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ChatAuthority {
    ExplicitUser,
    ChatAnalysis,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ChatAlternative {
    direction: String,
    reason: String,
}

pub fn import(store: &Store, repo: &Path, bundle: &Path) -> Result<serde_json::Value> {
    // Read and validate the whole small portable input before creating an Effort.
    let mut archive = ZipArchive::new(
        File::open(bundle).with_context(|| format!("cannot open {}", bundle.display()))?,
    )?;
    ensure!(
        archive.len() <= 65,
        "Chat Discovery ZIP has too many entries"
    );
    let mut discovery = None;
    let mut phases = BTreeMap::new();
    let mut phase_dirs = HashSet::new();
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let name = entry.name().to_owned();
        if entry.is_dir() {
            let phase_dir = name
                .strip_prefix("build/")
                .and_then(|path| path.strip_suffix('/'));
            ensure!(
                name == "build/" || phase_dir.is_some_and(valid_phase_name),
                "unexpected ZIP entry {name}"
            );
            if let Some(phase) = phase_dir {
                phase_dirs.insert(phase.to_owned());
            }
            continue;
        }
        ensure!(entry.size() <= 1_000_000, "ZIP entry {name} exceeds 1 MB");
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes)?;
        if name == "discovery.json" {
            ensure!(discovery.is_none(), "duplicate discovery.json");
            discovery = Some(orchestrate_contracts::decode::<ChatDiscovery>(&bytes)?);
        } else {
            let phase = name
                .strip_prefix("build/")
                .and_then(|path| path.strip_suffix("/phase.md"));
            let phase = phase.context(format!("unexpected ZIP entry {name}"))?;
            ensure!(valid_phase_name(phase), "invalid Build phase name {phase}");
            ensure!(
                !String::from_utf8(bytes.clone())?.trim().is_empty(),
                "empty Build phase {phase}"
            );
            ensure!(
                phases.insert(phase.to_owned(), bytes).is_none(),
                "duplicate Build phase {phase}"
            );
        }
    }
    ensure!(
        phase_dirs.iter().all(|name| phases.contains_key(name)),
        "Build phase directory lacks phase.md"
    );
    let chat = discovery.context("ZIP lacks discovery.json")?;
    ensure!(
        !chat.effort.trim().is_empty() && !chat.request.trim().is_empty(),
        "effort and original request are required"
    );
    ensure!(
        !chat.problem.trim().is_empty()
            && !chat.selected_direction.trim().is_empty()
            && !chat.evidence_summary.trim().is_empty(),
        "problem, selected direction, and evidence summary are required"
    );
    ensure!(
        !chat.requirements.is_empty(),
        "implementation-ready Chat Discovery needs binding requirements"
    );
    ensure!(
        chat.blockers.is_empty(),
        "resolve Chat Discovery blockers before importing an implementation-ready bundle: {}",
        chat.blockers.join("; ")
    );
    ensure!(
        chat.constraints.iter().all(|s| !s.trim().is_empty()),
        "empty user constraint"
    );
    let mut ids = HashSet::new();
    for requirement in &chat.requirements {
        let native = Requirement {
            id: requirement.id.clone(),
            text: requirement.text.clone(),
            acceptance: requirement.acceptance.clone(),
            condition: requirement.condition.clone(),
            governing: false,
        };
        orchestrate_contracts::validate_requirement(&native)?;
        match requirement.authority {
            ChatAuthority::ExplicitUser => ensure!(
                requirement
                    .user_statement
                    .as_ref()
                    .is_some_and(|statement| !statement.trim().is_empty()),
                "explicit_user requirement {} needs a non-empty user_statement",
                native.id
            ),
            ChatAuthority::ChatAnalysis => ensure!(
                requirement.user_statement.is_none(),
                "chat_analysis requirement {} cannot claim a user_statement",
                native.id
            ),
        }
        ensure!(
            !native.id.starts_with("GOV-") && ids.insert(native.id.clone()),
            "duplicate or reserved requirement ID {}",
            native.id
        );
    }
    ensure!(
        chat.rejected_alternatives
            .iter()
            .all(|item| !item.direction.trim().is_empty() && !item.reason.trim().is_empty()),
        "rejected alternatives need direction and reason"
    );
    ensure!(
        chat.advisory_technical_suggestions
            .iter()
            .all(|item| !item.trim().is_empty()),
        "empty technical suggestion"
    );
    let source_commit = git_head(repo)?;
    let effort = store.init_effort(
        repo,
        &chat.effort,
        RequestKind::Freeform,
        chat.request.clone(),
        chat.constraints.clone(),
    )?;
    ensure!(
        effort.baseline_commit == source_commit,
        "existing Effort baseline differs from current repository HEAD"
    );
    ensure!(
        store.list_artifacts(&effort)?.is_empty()
            && !store.effort_dir(&effort).join("build/plan.json").exists(),
        "Effort already has artifacts or a Build plan; use a new effort slug"
    );

    let provenance = Provenance {
        host: "collaborative_chat".into(),
        provider: None,
        model: None,
        model_effort: None,
        guide_digest: String::new(),
        independence: Independence::Unknown,
    };
    let (run, workspace) = orchestrate_discovery::prepare(store, &effort, provenance.clone())?;
    let run_metadata: orchestrate_contracts::DiscoveryRun =
        orchestrate_contracts::decode(&std::fs::read(workspace.join("run.json"))?)?;
    // Keep the portable record intact in the ordinary Discovery's public specification.
    let spec = format!(
        "# Collaborative Chat Discovery\n\n```json\n{}\n```\n",
        serde_json::to_string_pretty(&chat)?
    );
    write_bytes_sync(&workspace.join("technical-spec.md"), spec.as_bytes())?;
    write_bytes_sync(&workspace.join("graph/D-1.md"), b"---\nid: D-1\nkind: decision\nstatus: accepted\n---\n\n# Collaborative chat handoff\n\nThe selected direction is recorded in technical-spec.md. This source is one collaborative conversation; no independent run or repository verification is claimed.\n")?;
    let source = orchestrate_discovery::finalize(store, &effort, &run)?;
    let source_ref = DiscoverySourceRef {
        discovery_artifact_id: source.artifact_id.clone(),
        node_id: None,
    };
    let evidence_summary = format!(
        "{}\n\nFindings: {}\nDecisions: {}",
        chat.evidence_summary,
        chat.findings.join("; "),
        chat.decisions.join("; ")
    );
    let requirements = chat.requirements.into_iter().map(|item| {
        let (governing, source_refs, user_clarification) = match item.authority {
            ChatAuthority::ExplicitUser => (true, vec![], item.user_statement),
            ChatAuthority::ChatAnalysis => (false, vec![source_ref.clone()], None),
        };
        ReconciledRequirement {
            requirement: Requirement { id: item.id, text: item.text, acceptance: item.acceptance, condition: item.condition, governing },
            source_refs, user_clarification, frozen_user_constraint: false,
        }
    }).chain(chat.constraints.iter().enumerate().map(|(index, text)| ReconciledRequirement {
        requirement: Requirement { id: format!("GOV-{}", index + 1), text: text.clone(), acceptance: "Preserve this explicit user constraint in the implementation and Audit.".into(), condition: None, governing: true },
        source_refs: vec![], user_clarification: None, frozen_user_constraint: true,
    })).collect();
    let reconciled = ReconciledDiscovery {
        reconciled_id: format!("reconciled-{run}"),
        context_id: effort.context.id.clone(),
        baseline_commit: effort.baseline_commit.clone(),
        baseline_tree: effort.baseline_tree.clone(),
        goal: effort.context.request.clone(),
        core_result: chat.selected_direction.clone(),
        problem: chat.problem,
        product_behavior_changed: chat.product_behavior_changed,
        product_behavior_unchanged: chat.product_behavior_unchanged,
        technical_behavior_changed: chat.technical_behavior_changed,
        technical_behavior_unchanged: chat.technical_behavior_unchanged,
        requirements,
        discovery_attribution: vec![DiscoveryAttribution {
            discovery_artifact_id: source.artifact_id.clone(),
            label: run_metadata.source_label,
            host: provenance.host.clone(),
            provider: None,
            model: None,
            model_effort: None,
        }],
        evidence_synthesis: vec![EvidenceSynthesis {
            id: "CHAT-1".into(),
            conclusion: chat.selected_direction,
            source_refs: vec![source_ref.clone()],
            verification_methods: vec![],
            evidence_summary,
            limitations: "One collaborative conversation; independence is not claimed.".into(),
        }],
        disagreements: chat.disagreements,
        rejected_alternatives: chat
            .rejected_alternatives
            .into_iter()
            .map(|item| RejectedAlternative {
                direction: item.direction,
                reason: item.reason,
                source_refs: vec![source_ref.clone()],
            })
            .collect(),
        implementation_risks: chat.implementation_risks,
        compatibility_concerns: chat.compatibility_concerns,
        caveats: chat.caveats,
        technical_suggestions: chat
            .advisory_technical_suggestions
            .into_iter()
            .enumerate()
            .map(|(index, text)| TechnicalSuggestion {
                id: format!("CHAT-SUG-{}", index + 1),
                text,
                source_refs: vec![source_ref.clone()],
            })
            .collect(),
        blocking_issues: vec![],
    };
    validate_reconciled_discovery(&reconciled)?;
    let mut files = BTreeMap::new();
    files.insert("reconciled-discovery.json".into(), encode(&reconciled)?);
    files.insert(
        "reconciled-discovery.md".into(),
        orchestrate_reconcile::render_reconciled_discovery(&reconciled).into_bytes(),
    );
    let reconciled_ref = store.publish_bundle(
        &effort,
        "reconcile",
        ArtifactKind::ReconciledDiscovery,
        reconciled.reconciled_id.clone(),
        "IMPLEMENTATION_READY".into(),
        vec![source.clone()],
        provenance,
        files,
    )?;

    let build_dir = store.phase_dir(&effort, "build")?;
    let phase_names = phases.keys().cloned().collect();
    for (name, bytes) in phases {
        let dir = build_dir.join(name);
        std::fs::create_dir_all(&dir)?;
        write_bytes_sync(&dir.join("phase.md"), &bytes)?;
    }
    let plan = orchestrate_build::state::BuildPlan {
        schema_version: orchestrate_build::state::PLAN_VERSION,
        reconciled: reconciled_ref.clone(),
        phases: phase_names,
    };
    write_bytes_sync(&build_dir.join("plan.json"), &encode(&plan)?)?;
    orchestrate_build::scaffold(store, &effort.id)?;
    if !plan.phases.is_empty() {
        orchestrate_build::state::load_plan(&build_dir.join("plan.json"))?;
    }
    Ok(
        serde_json::json!({"effort": effort.id, "baseline": effort.baseline_commit,
        "discovery": source, "reconciled": reconciled_ref, "build_dir": build_dir}),
    )
}

fn valid_phase_name(name: &str) -> bool {
    let Some(suffix) = name.strip_prefix("phase_") else {
        return false;
    };
    let bytes = suffix.as_bytes();
    bytes.len() >= 4
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2] == b'_'
        && bytes[3].is_ascii_alphanumeric()
        && bytes[4..]
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || *byte == b'_')
}

fn git_head(repo: &Path) -> Result<String> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()?;
    ensure!(
        output.status.success(),
        "target repository has no committed HEAD"
    );
    Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}
