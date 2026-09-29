use anyhow::{Context, Result, bail, ensure};
use orchestrate_audit::{
    adopt_for_build, finalize_audit_with_run_id, find_implementation,
    register_implementation_with_run_id,
};
use orchestrate_contracts::{
    ArtifactKind, AuditAssessment, AuditReport, ImplementationStatus, Independence, Provenance,
    ReconciledDiscovery, Verdict, digest_bytes, encode,
};
use orchestrate_core::{Effort, Store, now_ms, write_bytes_sync};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use crate::{
    adapter::{self, RuntimeSessions, Session},
    observation::{ActionOutcome, BuildEvent, BuildObservation, BuildObserver, FinishedAction},
    packet::create_action_packet,
    state::{
        BuildCompletion, BuildConfig, BuildPlan, BuildState, FeedbackRef, Gate, RoleConfig,
        STATE_VERSION, Scope, Status, Stop, StopKind, UnblockContext, current_action_dir,
        load_config, load_plan, load_state, read_build_file, save_state, validate_feedback_path,
    },
};

const STATE_FILE: &str = "state.json";
const PLAN_FILE: &str = "plan.json";
const CONFIG_FILE: &str = "config.toml";
static ACTION_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Debug)]
pub struct BuildRequest {
    pub effort: Option<String>,
    pub project: PathBuf,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BuildResult {
    Completed(BuildCompletion),
    Blocked { detail: String, state: PathBuf },
}

pub fn scaffold(store: &Store, effort_id: &str) -> Result<PathBuf> {
    let effort = store.load_effort(effort_id)?;
    let build_dir = store.phase_dir(&effort, "build")?;
    let plan = build_dir.join(PLAN_FILE);
    if !plan.exists() {
        let phase = build_dir.join("phase_01_foundation");
        fs::create_dir_all(&phase)?;
        if !phase.join("phase.md").exists() {
            write_bytes_sync(
                &phase.join("phase.md"),
                r#"# Foundation

A phase is a substantial, coherent implementation slice that can be built, meaningfully verified, and reviewed efficiently as one unit. Optimize its boundaries for efficient Work → Review cycles and useful accepted checkpoints. Include the tests or verification naturally associated with the slice.

Describe the whole phase: purpose, expected outcome, boundaries, dependencies, implementation guidance, and deliberate exclusions. Add immediate Markdown task/context files as useful.
"#
                .as_bytes(),
            )?;
        }
        write_bytes_sync(&plan, orchestrate_guides::templates::PLAN_JSON.as_bytes())?;
    }
    let config = build_dir.join(CONFIG_FILE);
    if !config.exists() {
        write_bytes_sync(
            &config,
            orchestrate_guides::templates::CONFIG_TOML.as_bytes(),
        )?;
    }

    Ok(build_dir)
}

pub fn status(store: &Store, effort_id: &str) -> Result<serde_json::Value> {
    let effort = store.load_effort(effort_id)?;
    let build_dir = store.effort_dir(&effort).join("build");
    let path = build_dir.join(STATE_FILE);
    if !path.exists() {
        return Ok(json!({"effort": effort.id, "status": "uninitialized", "build_dir": build_dir}));
    }
    Ok(serde_json::to_value(load_state(&path)?)?)
}

pub fn reset(store: &Store, effort_id: &str) -> Result<serde_json::Value> {
    let effort = store.load_effort(effort_id)?;
    let project = store.project_for(&effort)?;
    let build_dir = store.phase_dir(&effort, "build")?;
    let state_path = build_dir.join(STATE_FILE);
    let mut state = load_state(&state_path)?;
    ensure!(
        state.status == Status::Running
            || (state.status == Status::Stopped
                && state
                    .stop
                    .as_ref()
                    .is_some_and(|stop| stop.kind == StopKind::ResetRequired)),
        "build reset requires a running or reset_required state; relaunch Build for a blocked stop"
    );
    validate_commit(&project.canonical_locator, &state.checkpoint_commit)?;
    remove_current_worktree(
        &project.canonical_locator,
        &build_dir,
        state.current_action_id.as_deref(),
    )?;
    reset_checkout(&project.canonical_locator, &state.checkpoint_commit)?;
    state.current_action_id = None;
    state.stop = None;
    state.status = Status::Ready;
    save_state(&state_path, &state)?;
    eprintln!(
        "RESET checkpoint={} gate={:?}",
        state.checkpoint_commit, state.gate
    );
    Ok(serde_json::to_value(state)?)
}

fn prepare_continuation(
    repo: &Path,
    build_dir: &Path,
    state_path: &Path,
    state: &mut BuildState,
) -> Result<()> {
    ensure!(
        state.status == Status::Stopped
            && state
                .stop
                .as_ref()
                .is_some_and(|stop| stop.kind == StopKind::Blocked),
        "Build continuation requires a stopped blocked state"
    );
    let context = state
        .unblock
        .as_ref()
        .context("blocked stop lacks its originating gate context")?
        .clone();
    for feedback in &state.feedback {
        validate_feedback_path(&feedback.path)?;
        read_build_file(build_dir, &feedback.path).context("blocked stop lacks its feedback")?;
    }
    ensure_clean(repo).context(
        "operator repair is not a clean reviewable candidate; commit the intended repository changes or remove unintended files (nothing was discarded)",
    )?;
    let head = git_text(repo, &["rev-parse", "HEAD"])?;
    if head == state.checkpoint_commit {
        state.gate = context.gate.clone();
    } else {
        ensure!(
            git_is_ancestor(repo, &state.checkpoint_commit, &head)?,
            "operator repair HEAD is not descended from the saved checkpoint; nothing was discarded"
        );
        state.checkpoint_commit = head;
        state.implementation = None;
        state.gate = match context.scope {
            Scope::Phase { .. } => Gate::Review,
            Scope::Final => Gate::Audit,
        };
    }
    state.scope = context.scope.clone();
    state.unblock = None;
    state.current_action_id = None;
    state.stop = None;
    state.status = Status::Ready;
    save_state(state_path, state)?;
    Ok(())
}

pub fn run(store: &Store, request: BuildRequest) -> Result<BuildResult> {
    run_with_observer(store, request, &mut |_: BuildObservation<'_>| {})
}

/// Run Build with a synchronous read-only observer. The CLI owns presentation.
pub fn run_with_observer(
    store: &Store,
    request: BuildRequest,
    observer: &mut dyn BuildObserver,
) -> Result<BuildResult> {
    run_with_invoker_and_observer(store, request, &adapter::ProcessInvocationApi, observer)
}

#[cfg(test)]
fn run_with_invoker(
    store: &Store,
    request: BuildRequest,
    invoker: &dyn adapter::InvocationApi,
) -> Result<BuildResult> {
    run_with_invoker_and_observer(store, request, invoker, &mut |_: BuildObservation<'_>| {})
}

/// Deterministic invocation seam shared by the driver and observer integration tests.
pub fn run_with_invoker_and_observer(
    store: &Store,
    request: BuildRequest,
    invoker: &dyn adapter::InvocationApi,
    observer: &mut dyn BuildObserver,
) -> Result<BuildResult> {
    let effort = select_effort(store, &request)?;
    let project = store.project_for(&effort)?;
    ensure!(
        fs::canonicalize(&request.project).ok().as_ref() == Some(&project.canonical_locator),
        "current repository does not match selected effort"
    );
    let build_dir = store.phase_dir(&effort, "build")?;
    let state_path = build_dir.join(STATE_FILE);
    let fresh = !state_path.exists();
    if fresh {
        initialize(store, &effort, &project.canonical_locator, &build_dir)?;
    }
    let mut state = load_state(&state_path)?;
    let snapshot_plan = observation_plan(&build_dir, &state);
    observer.observe(BuildObservation {
        event: if fresh {
            BuildEvent::Initialized
        } else {
            BuildEvent::Snapshot
        },
        state: &state,
        plan: snapshot_plan.as_ref(),
    });
    let mut sessions = RuntimeSessions::default();
    let mut inputs = None;
    match state.status {
        Status::Complete => {
            observer.observe(BuildObservation {
                event: BuildEvent::Terminal,
                state: &state,
                plan: snapshot_plan.as_ref(),
            });
            return Ok(BuildResult::Completed(
                state
                    .completion
                    .context("complete Build lacks completion artifacts")?,
            ));
        }
        Status::Stopped => {
            if state
                .stop
                .as_ref()
                .is_some_and(|stop| stop.kind == StopKind::Blocked)
            {
                inputs = Some(load_execution_inputs(&build_dir, &state)?);
                prepare_continuation(
                    &project.canonical_locator,
                    &build_dir,
                    &state_path,
                    &mut state,
                )?;
                observer.observe(BuildObservation {
                    event: BuildEvent::Continued,
                    state: &state,
                    plan: inputs.as_ref().map(|inputs| &inputs.plan),
                });
            } else {
                observer.observe(BuildObservation {
                    event: BuildEvent::Terminal,
                    state: &state,
                    plan: snapshot_plan.as_ref(),
                });
                return Ok(BuildResult::Blocked {
                    detail: state
                        .stop
                        .as_ref()
                        .map_or_else(|| "Build is stopped".into(), |stop| stop.detail.clone()),
                    state: state_path,
                });
            }
        }
        Status::Running => {
            state.status = Status::Stopped;
            state.stop = Some(Stop { kind: StopKind::ResetRequired, detail: "Build restarted while an action was marked running; provider completion is uncertain. Run `orchestrate build reset --effort …` before retrying.".into() });
            save_state(&state_path, &state)?;
            observer.observe(BuildObservation {
                event: BuildEvent::Terminal,
                state: &state,
                plan: snapshot_plan.as_ref(),
            });
            return Ok(BuildResult::Blocked {
                detail: state.stop.as_ref().unwrap().detail.clone(),
                state: state_path,
            });
        }
        Status::Ready => {}
    }

    let inputs = match inputs {
        Some(inputs) => inputs,
        None => load_execution_inputs(&build_dir, &state)?,
    };

    loop {
        if state.status == Status::Complete {
            observer.observe(BuildObservation {
                event: BuildEvent::Terminal,
                state: &state,
                plan: Some(&inputs.plan),
            });
            return Ok(BuildResult::Completed(
                state
                    .completion
                    .clone()
                    .context("complete Build lacks completion artifacts")?,
            ));
        }
        if state.status == Status::Stopped {
            observer.observe(BuildObservation {
                event: BuildEvent::Terminal,
                state: &state,
                plan: Some(&inputs.plan),
            });
            return Ok(BuildResult::Blocked {
                detail: state
                    .stop
                    .as_ref()
                    .map_or_else(|| "Build is stopped".into(), |stop| stop.detail.clone()),
                state: state_path,
            });
        }
        let operation = execute_gate(
            store,
            &effort,
            &project.canonical_locator,
            &build_dir,
            &mut state,
            &mut sessions,
            &inputs,
            invoker,
            observer,
        );
        if let Err(error) = operation {
            state.status = Status::Stopped;
            state.stop = Some(Stop {
                kind: StopKind::ResetRequired,
                detail: format!(
                    "{}; reset to checkpoint {} before retrying",
                    error, state.checkpoint_commit
                ),
            });
            save_state(&state_path, &state)?;
            observer.observe(BuildObservation {
                event: BuildEvent::Terminal,
                state: &state,
                plan: Some(&inputs.plan),
            });
            return Ok(BuildResult::Blocked {
                detail: state.stop.as_ref().unwrap().detail.clone(),
                state: state_path,
            });
        }
    }
}

// Presentation may be unavailable, but must not reject a previously valid
// complete/stopped launch or display a changed plan as the accepted plan.
fn observation_plan(build_dir: &Path, state: &BuildState) -> Option<BuildPlan> {
    let path = build_dir.join(PLAN_FILE);
    let plan = load_plan(&path).ok()?;
    if digest_bytes(&fs::read(path).ok()?) != state.plan_digest
        || plan.reconciled != state.reconciled
        || matches!(state.scope, Scope::Phase { index } if index >= plan.phases.len())
    {
        return None;
    }
    Some(plan)
}

struct ExecutionInputs {
    plan: BuildPlan,
    config: BuildConfig,
}

fn load_execution_inputs(build_dir: &Path, state: &BuildState) -> Result<ExecutionInputs> {
    let plan_path = build_dir.join(PLAN_FILE);
    let plan = load_plan(&plan_path)?;
    ensure!(
        digest_bytes(&fs::read(&plan_path)?) == state.plan_digest
            && plan.reconciled == state.reconciled,
        "Build machine plan changed after initialization; restore the accepted plan.json or start a new Build according to current authority"
    );
    let config = load_config(&build_dir.join(CONFIG_FILE))?;
    Ok(ExecutionInputs { plan, config })
}

fn select_effort(store: &Store, request: &BuildRequest) -> Result<Effort> {
    if let Some(id) = &request.effort {
        return store.load_effort(id);
    }
    let locator = fs::canonicalize(&request.project)?;
    let project = store.project_by_locator(&locator)?.context(
        "current repository is not registered in the Orchestrate store; initialize it first",
    )?;
    match store.efforts_for_project(&project)?.as_slice() {
        [only] => Ok(only.clone()),
        [] => bail!("no effort is registered for the current project; initialize one first"),
        many => bail!(
            "multiple efforts exist for this project; pass --effort explicitly ({} candidates)",
            many.len()
        ),
    }
}

fn initialize(store: &Store, effort: &Effort, repo: &Path, build_dir: &Path) -> Result<()> {
    ensure_clean(repo)?;
    let head = git_text(repo, &["rev-parse", "HEAD"])?;
    let plan_path = build_dir.join(PLAN_FILE);
    let plan = load_plan(&plan_path)?;
    let plan_digest = digest_bytes(&fs::read(&plan_path)?);
    let (reconciled_envelope, reconciled): (_, ReconciledDiscovery) =
        store.load_json(effort, &plan.reconciled, "reconciled-discovery.json")?;
    ensure!(
        plan.reconciled.kind == ArtifactKind::ReconciledDiscovery
            && reconciled_envelope.outcome == "IMPLEMENTATION_READY",
        "plan must name an implementation-ready Reconciled Discovery"
    );
    let ancestor = Command::new("git")
        .args([
            "merge-base",
            "--is-ancestor",
            &reconciled.baseline_commit,
            &head,
        ])
        .current_dir(repo)
        .status()?;
    ensure!(
        ancestor.success(),
        "Discovery baseline {} is not an ancestor of HEAD",
        reconciled.baseline_commit
    );
    let adoption = adopt_for_build(
        store,
        effort,
        plan.reconciled.clone(),
        provenance("build-adoption", None, None),
    )?;
    let state = BuildState {
        schema_version: STATE_VERSION,
        reconciled: plan.reconciled,
        adoption,
        build_start_commit: head.clone(),
        plan_digest,
        scope: Scope::Phase { index: 0 },
        gate: Gate::Work,
        status: Status::Ready,
        checkpoint_commit: head,
        feedback: Vec::new(),
        unblock: None,
        current_action_id: None,
        implementation: None,
        completion: None,
        stop: None,
    };
    save_state(&build_dir.join(STATE_FILE), &state)?;
    Ok(())
}

fn execute_gate(
    store: &Store,
    effort: &Effort,
    repo: &Path,
    build_dir: &Path,
    state: &mut BuildState,
    sessions: &mut RuntimeSessions,
    inputs: &ExecutionInputs,
    invoker: &dyn adapter::InvocationApi,
    observer: &mut dyn BuildObserver,
) -> Result<()> {
    if state.gate == Gate::Audit {
        ensure_audit_implementation(store, effort, repo, state)?;
    }
    ensure_repo_at_checkpoint(repo, &state.checkpoint_commit)?;
    let action_id = action_id();
    let gate = state.gate.clone();
    let scope = state.scope.clone();
    let action_dir = current_action_dir(build_dir, &action_id)?;
    ensure_action_root(build_dir)?;
    fs::create_dir(&action_dir)?;
    let mut checkout = None;
    let cwd = if matches!(gate, Gate::Review | Gate::Audit | Gate::Unblock) {
        let path = action_dir.join("checkout");
        git(
            repo,
            &[
                "worktree",
                "add",
                "--detach",
                path.to_str().context("worktree path is not UTF-8")?,
                &state.checkpoint_commit,
            ],
        )?;
        checkout = Some(path.clone());
        path
    } else {
        repo.to_path_buf()
    };
    let packet = create_action_packet(store, effort, build_dir, &inputs.plan, state, &action_id)?;
    let role = role_for_gate(&inputs.config, &gate);
    let session = match gate {
        Gate::Work => sessions.worker.as_ref(),
        Gate::Review | Gate::Audit | Gate::Unblock => None,
    };
    let invocation = adapter::prepare_invocation(role, &packet.prompt, &cwd, session)?;
    write_bytes_sync(
        &packet.action_dir.join("invocation.json"),
        &encode(&invocation.record)?,
    )?;
    state.status = Status::Running;
    state.current_action_id = Some(action_id.clone());
    state.stop = None;
    save_state(&build_dir.join(STATE_FILE), state)?;
    observer.observe(BuildObservation {
        event: BuildEvent::ActionStarted,
        state,
        plan: Some(&inputs.plan),
    });

    let process = match invoker.invoke(&invocation, &cwd, &packet.action_dir) {
        Ok(outcome) => outcome,
        Err(error) => {
            if let Some(path) = &checkout {
                let _ = remove_worktree(repo, path);
            }
            return Err(error);
        }
    };
    ensure!(
        process.success,
        "provider exited unsuccessfully (status {:?}); stderr is recorded at actions/{action_id}/stderr.txt",
        process.exit_code
    );
    if gate != Gate::Work {
        ensure_repo_at_checkpoint(repo, &state.checkpoint_commit)?;
    }
    let response = process
        .final_response
        .as_deref()
        .context("provider returned no final assistant response")?;
    let feedback_ref = FeedbackRef {
        path: format!("actions/{action_id}/report.md"),
        purpose: gate_label(&gate).into(),
    };
    let outcome;
    match gate {
        Gate::Work => {
            let result: WorkResult = parse_result(response, &action_id)?;
            ensure!(
                matches!(result.outcome.as_str(), "complete" | "blocked"),
                "invalid Work outcome {:?}",
                result.outcome
            );
            ensure!(
                result.outcome != "blocked" || result.commit.is_none(),
                "Work blocked result must not include a commit"
            );
            persist_result(&packet.action_dir, response, &result.report, None)?;
            update_worker_session(sessions, role, process.observed_session);
            if result.outcome == "blocked" {
                outcome = ActionOutcome::Blocked;
                reset_checkout(repo, &state.checkpoint_commit)?;
                state.feedback.push(feedback_ref);
                route_blocked(state, Gate::Work, &result.report);
            } else {
                outcome = ActionOutcome::Complete;
                let commit = result.commit.context("Work complete result lacks commit")?;
                validate_work_commit(repo, &state.checkpoint_commit, &commit)?;
                state.checkpoint_commit = commit;
                state.implementation = None;
                state.feedback.push(feedback_ref);
                if matches!(state.scope, Scope::Final) {
                    state.gate = Gate::Audit;
                } else {
                    state.gate = Gate::Review;
                }
                state.unblock = None;
                state.status = Status::Ready;
            }
        }
        Gate::Review => {
            let result: ReviewResult = parse_result(response, &action_id)?;
            ensure!(
                matches!(
                    result.outcome.as_str(),
                    "pass" | "changes_required" | "blocked"
                ),
                "invalid Review outcome {:?}",
                result.outcome
            );
            persist_result(&packet.action_dir, response, &result.report, None)?;
            verify_disposable_review(
                checkout.as_deref().context("Review worktree missing")?,
                &state.checkpoint_commit,
            )?;
            remove_worktree(
                repo,
                checkout.as_deref().context("Review worktree missing")?,
            )?;
            ensure!(
                result.inspected_commit == state.checkpoint_commit,
                "Review inspected commit does not match the exact checkpoint"
            );
            match result.outcome.as_str() {
                "pass" => {
                    outcome = ActionOutcome::Pass;
                    sessions.worker = None;
                    state.feedback.clear();
                    state.unblock = None;
                    match state.scope {
                        Scope::Phase { index } if index + 1 < inputs.plan.phases.len() => {
                            state.scope = Scope::Phase { index: index + 1 };
                            state.gate = Gate::Work;
                        }
                        Scope::Phase { .. } => {
                            state.scope = Scope::Final;
                            state.gate = Gate::Audit;
                        }
                        Scope::Final => bail!("Review cannot pass in final Audit scope"),
                    }
                    state.status = Status::Ready;
                }
                "changes_required" => {
                    outcome = ActionOutcome::ChangesRequired;
                    state.feedback = vec![feedback_ref];
                    state.gate = Gate::Work;
                    state.unblock = None;
                    state.status = Status::Ready;
                }
                "blocked" => {
                    outcome = ActionOutcome::Blocked;
                    state.feedback.push(feedback_ref);
                    route_blocked(state, Gate::Review, &result.report);
                }
                _ => unreachable!(),
            }
        }
        Gate::Audit => {
            let result: AuditResult = parse_result(response, &action_id)?;
            ensure!(
                matches!(result.outcome.as_str(), "complete" | "blocked"),
                "invalid Audit outcome {:?}",
                result.outcome
            );
            ensure!(
                result.outcome != "blocked" || result.assessment.is_none(),
                "Audit blocked result must not include an assessment"
            );
            persist_result(
                &packet.action_dir,
                response,
                &result.report,
                result.assessment.as_ref(),
            )?;
            verify_disposable_review(
                checkout.as_deref().context("Audit worktree missing")?,
                &state.checkpoint_commit,
            )?;
            remove_worktree(repo, checkout.as_deref().context("Audit worktree missing")?)?;
            match result.outcome.as_str() {
                "blocked" => {
                    outcome = ActionOutcome::Blocked;
                    state.feedback.push(feedback_ref);
                    route_blocked(state, Gate::Audit, &result.report);
                }
                "complete" => {
                    let assessment = result
                        .assessment
                        .context("Audit complete result lacks assessment")?;
                    ensure!(
                        assessment.reconciled == state.reconciled
                            && assessment.adoption == state.adoption,
                        "Audit assessment names another Reconciled Discovery or Adoption"
                    );
                    let implementation = state
                        .implementation
                        .clone()
                        .context("Audit lacks registered implementation")?;
                    ensure!(
                        assessment.implementation == implementation,
                        "Audit assessment names another implementation artifact"
                    );
                    let audit = finalize_audit_with_run_id(
                        store,
                        effort,
                        assessment.clone(),
                        provenance(
                            role.adapter.as_str(),
                            role.model.as_deref(),
                            role.effort.as_deref(),
                        ),
                        format!("build-audit-{action_id}"),
                    )?;
                    let (_, report): (_, AuditReport) =
                        store.load_json(effort, &audit, "audit.json")?;
                    match report.verdict {
                        Verdict::Pass => {
                            outcome = ActionOutcome::Pass;
                            state.completion = Some(BuildCompletion {
                                implementation,
                                audit,
                            });
                            state.unblock = None;
                            state.status = Status::Complete;
                            state.stop = None;
                            state.feedback.clear();
                        }
                        Verdict::ChangesRequired => {
                            outcome = ActionOutcome::ChangesRequired;
                            state.feedback = vec![
                                feedback_ref,
                                FeedbackRef {
                                    path: format!("actions/{action_id}/assessment.json"),
                                    purpose: "Audit coverage assessment and required correction"
                                        .into(),
                                },
                            ];
                            state.scope = Scope::Final;
                            state.gate = Gate::Work;
                            state.unblock = None;
                            state.status = Status::Ready;
                        }
                        Verdict::Blocked => {
                            outcome = ActionOutcome::Blocked;
                            state.feedback.push(feedback_ref);
                            state.feedback.push(FeedbackRef {
                                path: format!("actions/{action_id}/assessment.json"),
                                purpose: "Audit unknown or incomplete requirement coverage".into(),
                            });
                            route_blocked(
                                state,
                                Gate::Audit,
                                "Audit has unknown or incomplete requirement coverage",
                            );
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
        Gate::Unblock => {
            let result: UnblockResult = parse_result(response, &action_id)?;
            ensure!(
                matches!(result.outcome.as_str(), "retry" | "blocked"),
                "invalid Unblock outcome {:?}",
                result.outcome
            );
            persist_result(&packet.action_dir, response, &result.report, None)?;
            verify_disposable_review(
                checkout.as_deref().context("Unblock worktree missing")?,
                &state.checkpoint_commit,
            )?;
            remove_worktree(
                repo,
                checkout.as_deref().context("Unblock worktree missing")?,
            )?;
            ensure!(
                state.unblock.is_some(),
                "Unblock has no originating gate context"
            );
            state.feedback.push(feedback_ref);
            match result.outcome.as_str() {
                "retry" => {
                    outcome = ActionOutcome::Retry;
                    let context = state.unblock.as_ref().unwrap().clone();
                    reset_checkout(repo, &state.checkpoint_commit)?;
                    state.scope = context.scope;
                    state.gate = context.gate;
                    state.status = Status::Ready;
                }
                "blocked" => {
                    outcome = ActionOutcome::Blocked;
                    state.status = Status::Stopped;
                    state.stop = Some(Stop {
                        kind: StopKind::Blocked,
                        detail: result.report.clone(),
                    });
                }
                _ => unreachable!(),
            }
        }
    }
    if state.status != Status::Stopped {
        state.current_action_id = None;
    }
    save_state(&build_dir.join(STATE_FILE), state)?;
    observer.observe(BuildObservation {
        event: BuildEvent::ActionRouted(FinishedAction {
            action_id,
            gate,
            scope,
            outcome,
        }),
        state,
        plan: Some(&inputs.plan),
    });
    Ok(())
}

fn role_for_gate<'a>(config: &'a BuildConfig, gate: &Gate) -> &'a RoleConfig {
    match gate {
        Gate::Work => &config.worker,
        Gate::Review | Gate::Audit => &config.reviewer,
        Gate::Unblock => config.unblocker.as_ref().unwrap_or(&config.reviewer),
    }
}

fn update_worker_session(
    sessions: &mut RuntimeSessions,
    role: &RoleConfig,
    observed: Option<String>,
) {
    sessions.worker = observed
        .filter(|id| !id.trim().is_empty())
        .map(|id| Session {
            adapter: role.adapter.clone(),
            id,
        });
}

fn route_blocked(state: &mut BuildState, gate: Gate, report: &str) {
    if state.unblock.is_some() {
        state.status = Status::Stopped;
        state.stop = Some(Stop {
            kind: StopKind::Blocked,
            detail: report.to_owned(),
        });
    } else {
        state.unblock = Some(UnblockContext {
            gate,
            scope: state.scope.clone(),
        });
        state.gate = Gate::Unblock;
        state.status = Status::Ready;
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct WorkResult {
    action_id: String,
    outcome: String,
    report: String,
    #[serde(default)]
    commit: Option<String>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct ReviewResult {
    action_id: String,
    outcome: String,
    report: String,
    inspected_commit: String,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct AuditResult {
    action_id: String,
    outcome: String,
    report: String,
    #[serde(default)]
    assessment: Option<AuditAssessment>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct UnblockResult {
    action_id: String,
    outcome: String,
    report: String,
}

fn parse_result<T: for<'de> Deserialize<'de> + Serialize>(
    response: &str,
    action_id: &str,
) -> Result<T> {
    let value: serde_json::Value = serde_json::from_str(response)
        .context("final assistant response must be exactly one JSON object")?;
    ensure!(
        value.is_object(),
        "final assistant response must be one JSON object"
    );
    let result: T = serde_json::from_value(value)
        .context("final assistant response does not match the gate schema")?;
    let encoded = serde_json::to_value(&result)?;
    ensure!(
        encoded.get("action_id").and_then(serde_json::Value::as_str) == Some(action_id),
        "final assistant response action_id does not match current action"
    );
    ensure!(
        encoded
            .get("report")
            .and_then(serde_json::Value::as_str)
            .is_some_and(|report| !report.trim().is_empty()),
        "final assistant response report must be non-empty"
    );
    let outcome = encoded
        .get("outcome")
        .and_then(serde_json::Value::as_str)
        .context("gate response has no outcome")?;
    ensure!(
        matches!(
            outcome,
            "complete" | "blocked" | "pass" | "changes_required" | "retry"
        ),
        "invalid gate outcome {outcome:?}"
    );
    Ok(result)
}

fn persist_result(
    action_dir: &Path,
    result_json: &str,
    report: &str,
    assessment: Option<&AuditAssessment>,
) -> Result<()> {
    let value: serde_json::Value = serde_json::from_str(result_json)?;
    write_bytes_sync(&action_dir.join("result.json"), &encode(&value)?)?;
    write_bytes_sync(&action_dir.join("report.md"), report.trim().as_bytes())?;
    if let Some(assessment) = assessment {
        write_bytes_sync(&action_dir.join("assessment.json"), &encode(assessment)?)?;
    }
    Ok(())
}

fn ensure_audit_implementation(
    store: &Store,
    effort: &Effort,
    repo: &Path,
    state: &mut BuildState,
) -> Result<()> {
    ensure_repo_at_checkpoint(repo, &state.checkpoint_commit)?;
    let reference =
        match find_implementation(store, effort, &state.adoption, &state.checkpoint_commit)? {
            Some(reference) => reference,
            None => {
                let snapshot = store.materialize_snapshot(repo, &state.build_start_commit)?;
                register_implementation_with_run_id(
                    store,
                    effort,
                    state.adoption.clone(),
                    &state.checkpoint_commit,
                    "Completed by the Build deterministic gate runner".into(),
                    ImplementationStatus::Submitted,
                    provenance("build", None, None),
                    format!(
                        "build-implementation-{}",
                        &state.checkpoint_commit[..state.checkpoint_commit.len().min(12)]
                    ),
                    state.build_start_commit.clone(),
                    snapshot.tree,
                )?
            }
        };
    state.implementation = Some(reference);
    Ok(())
}

fn provenance(provider: &str, model: Option<&str>, model_effort: Option<&str>) -> Provenance {
    let guide = orchestrate_guides::FINAL_AUDIT;
    Provenance {
        host: "orchestrate-build".into(),
        provider: Some(provider.into()),
        model: model.map(str::to_owned),
        model_effort: model_effort.map(str::to_owned),
        guide_digest: digest_bytes(guide.as_bytes()),
        independence: Independence::Unknown,
    }
}

fn validate_work_commit(repo: &Path, checkpoint: &str, commit: &str) -> Result<()> {
    validate_commit(repo, commit)?;
    ensure!(
        git_is_ancestor(repo, checkpoint, commit)?,
        "submitted Work commit is not descendant-or-equal to prior checkpoint"
    );
    let head = git_text(repo, &["rev-parse", "HEAD"])?;
    ensure!(
        head == commit,
        "submitted Work commit does not equal product HEAD"
    );
    ensure_clean(repo)?;
    Ok(())
}

fn git_is_ancestor(repo: &Path, ancestor: &str, descendant: &str) -> Result<bool> {
    let status = Command::new("git")
        .args(["merge-base", "--is-ancestor", ancestor, descendant])
        .current_dir(repo)
        .status()?;
    Ok(status.success())
}

fn verify_disposable_review(worktree: &Path, checkpoint: &str) -> Result<()> {
    ensure!(
        git_text(worktree, &["rev-parse", "HEAD"])? == checkpoint,
        "disposable Reviewer checkout changed HEAD"
    );
    ensure_clean(worktree)?;
    Ok(())
}

fn ensure_repo_at_checkpoint(repo: &Path, checkpoint: &str) -> Result<()> {
    ensure!(
        git_text(repo, &["rev-parse", "HEAD"])? == checkpoint,
        "product HEAD differs from saved checkpoint; run build reset"
    );
    ensure_clean(repo)
}

fn ensure_clean(repo: &Path) -> Result<()> {
    let output = Command::new("git")
        .args(["status", "--porcelain", "--untracked-files=normal"])
        .current_dir(repo)
        .output()?;
    ensure!(output.status.success(), "git status failed");
    let changes = String::from_utf8_lossy(&output.stdout);
    ensure!(
        changes.trim().is_empty(),
        "checkout is not clean: {}",
        changes.trim()
    );
    Ok(())
}

fn validate_commit(repo: &Path, commit: &str) -> Result<()> {
    ensure!(!commit.trim().is_empty(), "checkpoint commit is empty");
    let output = Command::new("git")
        .args(["cat-file", "-e", &format!("{commit}^{{commit}}")])
        .current_dir(repo)
        .output()?;
    ensure!(
        output.status.success(),
        "checkpoint commit {commit} is missing or invalid"
    );
    Ok(())
}

fn reset_checkout(repo: &Path, checkpoint: &str) -> Result<()> {
    validate_commit(repo, checkpoint)?;
    git(repo, &["reset", "--hard", checkpoint])?;
    git(repo, &["clean", "-fd"])?;
    ensure!(
        git_text(repo, &["rev-parse", "HEAD"])? == checkpoint,
        "Git reset did not restore checkpoint"
    );
    ensure_clean(repo)?;
    Ok(())
}

fn remove_current_worktree(repo: &Path, build_dir: &Path, action_id: Option<&str>) -> Result<()> {
    let Some(action_id) = action_id else {
        return Ok(());
    };
    let action_dir = current_action_dir(build_dir, action_id)?;
    let build_root = fs::canonicalize(build_dir)?;
    if !action_dir.exists() {
        return Ok(());
    }
    let action_root = fs::canonicalize(&action_dir)?;
    ensure!(
        action_root.starts_with(&build_root),
        "current action directory escapes the Build directory"
    );
    let checkout = action_dir.join("checkout");
    if checkout.exists() {
        let checkout_root = fs::canonicalize(&checkout)?;
        ensure!(
            checkout_root.starts_with(&action_root),
            "current disposable worktree escapes its action directory"
        );
        remove_worktree(repo, &checkout)?;
    }
    Ok(())
}

fn remove_worktree(repo: &Path, path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    git(
        repo,
        &[
            "worktree",
            "remove",
            "--force",
            path.to_str().context("worktree path is not UTF-8")?,
        ],
    )
}

fn ensure_action_root(build_dir: &Path) -> Result<()> {
    let build_root = fs::canonicalize(build_dir)?;
    let actions = build_dir.join("actions");
    if !actions.exists() {
        fs::create_dir(&actions)?;
    }
    let metadata = fs::symlink_metadata(&actions)?;
    ensure!(
        metadata.is_dir() && !metadata.file_type().is_symlink(),
        "Build actions path must be a real directory"
    );
    ensure!(
        fs::canonicalize(&actions)?.starts_with(&build_root),
        "Build actions path escapes the Build directory"
    );
    Ok(())
}

fn git(repo: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .with_context(|| format!("cannot run git {}", args.join(" ")))?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

fn git_text(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .with_context(|| format!("cannot run git {}", args.join(" ")))?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn action_id() -> String {
    format!(
        "a-{}-{}-{}",
        now_ms(),
        std::process::id(),
        ACTION_COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}
fn gate_label(gate: &Gate) -> &'static str {
    match gate {
        Gate::Work => "Worker report",
        Gate::Review => "Review report",
        Gate::Audit => "Audit report",
        Gate::Unblock => "Unblock report",
    }
}

#[cfg(test)]
#[path = "../test_support/mod.rs"]
mod test_support;

#[cfg(test)]
mod tests {
    use super::test_support::{
        FakeInvoker, Fixture, Step, git_ok, make_fixture, one_phase, packet_from_record, request,
    };
    use super::*;
    use crate::adapter::InvocationOutcome;
    use std::sync::Mutex;

    #[test]
    fn gate_results_require_the_exact_action_and_report() {
        let good = r#"{"action_id":"a-1","outcome":"complete","report":"done","commit":"abc"}"#;
        let parsed: WorkResult = parse_result(good, "a-1").unwrap();
        assert_eq!(parsed.outcome, "complete");
        assert!(parse_result::<WorkResult>(good, "a-2").is_err());
        assert!(
            parse_result::<WorkResult>(
                r#"{"action_id":"a-1","outcome":"complete","report":" "}"#,
                "a-1"
            )
            .is_err()
        );
        assert!(
            parse_result::<WorkResult>(
                r#"{"action_id":"a-1","outcome":"complete","report":"done","extra":1}"#,
                "a-1"
            )
            .is_err()
        );
    }

    #[test]
    fn initialization_requires_a_clean_checkout() {
        let fixture = make_fixture(one_phase());
        fs::write(fixture.repo.join("untracked.txt"), "dirty\n").unwrap();
        let fake = FakeInvoker::new([], &fixture.repo);
        let error = run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap_err();
        assert!(error.to_string().contains("checkout is not clean"));
        assert!(fake.records().is_empty());
        assert!(!fixture.build_dir.join(STATE_FILE).exists());
    }

    #[test]
    fn initialization_routes_two_phases_with_fresh_evaluators_and_worker_phases() {
        let phases = vec!["phase_01".into(), "phase_02".into()];
        let fixture = make_fixture(phases);
        let fake = FakeInvoker::new(
            [
                Step::WorkComplete,
                Step::ReviewPass,
                Step::WorkComplete,
                Step::ReviewPass,
                Step::AuditPass,
            ],
            &fixture.repo,
        );
        let result = run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap();
        assert!(matches!(result, BuildResult::Completed(_)));
        assert_eq!(fake.remaining(), 0);
        let records = fake.records();
        assert_eq!(
            records
                .iter()
                .map(|record| record
                    .argv
                    .last()
                    .unwrap()
                    .split("Gate: ")
                    .nth(1)
                    .unwrap()
                    .split(';')
                    .next()
                    .unwrap())
                .collect::<Vec<_>>(),
            vec!["Work", "Review", "Work", "Review", "Audit"]
        );
        assert_eq!(records[2].session_in, None);
        assert!(records[1].session_in.is_none());
        assert!(records[3].session_in.is_none());
        assert!(records[4].session_in.is_none());
        let (_, authority): (_, ReconciledDiscovery) = fixture
            .store
            .load_json(
                &fixture.effort,
                &fixture.reconciled,
                "reconciled-discovery.json",
            )
            .unwrap();
        for (index, record) in records.iter().enumerate() {
            let packet = packet_from_record(record);
            assert_eq!(packet["schema_version"], 2);
            assert_eq!(
                packet["binding_reconciled"],
                serde_json::to_value(&authority).unwrap()
            );
            assert_eq!(
                packet["binding_reconciled"]["requirements"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            assert!(packet.get("detailed_plan").is_none());
            if index < 4 {
                let phase = if index < 2 { "phase_01" } else { "phase_02" };
                assert_eq!(packet["phase"]["id"], phase);
                assert_eq!(packet["phase"]["index"], index / 2);
                let documents = packet["phase"]["documents"].as_array().unwrap();
                assert_eq!(documents.len(), 3);
                for (document, name) in
                    documents
                        .iter()
                        .zip(["phase.md", "a-notes.md", "z-context.md"])
                {
                    assert_eq!(document["path"], format!("{phase}/{name}"));
                    assert_eq!(
                        document["content"],
                        fs::read_to_string(fixture.build_dir.join(phase).join(name)).unwrap()
                    );
                }
                assert!(packet["phase"].get("tasks").is_none());
                assert!(packet["phase"].get("requirement_ids").is_none());
            } else {
                assert!(packet["phase"].is_null());
            }
        }
        assert!(
            packet_from_record(&records[2])["feedback"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        let state = load_state(&fixture.build_dir.join(STATE_FILE)).unwrap();
        assert_eq!(state.status, Status::Complete);
        assert_eq!(
            state.checkpoint_commit,
            git_text(&fixture.repo, &["rev-parse", "HEAD"]).unwrap()
        );
        let completion = state.completion.unwrap();
        let (_, audit): (_, AuditReport) = fixture
            .store
            .load_json(&fixture.effort, &completion.audit, "audit.json")
            .unwrap();
        assert_eq!(audit.assessment.reconciled, fixture.reconciled);
        assert_eq!(audit.assessment.implementation, completion.implementation);
        assert_eq!(audit.verdict, Verdict::Pass);
    }

    #[test]
    fn invocation_records_and_provider_audit_provenance_keep_neutral_effort() {
        let fixture = make_fixture(one_phase());
        let mut config = load_config(&fixture.build_dir.join(CONFIG_FILE)).unwrap();
        config.worker.effort = Some("high".into());
        config.worker.args = Some(vec![
            "--search".into(),
            "-c".into(),
            "model_reasoning_effort=\"low\"".into(),
        ]);
        config.reviewer = RoleConfig {
            adapter: "claude".into(),
            model: None,
            effort: Some("ultracode".into()),
            args: Some(vec!["--effort".into(), "low".into()]),
        };
        fs::write(
            fixture.build_dir.join(CONFIG_FILE),
            toml::to_string(&config).unwrap(),
        )
        .unwrap();

        let fake = FakeInvoker::new(
            [Step::WorkComplete, Step::ReviewPass, Step::AuditPass],
            &fixture.repo,
        );
        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap(),
            BuildResult::Completed(_)
        ));
        let records = fake.records();
        assert_eq!(records.len(), 3);
        assert_eq!(records[0].effort.as_deref(), Some("high"));
        assert_eq!(records[1].effort.as_deref(), Some("ultracode"));
        assert_eq!(records[2].effort.as_deref(), Some("ultracode"));
        assert!(
            records[0]
                .argv
                .windows(2)
                .any(|pair| pair == ["-c", "model_reasoning_effort=\"high\""])
        );
        assert!(
            records[0]
                .argv
                .windows(2)
                .any(|pair| pair == ["-c", "model_reasoning_effort=\"low\""])
        );
        assert!(
            records[1]
                .argv
                .windows(2)
                .any(|pair| pair == ["--effort", "ultracode"])
        );
        assert!(
            records[1]
                .argv
                .windows(2)
                .any(|pair| pair == ["--effort", "low"])
        );

        for record in &records {
            let packet = packet_from_record(record);
            let action_id = packet["action_id"].as_str().unwrap();
            let invocation_path = fixture
                .build_dir
                .join("actions")
                .join(action_id)
                .join("invocation.json");
            let persisted: serde_json::Value =
                serde_json::from_slice(&fs::read(invocation_path).unwrap()).unwrap();
            assert_eq!(persisted["effort"], record.effort.as_deref().unwrap());
            assert_eq!(
                persisted["argv"],
                serde_json::to_value(&record.argv).unwrap()
            );
        }

        let state = load_state(&fixture.build_dir.join(STATE_FILE)).unwrap();
        let completion = state.completion.unwrap();
        let adoption = fixture
            .store
            .load_envelope(&fixture.effort, &state.adoption)
            .unwrap();
        assert_eq!(adoption.provenance.model_effort, None);
        let implementation = fixture
            .store
            .load_envelope(&fixture.effort, &completion.implementation)
            .unwrap();
        assert_eq!(implementation.provenance.model_effort, None);
        let audit = fixture
            .store
            .load_envelope(&fixture.effort, &completion.audit)
            .unwrap();
        assert_eq!(audit.provenance.model_effort.as_deref(), Some("ultracode"));
    }

    #[test]
    fn worker_and_evaluator_sessions_follow_phase_and_final_scope_boundaries() {
        let phases = vec!["phase_01".into(), "phase_02".into()];
        let fixture = make_fixture(phases);
        let mut config = load_config(&fixture.build_dir.join("config.toml")).unwrap();
        config.reviewer = RoleConfig {
            adapter: "claude".into(),
            model: Some("reviewer-model".into()),
            effort: None,
            args: Some(vec!["--permission-mode".into(), "plan".into()]),
        };
        fs::write(
            fixture.build_dir.join("config.toml"),
            toml::to_string(&config).unwrap(),
        )
        .unwrap();

        let mut fake = FakeInvoker::new(
            [
                Step::WorkComplete,
                Step::ReviewChanges,
                Step::WorkComplete,
                Step::ReviewChanges,
                Step::WorkComplete,
                Step::ReviewPass,
                Step::WorkComplete,
                Step::ReviewPass,
                Step::AuditFail,
                Step::WorkComplete,
                Step::AuditFail,
                Step::WorkComplete,
                Step::AuditPass,
            ],
            &fixture.repo,
        );
        fake.set_observed_sessions([
            Some("phase-one-work-1".into()),
            Some("review-output-1".into()),
            Some("phase-one-work-2".into()),
            Some("review-output-2".into()),
            Some("phase-one-work-3".into()),
            Some("review-output-3".into()),
            Some("phase-two-work-1".into()),
            Some("review-output-4".into()),
            Some("audit-output-1".into()),
            Some("final-work-1".into()),
            Some("audit-output-2".into()),
            Some("final-work-2".into()),
            Some("audit-output-3".into()),
        ]);

        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap(),
            BuildResult::Completed(_)
        ));
        let records = fake.records();
        let gates = records
            .iter()
            .map(|record| {
                packet_from_record(record)["gate"]
                    .as_str()
                    .unwrap()
                    .to_owned()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            gates,
            [
                "work", "review", "work", "review", "work", "review", "work", "review", "audit",
                "work", "audit", "work", "audit"
            ]
        );

        assert!(records[0].session_in.is_none());
        assert_eq!(records[2].session_in.as_deref(), Some("phase-one-work-1"));
        assert_eq!(records[4].session_in.as_deref(), Some("phase-one-work-2"));
        assert!(records[6].session_in.is_none(), "next phase starts fresh");
        assert!(records[9].session_in.is_none(), "final scope starts fresh");
        assert_eq!(records[11].session_in.as_deref(), Some("final-work-1"));
        for index in [1, 3, 5, 7, 8, 10, 12] {
            let record = &records[index];
            assert!(record.session_in.is_none(), "evaluator {index} is fresh");
            assert_eq!(record.adapter, "claude");
            assert_eq!(record.model.as_deref(), Some("reviewer-model"));
            assert_eq!(record.args, ["--permission-mode", "plan"]);
            assert!(
                record
                    .argv
                    .windows(2)
                    .any(|pair| pair == ["--model", "reviewer-model"])
            );
            assert!(
                record
                    .argv
                    .windows(2)
                    .any(|pair| pair == ["--permission-mode", "plan"])
            );
            assert!(!record.argv.iter().any(|arg| arg == "--resume"));
        }
    }

    #[test]
    fn missing_and_blank_work_session_ids_clear_existing_worker_handles() {
        let fixture = make_fixture(one_phase());
        let mut fake = FakeInvoker::new(
            [
                Step::WorkComplete,
                Step::ReviewChanges,
                Step::WorkComplete,
                Step::ReviewChanges,
                Step::WorkComplete,
                Step::ReviewChanges,
                Step::WorkComplete,
                Step::ReviewChanges,
                Step::WorkComplete,
                Step::ReviewPass,
                Step::AuditPass,
            ],
            &fixture.repo,
        );
        fake.set_observed_sessions([
            Some("initial-work".into()),
            Some("review-output-1".into()),
            None,
            Some("review-output-2".into()),
            Some("recovered-work".into()),
            Some("review-output-3".into()),
            Some("  \t ".into()),
            Some("review-output-4".into()),
            Some("after-blank-work".into()),
            Some("review-output-5".into()),
            Some("audit-output".into()),
        ]);

        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap(),
            BuildResult::Completed(_)
        ));
        let records = fake.records();
        assert_eq!(records[2].session_in.as_deref(), Some("initial-work"));
        assert!(
            records[4].session_in.is_none(),
            "missing ID clears prior handle"
        );
        assert_eq!(records[6].session_in.as_deref(), Some("recovered-work"));
        assert!(
            records[8].session_in.is_none(),
            "blank ID clears prior handle"
        );
        for index in [1, 3, 5, 7, 9, 10] {
            assert!(records[index].session_in.is_none());
        }
    }

    #[test]
    fn review_and_audit_corrections_return_to_the_authorized_scope() {
        let fixture = make_fixture(one_phase());
        let fake = FakeInvoker::new(
            [
                Step::WorkComplete,
                Step::ReviewChanges,
                Step::WorkComplete,
                Step::ReviewPass,
                Step::AuditFail,
                Step::WorkComplete,
                Step::AuditPass,
            ],
            &fixture.repo,
        );
        let result = run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap();
        assert!(matches!(result, BuildResult::Completed(_)));
        let records = fake.records();
        let gates = records
            .iter()
            .map(|record| {
                record
                    .argv
                    .last()
                    .unwrap()
                    .split("Gate: ")
                    .nth(1)
                    .unwrap()
                    .split(';')
                    .next()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            gates,
            ["Work", "Review", "Work", "Review", "Audit", "Work", "Audit"]
        );
        let corrected_review = packet_from_record(&records[3]);
        assert_eq!(corrected_review["feedback"].as_array().unwrap().len(), 2);
        assert_eq!(corrected_review["feedback"][0]["purpose"], "Review report");
        assert_eq!(corrected_review["feedback"][1]["purpose"], "Worker report");
        let corrected_audit = packet_from_record(&records[6]);
        assert_eq!(corrected_audit["feedback"].as_array().unwrap().len(), 3);
        assert_eq!(corrected_audit["feedback"][0]["purpose"], "Audit report");
        assert_eq!(corrected_audit["feedback"][2]["purpose"], "Worker report");
        let state = load_state(&fixture.build_dir.join(STATE_FILE)).unwrap();
        assert_eq!(state.scope, Scope::Final);
        assert_eq!(state.status, Status::Complete);
    }

    #[test]
    fn one_unblock_retry_restores_checkpoint_and_repeated_block_stops() {
        let fixture = make_fixture(one_phase());
        let fake = FakeInvoker::new(
            [
                Step::WorkBlocked,
                Step::UnblockRetry,
                Step::WorkComplete,
                Step::ReviewPass,
                Step::AuditPass,
            ],
            &fixture.repo,
        );
        let result = run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap();
        assert!(matches!(result, BuildResult::Completed(_)));
        assert!(!fixture.repo.join("partial-untracked.txt").exists());
        assert_eq!(fake.remaining(), 0);
        let unblock = packet_from_record(&fake.records()[1]);
        assert_eq!(unblock["unblock_context"]["gate"], "work");
        assert_eq!(unblock["unblock_context"]["scope"]["kind"], "phase");
        assert!(
            unblock["source_checkout"]
                .as_str()
                .unwrap()
                .ends_with("/checkout")
        );

        let fixture = make_fixture(one_phase());
        let fake = FakeInvoker::new(
            [
                Step::WorkBlocked,
                Step::UnblockRetry,
                Step::WorkBlocked,
                Step::ReviewPass,
            ],
            &fixture.repo,
        );
        let result = run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap();
        assert!(matches!(result, BuildResult::Blocked { .. }));
        assert_eq!(fake.records().len(), 3);
        assert_eq!(
            fake.remaining(),
            1,
            "controller dispatched after the retry gate blocked"
        );
        let state = load_state(&fixture.build_dir.join(STATE_FILE)).unwrap();
        assert_eq!(state.status, Status::Stopped);
        assert_eq!(state.stop.as_ref().unwrap().kind, StopKind::Blocked);
        assert_eq!(state.feedback.len(), 3);
        assert_eq!(state.unblock.as_ref().unwrap().gate, Gate::Work);
        ensure_repo_at_checkpoint(&fixture.repo, &state.checkpoint_commit).unwrap();
        assert!(!fixture.repo.join("partial-untracked.txt").exists());
    }

    #[test]
    fn review_block_and_unknown_audit_each_use_one_unblock_detour() {
        let fixture = make_fixture(one_phase());
        let fake = FakeInvoker::new(
            [
                Step::WorkComplete,
                Step::ReviewBlocked,
                Step::UnblockRetry,
                Step::ReviewChanges,
                Step::WorkComplete,
                Step::ReviewPass,
                Step::AuditUnknown,
                Step::UnblockRetry,
                Step::AuditPass,
            ],
            &fixture.repo,
        );
        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap(),
            BuildResult::Completed(_)
        ));
        let records = fake.records();
        let gates = records
            .iter()
            .map(|record| {
                record
                    .argv
                    .last()
                    .unwrap()
                    .split("Gate: ")
                    .nth(1)
                    .unwrap()
                    .split(';')
                    .next()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(
            gates,
            [
                "Work", "Review", "Unblock", "Review", "Work", "Review", "Audit", "Unblock",
                "Audit"
            ]
        );
        assert!(records[2].session_in.is_none() && records[7].session_in.is_none());
        assert!(records[1].session_in.is_none() && records[3].session_in.is_none());
        assert!(records[5].session_in.is_none() && records[6].session_in.is_none());
        assert!(records[8].session_in.is_none());
        assert_eq!(records[4].session_in.as_deref(), Some("session-Work"));
    }

    #[test]
    fn explicit_audit_block_routes_to_unblock_once() {
        let fixture = make_fixture(one_phase());
        let fake = FakeInvoker::new(
            [
                Step::WorkComplete,
                Step::ReviewPass,
                Step::AuditBlocked,
                Step::UnblockRetry,
                Step::AuditPass,
            ],
            &fixture.repo,
        );
        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap(),
            BuildResult::Completed(_)
        ));
        assert_eq!(fake.remaining(), 0);
    }

    #[test]
    fn next_explicit_launch_continues_a_blocked_stop_in_one_action() {
        let fixture = make_fixture(one_phase());
        let fake = FakeInvoker::new(
            [
                Step::WorkBlocked,
                Step::UnblockBlocked,
                Step::WorkBlocked,
                Step::UnblockRetry,
                Step::WorkComplete,
                Step::ReviewPass,
                Step::AuditPass,
            ],
            &fixture.repo,
        );
        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap(),
            BuildResult::Blocked { .. }
        ));
        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap(),
            BuildResult::Completed(_)
        ));
        assert_eq!(fake.remaining(), 0);
    }

    #[test]
    fn continuation_preserves_operator_commit_and_routes_it_to_review() {
        let fixture = make_fixture(one_phase());
        let fake = FakeInvoker::new(
            [
                Step::WorkBlocked,
                Step::UnblockBlocked,
                Step::ReviewPass,
                Step::AuditPass,
            ],
            &fixture.repo,
        );
        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap(),
            BuildResult::Blocked { .. }
        ));
        fs::write(
            fixture.repo.join("operator-fix.txt"),
            "preserve this repair\n",
        )
        .unwrap();
        git_ok(&fixture.repo, &["add", "operator-fix.txt"]);
        git_ok(&fixture.repo, &["commit", "-m", "operator repair"]);
        let operator_commit = git_text(&fixture.repo, &["rev-parse", "HEAD"]).unwrap();

        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap(),
            BuildResult::Completed(_)
        ));
        let state = load_state(&fixture.build_dir.join(STATE_FILE)).unwrap();
        assert_eq!(state.checkpoint_commit, operator_commit);
        assert_eq!(state.unblock, None);
        assert_eq!(
            fs::read_to_string(fixture.repo.join("operator-fix.txt")).unwrap(),
            "preserve this repair\n"
        );
        assert!(
            fake.records()[2]
                .argv
                .last()
                .unwrap()
                .contains("Gate: Review;")
        );
    }

    #[test]
    fn continuation_rejects_dirty_operator_state_without_discarding_it() {
        let fixture = make_fixture(one_phase());
        let fake = FakeInvoker::new([Step::WorkBlocked, Step::UnblockBlocked], &fixture.repo);
        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap(),
            BuildResult::Blocked { .. }
        ));
        fs::write(
            fixture.repo.join("operator-fix.txt"),
            "uncommitted repair\n",
        )
        .unwrap();
        let error = run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap_err();
        assert!(error.to_string().contains("nothing was discarded"));
        assert_eq!(
            fs::read_to_string(fixture.repo.join("operator-fix.txt")).unwrap(),
            "uncommitted repair\n"
        );
        assert_eq!(
            load_state(&fixture.build_dir.join(STATE_FILE))
                .unwrap()
                .status,
            Status::Stopped
        );
    }

    #[test]
    fn reset_restores_tracked_state_deletes_untracked_preserves_ignored() {
        let fixture = make_fixture(one_phase());
        let state_path = fixture.build_dir.join(STATE_FILE);
        initialize(
            &fixture.store,
            &fixture.effort,
            &fixture.repo,
            &fixture.build_dir,
        )
        .unwrap();
        let mut state = load_state(&state_path).unwrap();
        state.status = Status::Stopped;
        state.stop = Some(Stop {
            kind: StopKind::ResetRequired,
            detail: "test".into(),
        });
        state.current_action_id = Some("a-reset-test".into());
        let action = current_action_dir(&fixture.build_dir, "a-reset-test").unwrap();
        fs::create_dir_all(&action).unwrap();
        let checkout = action.join("checkout");
        git_ok(
            &fixture.repo,
            &[
                "worktree",
                "add",
                "--detach",
                checkout.to_str().unwrap(),
                &state.checkpoint_commit,
            ],
        );
        save_state(&state_path, &state).unwrap();
        fs::write(fixture.repo.join("product.txt"), "dirty tracked change\n").unwrap();
        fs::write(fixture.repo.join("ordinary.tmp"), "remove me\n").unwrap();
        fs::write(fixture.repo.join("keep.ignored"), "preserve me\n").unwrap();
        reset(&fixture.store, &fixture.effort.id).unwrap();
        assert_eq!(
            git_text(&fixture.repo, &["rev-parse", "HEAD"]).unwrap(),
            state.checkpoint_commit
        );
        assert_eq!(
            fs::read_to_string(fixture.repo.join("product.txt")).unwrap(),
            "baseline\n"
        );
        assert!(!fixture.repo.join("ordinary.tmp").exists());
        assert_eq!(
            fs::read_to_string(fixture.repo.join("keep.ignored")).unwrap(),
            "preserve me\n"
        );
        assert!(!checkout.exists());
        let reset_state = load_state(&state_path).unwrap();
        assert_eq!(reset_state.status, Status::Ready);
        assert_eq!(reset_state.gate, Gate::Work);
        assert!(reset_state.current_action_id.is_none() && reset_state.stop.is_none());
    }

    #[test]
    fn failures_malformed_and_stale_results_stop_without_automatic_dispatch() {
        for step in [
            Step::InvocationError,
            Step::ProviderFailure,
            Step::MissingResponse,
            Step::Malformed,
            Step::StaleAction,
            Step::WorkInvalidCommit,
            Step::WorkWrongHead,
            Step::WorkDirtyAfterCommit,
        ] {
            let fixture = make_fixture(one_phase());
            let fake = FakeInvoker::new([step], &fixture.repo);
            let result = run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap();
            assert!(matches!(result, BuildResult::Blocked { .. }));
            assert_eq!(fake.records().len(), 1);
            let state = load_state(&fixture.build_dir.join(STATE_FILE)).unwrap();
            assert_eq!(state.status, Status::Stopped);
            assert_eq!(state.stop.unwrap().kind, StopKind::ResetRequired);
        }
    }

    #[test]
    fn invalid_launch_inputs_leave_ready_state_without_dispatch() {
        for invalid_config in [true, false] {
            let fixture = make_fixture(one_phase());
            initialize(
                &fixture.store,
                &fixture.effort,
                &fixture.repo,
                &fixture.build_dir,
            )
            .unwrap();
            if invalid_config {
                fs::write(fixture.build_dir.join(CONFIG_FILE), "not valid toml = [").unwrap();
            } else {
                fs::write(
                    fixture.build_dir.join(PLAN_FILE),
                    fs::read_to_string(fixture.build_dir.join(PLAN_FILE)).unwrap() + "\n",
                )
                .unwrap();
            }
            let fake = FakeInvoker::new([], &fixture.repo);
            assert!(run_with_invoker(&fixture.store, request(&fixture), &fake).is_err());
            assert!(fake.records().is_empty());
            let state = load_state(&fixture.build_dir.join(STATE_FILE)).unwrap();
            assert_eq!(state.status, Status::Ready);
            assert_eq!(state.stop, None);
        }
    }

    #[test]
    fn restart_from_running_dispatches_zero_actions_until_reset() {
        let fixture = make_fixture(one_phase());
        initialize(
            &fixture.store,
            &fixture.effort,
            &fixture.repo,
            &fixture.build_dir,
        )
        .unwrap();
        let state_path = fixture.build_dir.join(STATE_FILE);
        let mut state = load_state(&state_path).unwrap();
        state.status = Status::Running;
        state.current_action_id = Some("a-interrupted".into());
        save_state(&state_path, &state).unwrap();
        let fake = FakeInvoker::new([], &fixture.repo);
        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap(),
            BuildResult::Blocked { .. }
        ));
        assert!(fake.records().is_empty());
        assert_eq!(
            load_state(&state_path).unwrap().stop.unwrap().kind,
            StopKind::ResetRequired
        );
        assert!(reset(&fixture.store, &fixture.effort.id).is_ok());
    }

    #[test]
    fn plan_validation_checks_directories_and_documents_without_task_semantics() {
        let fixture = make_fixture(one_phase());
        let path = fixture.build_dir.join(PLAN_FILE);
        let mut plan = load_plan(&path).unwrap();
        for phases in [
            vec![],
            vec![""],
            vec!["  "],
            vec!["."],
            vec![".."],
            vec!["../outside"],
            vec!["nested/phase"],
            vec!["/absolute"],
            vec!["a\\b"],
            vec!["C:phase"],
            vec!["bad\0name"],
            vec!["phase_01", "phase_01"],
            vec!["missing"],
        ] {
            plan.phases = phases.into_iter().map(str::to_owned).collect();
            fs::write(&path, encode(&plan).unwrap()).unwrap();
            assert!(load_plan(&path).is_err(), "accepted {:?}", plan.phases);
        }
        plan.phases = one_phase();
        fs::write(&path, encode(&plan).unwrap()).unwrap();
        fs::remove_file(fixture.build_dir.join("phase_01/phase.md")).unwrap();
        assert!(load_plan(&path).is_err());
        fs::write(
            fixture.build_dir.join("phase_01/phase.md"),
            "Anything the Planner writes.",
        )
        .unwrap();
        fs::write(fixture.build_dir.join("phase_01/a-notes.md"), [0xff]).unwrap();
        assert!(load_plan(&path).unwrap_err().to_string().contains("UTF-8"));
        fs::write(
            fixture.build_dir.join("phase_01/a-notes.md"),
            "No task or requirement mapping.",
        )
        .unwrap();
        fs::create_dir(fixture.build_dir.join("phase_01/nested.md")).unwrap();
        assert!(load_plan(&path).is_ok());
        let mut value = serde_json::to_value(&plan).unwrap();
        value["detailed_plan"] = json!("obsolete.md");
        fs::write(&path, encode(&value).unwrap()).unwrap();
        assert!(load_plan(&path).is_err());
    }

    #[cfg(unix)]
    #[test]
    fn phase_document_loading_rejects_symlink_escapes() {
        use std::os::unix::fs::symlink;
        for name in ["phase.md", "a-notes.md", "directory"] {
            let fixture = make_fixture(one_phase());
            let phase = fixture.build_dir.join("phase_01");
            if name == "directory" {
                let outside = fixture.root.join("outside-phase");
                fs::rename(&phase, &outside).unwrap();
                symlink(outside, phase).unwrap();
            } else {
                let outside = fixture.root.join("outside.md");
                fs::write(&outside, "Outside Build").unwrap();
                fs::remove_file(phase.join(name)).unwrap();
                symlink(outside, phase.join(name)).unwrap();
            }
            assert!(load_plan(&fixture.build_dir.join(PLAN_FILE)).is_err());
        }
    }

    #[test]
    fn review_corrections_survive_blocked_continuation_and_clear_on_phase_pass() {
        let fixture = make_fixture(vec!["phase_01".into(), "phase_02".into()]);
        let first = FakeInvoker::new(
            [
                Step::WorkComplete,
                Step::ReviewChanges,
                Step::WorkBlocked,
                Step::UnblockRetry,
                Step::WorkBlocked,
            ],
            &fixture.repo,
        );
        run_with_invoker(&fixture.store, request(&fixture), &first).unwrap();
        let blocked_packet = packet_from_record(&first.records()[4]);
        assert_eq!(blocked_packet["feedback"][0]["purpose"], "Review report");
        let next = FakeInvoker::new(
            [
                Step::WorkComplete,
                Step::ReviewPass,
                Step::WorkComplete,
                Step::ReviewPass,
                Step::AuditPass,
            ],
            &fixture.repo,
        );
        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &next).unwrap(),
            BuildResult::Completed(_)
        ));
        let records = next.records();
        let correction_work = packet_from_record(&records[0]);
        let correction_review = packet_from_record(&records[1]);
        assert_eq!(correction_work["phase"]["id"], "phase_01");
        assert_eq!(correction_review["phase"], correction_work["phase"]);
        assert_eq!(correction_work["feedback"].as_array().unwrap().len(), 4);
        assert_eq!(correction_review["feedback"].as_array().unwrap().len(), 5);
        assert_eq!(
            correction_review["feedback"][0]["report"],
            "Correct the reported issue, then repeat review."
        );
        assert_eq!(correction_review["feedback"][4]["purpose"], "Worker report");
        let next_phase = packet_from_record(&records[2]);
        assert_eq!(next_phase["phase"]["id"], "phase_02");
        assert!(next_phase["feedback"].as_array().unwrap().is_empty());
    }

    fn steps_to_block(step: Step) -> Vec<Step> {
        match step {
            Step::WorkBlocked => vec![step],
            Step::ReviewBlocked => vec![Step::WorkComplete, step],
            Step::AuditBlocked | Step::AuditUnknown => {
                vec![Step::WorkComplete, Step::ReviewPass, step]
            }
            _ => panic!("not a blocking gate"),
        }
    }

    #[test]
    fn all_semantic_stops_preserve_context_and_allow_a_fresh_bounded_attempt() {
        for blocked in [
            Step::WorkBlocked,
            Step::ReviewBlocked,
            Step::AuditBlocked,
            Step::AuditUnknown,
        ] {
            for repeated in [false, true] {
                let fixture = make_fixture(one_phase());
                let mut steps = steps_to_block(blocked);
                if repeated {
                    steps.extend([Step::UnblockRetry, blocked]);
                } else {
                    steps.push(Step::UnblockBlocked);
                }
                let stop_count = steps.len();
                let first = FakeInvoker::new(steps, &fixture.repo);
                assert!(matches!(
                    run_with_invoker(&fixture.store, request(&fixture), &first).unwrap(),
                    BuildResult::Blocked { .. }
                ));
                assert_eq!(first.records().len(), stop_count);
                assert_eq!(first.remaining(), 0);
                let state_path = fixture.build_dir.join(STATE_FILE);
                let state = load_state(&state_path).unwrap();
                assert_eq!(state.status, Status::Stopped);
                assert_eq!(state.stop.as_ref().unwrap().kind, StopKind::Blocked);
                let origin = state.unblock.as_ref().unwrap();
                assert_eq!(origin.scope, state.scope);
                ensure_repo_at_checkpoint(&fixture.repo, &state.checkpoint_commit).unwrap();
                assert!(!fixture.repo.join("partial-untracked.txt").exists());
                for record in first.records() {
                    let packet = packet_from_record(&record);
                    if let Some(checkout) = packet["source_checkout"].as_str() {
                        assert!(!Path::new(checkout).exists());
                    }
                }
                let last_report = format!(
                    "actions/{}/report.md",
                    state.current_action_id.as_ref().unwrap()
                );
                assert!(state.feedback.iter().any(|item| item.path == last_report));
                for feedback in &state.feedback {
                    assert!(
                        !read_build_file(&fixture.build_dir, &feedback.path)
                            .unwrap()
                            .is_empty()
                    );
                }
                if repeated && matches!(blocked, Step::AuditUnknown) {
                    assert_eq!(
                        state
                            .feedback
                            .iter()
                            .filter(|item| item.path.ends_with("assessment.json"))
                            .count(),
                        2
                    );
                }
                let mut next_steps = vec![blocked, Step::UnblockRetry];
                next_steps.extend(match blocked {
                    Step::WorkBlocked => {
                        vec![Step::WorkComplete, Step::ReviewPass, Step::AuditPass]
                    }
                    Step::ReviewBlocked => vec![Step::ReviewPass, Step::AuditPass],
                    _ => vec![Step::AuditPass],
                });
                let next = FakeInvoker::new(next_steps, &fixture.repo);
                assert!(matches!(
                    run_with_invoker(&fixture.store, request(&fixture), &next).unwrap(),
                    BuildResult::Completed(_)
                ));
                assert_eq!(next.remaining(), 0);
                let resumed = packet_from_record(&next.records()[0]);
                assert_eq!(resumed["gate"], serde_json::to_value(&origin.gate).unwrap());
                assert!(resumed["unblock_context"].is_null());
                assert_eq!(
                    resumed["feedback"].as_array().unwrap().len(),
                    state.feedback.len()
                );
                for (actual, saved) in resumed["feedback"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .zip(&state.feedback)
                {
                    assert_eq!(actual["path"], saved.path);
                    assert_eq!(
                        actual["report"],
                        String::from_utf8(
                            read_build_file(&fixture.build_dir, &saved.path).unwrap()
                        )
                        .unwrap()
                    );
                }
                assert!(next.records()[0].session_in.is_none());
                assert!(next.records()[1].session_in.is_none());
            }
        }
    }

    #[test]
    fn continuation_accepts_markdown_refinement_but_does_not_consume_invalid_launches() {
        let fixture = make_fixture(one_phase());
        let fake = FakeInvoker::new(
            [Step::WorkBlocked, Step::UnblockRetry, Step::WorkBlocked],
            &fixture.repo,
        );
        run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap();
        let state_path = fixture.build_dir.join(STATE_FILE);
        let stopped_bytes = fs::read(&state_path).unwrap();
        let plan_path = fixture.build_dir.join(PLAN_FILE);
        let plan_bytes = fs::read(&plan_path).unwrap();
        fs::write(&plan_path, [plan_bytes.as_slice(), b"\n"].concat()).unwrap();
        let empty = FakeInvoker::new([], &fixture.repo);
        assert!(run_with_invoker(&fixture.store, request(&fixture), &empty).is_err());
        assert_eq!(fs::read(&state_path).unwrap(), stopped_bytes);
        fs::write(&plan_path, &plan_bytes).unwrap();
        let config_path = fixture.build_dir.join(CONFIG_FILE);
        let config_bytes = fs::read(&config_path).unwrap();
        fs::write(&config_path, "invalid = [").unwrap();
        assert!(run_with_invoker(&fixture.store, request(&fixture), &empty).is_err());
        assert_eq!(fs::read(&state_path).unwrap(), stopped_bytes);
        assert!(empty.records().is_empty());
        fs::write(&config_path, config_bytes).unwrap();
        fs::write(
            fixture.build_dir.join("phase_01/phase.md"),
            "Refined phase guidance after the blocker.",
        )
        .unwrap();
        fs::write(
            fixture.build_dir.join("phase_01/fix-context.md"),
            "New operator guidance.",
        )
        .unwrap();
        let next = FakeInvoker::new(
            [Step::WorkComplete, Step::ReviewPass, Step::AuditPass],
            &fixture.repo,
        );
        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &next).unwrap(),
            BuildResult::Completed(_)
        ));
        let packet = packet_from_record(&next.records()[0]);
        assert_eq!(
            packet["phase"]["documents"][0]["content"],
            "Refined phase guidance after the blocker."
        );
        assert_eq!(
            packet["phase"]["documents"][2]["content"],
            "New operator guidance."
        );
        assert_eq!(
            load_state(&state_path).unwrap().plan_digest,
            digest_bytes(&plan_bytes)
        );
    }

    #[test]
    fn continuation_preserves_descendant_fixes_after_repeated_blocks() {
        for blocked in [Step::WorkBlocked, Step::ReviewBlocked, Step::AuditBlocked] {
            let fixture = make_fixture(one_phase());
            let mut steps = steps_to_block(blocked);
            steps.extend([Step::UnblockRetry, blocked]);
            let first = FakeInvoker::new(steps, &fixture.repo);
            run_with_invoker(&fixture.store, request(&fixture), &first).unwrap();
            let before = load_state(&fixture.build_dir.join(STATE_FILE)).unwrap();
            fs::write(fixture.repo.join("operator-fix.txt"), "Committed repair").unwrap();
            git_ok(&fixture.repo, &["add", "operator-fix.txt"]);
            git_ok(&fixture.repo, &["commit", "-m", "operator fix"]);
            let fixed = git_text(&fixture.repo, &["rev-parse", "HEAD"]).unwrap();
            let final_scope = matches!(before.scope, Scope::Final);
            let next = FakeInvoker::new(
                if final_scope {
                    vec![Step::AuditPass]
                } else {
                    vec![Step::ReviewPass, Step::AuditPass]
                },
                &fixture.repo,
            );
            assert!(matches!(
                run_with_invoker(&fixture.store, request(&fixture), &next).unwrap(),
                BuildResult::Completed(_)
            ));
            assert_eq!(
                git_text(&fixture.repo, &["rev-parse", "HEAD"]).unwrap(),
                fixed
            );
            let packet = packet_from_record(&next.records()[0]);
            assert_eq!(packet["gate"], if final_scope { "audit" } else { "review" });
            assert_eq!(packet["checkpoint_commit"], fixed);
            assert_eq!(
                packet["feedback"].as_array().unwrap().len(),
                before.feedback.len()
            );
            if final_scope {
                assert_ne!(
                    packet["implementation"],
                    serde_json::to_value(before.implementation).unwrap()
                );
                let state = load_state(&fixture.build_dir.join(STATE_FILE)).unwrap();
                assert_eq!(
                    packet["implementation"],
                    serde_json::to_value(state.completion.unwrap().implementation).unwrap()
                );
            }
        }
    }

    #[test]
    fn continuation_rejects_unrelated_commits_without_altering_state_or_checkout() {
        let fixture = make_fixture(one_phase());
        let first = FakeInvoker::new(
            [Step::WorkBlocked, Step::UnblockRetry, Step::WorkBlocked],
            &fixture.repo,
        );
        run_with_invoker(&fixture.store, request(&fixture), &first).unwrap();
        let state_path = fixture.build_dir.join(STATE_FILE);
        let stopped = fs::read(&state_path).unwrap();
        git_ok(&fixture.repo, &["checkout", "--orphan", "unrelated"]);
        git_ok(&fixture.repo, &["commit", "-m", "independent root"]);
        let head = git_text(&fixture.repo, &["rev-parse", "HEAD"]).unwrap();
        let empty = FakeInvoker::new([], &fixture.repo);
        assert!(
            run_with_invoker(&fixture.store, request(&fixture), &empty)
                .unwrap_err()
                .to_string()
                .contains("nothing was discarded")
        );
        assert_eq!(fs::read(&state_path).unwrap(), stopped);
        assert_eq!(
            git_text(&fixture.repo, &["rev-parse", "HEAD"]).unwrap(),
            head
        );
        ensure_clean(&fixture.repo).unwrap();
        assert!(empty.records().is_empty());
    }

    #[test]
    fn fresh_process_continues_ready_state_with_only_packet_context() {
        let fixture = make_fixture(one_phase());
        initialize(
            &fixture.store,
            &fixture.effort,
            &fixture.repo,
            &fixture.build_dir,
        )
        .unwrap();
        let state_path = fixture.build_dir.join(STATE_FILE);
        let mut state = load_state(&state_path).unwrap();
        let inputs = load_execution_inputs(&fixture.build_dir, &state).unwrap();
        let fake = FakeInvoker::new(
            [
                Step::WorkComplete,
                Step::ReviewChanges,
                Step::WorkComplete,
                Step::ReviewPass,
                Step::AuditPass,
            ],
            &fixture.repo,
        );
        let mut sessions = RuntimeSessions::default();
        for _ in 0..2 {
            execute_gate(
                &fixture.store,
                &fixture.effort,
                &fixture.repo,
                &fixture.build_dir,
                &mut state,
                &mut sessions,
                &inputs,
                &fake,
                &mut |_: BuildObservation<'_>| {},
            )
            .unwrap();
        }
        assert!(sessions.worker.is_some());
        drop(sessions);
        drop(state);
        let durable: serde_json::Value =
            serde_json::from_slice(&fs::read(&state_path).unwrap()).unwrap();
        assert_eq!(durable["schema_version"], 5);
        assert!(
            durable.get("worker_session").is_none() && durable.get("reviewer_session").is_none()
        );
        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap(),
            BuildResult::Completed(_)
        ));
        let records = fake.records();
        assert!(records[2].session_in.is_none() && records[3].session_in.is_none());
        assert!(records[4].session_in.is_none());
        assert_eq!(
            packet_from_record(&records[2])["feedback"][0]["purpose"],
            "Review report"
        );
        assert_eq!(
            packet_from_record(&records[3])["feedback"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn providers_without_session_ids_complete_the_same_correction_loop() {
        let fixture = make_fixture(one_phase());
        let mut fake = FakeInvoker::new(
            [
                Step::WorkComplete,
                Step::ReviewChanges,
                Step::WorkComplete,
                Step::ReviewPass,
                Step::AuditPass,
            ],
            &fixture.repo,
        );
        fake.no_sessions = true;
        assert!(matches!(
            run_with_invoker(&fixture.store, request(&fixture), &fake).unwrap(),
            BuildResult::Completed(_)
        ));
        assert_eq!(fake.records().len(), 5);
        assert!(
            fake.records()
                .iter()
                .all(|record| record.session_in.is_none())
        );
    }

    mod observations {
        use super::*;
        use std::sync::{Arc, mpsc};
        use std::time::Duration;

        #[derive(Clone, Debug)]
        struct Recorded {
            event: BuildEvent,
            state: BuildState,
            plan: Option<BuildPlan>,
        }

        struct Recorder {
            build_dir: PathBuf,
            events: Vec<Recorded>,
        }

        impl Recorder {
            fn new(fixture: &Fixture) -> Self {
                Self {
                    build_dir: fixture.build_dir.clone(),
                    events: Vec::new(),
                }
            }

            fn routes(&self) -> Vec<(&FinishedAction, &BuildState)> {
                self.events
                    .iter()
                    .filter_map(|event| match &event.event {
                        BuildEvent::ActionRouted(action) => Some((action, &event.state)),
                        _ => None,
                    })
                    .collect()
            }
        }

        impl BuildObserver for Recorder {
            fn observe(&mut self, observation: BuildObservation<'_>) {
                // Every notification is already durable, including the retained
                // action ID on stops and the evidence needed to explain routes.
                assert_eq!(
                    load_state(&self.build_dir.join(STATE_FILE)).unwrap(),
                    *observation.state
                );
                match &observation.event {
                    BuildEvent::ActionStarted => {
                        assert_eq!(observation.state.status, Status::Running);
                        let id = observation.state.current_action_id.as_ref().unwrap();
                        let dir = current_action_dir(&self.build_dir, id).unwrap();
                        assert!(dir.join("action.json").is_file());
                        assert!(dir.join("invocation.json").is_file());
                        assert!(!dir.join("result.json").exists());
                    }
                    BuildEvent::ActionRouted(action) => {
                        let dir = current_action_dir(&self.build_dir, &action.action_id).unwrap();
                        assert!(dir.join("result.json").is_file());
                        assert!(dir.join("report.md").is_file());
                        let start = self.events.last().unwrap();
                        assert_eq!(start.event, BuildEvent::ActionStarted);
                        assert_eq!(
                            start.state.current_action_id.as_ref(),
                            Some(&action.action_id)
                        );
                        assert_eq!(start.state.gate, action.gate);
                        assert_eq!(start.state.scope, action.scope);
                    }
                    BuildEvent::Terminal => assert!(matches!(
                        observation.state.status,
                        Status::Stopped | Status::Complete
                    )),
                    _ => {}
                }
                self.events.push(Recorded {
                    event: observation.event,
                    state: observation.state.clone(),
                    plan: observation.plan.cloned(),
                });
            }
        }

        fn observed_run(fixture: &Fixture, steps: impl IntoIterator<Item = Step>) -> Recorder {
            let fake = FakeInvoker::new(steps, &fixture.repo);
            let mut recorder = Recorder::new(fixture);
            run_with_invoker_and_observer(&fixture.store, request(fixture), &fake, &mut recorder)
                .unwrap();
            assert_eq!(fake.remaining(), 0);
            recorder
        }

        #[test]
        fn observations_preserve_executed_scope_and_confirm_review_and_audit_verdicts() {
            let phases = vec!["phase_01_foundation".into(), "phase_02_delivery".into()];
            let fixture = make_fixture(phases.clone());
            let recorder = observed_run(
                &fixture,
                [
                    Step::WorkComplete,
                    Step::ReviewChanges,
                    Step::WorkComplete,
                    Step::ReviewPass,
                    Step::WorkComplete,
                    Step::ReviewPass,
                    Step::AuditFail,
                    Step::WorkComplete,
                    Step::AuditPass,
                ],
            );
            assert_eq!(
                recorder.events.first().unwrap().event,
                BuildEvent::Initialized
            );
            assert_eq!(recorder.events.last().unwrap().event, BuildEvent::Terminal);
            assert_eq!(recorder.events.len(), 20); // initial, nine start/route pairs, terminal
            assert!(
                recorder
                    .events
                    .iter()
                    .all(|event| event.plan.as_ref().unwrap().phases == phases)
            );
            let expected = [
                (
                    Gate::Work,
                    Scope::Phase { index: 0 },
                    ActionOutcome::Complete,
                    Gate::Review,
                    Scope::Phase { index: 0 },
                ),
                (
                    Gate::Review,
                    Scope::Phase { index: 0 },
                    ActionOutcome::ChangesRequired,
                    Gate::Work,
                    Scope::Phase { index: 0 },
                ),
                (
                    Gate::Work,
                    Scope::Phase { index: 0 },
                    ActionOutcome::Complete,
                    Gate::Review,
                    Scope::Phase { index: 0 },
                ),
                (
                    Gate::Review,
                    Scope::Phase { index: 0 },
                    ActionOutcome::Pass,
                    Gate::Work,
                    Scope::Phase { index: 1 },
                ),
                (
                    Gate::Work,
                    Scope::Phase { index: 1 },
                    ActionOutcome::Complete,
                    Gate::Review,
                    Scope::Phase { index: 1 },
                ),
                (
                    Gate::Review,
                    Scope::Phase { index: 1 },
                    ActionOutcome::Pass,
                    Gate::Audit,
                    Scope::Final,
                ),
                (
                    Gate::Audit,
                    Scope::Final,
                    ActionOutcome::ChangesRequired,
                    Gate::Work,
                    Scope::Final,
                ),
                (
                    Gate::Work,
                    Scope::Final,
                    ActionOutcome::Complete,
                    Gate::Audit,
                    Scope::Final,
                ),
                (
                    Gate::Audit,
                    Scope::Final,
                    ActionOutcome::Pass,
                    Gate::Audit,
                    Scope::Final,
                ),
            ];
            for ((action, state), (gate, scope, outcome, next_gate, next_scope)) in
                recorder.routes().into_iter().zip(expected)
            {
                assert_eq!(
                    (&action.gate, &action.scope, &action.outcome),
                    (&gate, &scope, &outcome)
                );
                assert_eq!((&state.gate, &state.scope), (&next_gate, &next_scope));
                assert_eq!(
                    state.status,
                    if gate == Gate::Audit && outcome == ActionOutcome::Pass {
                        Status::Complete
                    } else {
                        Status::Ready
                    }
                );
            }
        }

        #[test]
        fn observations_distinguish_blocked_routes_retries_and_terminal_stops() {
            for repeated in [false, true] {
                let fixture = make_fixture(one_phase());
                let recorder = observed_run(
                    &fixture,
                    if repeated {
                        vec![Step::WorkBlocked, Step::UnblockRetry, Step::WorkBlocked]
                    } else {
                        vec![Step::WorkBlocked, Step::UnblockBlocked]
                    },
                );
                let routes = recorder.routes();
                assert_eq!(routes[0].0.outcome, ActionOutcome::Blocked);
                assert_eq!(routes[0].1.gate, Gate::Unblock);
                assert_eq!(routes[0].1.status, Status::Ready);
                assert_eq!(routes[1].0.gate, Gate::Unblock);
                assert_eq!(
                    routes[1].0.outcome,
                    if repeated {
                        ActionOutcome::Retry
                    } else {
                        ActionOutcome::Blocked
                    }
                );
                let last = recorder.events.last().unwrap();
                assert_eq!(last.state.stop.as_ref().unwrap().kind, StopKind::Blocked);
                assert_eq!(
                    last.state.current_action_id.as_ref(),
                    Some(&routes.last().unwrap().0.action_id)
                );

                let resumed = observed_run(
                    &fixture,
                    [Step::WorkComplete, Step::ReviewPass, Step::AuditPass],
                );
                assert_eq!(resumed.events[0].event, BuildEvent::Snapshot);
                assert_eq!(resumed.events[0].state, last.state);
                assert_eq!(resumed.events[1].event, BuildEvent::Continued);
                assert_eq!(resumed.events[1].state.status, Status::Ready);
                assert!(resumed.events[1].state.current_action_id.is_none());
                assert_eq!(
                    resumed.events.last().unwrap().state.status,
                    Status::Complete
                );
            }
            for audit_step in [Step::AuditUnknown, Step::AuditBlocked] {
                let fixture = make_fixture(one_phase());
                let recorder = observed_run(
                    &fixture,
                    [
                        Step::WorkComplete,
                        Step::ReviewBlocked,
                        Step::UnblockRetry,
                        Step::ReviewPass,
                        audit_step,
                        Step::UnblockRetry,
                        Step::AuditPass,
                    ],
                );
                let routes = recorder.routes();
                assert_eq!(routes[1].0.outcome, ActionOutcome::Blocked);
                assert_eq!(routes[1].1.gate, Gate::Unblock);
                assert_eq!(routes[4].0.gate, Gate::Audit);
                assert_eq!(routes[4].0.outcome, ActionOutcome::Blocked);
                assert_eq!(routes[4].0.scope, Scope::Final);
                assert_eq!(routes[4].1.status, Status::Ready);
                assert_eq!(routes[4].1.gate, Gate::Unblock);
                assert_eq!(routes[5].0.outcome, ActionOutcome::Retry);
                assert_eq!(routes[5].1.gate, Gate::Audit);
                assert_eq!(routes[6].0.outcome, ActionOutcome::Pass);
            }
        }

        #[test]
        fn observations_report_errors_and_restarted_running_without_a_confirmed_route() {
            for step in [
                Step::InvocationError,
                Step::ProviderFailure,
                Step::Malformed,
                Step::WorkInvalidCommit,
            ] {
                let fixture = make_fixture(one_phase());
                let recorder = observed_run(&fixture, [step]);
                assert_eq!(
                    recorder
                        .events
                        .iter()
                        .map(|event| &event.event)
                        .collect::<Vec<_>>(),
                    vec![
                        &BuildEvent::Initialized,
                        &BuildEvent::ActionStarted,
                        &BuildEvent::Terminal
                    ]
                );
                let last = recorder.events.last().unwrap();
                assert_eq!(
                    last.state.stop.as_ref().unwrap().kind,
                    StopKind::ResetRequired
                );
                assert_eq!(
                    last.state.current_action_id,
                    recorder.events[1].state.current_action_id
                );
            }
            let fixture = make_fixture(one_phase());
            initialize(
                &fixture.store,
                &fixture.effort,
                &fixture.repo,
                &fixture.build_dir,
            )
            .unwrap();
            let path = fixture.build_dir.join(STATE_FILE);
            let mut state = load_state(&path).unwrap();
            state.status = Status::Running;
            state.current_action_id = Some("a-interrupted".into());
            save_state(&path, &state).unwrap();
            fs::remove_file(fixture.build_dir.join(PLAN_FILE)).unwrap();
            fs::remove_file(fixture.build_dir.join(CONFIG_FILE)).unwrap();
            let recorder = observed_run(&fixture, []);
            assert_eq!(recorder.events.len(), 2);
            assert_eq!(recorder.events[0].event, BuildEvent::Snapshot);
            assert_eq!(recorder.events[0].state, state);
            assert!(recorder.events.iter().all(|event| event.plan.is_none()));
            let last = &recorder.events[1];
            assert_eq!(last.event, BuildEvent::Terminal);
            assert_eq!(last.state.current_action_id, state.current_action_id);
            assert_eq!(
                last.state.stop.as_ref().unwrap().kind,
                StopKind::ResetRequired
            );
            let stopped = observed_run(&fixture, []);
            assert_eq!(stopped.events[0].state, last.state);
            assert_eq!(stopped.events[1].state, last.state);
        }

        #[test]
        fn observations_of_completed_builds_do_not_require_config_or_usable_plan() {
            let fixture = make_fixture(one_phase());
            let completed = observed_run(
                &fixture,
                [Step::WorkComplete, Step::ReviewPass, Step::AuditPass],
            );
            let final_state = &completed.events.last().unwrap().state;
            fs::remove_file(fixture.build_dir.join(CONFIG_FILE)).unwrap();
            let plan_path = fixture.build_dir.join(PLAN_FILE);
            let accepted = fs::read(&plan_path).unwrap();
            // A readable but altered plan must not supply unaccepted counts.
            let mut changed = accepted.clone();
            changed.push(b' ');
            for contents in [
                Some(accepted),
                Some(changed),
                Some(b"invalid".to_vec()),
                None,
            ] {
                if let Some(bytes) = &contents {
                    fs::write(&plan_path, bytes).unwrap();
                } else {
                    fs::remove_file(&plan_path).unwrap();
                }
                let recorder = observed_run(&fixture, []);
                assert_eq!(recorder.events.len(), 2);
                assert_eq!(recorder.events[0].event, BuildEvent::Snapshot);
                assert_eq!(recorder.events[1].event, BuildEvent::Terminal);
                for event in &recorder.events {
                    assert_eq!(&event.state, final_state);
                    assert_eq!(
                        event.plan.is_some(),
                        contents
                            .as_ref()
                            .is_some_and(|bytes| digest_bytes(bytes) == final_state.plan_digest)
                    );
                }
            }
        }

        #[test]
        fn observations_seed_resumed_phase_and_final_scope_from_persisted_state() {
            for gates in [2, 4] {
                let fixture = make_fixture(vec!["phase_01".into(), "phase_02".into()]);
                initialize(
                    &fixture.store,
                    &fixture.effort,
                    &fixture.repo,
                    &fixture.build_dir,
                )
                .unwrap();
                let mut state = load_state(&fixture.build_dir.join(STATE_FILE)).unwrap();
                let inputs = load_execution_inputs(&fixture.build_dir, &state).unwrap();
                let fake = FakeInvoker::new(
                    [
                        Step::WorkComplete,
                        Step::ReviewPass,
                        Step::WorkComplete,
                        Step::ReviewPass,
                        Step::AuditPass,
                    ],
                    &fixture.repo,
                );
                for _ in 0..gates {
                    execute_gate(
                        &fixture.store,
                        &fixture.effort,
                        &fixture.repo,
                        &fixture.build_dir,
                        &mut state,
                        &mut RuntimeSessions::default(),
                        &inputs,
                        &fake,
                        &mut |_: BuildObservation<'_>| {},
                    )
                    .unwrap();
                }
                let mut recorder = Recorder::new(&fixture);
                run_with_invoker_and_observer(
                    &fixture.store,
                    request(&fixture),
                    &fake,
                    &mut recorder,
                )
                .unwrap();
                assert_eq!(recorder.events[0].event, BuildEvent::Snapshot);
                assert_eq!(recorder.events[0].state, state);
                assert_eq!(
                    state.scope,
                    if gates == 2 {
                        Scope::Phase { index: 1 }
                    } else {
                        Scope::Final
                    }
                );
                assert_eq!(recorder.events[0].plan.as_ref().unwrap().phases.len(), 2);
                assert_eq!(
                    recorder.events.last().unwrap().state.status,
                    Status::Complete
                );
            }
        }

        struct PausedInvoker {
            fake: FakeInvoker,
            entered: mpsc::Sender<()>,
            release: Mutex<mpsc::Receiver<()>>,
        }

        impl adapter::InvocationApi for PausedInvoker {
            fn invoke(
                &self,
                plan: &adapter::InvocationPlan,
                cwd: &Path,
                action_dir: &Path,
            ) -> Result<InvocationOutcome> {
                self.entered.send(()).unwrap();
                self.release
                    .lock()
                    .unwrap()
                    .recv_timeout(Duration::from_secs(10))
                    .unwrap();
                self.fake.invoke(plan, cwd, action_dir)
            }
        }

        #[test]
        fn observations_expose_running_action_while_invocation_is_paused() {
            let fixture = make_fixture(one_phase());
            let (entered_tx, entered_rx) = mpsc::channel();
            let (release_tx, release_rx) = mpsc::channel();
            let invoker = PausedInvoker {
                fake: FakeInvoker::new([Step::ProviderFailure], &fixture.repo),
                entered: entered_tx,
                release: Mutex::new(release_rx),
            };
            let recorder = Arc::new(Mutex::new(Recorder::new(&fixture)));
            std::thread::scope(|scope| {
                let recorder_copy = Arc::clone(&recorder);
                let fixture_ref = &fixture;
                let invoker_ref = &invoker;
                let worker = scope.spawn(move || {
                    run_with_invoker_and_observer(
                        &fixture_ref.store,
                        request(fixture_ref),
                        invoker_ref,
                        &mut |event: BuildObservation<'_>| {
                            recorder_copy.lock().unwrap().observe(event)
                        },
                    )
                    .unwrap()
                });
                entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                {
                    let recorder = recorder.lock().unwrap();
                    assert_eq!(recorder.events.len(), 2);
                    let started = &recorder.events[1];
                    assert_eq!(started.event, BuildEvent::ActionStarted);
                    assert_eq!(started.state.status, Status::Running);
                    assert_eq!(started.state.gate, Gate::Work);
                    assert_eq!(started.state.scope, Scope::Phase { index: 0 });
                    assert!(started.state.current_action_id.is_some());
                    assert!(!started.state.checkpoint_commit.is_empty());
                    assert_eq!(invoker.fake.records().len(), 0);
                }
                release_tx.send(()).unwrap();
                assert!(matches!(
                    worker.join().unwrap(),
                    BuildResult::Blocked { .. }
                ));
            });
            assert_eq!(
                recorder.lock().unwrap().events.last().unwrap().event,
                BuildEvent::Terminal
            );
        }
    }
}
