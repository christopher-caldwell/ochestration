//! Explicit Agreement adoption, external implementation registration, and Audit validation.

use anyhow::{Result, ensure};
use orchestrate_contracts::{
    Adoption, Agreement, ArtifactKind, ArtifactRef, AuditAssessment, AuditReport, Coverage,
    Finding, Implementation, ImplementationStatus, Provenance, Verdict, derive_verdict,
};
use orchestrate_core::{Effort, Store, invoke_provider_json, now_ms};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path, process::Command};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct AuditProposal {
    pub coverage: Vec<Coverage>,
    pub findings: Vec<Finding>,
    pub assessor_context: String,
}

pub fn adopt(
    store: &Store,
    effort: &Effort,
    agreement_ref: ArtifactRef,
    authorization_label: String,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    ensure!(
        agreement_ref.kind == ArtifactKind::Agreement,
        "only an Agreement can be adopted"
    );
    let (envelope, _): (_, Agreement) =
        store.load_json(effort, &agreement_ref, "agreement.json")?;
    ensure!(
        envelope.outcome == "ELIGIBLE_CANDIDATE",
        "Agreement is not eligible for adoption"
    );
    let adoption = Adoption {
        agreement: agreement_ref.clone(),
        authorization_label,
        adopted_at_ms: now_ms(),
    };
    let mut files = BTreeMap::new();
    files.insert(
        "adoption.json".into(),
        orchestrate_contracts::encode(&adoption)?,
    );
    let reference = store.publish_bundle(
        effort,
        "agreement",
        ArtifactKind::Adoption,
        format!("adoption-{}", now_ms()),
        "ADOPTED".into(),
        vec![agreement_ref],
        provenance,
        files,
    )?;
    store.append_journal(
        effort,
        "agreement_adopted",
        None,
        serde_json::json!({"artifact":reference.artifact_id}),
    )?;
    Ok(reference)
}

#[allow(clippy::too_many_arguments)]
pub fn register_implementation(
    store: &Store,
    effort: &Effort,
    adoption_ref: ArtifactRef,
    repo: &Path,
    commit: &str,
    declaration: String,
    status: ImplementationStatus,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    ensure!(
        adoption_ref.kind == ArtifactKind::Adoption,
        "implementation needs an adoption receipt"
    );
    let (_, adoption): (_, Adoption) = store.load_json(effort, &adoption_ref, "adoption.json")?;
    let (_, agreement): (_, Agreement) =
        store.load_json(effort, &adoption.agreement, "agreement.json")?;
    let project = store.project_for(effort)?;
    ensure!(
        std::fs::canonicalize(repo)? == project.canonical_locator,
        "implementation repository does not match the effort project"
    );
    let ancestry = Command::new("git")
        .args([
            "merge-base",
            "--is-ancestor",
            &agreement.baseline_commit,
            commit,
        ])
        .current_dir(repo)
        .status()?;
    ensure!(
        ancestry.success(),
        "implementation target is not based on the adopted Agreement baseline"
    );
    let snapshot = store.materialize_snapshot(repo, commit)?;
    let implementation = Implementation {
        adoption: adoption_ref.clone(),
        agreement: adoption.agreement,
        starting_baseline: agreement.baseline_commit,
        target_commit: snapshot.commit,
        target_tree: snapshot.tree,
        producer_declaration: declaration,
        status,
        declared_checks: vec![],
        deviations: vec![],
        unresolved_questions: vec![],
    };
    let mut files = BTreeMap::new();
    files.insert(
        "implementation.json".into(),
        orchestrate_contracts::encode(&implementation)?,
    );
    let reference = store.publish_bundle(
        effort,
        "build",
        ArtifactKind::Implementation,
        format!("implementation-{}", now_ms()),
        "REGISTERED_EXTERNAL".into(),
        vec![adoption_ref],
        provenance,
        files,
    )?;
    store.append_journal(
        effort,
        "implementation_registered",
        None,
        serde_json::json!({"artifact":reference.artifact_id,"commit":implementation.target_commit}),
    )?;
    Ok(reference)
}

