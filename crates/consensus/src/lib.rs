//! Consensus consumes finalized Discovery bundles and publishes a coherent Agreement candidate.

use anyhow::{Context, Result, bail, ensure};
use orchestrate_contracts::{
    Agreement, AgreementRequirement, ArtifactKind, ArtifactRef, ConsensusProposal, Provenance,
    Requirement, validate_agreement, validate_requirement,
};
use orchestrate_core::{Effort, Store};
use serde::Serialize;
use std::collections::{BTreeMap, HashSet};

#[derive(Clone, Debug, Serialize)]
pub struct Comparison {
    pub opinions: Vec<ArtifactRef>,
    pub proposal: ConsensusProposal,
    pub eligible: bool,
    pub reason: String,
}
#[derive(Clone, Debug)]
pub struct ConsensusResult {
    pub comparison: ArtifactRef,
    pub agreement: Option<ArtifactRef>,
}

pub fn mandatory_package_support(
    proposal: &ConsensusProposal,
) -> Result<(Vec<AgreementRequirement>, HashSet<String>)> {
    let mut ids = HashSet::new();
    let mut common: Option<HashSet<String>> = None;
    let mut requirements = Vec::new();
    for item in &proposal.requirements {
        validate_requirement(&item.requirement)?;
        ensure!(
            !item.requirement.governing,
            "governing requirements are not Consensus votes"
        );
        ensure!(
            ids.insert(&item.requirement.id),
            "duplicate Consensus requirement id {}",
            item.requirement.id
        );
        let supporters: HashSet<_> = item.supporters.iter().cloned().collect();
        ensure!(
            supporters.len() >= 2,
            "Consensus requirement {} lacks two supporting slots",
            item.requirement.id
        );
        for slot in &supporters {
            ensure!(
                matches!(slot.as_str(), "a" | "b" | "c"),
                "unknown supporter slot {slot}"
            );
        }
        for slot in item.source_refs.keys() {
            ensure!(
                supporters.contains(slot),
                "source references for {slot} lack a corresponding supporter"
            );
        }
        common = Some(match common {
            Some(current) => current.intersection(&supporters).cloned().collect(),
            None => supporters,
        });
        requirements.push(AgreementRequirement {
            requirement: item.requirement.clone(),
            supporters: item.supporters.clone(),
            source_refs: item.source_refs.clone(),
        });
    }
    Ok((requirements, common.unwrap_or_default()))
}

/// Select the sole eligible finalized Discovery artifact for every slot.
///
/// Inference never guesses: a slot with no candidate or more than one candidate stops
/// the operation and requires explicit `--opinion` selectors.
pub fn select_opinions(store: &Store, effort: &Effort) -> Result<BTreeMap<String, ArtifactRef>> {
    let mut candidates: BTreeMap<String, Vec<ArtifactRef>> = BTreeMap::new();
    for reference in store.list_artifacts(effort)? {
        if let Some(slot) = eligible_slot(store, effort, &reference)? {
            candidates.entry(slot).or_default().push(reference);
        }
    }
    let mut selected = BTreeMap::new();
    for slot in ["a", "b", "c"] {
        let found = candidates.remove(slot).unwrap_or_default();
        match found.as_slice() {
            [only] => {
                selected.insert(slot.to_owned(), only.clone());
            }
            [] => bail!(
                "no eligible finalized Discovery artifact exists for slot {slot}; finalize an implementation-ready Discovery for every slot or pass three explicit --opinion selectors"
            ),
            many => bail!(
                "multiple eligible Discovery artifacts exist for slot {slot}: {}; pass three explicit --opinion selectors",
                artifact_ids(many)
            ),
        }
    }
    Ok(selected)
}

