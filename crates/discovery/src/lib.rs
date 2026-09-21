//! Discovery workspaces, Markdown evidence graphs, and immutable publication.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fs,
};

use anyhow::{Context, Result, ensure};
use orchestrate_contracts::{
    ArtifactKind, ArtifactRef, DiscoveryRun, DiscoverySummary, EvidenceKind, EvidenceNode,
    EvidenceStatus, Independence, Provenance, RequestKind, Verification, validate_evidence_node,
};
use orchestrate_core::{Effort, Store};
use serde::Deserialize;

#[derive(Clone, Debug, Deserialize)]
struct WorkspaceContext {
    context_id: String,
    request_kind: RequestKind,
    baseline_commit: String,
    baseline_tree: String,
}
#[derive(Clone, Debug, Deserialize)]
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
    #[serde(default)]
    verification: Option<Verification>,
}
#[derive(Clone, Debug)]
pub struct ValidatedDiscovery {
    pub run: DiscoveryRun,
    pub summary: DiscoverySummary,
    pub technical_spec: String,
    pub nodes: Vec<EvidenceNode>,
    pub files: BTreeMap<String, Vec<u8>>,
}

pub fn prepare(
    store: &Store,
    effort: &Effort,
    provenance: Provenance,
) -> Result<(String, std::path::PathBuf)> {
    let run_id = format!("discovery-{}", suffix());
    let source_label = source_label(&provenance.host, &run_id);
    let run = DiscoveryRun {
        run_id: run_id.clone(),
        phase: "discovery".into(),
        effort_id: effort.id.clone(),
        context_id: effort.context.id.clone(),
        request_kind: effort.context.request_kind.clone(),
        baseline_commit: effort.baseline_commit.clone(),
        baseline_tree: effort.baseline_tree.clone(),
        host: provenance.host,
        provider: provenance.provider,
        model: provenance.model,
        model_effort: provenance.model_effort,
        independence: provenance.independence,
        guide_digest: provenance.guide_digest,
        source_label,
    };
    let workspace = store.discovery_workspace(effort, &run)?;
    Ok((run_id, workspace))
}

pub fn validate(store: &Store, effort: &Effort, run_id: &str) -> Result<ValidatedDiscovery> {
    let root = store.phase_dir(effort, "discovery")?.join(run_id);
    let run_bytes =
        fs::read(root.join("run.json")).context("Discovery workspace lacks run.json")?;
    let run: DiscoveryRun = orchestrate_contracts::decode(&run_bytes)?;
    ensure!(
        run.run_id == run_id
            && run.phase == "discovery"
            && run.effort_id == effort.id
            && run.context_id == effort.context.id
            && run.request_kind == effort.context.request_kind
            && run.baseline_commit == effort.baseline_commit
            && run.baseline_tree == effort.baseline_tree,
        "Discovery run belongs to another effort, context, or baseline"
    );
    let context: WorkspaceContext = orchestrate_contracts::decode(
        &fs::read(root.join("context.json")).context("Discovery workspace lacks context.json")?,
    )?;
    ensure!(
        context.context_id == effort.context.id
            && context.request_kind == effort.context.request_kind
            && context.baseline_commit == effort.baseline_commit
            && context.baseline_tree == effort.baseline_tree,
        "Discovery workspace belongs to another effort or baseline"
    );
    let technical_bytes = fs::read(root.join("technical-spec.md"))
        .context("Discovery workspace lacks technical-spec.md")?;
    let technical_spec =
        String::from_utf8(technical_bytes.clone()).context("technical-spec.md is not UTF-8")?;
    validate_technical_spec(&technical_spec)?;
    let mut nodes = Vec::new();
    let mut files = BTreeMap::new();
    let request =
        fs::read(root.join("request.md")).context("Discovery workspace lacks request.md")?;
    ensure!(
        request == effort.context.request.as_bytes(),
        "Discovery workspace request differs from the effort request"
    );
    files.insert("run.json".into(), run_bytes);
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
    let blocked = nodes
        .iter()
        .any(|n| n.kind == EvidenceKind::Question && n.status == EvidenceStatus::Blocked);
    // A blocked result is a valid final Discovery artifact; readiness is only required otherwise.
    let outcome = if blocked {
        "BLOCKED"
    } else {
        validate_ready(&nodes)?;
        "IMPLEMENTATION_READY"
    };
    let summary = DiscoverySummary {
        context_id: context.context_id,
        baseline_commit: context.baseline_commit,
        baseline_tree: context.baseline_tree,
        outcome: outcome.into(),
        node_ids: nodes.iter().map(|n| n.id.clone()).collect(),
    };
    files.insert(
        "discovery.json".into(),
        orchestrate_contracts::encode(&summary)?,
    );
    Ok(ValidatedDiscovery {
        run,
        summary,
        technical_spec,
        nodes,
        files,
    })
}