pub fn finalize_audit(
    store: &Store,
    effort: &Effort,
    assessment: AuditAssessment,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    ensure!(
        assessment.agreement.kind == ArtifactKind::Agreement
            && assessment.adoption.kind == ArtifactKind::Adoption
            && assessment.implementation.kind == ArtifactKind::Implementation,
        "audit parent kinds are invalid"
    );
    let (_, agreement): (_, Agreement) =
        store.load_json(effort, &assessment.agreement, "agreement.json")?;
    let (_, adoption): (_, Adoption) =
        store.load_json(effort, &assessment.adoption, "adoption.json")?;
    ensure!(
        adoption.agreement == assessment.agreement,
        "audit adoption is for another Agreement"
    );
    let (_, implementation): (_, Implementation) =
        store.load_json(effort, &assessment.implementation, "implementation.json")?;
    ensure!(
        implementation.adoption == assessment.adoption
            && implementation.agreement == assessment.agreement,
        "audit implementation is for another authority"
    );
    let verdict = derive_verdict(&agreement, &assessment, &implementation.status)?;
    let report = AuditReport {
        assessment,
        verdict: verdict.clone(),
    };
    let outcome = match verdict {
        Verdict::Pass => "PASS",
        Verdict::ChangesRequired => "CHANGES_REQUIRED",
        Verdict::Blocked => "BLOCKED",
    };
    let mut files = BTreeMap::new();
    files.insert("audit.json".into(), orchestrate_contracts::encode(&report)?);
    let reference = store.publish_bundle(
        effort,
        "audit",
        ArtifactKind::Audit,
        format!("audit-{}", now_ms()),
        outcome.into(),
        vec![
            report.assessment.agreement.clone(),
            report.assessment.adoption.clone(),
            report.assessment.implementation.clone(),
        ],
        provenance,
        files,
    )?;
    store.append_journal(
        effort,
        "audit_finalized",
        None,
        serde_json::json!({"artifact":reference.artifact_id,"outcome":outcome}),
    )?;
    Ok(reference)
}

#[allow(clippy::too_many_arguments)]
pub fn run_provider(
    store: &Store,
    effort: &Effort,
    agreement_ref: ArtifactRef,
    adoption_ref: ArtifactRef,
    implementation_ref: ArtifactRef,
    provider: &Path,
    guide: &str,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    let (_, agreement): (_, Agreement) =
        store.load_json(effort, &agreement_ref, "agreement.json")?;
    let (_, adoption): (_, Adoption) = store.load_json(effort, &adoption_ref, "adoption.json")?;
    let (_, implementation): (_, Implementation) =
        store.load_json(effort, &implementation_ref, "implementation.json")?;
    ensure!(
        adoption.agreement == agreement_ref
            && implementation.adoption == adoption_ref
            && implementation.agreement == agreement_ref,
        "audit authority chain is inconsistent"
    );
    let target_source = store
        .root()
        .join("snapshots")
        .join(&implementation.target_commit)
        .join("source");
    ensure!(
        target_source.exists(),
        "retained exact implementation source is missing"
    );
    store.append_journal(
        effort,
        "provider_started",
        None,
        serde_json::json!({"phase":"audit"}),
    )?;
    let packet = serde_json::json!({"phase":"audit","agreement_ref":agreement_ref,"adoption_ref":adoption_ref,"implementation_ref":implementation_ref,"agreement":agreement,"implementation":implementation,"target_source":target_source,"guide":guide});
    let response = invoke_provider_json::<_, AuditProposal>(provider, &packet, 1_048_576);
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
        serde_json::json!({"phase":"audit"}),
    )?;
    let agreement_ref: ArtifactRef = serde_json::from_value(packet["agreement_ref"].clone())?;
    let adoption_ref: ArtifactRef = serde_json::from_value(packet["adoption_ref"].clone())?;
    let implementation_ref: ArtifactRef =
        serde_json::from_value(packet["implementation_ref"].clone())?;
    finalize_audit(
        store,
        effort,
        AuditAssessment {
            agreement: agreement_ref,
            adoption: adoption_ref,
            implementation: implementation_ref,
            coverage: response.value.coverage,
            findings: response.value.findings,
            assessor_context: response.value.assessor_context,
        },
        provenance,
    )
}
