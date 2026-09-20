//! Closed-world reconciliation of explicitly selected finalized Discovery artifacts.

use anyhow::{Result, ensure};
use orchestrate_contracts::{
    ArtifactKind, ArtifactRef, DiscoverySourceRef, Provenance, ReconcileProposal,
    ReconciledDiscovery, ReconciledRequirement, Requirement, TechnicalSuggestion,
    validate_reconciled_discovery,
};
use orchestrate_core::{Effort, Store};
use std::collections::{BTreeMap, HashSet};

/// Resolve the exact, explicit input set for a Reconcile pass. This is read-only.
pub fn bind_inputs(
    store: &Store,
    effort: &Effort,
    selectors: &[String],
) -> Result<Vec<ArtifactRef>> {
    ensure!(
        selectors.len() >= 2,
        "Reconcile requires at least two distinct finalized Discovery artifacts"
    );
    let mut selected = Vec::with_capacity(selectors.len());
    let mut seen = HashSet::new();
    for selector in selectors {
        let reference = store.find_artifact_ref(effort, selector)?;
        ensure!(
            reference.kind == ArtifactKind::Discovery,
            "Reconcile inputs must be Discovery artifacts"
        );
        ensure!(
            seen.insert(reference.artifact_id.clone()),
            "duplicate Discovery artifact {}",
            reference.artifact_id
        );
        let (envelope, summary, _) =
            orchestrate_discovery::load_discovery(store, effort, &reference)?;
        ensure!(
            envelope.outcome == "IMPLEMENTATION_READY" && summary.outcome == "IMPLEMENTATION_READY",
            "Discovery artifact {} is not implementation-ready",
            reference.artifact_id
        );
        ensure!(
            envelope.effort_id == effort.id
                && envelope.project_id == effort.project_id
                && summary.context_id == effort.context.id
                && summary.baseline_commit == effort.baseline_commit
                && summary.baseline_tree == effort.baseline_tree,
            "Discovery artifact {} belongs to another effort, context, or frozen baseline",
            reference.artifact_id
        );
        selected.push(reference);
    }
    Ok(selected)
}

pub fn finalize(
    store: &Store,
    effort: &Effort,
    selected: Vec<ArtifactRef>,
    proposal: ReconcileProposal,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    let selectors = selected
        .iter()
        .map(|reference| reference.artifact_id.clone())
        .collect::<Vec<_>>();
    let selected = bind_inputs(store, effort, &selectors)?;
    store.append_journal(
        effort,
        "reconcile_started",
        None,
        serde_json::json!({"inputs": selected.iter().map(|r| &r.artifact_id).collect::<Vec<_>>() }),
    )?;

    let result = (|| -> Result<ArtifactRef> {
        ensure!(
            !proposal.reconciliation_md.trim().is_empty(),
            "a Reconciled Discovery needs a nonempty reconciled-discovery.md"
        );
        let mut requirements = proposal.requirements.clone();
        for (index, text) in effort.context.constraints.iter().enumerate() {
            requirements.push(ReconciledRequirement {
                requirement: Requirement {
                    id: format!("GOV-{}", index + 1),
                    text: text.clone(),
                    acceptance:
                        "Preserve this explicit user constraint in the implementation and Audit."
                            .into(),
                    condition: None,
                    governing: true,
                },
                source_refs: vec![],
            });
        }
        validate_sources(
            store,
            effort,
            &selected,
            &requirements,
            &proposal.technical_suggestions,
        )?;
        if proposal.blocking_issues.is_empty() {
            ensure!(
                !requirements.is_empty(),
                "an implementation-ready Reconciled Discovery needs binding requirements"
            );
        }
        let reconciled_id = format!("reconciled-{}", suffix());
        let reconciled = ReconciledDiscovery {
            reconciled_id: reconciled_id.clone(),
            context_id: effort.context.id.clone(),
            baseline_commit: effort.baseline_commit.clone(),
            baseline_tree: effort.baseline_tree.clone(),
            goal: effort.context.request.clone(),
            core_result: proposal.core_result,
            requirements,
            technical_suggestions: proposal.technical_suggestions,
            blocking_issues: proposal.blocking_issues,
        };
        validate_reconciled_discovery(&reconciled)?;
        let outcome = if reconciled.blocking_issues.is_empty() {
            "IMPLEMENTATION_READY"
        } else {
            "BLOCKED"
        };
        let mut reconciliation_md = proposal.reconciliation_md;
        if !effort.context.constraints.is_empty() {
            reconciliation_md.push_str("\n\n## Governing user constraints\n");
            for constraint in &effort.context.constraints {
                reconciliation_md.push_str(&format!("\n- {constraint}\n"));
            }
        }
        let mut files = BTreeMap::new();
        files.insert(
            "reconciled-discovery.md".into(),
            reconciliation_md.into_bytes(),
        );
        files.insert(
            "reconciled-discovery.json".into(),
            orchestrate_contracts::encode(&reconciled)?,
        );
        store.publish_bundle(
            effort,
            "reconcile",
            ArtifactKind::ReconciledDiscovery,
            reconciled_id,
            outcome.into(),
            selected,
            provenance,
            files,
        )
    })();
    match &result {
        Ok(reference) => store.append_journal(
            effort,
            "reconcile_finalized",
            None,
            serde_json::json!({"artifact": reference.artifact_id}),
        )?,
        Err(error) => {
            let _ = store.append_journal(
                effort,
                "finalization_rejected",
                None,
                serde_json::json!({"reason": error.to_string()}),
            );
        }
    }
    result
}

fn validate_sources(
    store: &Store,
    effort: &Effort,
    selected: &[ArtifactRef],
    requirements: &[ReconciledRequirement],
    suggestions: &[TechnicalSuggestion],
) -> Result<()> {
    let selected_ids = selected
        .iter()
        .map(|reference| reference.artifact_id.as_str())
        .collect::<HashSet<_>>();
    for requirement in requirements {
        if !requirement.requirement.governing {
            ensure!(
                !requirement.source_refs.is_empty(),
                "binding requirement {} needs a selected Discovery source reference",
                requirement.requirement.id
            );
        }
        for source in &requirement.source_refs {
            validate_source(store, effort, &selected_ids, source)?;
        }
    }
    for suggestion in suggestions {
        ensure!(
            !suggestion.source_refs.is_empty(),
            "technical suggestion {} needs a selected Discovery source reference",
            suggestion.id
        );
        for source in &suggestion.source_refs {
            validate_source(store, effort, &selected_ids, source)?;
        }
    }
    Ok(())
}

fn validate_source(
    store: &Store,
    effort: &Effort,
    selected_ids: &HashSet<&str>,
    source: &DiscoverySourceRef,
) -> Result<()> {
    ensure!(
        selected_ids.contains(source.discovery_artifact_id.as_str()),
        "source references unselected Discovery artifact {}",
        source.discovery_artifact_id
    );
    if let Some(node_id) = &source.node_id {
        let reference = store.find_artifact_ref(effort, &source.discovery_artifact_id)?;
        let (_, _, nodes) = orchestrate_discovery::load_discovery(store, effort, &reference)?;
        ensure!(
            nodes.iter().any(|node| node.id == *node_id),
            "source references missing evidence node {} in Discovery artifact {}",
            node_id,
            source.discovery_artifact_id
        );
    }
    Ok(())
}

fn suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
        .to_string()
}
