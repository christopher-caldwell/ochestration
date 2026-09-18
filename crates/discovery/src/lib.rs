//! Discovery workspaces, Markdown evidence graphs, and immutable publication.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
    path::Path,
};

use anyhow::{Context, Result, bail, ensure};
use orchestrate_contracts::{
    ArtifactKind, ArtifactRef, DiscoverySubmission, DiscoverySummary, EvidenceKind, EvidenceNode,
    EvidenceStatus, Independence, Provenance, validate_evidence_node,
};
use orchestrate_core::{Effort, Store, invoke_provider_json, write_bytes_sync};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize)]
struct WorkspaceContext {
    cohort_id: String,
    context_id: String,
    slot: String,
    baseline_commit: String,
    baseline_tree: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Frontmatter {
    id: String,
    kind: EvidenceKind,
    status: EvidenceStatus,
    #[serde(default)]
    depends_on: Vec<String>,
    #[serde(default)]
    sources: Vec<String>,
    #[serde(default)]
    required: bool,
    #[serde(default)]
    mandatory: bool,
}
#[derive(Clone, Debug)]
pub struct ValidatedDiscovery {
    pub summary: DiscoverySummary,
    pub technical_spec: String,
    pub nodes: Vec<EvidenceNode>,
    pub files: BTreeMap<String, Vec<u8>>,
}

pub fn prepare(store: &Store, effort: &Effort, slot: &str) -> Result<(String, std::path::PathBuf)> {
    ensure!(matches!(slot, "a" | "b" | "c"), "slot must be a, b, or c");
    let run_id = format!("discovery-{}-{}", slot, suffix());
    let workspace = store.discovery_workspace(effort, &run_id, slot)?;
    Ok((run_id, workspace))
}

pub fn validate(
    store: &Store,
    effort: &Effort,
    run_id: &str,
    requested_outcome: Option<&str>,
) -> Result<ValidatedDiscovery> {
    let root = store.phase_dir(effort, "discovery")?.join(run_id);
    let context: WorkspaceContext = orchestrate_contracts::decode(
        &fs::read(root.join("context.json")).context("Discovery workspace lacks context.json")?,
    )?;
    ensure!(
        context.cohort_id == effort.cohort.id
            && context.context_id == effort.context.id
            && context.baseline_commit == effort.cohort.baseline_commit
            && context.baseline_tree == effort.cohort.baseline_tree,
        "Discovery workspace belongs to another cohort or baseline"
    );
    let technical_bytes = fs::read(root.join("technical-spec.md"))
        .context("Discovery workspace lacks technical-spec.md")?;
    let technical_spec =
        String::from_utf8(technical_bytes.clone()).context("technical-spec.md is not UTF-8")?;
    validate_technical_spec(&technical_spec)?;
    let mut nodes = Vec::new();
    let mut files = BTreeMap::new();
    files.insert("technical-spec.md".into(), technical_bytes);
    let graph = root.join("graph");
    ensure!(graph.is_dir(), "Discovery workspace lacks graph directory");
    for entry in fs::read_dir(&graph)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let bytes = fs::read(&path)?;
        let node =
            parse_node(&String::from_utf8(bytes.clone()).context("graph node is not UTF-8")?)?;
        validate_evidence_node(&node)?;
        files.insert(
            format!("graph/{}", entry.file_name().to_string_lossy()),
            bytes,
        );
        nodes.push(node);
    }
    validate_graph(&nodes)?;
    let blocked = nodes.iter().any(|n| {
        n.kind == EvidenceKind::Question && n.required && n.status == EvidenceStatus::Blocked
    });
    let outcome = requested_outcome.unwrap_or(if blocked {
        "BLOCKED"
    } else {
        "IMPLEMENTATION_READY"
    });
    match outcome {
        "IMPLEMENTATION_READY" => validate_ready(&nodes)?,
        "BLOCKED" => ensure!(
            blocked,
            "blocked Discovery needs a required blocked question"
        ),
        _ => bail!("Discovery outcome must be IMPLEMENTATION_READY or BLOCKED"),
    }
    let summary = DiscoverySummary {
        context_id: context.context_id,
        cohort_id: context.cohort_id,
        baseline_commit: context.baseline_commit,
        baseline_tree: context.baseline_tree,
        slot: context.slot,
        outcome: outcome.into(),
        node_ids: nodes.iter().map(|n| n.id.clone()).collect(),
    };
    files.insert(
        "discovery.json".into(),
        orchestrate_contracts::encode(&summary)?,
    );
    Ok(ValidatedDiscovery {
        summary,
        technical_spec,
        nodes,
        files,
    })
}