pub fn finalize(store: &Store, effort: &Effort, run_id: &str) -> Result<ArtifactRef> {
    let validated = match validate(store, effort, run_id) {
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
    let provenance = validated.run.provenance();
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
        verification: front.verification,
    })
}
fn validate_technical_spec(spec: &str) -> Result<()> {
    ensure!(!spec.trim().is_empty(), "technical-spec.md is empty");
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
        validate_evidence_node(node)?;
        ensure!(
            node.kind != EvidenceKind::Finding
                || node.status != EvidenceStatus::Accepted
                || node.sources.iter().any(|source| !source.trim().is_empty()),
            "accepted finding {} needs at least one source reference",
            node.id
        );
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
    ensure!(
        !nodes.is_empty(),
        "implementation-ready Discovery needs at least one evidence node"
    );
    let map: HashMap<_, _> = nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    for node in nodes {
        if node.kind == EvidenceKind::Question && node.required {
            ensure!(
                matches!(
                    node.status,
                    EvidenceStatus::Answered | EvidenceStatus::NoChange
                ),
                "required question {} lacks a final disposition",
                node.id
            );
            if node.status == EvidenceStatus::Answered {
                ensure!(
                    nodes.iter().any(|candidate| {
                        candidate.kind == EvidenceKind::Finding
                            && candidate.status == EvidenceStatus::Accepted
                            && candidate.depends_on.iter().any(|id| id == &node.id)
                    }),
                    "answered required question {} lacks a downstream accepted finding",
                    node.id
                );
            }
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

fn source_label(host: &str, run_id: &str) -> String {
    let host = host
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect::<String>();
    let host = host.trim_matches('_');
    let host = if host.is_empty() { "discovery" } else { host };
    format!(
        "{}_{}",
        host,
        &orchestrate_contracts::digest_bytes(run_id.as_bytes())[..8]
    )
}
fn suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(id: &str, kind: EvidenceKind, status: EvidenceStatus) -> EvidenceNode {
        let verification =
            matches!(kind, EvidenceKind::Finding).then_some(Verification::Inspection);
        EvidenceNode {
            id: id.into(),
            kind,
            status,
            depends_on: vec![],
            sources: vec!["src/lib.rs:1".into()],
            required: false,
            mandatory: false,
            title: id.into(),
            body: "evidence".into(),
            verification,
        }
    }

    #[test]
    fn technical_spec_needs_content_not_prescribed_headings() {
        assert!(validate_technical_spec("A useful technical specification.").is_ok());
    }

    #[test]
    fn accepted_findings_need_a_nonempty_source() {
        let mut finding = node("F-1", EvidenceKind::Finding, EvidenceStatus::Accepted);
        finding.sources = vec![" ".into()];
        assert!(validate_graph(&[finding]).is_err());
    }

    #[test]
    fn implementation_ready_discovery_needs_evidence() {
        assert!(validate_ready(&[]).is_err());
    }

    #[test]
    fn answered_required_question_needs_an_accepted_finding() {
        let mut question = node("Q-1", EvidenceKind::Question, EvidenceStatus::Answered);
        question.required = true;
        assert!(validate_ready(&[question.clone()]).is_err());

        let mut finding = node("F-1", EvidenceKind::Finding, EvidenceStatus::Accepted);
        finding.depends_on = vec![question.id.clone()];
        assert!(validate_ready(&[question, finding]).is_ok());
    }

    #[test]
    fn no_change_is_terminal_but_open_and_blocked_are_not_ready() {
        let mut no_change = node("Q-1", EvidenceKind::Question, EvidenceStatus::NoChange);
        no_change.required = true;
        assert!(validate_ready(&[no_change]).is_ok());

        let mut open = node("Q-2", EvidenceKind::Question, EvidenceStatus::Open);
        open.required = true;
        assert!(validate_ready(&[open]).is_err());

        let mut blocked = node("Q-3", EvidenceKind::Question, EvidenceStatus::Blocked);
        blocked.required = true;
        assert!(validate_ready(&[blocked]).is_err());
    }
}