/// Bind an explicit set of Consensus input selectors to the exact eligible artifacts.
///
/// This is the only way an explicit parent selection enters a Consensus session, so it
/// validates the whole set before any reconciliation can depend on it: exactly three
/// selectors, each an eligible finalized Discovery artifact, one per distinct slot.
pub fn bind_opinions(
    store: &Store,
    effort: &Effort,
    selectors: &[String],
) -> Result<BTreeMap<String, ArtifactRef>> {
    ensure!(
        selectors.len() == 3,
        "pass exactly three --opinion selectors, or omit --opinion to infer the eligible artifacts"
    );
    let mut selected: BTreeMap<String, ArtifactRef> = BTreeMap::new();
    for selector in selectors {
        let reference = store.find_artifact_ref(effort, selector)?;
        ensure!(
            reference.kind == ArtifactKind::Discovery,
            "consensus needs Discovery artifacts"
        );
        let slot = eligible_slot(store, effort, &reference)?.with_context(|| {
            format!(
                "Discovery artifact {} is not an eligible Consensus opinion",
                reference.artifact_id
            )
        })?;
        ensure!(
            selected.insert(slot, reference).is_none(),
            "duplicate Discovery slot"
        );
    }
    ensure!(
        ["a", "b", "c"]
            .iter()
            .all(|slot| selected.contains_key(*slot)),
        "all three distinct eligible slots are required"
    );
    Ok(selected)
}

pub fn finalize(
    store: &Store,
    effort: &Effort,
    opinions: BTreeMap<String, ArtifactRef>,
    proposal: ConsensusProposal,
    provenance: Provenance,
) -> Result<ConsensusResult> {
    // Finalization re-binds its parents through the same resolution path that produced
    // them, so it can never publish a comparison against an ineligible artifact.
    let selectors = opinions
        .values()
        .map(|reference| reference.artifact_id.clone())
        .collect::<Vec<_>>();
    let opinions = bind_opinions(store, effort, &selectors)?;
    let refs: Vec<ArtifactRef> = opinions.values().cloned().collect();
    store.append_journal(
        effort,
        "consensus_started",
        None,
        serde_json::json!({"inputs":refs.iter().map(|r|&r.artifact_id).collect::<Vec<_>>()}),
    )?;
    let outcome = (|| -> Result<ConsensusResult> {
        let (mut requirements, common) = mandatory_package_support(&proposal)?;
        let eligible =
            !proposal.counterexample_blocks && !requirements.is_empty() && common.len() >= 2;
        let reason = if proposal.counterexample_blocks {
            "A concrete counterexample blocks the package."
        } else if requirements.is_empty() {
            "No consensus-derived mandatory package was supplied."
        } else if common.len() < 2 {
            "Mandatory package lacks one common strict majority."
        } else {
            "Eligible majority package."
        }
        .to_owned();
        let comparison = Comparison {
            opinions: refs.to_vec(),
            proposal: proposal.clone(),
            eligible,
            reason: reason.clone(),
        };
        let mut comparison_files = BTreeMap::new();
        comparison_files.insert(
            "comparison.md".into(),
            proposal.comparison_md.as_bytes().to_vec(),
        );
        comparison_files.insert(
            "comparison.json".into(),
            orchestrate_contracts::encode(&comparison)?,
        );
        let comparison_ref = store.publish_bundle(
            effort,
            "consensus",
            ArtifactKind::ConsensusComparison,
            format!("consensus-{}", suffix()),
            if eligible {
                "COMPLETE".into()
            } else {
                "NO_CONSENSUS".into()
            },
            refs.to_vec(),
            provenance.clone(),
            comparison_files,
        )?;
        if !eligible {
            return Ok(ConsensusResult {
                comparison: comparison_ref,
                agreement: None,
            });
        }
        for (index, text) in effort.context.constraints.iter().enumerate() {
            requirements.push(AgreementRequirement {
                requirement: Requirement {
                    id: format!("GOV-{}", index + 1),
                    text: text.clone(),
                    acceptance:
                        "Preserve this governing requirement in the implementation and Audit."
                            .into(),
                    condition: None,
                    governing: true,
                },
                supporters: vec![],
                source_refs: BTreeMap::new(),
            });
        }
        let agreement_id = format!("agreement-{}", suffix());
        let agreement = Agreement {
            agreement_id: agreement_id.clone(),
            context_id: effort.context.id.clone(),
            cohort_id: effort.cohort.id.clone(),
            baseline_commit: effort.cohort.baseline_commit.clone(),
            goal: effort.context.request.clone(),
            requirements,
        };
        validate_agreement(&agreement)?;
        let mut agreement_files = BTreeMap::new();
        agreement_files.insert(
            "agreement.json".into(),
            orchestrate_contracts::encode(&agreement)?,
        );
        agreement_files.insert(
            "agreement.md".into(),
            render_agreement(&agreement).into_bytes(),
        );
        let agreement_ref = store.publish_bundle(
            effort,
            "agreement",
            ArtifactKind::Agreement,
            agreement_id,
            "ELIGIBLE_CANDIDATE".into(),
            vec![comparison_ref.clone()],
            provenance,
            agreement_files,
        )?;
        Ok(ConsensusResult {
            comparison: comparison_ref,
            agreement: Some(agreement_ref),
        })
    })();
    match &outcome {
        Ok(result) => {
            store.append_journal(effort, "consensus_finalized", None, serde_json::json!({"comparison":result.comparison.artifact_id,"agreement":result.agreement.as_ref().map(|r|&r.artifact_id)}))?;
        }
        Err(error) => {
            let _ = store.append_journal(
                effort,
                "finalization_rejected",
                None,
                serde_json::json!({"reason":error.to_string()}),
            );
        }
    }
    outcome
}

