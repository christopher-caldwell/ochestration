//! Consensus consumes only finalized Discovery public artifacts and produces an Agreement candidate.

use std::collections::{BTreeMap, HashMap, HashSet};

use anyhow::{Result, bail, ensure};
use orchestrate_contracts::{
    Agreement, ArtifactKind, ArtifactRef, ConsensusProposal, Independence, Opinion, Position,
    Provenance, Requirement, validate_agreement,
};
use orchestrate_core::{Effort, Store, invoke_provider_json};
use serde::Serialize;

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

/// Validates the proposal's fixed three-slot matrix and computes support for the entire package.
/// It intentionally intersects supporter sets; per-claim majorities cannot be unioned.
pub fn mandatory_package_support(
    proposal: &ConsensusProposal,
) -> Result<(Vec<Requirement>, HashSet<String>)> {
    let adopted: HashSet<_> = proposal
        .adopted_requirement_ids
        .iter()
        .map(String::as_str)
        .collect();
    ensure!(
        adopted.len() == proposal.adopted_requirement_ids.len(),
        "duplicate adopted requirement id"
    );
    let records: HashMap<_, _> = proposal
        .positions
        .iter()
        .map(|record| (record.requirement.id.as_str(), record))
        .collect();
    ensure!(
        records.len() == proposal.positions.len(),
        "duplicate proposal requirement id"
    );
    let mut common: Option<HashSet<String>> = None;
    let mut requirements = Vec::new();
    for id in &proposal.adopted_requirement_ids {
        let record = records.get(id.as_str()).copied().ok_or_else(|| {
            anyhow::anyhow!("adopted requirement {id} lacks a validated position record")
        })?;
        ensure!(
            record.positions.len() == 3
                && ["a", "b", "c"]
                    .iter()
                    .all(|slot| record.positions.contains_key(*slot)),
            "position matrix is incomplete for {id}"
        );
        let supporters: HashSet<_> = record
            .positions
            .iter()
            .filter_map(|(slot, position)| {
                matches!(position, Position::Supports).then_some(slot.clone())
            })
            .collect();
        common = Some(match common {
            Some(existing) => existing.intersection(&supporters).cloned().collect(),
            None => supporters,
        });
        requirements.push(record.requirement.clone());
    }
    Ok((requirements, common.unwrap_or_default()))
}

