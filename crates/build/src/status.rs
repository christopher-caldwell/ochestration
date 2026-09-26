//! Read-only Build status.
//!
//! Everything here is derived from durable facts on disk.  It mutates no state,
//! creates no directory, and never claims that a quiet run, a heartbeat or a
//! successful process proves engineering progress or acceptance.

use std::{
    collections::HashSet,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result};
use orchestrate_contracts::{AuditReport, Verdict};
use orchestrate_core::{Effort, Store};
use serde_json::json;

use crate::{
    BuildPlan, BuildState, DispatchState, RESOLUTIONS_DIR, effective_config, load_plan, read_json,
    role_for,
};

/// The full read-only status projection for one effort's Build.
pub fn build_status(store: &Store, effort: &Effort) -> Result<serde_json::Value> {
    let build_dir = store.phase_dir(effort, "build")?;
    let state_path = build_dir.join("state.json");
    let lock = controller_lock(store, effort);
    if !state_path.is_file() {
        return Ok(json!({
            "effort": effort.id,
            "build_dir": build_dir,
            "has_state": false,
            "prepared": build_dir.join("plan.json").is_file() && build_dir.join("config.toml").is_file(),
            "controller": lock,
            "note": "this effort has no Build state yet; nothing has been dispatched",
        }));
    }
    let state: BuildState = read_json(&state_path).with_context(|| {
        format!(
            "cannot read Build state {}; it is controller-owned and is never repaired in place",
            state_path.display()
        )
    })?;
    let plan = load_plan(store, effort, &build_dir).ok();
    let config = effective_config(&build_dir);
    let (role, role_config, config_version) = match (&config, &plan) {
        (Ok(config), Some(_)) => {
            let (name, role) = role_for(&config.config, &state.action.kind);
            (
                serde_json::json!(name),
                // A once-over whose configuration an overlay removed has no
                // settings to report; that is reported as unknown, not guessed.
                role.map(role_json).unwrap_or(serde_json::Value::Null),
                serde_json::json!(config.version),
            )
        }
        _ => (serde_json::json!(null), json!(null), json!(null)),
    };
    let (accepted, total, tasks_in_accepted) = plan
        .as_ref()
        .map(|plan| accepted_phases(&state, plan))
        .unwrap_or((0, 0, 0));
    let activity = provider_activity(&build_dir, &state);
    let (audit_verdict, unresolved) = current_acceptance(store, effort, &state)?;
    let controller = lock;
    Ok(json!({
        "effort": effort.id,
        "build_dir": build_dir,
        "has_state": true,
        "delivery": {
            "phases_accepted": accepted,
            "phases_total": total,
            "tasks_in_accepted_phases": tasks_in_accepted,
            "note": "counts are tasks in accepted phases, never individually verified task completion",
        },
        "current": {
            "action_id": state.action.id,
            "kind": state.action.kind,
            "scope": state.action.scope,
            "role": role,
            "configured_role": role_config,
            "config_version": config_version,
            "dispatch": state.action.dispatch,
            "target_commit": state.action.target_commit,
            "accepted_at_ms": state.action.accepted_at_ms,
            "elapsed_ms": elapsed_ms(&state),
            "provider_activity": activity,
        },
        "acceptance": {
            "state": if state.terminal.is_some() { json!("terminal") } else if state.stop.is_some() { json!("stopped") } else { json!("running") },
            "current_audit": state.current_audit,
            "last_audit_verdict": audit_verdict,
            "unresolved_requirement_ids": unresolved,
            "note": "a completed assessment with unknown coverage derives BLOCKED; a process success is never PASS",
        },
        "stop": state.stop,
        "controller": controller,
        "advisory_once_over": state.once_over,
        "cleanup": state.cleanup,
        "recovery": {
            "continuation_used": state.recovery.continuation_used,
            "replacement_used": state.recovery.replacement_used,
            "unblock_used": state.recovery.unblock_used,
        },
        "resolutions": resolution_count(&build_dir)?,
        "terminal": state.terminal,
        "note": "read-only projection; it wrote nothing and dispatched nothing",
    }))
}

fn role_json(config: &crate::RoleConfig) -> serde_json::Value {
    json!({
        "adapter": config.adapter,
        // `model` is the native-name field retained by frozen schema v2.
        // Schema v3 requests a provider-neutral tier through model_strength;
        // neither field is a provider-observed effective model.
        "model": config.model,
        "model_strength": config.model_strength,
        "requested_model": {
            "model_strength": config.model_strength,
            "legacy_native_model": config.model,
        },
        "reasoning_effort": config.reasoning_effort,
        "permission": config.permission,
        "full_access": config.permission.as_deref() == Some("full_access"),
        "unset_attributes": "provider default; not a frozen effective value",
    })
}

