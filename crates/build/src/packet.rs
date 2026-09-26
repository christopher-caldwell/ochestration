use anyhow::{Context, Result, ensure};
use orchestrate_contracts::{Adoption, ReconciledDiscovery};
use orchestrate_core::{Effort, Store, write_bytes_sync};
use serde_json::json;
use std::path::Path;

use crate::state::{
    BuildPlan, BuildState, Gate, Scope, current_action_dir, load_phase_documents, read_build_file,
    validate_feedback_path,
};

pub struct ActionPacket {
    pub action_dir: std::path::PathBuf,
    pub prompt: String,
}

pub fn create_action_packet(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    plan: &BuildPlan,
    state: &BuildState,
    action_id: &str,
) -> Result<ActionPacket> {
    let action_dir = current_action_dir(build_dir, action_id)?;
    let source_checkout = (state.gate != Gate::Work).then(|| action_dir.join("checkout"));
    let (_, reconciled): (_, ReconciledDiscovery) =
        store.load_json(effort, &state.reconciled, "reconciled-discovery.json")?;
    let (_, adoption): (_, Adoption) = store.load_json(effort, &state.adoption, "adoption.json")?;
    ensure!(
        adoption.reconciled == state.reconciled,
        "Build Adoption does not match the state Reconciled Discovery"
    );
    let phase = match &state.scope {
        Scope::Phase { index } => {
            let id = plan
                .phases
                .get(*index)
                .context("Build scope refers to a missing phase")?;
            Some(
                json!({"index": index, "id": id, "documents": load_phase_documents(build_dir, id)?}),
            )
        }
        Scope::Final => None,
    };
    let mut feedback = Vec::new();
    for item in &state.feedback {
        validate_feedback_path(&item.path)?;
        let content = String::from_utf8(read_build_file(build_dir, &item.path)?)
            .with_context(|| format!("feedback {} is not UTF-8", item.path))?;
        feedback.push(json!({"purpose": item.purpose, "path": item.path, "report": content}));
    }
    let guide = match state.gate {
        Gate::Work => orchestrate_guides::WORK,
        Gate::Review => orchestrate_guides::REVIEW,
        Gate::Audit => orchestrate_guides::FINAL_AUDIT,
        Gate::Unblock => orchestrate_guides::UNBLOCK,
    };
    let packet = json!({
        "schema_version": 2,
        "action_id": action_id,
        "gate": state.gate,
        "scope": state.scope,
        "checkpoint_commit": state.checkpoint_commit,
        "build_start_commit": state.build_start_commit,
        "reconciled": state.reconciled,
        "binding_reconciled": reconciled,
        "adoption": state.adoption,
        "phase": phase,
        "feedback": feedback,
        "unblock_context": state.unblock,
        "source_checkout": source_checkout,
        "implementation": state.implementation,
        "audit_contract": "Return an assessment object with exact reconciled, adoption, and implementation ArtifactRefs; coverage must address every binding requirement."
    });
    write_bytes_sync(
        &action_dir.join("action.json"),
        &orchestrate_contracts::encode(&packet)?,
    )?;
    let packet_text = serde_json::to_string_pretty(&packet)?;
    let output_shape = match state.gate {
        Gate::Work => format!(
            r#"{{"action_id":"{action_id}","outcome":"complete|blocked","report":"non-empty","commit":"required only when complete"}}"#
        ),
        Gate::Review => format!(
            r#"{{"action_id":"{action_id}","outcome":"pass|changes_required|blocked","report":"non-empty","inspected_commit":"required"}}"#
        ),
        Gate::Audit => format!(
            r#"{{"action_id":"{action_id}","outcome":"complete|blocked","report":"non-empty","assessment":"required only when complete"}}"#
        ),
        Gate::Unblock => format!(
            r#"{{"action_id":"{action_id}","outcome":"retry|blocked","report":"non-empty"}}"#
        ),
    };
    let prompt = format!(
        "You are the {role} role in Orchestrate Build.\n\n{guide}\n\nThe authoritative action packet follows. Do not modify or create controller-owned evidence files. Perform only the assigned gate. Return exactly one JSON object and no surrounding prose or code fence. Required shape: {shape}\n\nGate: {gate:?}; action_id: {action_id}. The response action_id must match exactly. The report must be concise but complete enough to guide the next role.\n\nACTION PACKET:\n{packet_text}\n",
        role = match state.gate {
            Gate::Work => "Worker",
            Gate::Review => "Reviewer",
            Gate::Audit => "Audit",
            Gate::Unblock => "Unblock",
        },
        guide = guide.trim(),
        shape = output_shape,
        gate = state.gate,
        packet_text = packet_text,
    );
    write_bytes_sync(&action_dir.join("prompt.md"), prompt.as_bytes())?;
    Ok(ActionPacket { action_dir, prompt })
}