pub fn finalize(
    store: &Store,
    effort: &Effort,
    refs: [ArtifactRef; 3],
    proposal: ConsensusProposal,
    provenance: Provenance,
) -> Result<ConsensusResult> {
    let mut opinions = BTreeMap::new();
    for reference in &refs {
        ensure!(
            reference.kind == ArtifactKind::DiscoveryOpinion,
            "consensus needs Discovery opinions"
        );
        ensure!(
            store.lineage_invalidation(effort, reference)?.is_none(),
            "selected Discovery opinion has an invalidated authority lineage"
        );
        let (envelope, opinion): (_, Opinion) = store.load_artifact(effort, reference)?;
        ensure!(
            envelope.outcome == "FINALIZED",
            "consensus input is not finalized"
        );
        ensure!(
            envelope.cohort_id.as_deref() == Some(&effort.cohort.id),
            "opinion comes from another cohort"
        );
        ensure!(
            !matches!(envelope.provenance.independence, Independence::Compromised),
            "compromised opinion is ineligible"
        );
        ensure!(
            opinion.context_id == effort.context.id
                && opinion.baseline_commit == effort.cohort.baseline_commit,
            "incompatible opinion baseline/context"
        );
        if opinions
            .insert(opinion.slot.clone(), (reference.clone(), opinion))
            .is_some()
        {
            bail!("duplicate Discovery slot");
        }
    }
    ensure!(
        opinions.len() == 3
            && ["a", "b", "c"]
                .iter()
                .all(|slot| opinions.contains_key(*slot)),
        "all three distinct eligible slots are required"
    );
    let (mut requirements, common) = mandatory_package_support(&proposal)?;
    let governing = effort
        .context
        .constraints
        .iter()
        .enumerate()
        .map(|(index, text)| Requirement {
            id: format!("GOV-{}", index + 1),
            text: text.clone(),
            acceptance: "Preserve this governing requirement in the implementation and Audit."
                .to_owned(),
            condition: None,
            governing: true,
        })
        .collect::<Vec<_>>();
    requirements.extend(governing);
    let eligible = !proposal.counterexample_blocks
        && !proposal.adopted_requirement_ids.is_empty()
        && common.len() >= 2;
    let reason = if proposal.counterexample_blocks {
        "A concrete counterexample blocks the package."
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
    let comparison_ref = store.publish(
        effort,
        "consensus",
        ArtifactKind::ConsensusComparison,
        format!("consensus-{}", unique_suffix()),
        if eligible { "COMPLETE" } else { "NO_CONSENSUS" }.to_owned(),
        refs.to_vec(),
        provenance.clone(),
        &comparison,
    )?;
    if !eligible {
        return Ok(ConsensusResult {
            comparison: comparison_ref,
            agreement: None,
        });
    }
    let agreement = Agreement {
        context_id: effort.context.id.clone(),
        baseline_commit: effort.cohort.baseline_commit.clone(),
        goal: effort.context.request.clone(),
        requirements,
        exclusions: vec!["No untracked minority-only scope is mandatory.".to_owned()],
        implementation_latitude: vec![
            "Choose ordinary technical details consistent with the required behavior.".to_owned(),
        ],
        verification_expectations: vec![
            "Audit every stable requirement ID with attributable evidence.".to_owned(),
        ],
        common_supporters: common.into_iter().collect(),
        dissent: proposal.dissent,
    };
    validate_agreement(&agreement)?;
    let agreement_ref = store.publish(
        effort,
        "agreement",
        ArtifactKind::Agreement,
        format!("agreement-{}", unique_suffix()),
        "ELIGIBLE_CANDIDATE".to_owned(),
        vec![comparison_ref.clone()],
        provenance,
        &agreement,
    )?;
    Ok(ConsensusResult {
        comparison: comparison_ref,
        agreement: Some(agreement_ref),
    })
}

/// Invokes one selected reconciler with an allowlisted projection of finalized public opinions.
/// Private Discovery state, source workspaces, and sibling run directories are not included.
pub fn run_provider(
    store: &Store,
    effort: &Effort,
    refs: [ArtifactRef; 3],
    provider: &std::path::Path,
    provenance: Provenance,
) -> Result<ConsensusResult> {
    let mut opinions = Vec::new();
    for reference in &refs {
        let (_, opinion): (_, Opinion) = store.load_artifact(effort, reference)?;
        opinions.push(serde_json::json!({"artifact": reference, "opinion": opinion}));
    }
    let packet = serde_json::json!({
        "phase": "consensus",
        "context_id": effort.context.id,
        "request": effort.context.request,
        "constraints": effort.context.constraints,
        "baseline_commit": effort.cohort.baseline_commit,
        "opinions": opinions,
    });
    let response = invoke_provider_json::<_, ConsensusProposal>(provider, &packet, 1_048_576)?;
    finalize(store, effort, refs, response.value, provenance)
}

fn unique_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestrate_contracts::PositionRecord;
    fn record(id: &str, supporters: &[&str]) -> PositionRecord {
        let positions = ["a", "b", "c"]
            .into_iter()
            .map(|slot| {
                (
                    slot.to_owned(),
                    if supporters.contains(&slot) {
                        Position::Supports
                    } else {
                        Position::NotObserved
                    },
                )
            })
            .collect();
        PositionRecord {
            requirement: Requirement {
                id: id.to_owned(),
                text: id.to_owned(),
                acceptance: "check".to_owned(),
                condition: None,
                governing: false,
            },
            positions,
        }
    }
    #[test]
    fn rotating_majorities_do_not_create_a_package_majority() {
        let proposal = ConsensusProposal {
            positions: vec![record("X", &["a", "b"]), record("Y", &["b", "c"])],
            adopted_requirement_ids: vec!["X".to_owned(), "Y".to_owned()],
            selection_rationale: "fixture".to_owned(),
            dissent: Vec::new(),
            counterexample_blocks: false,
        };
        let (_, supporters) = mandatory_package_support(&proposal).unwrap();
        assert_eq!(supporters, HashSet::from(["b".to_owned()]));
    }
}
