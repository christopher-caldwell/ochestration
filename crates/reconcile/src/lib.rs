//! Closed-world reconciliation of explicitly selected finalized Discovery artifacts.

use anyhow::{Result, ensure};
use orchestrate_contracts::{
    ArtifactKind, ArtifactRef, DiscoveryAttribution, DiscoverySourceRef, EvidenceSynthesis,
    Provenance, ReconcileProposal, ReconciledDiscovery, ReconciledRequirement, RejectedAlternative,
    Requirement, TechnicalSuggestion, validate_reconciled_discovery,
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
        let mut requirements = proposal.requirements.clone();
        for requirement in &mut requirements {
            ensure!(
                !requirement.requirement.governing,
                "Reconcile proposals cannot declare requirement {} governing",
                requirement.requirement.id
            );
            ensure!(
                !requirement.frozen_user_constraint,
                "Reconcile proposals cannot declare requirement {} a frozen user constraint",
                requirement.requirement.id
            );
            if let Some(clarification) = &requirement.user_clarification {
                ensure!(
                    !clarification.trim().is_empty(),
                    "user clarification for requirement {} must not be empty",
                    requirement.requirement.id
                );
                ensure!(
                    requirement.source_refs.is_empty(),
                    "user-clarification requirement {} cannot cite Discovery sources",
                    requirement.requirement.id
                );
                requirement.requirement.governing = true;
            }
        }
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
                user_clarification: None,
                frozen_user_constraint: true,
            });
        }
        validate_sources(
            store,
            effort,
            &selected,
            &requirements,
            &proposal.technical_suggestions,
            &proposal.evidence_synthesis,
            &proposal.rejected_alternatives,
        )?;
        if proposal.blocking_issues.is_empty() {
            ensure!(
                !requirements.is_empty(),
                "an implementation-ready Reconciled Discovery needs binding requirements"
            );
        }
        let mut discovery_attribution = Vec::new();
        for reference in &selected {
            let (envelope, _, _) = orchestrate_discovery::load_discovery(store, effort, reference)?;
            let (_, files) = store.load_bundle(effort, reference)?;
            let run: orchestrate_contracts::DiscoveryRun = orchestrate_contracts::decode(
                files
                    .get("run.json")
                    .expect("validated Discovery contains run.json"),
            )?;
            discovery_attribution.push(DiscoveryAttribution {
                discovery_artifact_id: reference.artifact_id.clone(),
                label: run.source_label,
                host: envelope.provenance.host,
                provider: envelope.provenance.provider,
                model: envelope.provenance.model,
                model_effort: envelope.provenance.model_effort,
            });
        }
        let reconciled_id = format!("reconciled-{}", suffix());
        let reconciled = ReconciledDiscovery {
            reconciled_id: reconciled_id.clone(),
            context_id: effort.context.id.clone(),
            baseline_commit: effort.baseline_commit.clone(),
            baseline_tree: effort.baseline_tree.clone(),
            goal: effort.context.request.clone(),
            core_result: proposal.core_result,
            problem: proposal.problem,
            product_behavior_changed: proposal.product_behavior_changed,
            product_behavior_unchanged: proposal.product_behavior_unchanged,
            technical_behavior_changed: proposal.technical_behavior_changed,
            technical_behavior_unchanged: proposal.technical_behavior_unchanged,
            requirements,
            discovery_attribution,
            evidence_synthesis: proposal.evidence_synthesis,
            disagreements: proposal.disagreements,
            rejected_alternatives: proposal.rejected_alternatives,
            implementation_risks: proposal.implementation_risks,
            compatibility_concerns: proposal.compatibility_concerns,
            caveats: proposal.caveats,
            technical_suggestions: proposal.technical_suggestions,
            blocking_issues: proposal.blocking_issues,
        };
        validate_reconciled_discovery(&reconciled)?;
        let outcome = if reconciled.blocking_issues.is_empty() {
            "IMPLEMENTATION_READY"
        } else {
            "BLOCKED"
        };
        let mut files = BTreeMap::new();
        files.insert(
            "reconciled-discovery.md".into(),
            render_reconciled_discovery(&reconciled).into_bytes(),
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
    evidence_synthesis: &[EvidenceSynthesis],
    rejected_alternatives: &[RejectedAlternative],
) -> Result<()> {
    let selected_ids = selected
        .iter()
        .map(|reference| reference.artifact_id.as_str())
        .collect::<HashSet<_>>();
    for requirement in requirements {
        if requirement.user_clarification.is_none() && !requirement.frozen_user_constraint {
            ensure!(
                !requirement.source_refs.is_empty(),
                "binding requirement {} needs a selected Discovery source reference",
                requirement.requirement.id
            );
            for source in &requirement.source_refs {
                validate_source(store, effort, &selected_ids, source)?;
            }
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
    for synthesis in evidence_synthesis {
        for source in &synthesis.source_refs {
            validate_source(store, effort, &selected_ids, source)?;
        }
    }
    for alternative in rejected_alternatives {
        for source in &alternative.source_refs {
            validate_source(store, effort, &selected_ids, source)?;
        }
    }
    Ok(())
}

/// Render the one human-readable Reconciled Discovery from its authoritative structured contract.
pub fn render_reconciled_discovery(reconciled: &ReconciledDiscovery) -> String {
    let mut markdown = format!(
        "# Reconciled Discovery\n\n## Selected direction\n\n{}\n\n## Original goal\n\n{}\n\n## Problem established by Discovery\n\n{}\n",
        reconciled.core_result, reconciled.goal, reconciled.problem
    );
    render_list(
        &mut markdown,
        "Product behavior changed",
        &reconciled.product_behavior_changed,
    );
    render_list(
        &mut markdown,
        "Product behavior intentionally unchanged",
        &reconciled.product_behavior_unchanged,
    );
    render_list(
        &mut markdown,
        "Technical behavior changed",
        &reconciled.technical_behavior_changed,
    );
    render_list(
        &mut markdown,
        "Technical behavior intentionally unchanged",
        &reconciled.technical_behavior_unchanged,
    );
    markdown.push_str("\n## Discovery sources\n");
    for source in &reconciled.discovery_attribution {
        markdown.push_str(&format!(
            "\n- **{}** — `{}`; host `{}`",
            source.label, source.discovery_artifact_id, source.host
        ));
        if let Some(model) = &source.model {
            markdown.push_str(&format!("; model `{model}`"));
        }
        markdown.push('\n');
    }
    markdown.push_str("\n## Binding requirements\n");
    if reconciled.requirements.is_empty() {
        markdown.push_str("\nNone.\n");
    }
    for item in &reconciled.requirements {
        let requirement = &item.requirement;
        markdown.push_str(&format!(
            "\n### {}\n\n{}\n\n**Acceptance criteria:** {}\n",
            requirement.id, requirement.text, requirement.acceptance
        ));
        if let Some(condition) = &requirement.condition {
            markdown.push_str(&format!("\n**Condition:** {condition}\n"));
        }
        match (&item.user_clarification, item.frozen_user_constraint) {
            (None, false) => {
                markdown.push_str("\n**Authority:** Selected Discovery evidence\n");
                for source in &item.source_refs {
                    let node = source
                        .node_id
                        .as_deref()
                        .map_or(String::new(), |node| format!(" / {node}"));
                    markdown.push_str(&format!(
                        "\n- {}{}\n",
                        source_name(reconciled, source),
                        node
                    ));
                }
            }
            (Some(clarification), false) => {
                markdown.push_str(&format!(
                    "\n**Authority:** Explicit Reconcile-time user clarification\n\n> {clarification}\n"
                ));
            }
            (None, true) => {
                markdown.push_str("\n**Authority:** Frozen explicit user constraint\n");
            }
            (Some(_), true) => unreachable!("validated reconciled requirement authority"),
        }
    }
    markdown.push_str("\n## Evidence synthesis\n");
    for item in &reconciled.evidence_synthesis {
        markdown.push_str(&format!(
            "\n### {}\n\n{}\n\n**Evidence:** {}\n\n**Verification:** {}\n\n**Limitations:** {}\n\n**Sources:**\n",
            item.id,
            item.conclusion,
            item.evidence_summary,
            item.verification_methods.iter().map(|method| format!("{:?}", method).to_lowercase()).collect::<Vec<_>>().join(", "),
            if item.limitations.trim().is_empty() { "None reported." } else { &item.limitations }
        ));
        for source in &item.source_refs {
            markdown.push_str(&format!("\n- {}\n", source_name(reconciled, source)));
        }
    }
    render_list(
        &mut markdown,
        "Important disagreements",
        &reconciled.disagreements,
    );
    markdown.push_str("\n## Strongest rejected alternatives\n");
    if reconciled.rejected_alternatives.is_empty() {
        markdown.push_str("\nNone.\n");
    }
    for alternative in &reconciled.rejected_alternatives {
        markdown.push_str(&format!(
            "\n### {}\n\n{}\n",
            alternative.direction, alternative.reason
        ));
        for source in &alternative.source_refs {
            markdown.push_str(&format!("\n- {}\n", source_name(reconciled, source)));
        }
    }
    render_list(
        &mut markdown,
        "Implementation risks",
        &reconciled.implementation_risks,
    );
    render_list(
        &mut markdown,
        "Compatibility concerns",
        &reconciled.compatibility_concerns,
    );
    render_list(&mut markdown, "Caveats", &reconciled.caveats);
    markdown.push_str("\n## Advisory technical suggestions\n");
    if reconciled.technical_suggestions.is_empty() {
        markdown.push_str("\nNone.\n");
    }
    for suggestion in &reconciled.technical_suggestions {
        markdown.push_str(&format!(
            "\n### {}\n\n{}\n\n**Discovery evidence:**\n",
            suggestion.id, suggestion.text
        ));
        for source in &suggestion.source_refs {
            let node = source
                .node_id
                .as_deref()
                .map_or(String::new(), |node| format!(" / {node}"));
            markdown.push_str(&format!(
                "\n- {}{}\n",
                source_name(reconciled, source),
                node
            ));
        }
    }
    markdown.push_str("\n## Blocking issues\n");
    if reconciled.blocking_issues.is_empty() {
        markdown.push_str("\nNone.\n");
    } else {
        for issue in &reconciled.blocking_issues {
            markdown.push_str(&format!("\n- {issue}\n"));
        }
    }
    markdown
}

fn render_list(markdown: &mut String, title: &str, values: &[String]) {
    markdown.push_str(&format!("\n## {title}\n"));
    if values.is_empty() {
        markdown.push_str("\nNone.\n");
    } else {
        for value in values {
            markdown.push_str(&format!("\n- {value}\n"));
        }
    }
}

fn source_name(reconciled: &ReconciledDiscovery, source: &DiscoverySourceRef) -> String {
    reconciled
        .discovery_attribution
        .iter()
        .find(|item| item.discovery_artifact_id == source.discovery_artifact_id)
        .map(|item| format!("{} (`{}`)", item.label, item.discovery_artifact_id))
        .unwrap_or_else(|| source.discovery_artifact_id.clone())
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
