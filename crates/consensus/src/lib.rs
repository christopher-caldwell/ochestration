//! Consensus consumes finalized Discovery bundles and publishes a coherent Agreement candidate.

use anyhow::{Context, Result, bail, ensure};
use orchestrate_contracts::{
    Agreement, AgreementRequirement, ArtifactKind, ArtifactRef, ConsensusProposal, EvidenceNode,
    Provenance, Requirement, validate_agreement, validate_requirement,
};
use orchestrate_core::{Effort, Store, invoke_provider_json};
use serde::Serialize;
use std::collections::{BTreeMap, HashMap, HashSet};

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
    nodes_by_slot: &HashMap<String, Vec<EvidenceNode>>,
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
        let supporters: HashSet<_> = item.support.keys().cloned().collect();
        ensure!(
            supporters.len() >= 2,
            "Consensus requirement {} lacks two supporting slots",
            item.requirement.id
        );
        for (slot, ids) in &item.support {
            ensure!(
                matches!(slot.as_str(), "a" | "b" | "c"),
                "unknown supporter slot {slot}"
            );
            let nodes = nodes_by_slot
                .get(slot)
                .context("supporter slot lacks Discovery nodes")?;
            for id in ids {
                let node = nodes.iter().find(|node| node.id == *id).ok_or_else(|| {
                    anyhow::anyhow!("support reference {id} does not exist in slot {slot}")
                })?;
                ensure!(
                    matches!(node.status, orchestrate_contracts::EvidenceStatus::Accepted),
                    "support reference {id} in slot {slot} is not accepted"
                );
            }
        }
        common = Some(match common {
            Some(current) => current.intersection(&supporters).cloned().collect(),
            None => supporters,
        });
        requirements.push(AgreementRequirement {
            requirement: item.requirement.clone(),
            support: item.support.clone(),
        });
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
    store.append_journal(
        effort,
        "consensus_started",
        None,
        serde_json::json!({"inputs":refs.iter().map(|r|&r.artifact_id).collect::<Vec<_>>()}),
    )?;
    let mut opinions = BTreeMap::new();
    let mut nodes_by_slot = HashMap::new();
    let mut public_specs = Vec::new();
    for reference in &refs {
        ensure!(
            reference.kind == ArtifactKind::Discovery,
            "consensus needs Discovery artifacts"
        );
        let (envelope, summary, nodes) =
            orchestrate_discovery::load_discovery(store, effort, reference)?;
        ensure!(
            envelope.outcome == "IMPLEMENTATION_READY" && summary.outcome == "IMPLEMENTATION_READY",
            "blocked Discovery is not an eligible Consensus opinion"
        );
        ensure!(
            envelope.cohort_id.as_deref() == Some(&effort.cohort.id)
                && summary.context_id == effort.context.id
                && summary.baseline_commit == effort.cohort.baseline_commit,
            "incompatible Discovery cohort or baseline"
        );
        if opinions
            .insert(summary.slot.clone(), reference.clone())
            .is_some()
        {
            bail!("duplicate Discovery slot");
        }
        nodes_by_slot.insert(summary.slot.clone(), nodes);
        public_specs.push(summary.slot);
    }
    ensure!(
        opinions.len() == 3
            && ["a", "b", "c"]
                .iter()
                .all(|slot| opinions.contains_key(*slot)),
        "all three distinct eligible slots are required"
    );
    let outcome = (|| -> Result<ConsensusResult> {
        let (mut requirements, common) = mandatory_package_support(&proposal, &nodes_by_slot)?;
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
                support: BTreeMap::new(),
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
            exclusions: vec!["No minority-only scope is mandatory.".into()],
            implementation_latitude: vec![
                "Choose ordinary technical details consistent with the Agreement.".into(),
            ],
            verification_expectations: vec![
                "Audit every stable requirement ID with attributable evidence.".into(),
            ],
            common_supporters: common.into_iter().collect(),
            dissent: proposal.dissent.clone(),
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
                "consensus_rejected",
                None,
                serde_json::json!({"reason":error.to_string()}),
            );
        }
    }
    outcome
}

pub fn run_provider(
    store: &Store,
    effort: &Effort,
    refs: [ArtifactRef; 3],
    provider: &std::path::Path,
    guide: &str,
    provenance: Provenance,
) -> Result<ConsensusResult> {
    let mut opinions = Vec::new();
    for reference in &refs {
        let (_, summary, _) = orchestrate_discovery::load_discovery(store, effort, reference)?;
        let (_, files) = store.load_bundle(effort, reference)?;
        let technical_spec = String::from_utf8(
            files
                .get("technical-spec.md")
                .context("Discovery lacks technical specification")?
                .clone(),
        )?;
        opinions.push(serde_json::json!({"artifact":reference,"summary":summary,"technical_spec":technical_spec}));
    }
    store.append_journal(
        effort,
        "provider_started",
        None,
        serde_json::json!({"phase":"consensus"}),
    )?;
    let packet = serde_json::json!({"phase":"consensus","context_id":effort.context.id,"request":effort.context.request,"constraints":effort.context.constraints,"baseline_commit":effort.cohort.baseline_commit,"opinions":opinions,"guide":guide});
    let response = invoke_provider_json::<_, ConsensusProposal>(provider, &packet, 1_048_576);
    let response = match response {
        Ok(v) => v,
        Err(e) => {
            let _ = store.append_journal(
                effort,
                "provider_failed",
                None,
                serde_json::json!({"reason":e.to_string()}),
            );
            return Err(e);
        }
    };
    store.append_journal(
        effort,
        "provider_completed",
        None,
        serde_json::json!({"phase":"consensus"}),
    )?;
    finalize(store, effort, refs, response.value, provenance)
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
    out.push_str("\n## Dissent\n");
    for dissent in &agreement.dissent {
        out.push_str(&format!("\n- {dissent}"));
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
