//! Explicit Reconciled Discovery adoption, external implementation registration, and Audit validation.

use anyhow::{Result, bail, ensure};
use orchestrate_contracts::{
    Adoption, ArtifactKind, ArtifactRef, AuditAssessment, AuditReport, Implementation,
    ImplementationStatus, Provenance, ReconciledDiscovery, Verdict, derive_verdict,
};
use orchestrate_core::{Effort, Store, now_ms};
use std::{collections::BTreeMap, process::Command};

pub fn adopt(
    store: &Store,
    effort: &Effort,
    reconciled_ref: ArtifactRef,
    authorization_label: String,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    adopt_with_run_id(
        store,
        effort,
        reconciled_ref,
        authorization_label,
        provenance,
        format!("adoption-{}", now_ms()),
    )
}

fn adopt_with_run_id(
    store: &Store,
    effort: &Effort,
    reconciled_ref: ArtifactRef,
    authorization_label: String,
    provenance: Provenance,
    run_id: String,
) -> Result<ArtifactRef> {
    ensure!(
        reconciled_ref.kind == ArtifactKind::ReconciledDiscovery,
        "only an implementation-ready Reconciled Discovery can be adopted"
    );
    let (envelope, _): (_, ReconciledDiscovery) =
        store.load_json(effort, &reconciled_ref, "reconciled-discovery.json")?;
    ensure!(
        envelope.outcome == "IMPLEMENTATION_READY",
        "Reconciled Discovery is not implementation-ready for adoption"
    );
    let adoption = Adoption {
        reconciled: reconciled_ref.clone(),
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
        "adoption",
        ArtifactKind::Adoption,
        run_id.clone(),
        "ADOPTED".into(),
        vec![reconciled_ref],
        provenance,
        files,
    )?;
    store.append_journal(
        effort,
        "reconciled_discovery_adopted",
        None,
        serde_json::json!({"artifact":reference.artifact_id}),
    )?;
    Ok(reference)
}

/// Create or recover the Adoption receipt authorized by an explicit Build invocation.
pub fn adopt_for_build(
    store: &Store,
    effort: &Effort,
    reconciled_ref: ArtifactRef,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    let mut existing = Vec::new();
    for reference in store.list_artifacts(effort)? {
        if reference.kind != ArtifactKind::Adoption {
            continue;
        }
        let (_, adoption): (_, Adoption) = store.load_json(effort, &reference, "adoption.json")?;
        if adoption.reconciled == reconciled_ref {
            existing.push(reference);
        }
    }
    match existing.as_slice() {
        [only] => return Ok(only.clone()),
        many if !many.is_empty() => bail!(
            "multiple Adoption receipts already exist for Reconciled Discovery {}: {}",
            reconciled_ref.artifact_id,
            artifact_ids(many)
        ),
        _ => {}
    }
    let run_id = format!("build-adoption-{}", &reconciled_ref.digest[..16]);
    adopt_with_run_id(
        store,
        effort,
        reconciled_ref,
        "explicit Build invocation".into(),
        provenance,
        run_id,
    )
}

/// Find the sole eligible Reconciled Discovery for this effort.
///
/// Inference never guesses: no candidate or more than one candidate stops the
/// operation and asks for an explicit `--reconciled`.
pub fn select_reconciled(store: &Store, effort: &Effort) -> Result<ArtifactRef> {
    let found = eligible_artifacts(store, effort, &eligible_reconciled)?;
    match found.as_slice() {
        [only] => Ok(only.clone()),
        [] => bail!(
            "no implementation-ready Reconciled Discovery exists for effort {}; finalize Reconcile first or pass --reconciled explicitly",
            effort.id
        ),
        many => bail!(
            "multiple eligible Reconciled Discovery artifacts exist: {}; pass --reconciled explicitly with the intended artifact",
            artifact_ids(many)
        ),
    }
}

/// Find the sole Adoption receipt for this effort.
///
/// Inference never guesses: no candidate or more than one candidate stops the
/// operation and asks for an explicit `--adoption`.
pub fn select_adoption(store: &Store, effort: &Effort) -> Result<ArtifactRef> {
    let found = eligible_artifacts(store, effort, &eligible_adoption)?;
    match found.as_slice() {
        [only] => Ok(only.clone()),
        [] => bail!(
            "no eligible Adoption artifact exists for effort {}; adopt a Reconciled Discovery first or pass --adoption explicitly",
            effort.id
        ),
        many => bail!(
            "multiple eligible Adoption artifacts exist: {}; pass --adoption explicitly with the intended artifact",
            artifact_ids(many)
        ),
    }
}