/// Accepted phases are the phases whose review actually passed, so the count
/// never depends on a task merely being listed in the plan.
fn accepted_phases(state: &BuildState, plan: &BuildPlan) -> (usize, usize, usize) {
    let reviewed: HashSet<&str> = state
        .phase_reviews
        .iter()
        .map(|review| review.scope.as_str())
        .collect();
    let accepted = if state.phase_reviews.is_empty() {
        // State written before the review ledger existed still reports the
        // controller's own counter rather than claiming more than it knows.
        state.phase_index
    } else {
        plan.delivery_phases
            .iter()
            .filter(|phase| reviewed.contains(phase.id.as_str()))
            .count()
    };
    let tasks = plan
        .delivery_phases
        .iter()
        .filter(|phase| {
            reviewed.contains(phase.id.as_str())
                || (state.phase_reviews.is_empty()
                    && plan
                        .delivery_phases
                        .iter()
                        .position(|candidate| candidate.id == phase.id)
                        .is_some_and(|index| index < state.phase_index))
        })
        .map(|phase| phase.tasks.len())
        .sum();
    (accepted, plan.delivery_phases.len(), tasks)
}

/// How long the current invocation has been running, and how long ago the
/// provider last produced any output.  Prepared-but-never-accepted time is not
/// model time.
fn elapsed_ms(state: &BuildState) -> Option<u128> {
    let accepted = state.action.accepted_at_ms?;
    match state.action.dispatch {
        DispatchState::Prepared => None,
        _ => Some(now_ms().saturating_sub(accepted)),
    }
}

fn provider_activity(build_dir: &Path, state: &BuildState) -> serde_json::Value {
    let action_dir = build_dir.join("artifacts").join(&state.action.id);
    let mut newest: Option<(u128, PathBuf)> = None;
    for name in [
        "transport.jsonl",
        "provider-stderr.log",
        "transport.completed",
    ] {
        let path = action_dir.join(name);
        let Ok(metadata) = fs::metadata(&path) else {
            continue;
        };
        let Ok(modified) = metadata.modified() else {
            continue;
        };
        let millis = modified
            .duration_since(UNIX_EPOCH)
            .map_or(0, |duration| duration.as_millis());
        if newest.as_ref().is_none_or(|(seen, _)| millis > *seen) {
            newest = Some((millis, path));
        }
    }
    match newest {
        Some((millis, path)) => json!({
            "last_activity_path": path,
            "last_activity_ms": millis,
            "age_ms": now_ms().saturating_sub(millis),
            "note": "activity means the provider wrote transport output; it is not engineering progress",
        }),
        None => json!({
            "last_activity_path": null,
            "last_activity_ms": null,
            "age_ms": null,
            "note": "no provider activity has been observed for this action yet",
        }),
    }
}

/// The exact latest acceptance evidence, with its unresolved requirement IDs.
fn current_acceptance(
    store: &Store,
    effort: &Effort,
    state: &BuildState,
) -> Result<(serde_json::Value, Vec<String>)> {
    let Some(reference) = &state.current_audit else {
        return Ok((json!(null), Vec::new()));
    };
    let report: Result<(_, AuditReport)> = store.load_json(effort, reference, "audit.json");
    match report {
        Ok((_, report)) => {
            let unresolved = crate::unresolved_requirement_ids(
                store,
                effort,
                &state.frozen.reconciled,
                &report.assessment,
            )?;
            let verdict = match report.verdict {
                Verdict::Pass => "PASS",
                Verdict::ChangesRequired => "CHANGES_REQUIRED",
                Verdict::Blocked => "BLOCKED",
            };
            Ok((json!(verdict), unresolved))
        }
        Err(error) => Ok((json!(format!("unreadable: {error}")), Vec::new())),
    }
}

fn resolution_count(build_dir: &Path) -> Result<usize> {
    let dir = build_dir.join(RESOLUTIONS_DIR);
    if !dir.is_dir() {
        return Ok(0);
    }
    Ok(fs::read_dir(dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().and_then(|value| value.to_str()) == Some("json"))
        .count())
}