pub fn finalize(
    store: &Store,
    effort: &Effort,
    run_id: &str,
    outcome: Option<&str>,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    store.append_journal(
        effort,
        "discovery_finalization_started",
        Some(run_id),
        serde_json::json!({}),
    )?;
    let validated = match validate(store, effort, run_id, outcome) {
        Ok(value) => value,
        Err(error) => {
            let _ = store.append_journal(
                effort,
                "finalization_rejected",
                Some(run_id),
                serde_json::json!({"reason": error.to_string()}),
            );
            return Err(error);
        }
    };
    ensure!(
        !matches!(provenance.independence, Independence::Compromised),
        "known-compromised Discovery cannot finalize"
    );
    let reference = store.publish_bundle(
        effort,
        "discovery",
        ArtifactKind::Discovery,
        run_id.into(),
        validated.summary.outcome.clone(),
        Vec::new(),
        provenance,
        validated.files,
    )?;
    store.append_journal(effort, "discovery_finalized", Some(run_id), serde_json::json!({"artifact": reference.artifact_id, "outcome": validated.summary.outcome}))?;
    Ok(reference)
}

pub fn run_provider(
    store: &Store,
    effort: &Effort,
    slot: &str,
    provider: &Path,
    guide: &str,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    let (run_id, workspace) = prepare(store, effort, slot)?;
    store.append_journal(
        effort,
        "provider_started",
        Some(&run_id),
        serde_json::json!({"phase":"discovery"}),
    )?;
    let packet = serde_json::json!({"phase":"discovery", "run_id":run_id, "slot":slot, "request":effort.context.request, "constraints":effort.context.constraints, "context_id":effort.context.id, "baseline_commit":effort.cohort.baseline_commit, "frozen_source":workspace.join("source"), "guide":guide});
    let response = invoke_provider_json::<_, DiscoverySubmission>(provider, &packet, 1_048_576);
    let response = match response {
        Ok(v) => v,
        Err(e) => {
            let _ = store.append_journal(
                effort,
                "provider_failed",
                Some(&run_id),
                serde_json::json!({"reason":e.to_string()}),
            );
            return Err(e);
        }
    };
    materialize_submission(&workspace, &response.value)?;
    store.append_journal(
        effort,
        "provider_completed",
        Some(&run_id),
        serde_json::json!({"phase":"discovery"}),
    )?;
    finalize(store, effort, &run_id, None, provenance)
}

pub fn load_discovery(
    store: &Store,
    effort: &Effort,
    reference: &ArtifactRef,
) -> Result<(
    orchestrate_contracts::Envelope,
    DiscoverySummary,
    Vec<EvidenceNode>,
)> {
    ensure!(
        reference.kind == ArtifactKind::Discovery,
        "expected a Discovery artifact"
    );
    let (envelope, files) = store.load_bundle(effort, reference)?;
    let summary: DiscoverySummary = orchestrate_contracts::decode(
        files
            .get("discovery.json")
            .context("Discovery bundle lacks discovery.json")?,
    )?;
    let mut nodes = Vec::new();
    for (path, bytes) in files {
        if path.starts_with("graph/") {
            nodes.push(parse_node(&String::from_utf8(bytes)?)?);
        }
    }
    validate_graph(&nodes)?;
    Ok((envelope, summary, nodes))
}