pub fn register_implementation(
    store: &Store,
    effort: &Effort,
    adoption_ref: ArtifactRef,
    commit: &str,
    declaration: String,
    status: ImplementationStatus,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    let project = store.project_for(effort)?;
    let build_start = store.materialize_snapshot(&project.canonical_locator, "HEAD")?;
    register_implementation_with_run_id(
        store,
        effort,
        adoption_ref,
        commit,
        declaration,
        status,
        provenance,
        format!("implementation-{}", now_ms()),
        build_start.commit,
        build_start.tree,
    )
}

/// Register an implementation using a caller-owned stable operation identity.
/// Build uses this to make a publish recoverable if the process dies after the
/// immutable bundle has been committed but before its controller state updates.
#[allow(clippy::too_many_arguments)]
pub fn register_implementation_with_run_id(
    store: &Store,
    effort: &Effort,
    adoption_ref: ArtifactRef,
    commit: &str,
    declaration: String,
    status: ImplementationStatus,
    provenance: Provenance,
    run_id: String,
    build_start_commit: String,
    build_start_tree: String,
) -> Result<ArtifactRef> {
    ensure!(
        adoption_ref.kind == ArtifactKind::Adoption,
        "implementation needs an adoption receipt"
    );
    let (_, adoption): (_, Adoption) = store.load_json(effort, &adoption_ref, "adoption.json")?;
    let (_, reconciled): (_, ReconciledDiscovery) =
        store.load_json(effort, &adoption.reconciled, "reconciled-discovery.json")?;
    let project = store.project_for(effort)?;
    let repo = project.canonical_locator.as_path();
    ensure!(
        std::fs::canonicalize(repo)? == project.canonical_locator,
        "the effort project is no longer at its canonical location {}",
        project.canonical_locator.display()
    );
    let ancestry = Command::new("git")
        .args([
            "merge-base",
            "--is-ancestor",
            &reconciled.baseline_commit,
            commit,
        ])
        .current_dir(repo)
        .status()?;
    ensure!(
        ancestry.success(),
        "implementation target is not based on the adopted Reconciled Discovery baseline"
    );
    let start_ancestry = Command::new("git")
        .args(["merge-base", "--is-ancestor", &build_start_commit, commit])
        .current_dir(repo)
        .status()?;
    ensure!(
        start_ancestry.success(),
        "implementation target is not based on the recorded Build starting commit"
    );
    let actual_start = store.materialize_snapshot(repo, &build_start_commit)?;
    ensure!(
        actual_start.tree == build_start_tree,
        "recorded Build starting tree does not match its commit"
    );
    let snapshot = store.materialize_snapshot(repo, commit)?;
    let implementation = Implementation {
        adoption: adoption_ref.clone(),
        reconciled: adoption.reconciled,
        discovery_baseline_commit: reconciled.baseline_commit,
        discovery_baseline_tree: reconciled.baseline_tree,
        build_start_commit: actual_start.commit,
        build_start_tree: actual_start.tree,
        target_commit: snapshot.commit,
        target_tree: snapshot.tree,
        producer_declaration: declaration,
        status,
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
        run_id,
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

/// Locate one already-published submitted implementation for the exact adopted
/// authority and commit.  More than one is an ambiguity rather than a recency
/// choice.
pub fn find_implementation(
    store: &Store,
    effort: &Effort,
    adoption: &ArtifactRef,
    commit: &str,
) -> Result<Option<ArtifactRef>> {
    let mut matches = Vec::new();
    for reference in store.list_artifacts(effort)? {
        if reference.kind != ArtifactKind::Implementation {
            continue;
        }
        let (_, implementation): (_, Implementation) =
            store.load_json(effort, &reference, "implementation.json")?;
        if implementation.adoption == *adoption
            && implementation.target_commit == commit
            && implementation.status == ImplementationStatus::Submitted
        {
            matches.push(reference);
        }
    }
    match matches.as_slice() {
        [] => Ok(None),
        [only] => Ok(Some(only.clone())),
        many => bail!(
            "multiple registered implementations match adoption {} and commit {}: {}",
            adoption.artifact_id,
            commit,
            artifact_ids(many)
        ),
    }
}

fn eligible_artifacts<F>(store: &Store, effort: &Effort, eligible: &F) -> Result<Vec<ArtifactRef>>
where
    F: Fn(&Store, &Effort, &ArtifactRef) -> Result<bool>,
{
    let mut found = Vec::new();
    for reference in store.list_artifacts(effort)? {
        if eligible(store, effort, &reference)? {
            found.push(reference);
        }
    }
    Ok(found)
}

fn eligible_reconciled(store: &Store, effort: &Effort, reference: &ArtifactRef) -> Result<bool> {
    if reference.kind != ArtifactKind::ReconciledDiscovery {
        return Ok(false);
    }
    let (envelope, reconciled): (_, ReconciledDiscovery) =
        store.load_json(effort, reference, "reconciled-discovery.json")?;
    Ok(envelope.outcome == "IMPLEMENTATION_READY"
        && envelope.effort_id == effort.id
        && reconciled.context_id == effort.context.id
        && reconciled.baseline_commit == effort.baseline_commit
        && reconciled.baseline_tree == effort.baseline_tree)
}

fn eligible_adoption(store: &Store, effort: &Effort, reference: &ArtifactRef) -> Result<bool> {
    if reference.kind != ArtifactKind::Adoption {
        return Ok(false);
    }
    let (envelope, adoption): (_, Adoption) =
        store.load_json(effort, reference, "adoption.json")?;
    if envelope.outcome != "ADOPTED" {
        return Ok(false);
    }
    eligible_reconciled(store, effort, &adoption.reconciled)
}

fn artifact_ids(references: &[ArtifactRef]) -> String {
    references
        .iter()
        .map(|reference| reference.artifact_id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

pub fn finalize_audit(
    store: &Store,
    effort: &Effort,
    assessment: AuditAssessment,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    finalize_audit_with_run_id(
        store,
        effort,
        assessment,
        provenance,
        format!("audit-{}", now_ms()),
    )
}

/// Publish an Audit result with a stable Build-owned identity.
pub fn finalize_audit_with_run_id(
    store: &Store,
    effort: &Effort,
    assessment: AuditAssessment,
    provenance: Provenance,
    run_id: String,
) -> Result<ArtifactRef> {
    ensure!(
        assessment.reconciled.kind == ArtifactKind::ReconciledDiscovery
            && assessment.adoption.kind == ArtifactKind::Adoption
            && assessment.implementation.kind == ArtifactKind::Implementation,
        "audit parent kinds are invalid"
    );
    let (_, reconciled): (_, ReconciledDiscovery) =
        store.load_json(effort, &assessment.reconciled, "reconciled-discovery.json")?;
    let (_, adoption): (_, Adoption) =
        store.load_json(effort, &assessment.adoption, "adoption.json")?;
    ensure!(
        adoption.reconciled == assessment.reconciled,
        "audit adoption is for another Reconciled Discovery"
    );
    let (_, implementation): (_, Implementation) =
        store.load_json(effort, &assessment.implementation, "implementation.json")?;
    ensure!(
        implementation.adoption == assessment.adoption
            && implementation.reconciled == assessment.reconciled,
        "audit implementation is for another authority"
    );
    let verdict = derive_verdict(&reconciled, &assessment, &implementation.status)?;
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
        run_id.clone(),
        outcome.into(),
        vec![
            report.assessment.reconciled.clone(),
            report.assessment.adoption.clone(),
            report.assessment.implementation.clone(),
        ],
        provenance,
        files,
    )?;
    // This exact attempt may already have been finalized before an interruption,
    // and its publication may already have been journaled.  Replay reuses both.
    let already_journaled = store.read_journal(effort)?.iter().any(|entry| {
        entry.event == "audit_finalized"
            && (entry.run_id.as_deref() == Some(run_id.as_str())
                || entry
                    .details
                    .get("artifact")
                    .and_then(|value| value.as_str())
                    == Some(reference.artifact_id.as_str()))
    });
    if !already_journaled {
        store.append_journal(
            effort,
            "audit_finalized",
            Some(&run_id),
            serde_json::json!({"artifact":reference.artifact_id,"outcome":outcome,"run_id":run_id}),
        )?;
    }
    Ok(reference)
}