/// A Discovery artifact is an eligible Consensus opinion only when it is finalized,
/// implementation-ready, belongs to this effort/cohort/context/baseline, and names
/// a valid slot.
fn eligible_slot(
    store: &Store,
    effort: &Effort,
    reference: &ArtifactRef,
) -> Result<Option<String>> {
    if reference.kind != ArtifactKind::Discovery {
        return Ok(None);
    }
    let (envelope, summary, _) = orchestrate_discovery::load_discovery(store, effort, reference)?;
    let eligible = envelope.outcome == "IMPLEMENTATION_READY"
        && summary.outcome == "IMPLEMENTATION_READY"
        && envelope.cohort_id.as_deref() == Some(effort.cohort.id.as_str())
        && summary.context_id == effort.context.id
        && summary.baseline_commit == effort.cohort.baseline_commit
        && matches!(summary.slot.as_str(), "a" | "b" | "c");
    Ok(if eligible { Some(summary.slot) } else { None })
}

fn artifact_ids(references: &[ArtifactRef]) -> String {
    references
        .iter()
        .map(|reference| reference.artifact_id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}
fn render_agreement(agreement: &Agreement) -> String {
    let mut out = format!(
        "# Agreement {}\n\n## Goal\n\n{}\n\n## Requirements\n",
        agreement.agreement_id, agreement.goal
    );
    for item in &agreement.requirements {
        out.push_str(&format!(
            "\n### {}\n\n{}\n\n**Acceptance:** {}\n",
            item.requirement.id, item.requirement.text, item.requirement.acceptance
        ));
        if let Some(condition) = &item.requirement.condition {
            out.push_str(&format!("\n**Condition:** {condition}\n"));
        }
        if item.requirement.governing {
            out.push_str("\n_Governing user requirement._\n");
        }
    }
    out.push('\n');
    out
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
    use orchestrate_contracts::ConsensusRequirement;

    fn requirement(id: &str, supporters: &[&str]) -> ConsensusRequirement {
        ConsensusRequirement {
            requirement: Requirement {
                id: id.into(),
                text: "required behavior".into(),
                acceptance: "proof".into(),
                condition: None,
                governing: false,
            },
            supporters: supporters.iter().map(|slot| (*slot).into()).collect(),
            source_refs: BTreeMap::new(),
        }
    }

    #[test]
    fn rotating_majorities_do_not_form_an_eligible_package() {
        let proposal = ConsensusProposal {
            requirements: vec![
                requirement("R1", &["a", "b"]),
                requirement("R2", &["b", "c"]),
            ],
            comparison_md: "comparison".into(),
            selection_rationale: "selection".into(),
            dissent: vec![],
            counterexample_blocks: false,
        };

        let (requirements, common) = mandatory_package_support(&proposal).unwrap();
        assert_eq!(common, HashSet::from(["b".to_owned()]));
        let eligible =
            !proposal.counterexample_blocks && !requirements.is_empty() && common.len() >= 2;
        assert!(!eligible);
    }
}