fn materialize_submission(workspace: &Path, submission: &DiscoverySubmission) -> Result<()> {
    write_bytes_sync(
        &workspace.join("technical-spec.md"),
        submission.technical_spec.as_bytes(),
    )?;
    for node in &submission.nodes {
        validate_evidence_node(node)?;
        write_bytes_sync(
            &workspace.join("graph").join(format!("{}.md", node.id)),
            render_node(node).as_bytes(),
        )?;
    }
    Ok(())
}
fn render_node(node: &EvidenceNode) -> String {
    let front = Frontmatter {
        id: node.id.clone(),
        kind: node.kind.clone(),
        status: node.status.clone(),
        depends_on: node.depends_on.clone(),
        sources: node.sources.clone(),
        required: node.required,
        mandatory: node.mandatory,
    };
    format!(
        "---\n{}---\n\n# {}\n\n{}\n",
        serde_yaml::to_string(&front).unwrap_or_default(),
        if node.title.is_empty() {
            &node.id
        } else {
            &node.title
        },
        node.body
    )
}
fn parse_node(input: &str) -> Result<EvidenceNode> {
    let rest = input
        .strip_prefix("---\n")
        .or_else(|| input.strip_prefix("---\r\n"))
        .context("graph node lacks YAML frontmatter")?;
    let (front, body) = rest
        .split_once("\n---")
        .or_else(|| rest.split_once("\r\n---"))
        .context("graph node has unterminated frontmatter")?;
    let front: Frontmatter =
        serde_yaml::from_str(front).context("invalid graph node frontmatter")?;
    let content = body.trim_start_matches('-').trim();
    let title = content
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("# "))
        .unwrap_or(&front.id)
        .to_owned();
    let body = content
        .lines()
        .skip(1)
        .collect::<Vec<_>>()
        .join("\n")
        .trim()
        .to_owned();
    Ok(EvidenceNode {
        id: front.id,
        kind: front.kind,
        status: front.status,
        depends_on: front.depends_on,
        sources: front.sources,
        required: front.required,
        mandatory: front.mandatory,
        title,
        body,
    })
}
fn validate_technical_spec(spec: &str) -> Result<()> {
    ensure!(!spec.trim().is_empty(), "technical-spec.md is empty");
    let lower = spec.to_ascii_lowercase();
    for heading in [
        "interpreted request",
        "current behavior",
        "recommendation",
        "required changes",
        "unchanged behavior",
        "decisions",
        "requirements",
        "conditions",
        "alternatives",
        "limitations",
        "verification",
    ] {
        ensure!(
            lower.contains(heading),
            "technical-spec.md lacks required section: {heading}"
        );
    }
    Ok(())
}
fn validate_graph(nodes: &[EvidenceNode]) -> Result<()> {
    let mut map = HashMap::new();
    for node in nodes {
        ensure!(
            map.insert(node.id.as_str(), node).is_none(),
            "duplicate evidence node id {}",
            node.id
        );
    }
    for node in nodes {
        for dep in &node.depends_on {
            ensure!(
                map.contains_key(dep.as_str()),
                "evidence node {} references missing node {dep}",
                node.id
            );
        }
    }
    let mut visiting = HashSet::new();
    let mut done = HashSet::new();
    for node in nodes {
        visit(node.id.as_str(), &map, &mut visiting, &mut done)?;
    }
    Ok(())
}
fn visit<'a>(
    id: &'a str,
    map: &HashMap<&'a str, &'a EvidenceNode>,
    visiting: &mut HashSet<&'a str>,
    done: &mut HashSet<&'a str>,
) -> Result<()> {
    if done.contains(id) {
        return Ok(());
    }
    ensure!(
        visiting.insert(id),
        "evidence graph contains a cycle at {id}"
    );
    for dep in &map[id].depends_on {
        visit(dep, map, visiting, done)?;
    }
    visiting.remove(id);
    done.insert(id);
    Ok(())
}
fn validate_ready(nodes: &[EvidenceNode]) -> Result<()> {
    let map: HashMap<_, _> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    for node in nodes {
        if node.kind == EvidenceKind::Question && node.required {
            ensure!(
                matches!(
                    node.status,
                    EvidenceStatus::Resolved | EvidenceStatus::NoChange
                ),
                "required question {} lacks a final disposition",
                node.id
            );
        }
    }
    let mut memo = HashMap::new();
    for node in nodes {
        if node.kind == EvidenceKind::Requirement
            && node.mandatory
            && node.status == EvidenceStatus::Accepted
        {
            ensure!(
                has_accepted_basis(node.id.as_str(), &map, &mut memo, &mut HashSet::new())?,
                "mandatory requirement {} lacks an accepted finding or decision basis",
                node.id
            );
            ensure!(
                !is_stale(
                    node.id.as_str(),
                    &map,
                    &mut HashMap::new(),
                    &mut HashSet::new()
                )?,
                "mandatory requirement {} is stale",
                node.id
            );
        }
    }
    Ok(())
}
fn has_accepted_basis<'a>(
    id: &'a str,
    map: &HashMap<&'a str, &'a EvidenceNode>,
    memo: &mut HashMap<&'a str, bool>,
    visiting: &mut HashSet<&'a str>,
) -> Result<bool> {
    if let Some(v) = memo.get(id) {
        return Ok(*v);
    }
    ensure!(visiting.insert(id), "evidence graph contains a cycle");
    let node = map[id];
    let own = matches!(node.kind, EvidenceKind::Finding | EvidenceKind::Decision)
        && node.status == EvidenceStatus::Accepted;
    let child = node
        .depends_on
        .iter()
        .map(|d| has_accepted_basis(d, map, memo, visiting))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .any(|v| v);
    visiting.remove(id);
    let result = own || child;
    memo.insert(id, result);
    Ok(result)
}
fn is_stale<'a>(
    id: &'a str,
    map: &HashMap<&'a str, &'a EvidenceNode>,
    memo: &mut HashMap<&'a str, bool>,
    visiting: &mut HashSet<&'a str>,
) -> Result<bool> {
    if let Some(v) = memo.get(id) {
        return Ok(*v);
    }
    ensure!(visiting.insert(id), "evidence graph contains a cycle");
    let node = map[id];
    let direct = matches!(
        node.status,
        EvidenceStatus::Rejected | EvidenceStatus::Invalidated
    );
    let below = node
        .depends_on
        .iter()
        .map(|d| is_stale(d, map, memo, visiting))
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .any(|v| v);
    visiting.remove(id);
    let result = direct || below;
    memo.insert(id, result);
    Ok(result)
}
fn suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
        .to_string()
}
