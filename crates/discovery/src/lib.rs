//! Discovery publication boundary. Semantic investigation is supplied by an operator or provider;
//! this crate validates and finalizes one attributable opinion for one cohort slot.

use anyhow::{Result, bail, ensure};
use orchestrate_contracts::{
    ArtifactKind, ArtifactRef, Independence, Opinion, Provenance, validate_opinion,
};
use orchestrate_core::{Effort, Store, invoke_provider_json};

pub fn prepare(store: &Store, effort: &Effort, slot: &str) -> Result<(String, std::path::PathBuf)> {
    ensure!(matches!(slot, "a" | "b" | "c"), "slot must be a, b, or c");
    let run_id = format!("discovery-{}", unique_suffix());
    let workspace = store.discovery_workspace(effort, &run_id)?;
    Ok((run_id, workspace))
}

pub fn finalize(
    store: &Store,
    effort: &Effort,
    mut opinion: Opinion,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    ensure!(
        opinion.context_id == effort.context.id,
        "opinion has a different request/context revision"
    );
    ensure!(
        opinion.baseline_commit == effort.cohort.baseline_commit,
        "opinion has a different source baseline"
    );
    if matches!(provenance.independence, Independence::Compromised) {
        bail!("known-compromised opinion cannot finalize as an eligible independent slot");
    }
    validate_opinion(&opinion)?;
    let slot = std::mem::take(&mut opinion.slot);
    opinion.slot = slot.clone();
    let run_id = format!("discovery-{}-{}", slot, unique_suffix());
    store.publish(
        effort,
        "discovery",
        ArtifactKind::DiscoveryOpinion,
        run_id,
        "FINALIZED".to_owned(),
        Vec::new(),
        provenance,
        &opinion,
    )
}

/// Runs a configured one-shot provider against an allowlisted Discovery packet, then finalizes it.
pub fn run_provider(
    store: &Store,
    effort: &Effort,
    slot: &str,
    provider: &std::path::Path,
    provenance: Provenance,
) -> Result<ArtifactRef> {
    let (run_id, workspace) = prepare(store, effort, slot)?;
    let packet = serde_json::json!({"phase":"discovery", "run_id":run_id, "slot":slot, "request":effort.context.request, "constraints":effort.context.constraints, "context_id":effort.context.id, "baseline_commit":effort.cohort.baseline_commit, "frozen_source":workspace});
    let response = invoke_provider_json::<_, Opinion>(provider, &packet, 1_048_576)?;
    finalize(store, effort, response.value, provenance)
}

fn unique_suffix() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos())
        .to_string()
}
