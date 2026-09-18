//! External implementation registration and independent Audit contract validation.

use anyhow::{Result, ensure};
use orchestrate_contracts::{
    Adoption, Agreement, ArtifactKind, ArtifactRef, AuditAssessment, AuditReport, Coverage,
    Finding, Implementation, Provenance, artifact_ref, derive_verdict,
};
use orchestrate_core::{Effort, Store, invoke_provider_json};
use serde::{Deserialize, Serialize};
use std::process::Command;

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
    authorized_by: String,
    scope: String,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    ensure!(
        agreement_ref.kind == ArtifactKind::Agreement,
        "only an Agreement can be adopted"
    );
    ensure!(
        store
            .lineage_invalidation(effort, &agreement_ref)?
            .is_none(),
        "Agreement authority has an invalidated lineage"
    );
    let (envelope, _agreement): (_, Agreement) = store.load_artifact(effort, &agreement_ref)?;
    ensure!(
        envelope.outcome == "ELIGIBLE_CANDIDATE",
        "Agreement is not eligible for adoption"
    );
    let adoption = Adoption {
        agreement: agreement_ref.clone(),
        authorized_by,
        execution_scope: scope,
    };
    store.publish(
        effort,
        "agreement",
        ArtifactKind::Adoption,
        format!("adoption-{}", suffix()),
        "ADOPTED".to_owned(),
        vec![agreement_ref],
        provenance,
        &adoption,
    )
}

#[allow(clippy::too_many_arguments)] // This is the explicit external Build handoff contract.
pub fn register_implementation(
    store: &Store,
    effort: &Effort,
    adoption_ref: ArtifactRef,
    repo: &std::path::Path,
    commit: &str,
    declaration: String,
    status: String,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    ensure!(
        adoption_ref.kind == ArtifactKind::Adoption,
        "implementation needs an adoption receipt"
    );
    ensure!(
        store.lineage_invalidation(effort, &adoption_ref)?.is_none(),
        "adoption authority has an invalidated lineage"
    );
    let (_adoption_envelope, adoption): (_, Adoption) =
        store.load_artifact(effort, &adoption_ref)?;
    let (_, agreement): (_, Agreement) = store.load_artifact(effort, &adoption.agreement)?;
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
        declared_checks: Vec::new(),
        deviations: Vec::new(),
        unresolved_questions: Vec::new(),
    };
    store.publish(
        effort,
        "build",
        ArtifactKind::Implementation,
        format!("implementation-{}", suffix()),
        "REGISTERED_EXTERNAL".to_owned(),
        vec![adoption_ref],
        provenance,
        &implementation,
    )
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
    for parent in [
        &assessment.agreement,
        &assessment.adoption,
        &assessment.implementation,
    ] {
        ensure!(
            store.lineage_invalidation(effort, parent)?.is_none(),
            "audit authority has an invalidated lineage"
        );
    }
    let (_, agreement): (_, Agreement) = store.load_artifact(effort, &assessment.agreement)?;
    let (_, adoption): (_, Adoption) = store.load_artifact(effort, &assessment.adoption)?;
    ensure!(
        adoption.agreement == assessment.agreement,
        "audit adoption is for another Agreement"
    );
    let (_, implementation): (_, Implementation) =
        store.load_artifact(effort, &assessment.implementation)?;
    ensure!(
        implementation.adoption == assessment.adoption
            && implementation.agreement == assessment.agreement,
        "audit implementation is for another authority"
    );
    let verdict = derive_verdict(&agreement, &assessment)?;
    let report = AuditReport {
        assessment,
        verdict: verdict.clone(),
    };
    let outcome = match verdict {
        orchestrate_contracts::Verdict::Pass => "PASS",
        orchestrate_contracts::Verdict::ChangesRequired => "CHANGES_REQUIRED",
        orchestrate_contracts::Verdict::Blocked => "BLOCKED",
    };
    store.publish(
        effort,
        "audit",
        ArtifactKind::Audit,
        format!("audit-{}", suffix()),
        outcome.to_owned(),
        vec![
            report.assessment.agreement.clone(),
            report.assessment.adoption.clone(),
            report.assessment.implementation.clone(),
        ],
        provenance,
        &report,
    )
}

/// Runs a fresh assessor over immutable public authority and implementation contracts only.
pub fn run_provider(
    store: &Store,
    effort: &Effort,
    agreement_ref: ArtifactRef,
    adoption_ref: ArtifactRef,
    implementation_ref: ArtifactRef,
    provider: &std::path::Path,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    for parent in [&agreement_ref, &adoption_ref, &implementation_ref] {
        ensure!(
            store.lineage_invalidation(effort, parent)?.is_none(),
            "audit authority has an invalidated lineage"
        );
    }
    let (_, agreement): (_, Agreement) = store.load_artifact(effort, &agreement_ref)?;
    let (_, adoption): (_, Adoption) = store.load_artifact(effort, &adoption_ref)?;
    let (_, implementation): (_, Implementation) =
        store.load_artifact(effort, &implementation_ref)?;
    ensure!(
        adoption.agreement == agreement_ref,
        "adoption is for another Agreement"
    );
    ensure!(
        implementation.adoption == adoption_ref && implementation.agreement == agreement_ref,
        "implementation is for another authority"
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
    let packet = serde_json::json!({"phase":"audit", "agreement_ref":agreement_ref, "adoption_ref":adoption_ref, "implementation_ref":implementation_ref, "agreement":agreement, "implementation":implementation, "target_source":target_source});
    let response = invoke_provider_json::<_, AuditProposal>(provider, &packet, 1_048_576)?;
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

pub fn ref_from_manifest(envelope: &orchestrate_contracts::Envelope, bytes: &[u8]) -> ArtifactRef {
    artifact_ref(envelope, bytes)
}
fn suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
        .to_string()
}