/// Controller liveness from current evidence, kept explicitly separate from the
/// question of whether a provider process is still running.
fn controller_lock(store: &Store, effort: &Effort) -> serde_json::Value {
    let Ok(project) = store.project_for(effort) else {
        return json!({"state": "unknown", "detail": "this effort's project could not be resolved"});
    };
    let path = store.project_dir(&project).join(".build-controller.lock");
    if !path.is_file() {
        return json!({
            "lock": path,
            "state": "not_held",
            "detail": "no Build controller lock is held for this checkout. That is not proof that no provider process is running: an interrupted or externally started provider may still be working, and Build state alone cannot tell.",
        });
    }
    let pid = fs::read_to_string(&path)
        .ok()
        .and_then(|value| value.trim().parse::<u32>().ok());
    match pid {
        Some(pid) if crate::process_alive(pid) => json!({
            "lock": path,
            "state": "held",
            "pid": pid,
            "detail": "a Build controller is running for this checkout; a quiet controller may be waiting on its provider",
        }),
        Some(pid) => json!({
            "lock": path,
            "state": "stale",
            "pid": pid,
            "detail": "the recorded controller process is not running. The provider may still be running or may have been interrupted; a later start reclaims this lock.",
        }),
        None => json!({
            "lock": path,
            "state": "unreadable",
            "detail": "the controller lock exists but its recorded controller identity is unreadable",
        }),
    }
}

/// A short terminal summary of the same facts, for the operator's terminal.
pub fn status_lines(store: &Store, effort: &Effort) -> Result<Vec<String>> {
    let value = build_status(store, effort)?;
    let mut lines = Vec::new();
    if !value["has_state"].as_bool().unwrap_or(false) {
        lines.push(format!(
            "Orchestrate Build — {}",
            value["effort"].as_str().unwrap_or("?")
        ));
        lines.push(if value["prepared"].as_bool().unwrap_or(false) {
            "no Build state yet: plan and configuration are prepared and nothing has been dispatched"
                .into()
        } else {
            "no Build state yet: this effort has no prepared plan and configuration".to_owned()
        });
        if let Some(detail) = value["controller"]["detail"].as_str() {
            lines.push(format!("controller: {detail}"));
        }
        return Ok(lines);
    }
    let delivery = &value["delivery"];
    lines.push(format!(
        "Orchestrate Build — {}",
        value["effort"].as_str().unwrap_or("?")
    ));
    lines.push(format!(
        "delivery: {} / {} phases accepted ({} tasks in accepted phases)",
        delivery["phases_accepted"], delivery["phases_total"], delivery["tasks_in_accepted_phases"]
    ));
    let current = &value["current"];
    if current["action_id"].is_string() {
        let role = current["role"].as_str().unwrap_or("?");
        let adapter = current["configured_role"]["adapter"]
            .as_str()
            .unwrap_or("?");
        let model = current["configured_role"]["model_strength"]
            .as_str()
            .map(|strength| format!("requested {strength} model strength"))
            .or_else(|| {
                current["configured_role"]["model"]
                    .as_str()
                    .map(|model| format!("requested native model {model}"))
            })
            .unwrap_or_else(|| "provider default".into());
        let dispatch = serde_json::to_string(&current["dispatch"])?;
        let elapsed = current["elapsed_ms"]
            .as_u64()
            .map(|value| format!("{}s elapsed", value / 1000))
            .unwrap_or_else(|| "not dispatched".into());
        lines.push(format!(
            "current: phase {} · {} ({role} · {adapter} · {model}) · {} · {elapsed}",
            current["scope"].as_str().unwrap_or("?"),
            current["kind"].as_str().unwrap_or("?"),
            dispatch.trim_matches('"'),
        ));
    }
    if let Some(age) = current["provider_activity"]["age_ms"].as_u64() {
        lines.push(format!(
            "last provider activity: {}s ago (activity is not completion)",
            age / 1000
        ));
    }
    let acceptance = &value["acceptance"];
    match acceptance["last_audit_verdict"].as_str() {
        Some(verdict) => lines.push(format!(
            "acceptance: {verdict} — unresolved: {}",
            acceptance["unresolved_requirement_ids"]
                .as_array()
                .map(|ids| ids
                    .iter()
                    .filter_map(|id| id.as_str())
                    .collect::<Vec<_>>()
                    .join(", "))
                .filter(|ids| !ids.is_empty())
                .unwrap_or_else(|| "none".into())
        )),
        None => lines.push("acceptance: no formal Audit has been published yet".into()),
    }
    if let Some(stop) = value["stop"].as_object() {
        lines.push(format!(
            "stopped: {} — {}",
            stop["trigger"].as_str().unwrap_or("?"),
            stop["detail"].as_str().unwrap_or("")
        ));
        if let Some(trigger) = &value["stop"]["action_failure"].as_str() {
            lines.push(format!("stopped action: {trigger}"));
        }
    }
    if let Some(detail) = value["controller"]["detail"].as_str() {
        lines.push(format!("controller: {detail}"));
    }
    Ok(lines)
}

fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}
