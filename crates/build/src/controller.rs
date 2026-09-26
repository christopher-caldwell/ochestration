use anyhow::{Context, Result, bail, ensure};
use orchestrate_audit::{
    adopt_for_build, finalize_audit_with_run_id, find_implementation,
    register_implementation_with_run_id,
};
use orchestrate_contracts::{
    ArtifactKind, AuditAssessment, AuditReport, ImplementationStatus, Independence, Provenance,
    ReconciledDiscovery, Verdict, digest_bytes, encode, safe_relative_path,
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
    adapter,
    packet::create_action_packet,
    state::{
        BuildCompletion, BuildConfig, BuildPlan, BuildState, FeedbackRef, Gate, RoleConfig,
        STATE_VERSION, Scope, Session, Status, Stop, StopKind, UnblockContext, current_action_dir,
        load_config, load_plan, load_state, read_build_file, save_state,
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
        write_bytes_sync(&plan, orchestrate_guides::templates::PLAN_JSON.as_bytes())?;
    }
    let config = build_dir.join(CONFIG_FILE);
    if !config.exists() {
        write_bytes_sync(
            &config,
            orchestrate_guides::templates::CONFIG_TOML.as_bytes(),
        )?;
    }
    let details = build_dir.join("implementation-plan.md");
    if !details.exists() {
        write_bytes_sync(&details, b"# Implementation plan\n\nReplace this scaffold with the approved, detailed implementation plan.\n")?;
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
        "build reset requires a running or reset_required state; relaunch Build for an external_requirement stop"
    );
    validate_commit(&project.canonical_locator, &state.checkpoint_commit)?;
    remove_current_worktree(
        &project.canonical_locator,
        &build_dir,
        state.current_action_id.as_deref(),
    )?;
    reset_checkout(&project.canonical_locator, &state.checkpoint_commit)?;
    state.worker_session = None;
    state.reviewer_session = None;
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
                .is_some_and(|stop| stop.kind == StopKind::ExternalRequirement),
        "Build continuation requires a stopped external_requirement state"
    );
    let context = state
        .unblock
        .as_ref()
        .context("external stop lacks its Unblock context")?
        .clone();
    let unblock_action = state
        .current_action_id
        .as_deref()
        .context("external stop lacks the Unblock action id")?;
    let report_path = format!("actions/{unblock_action}/report.md");
    read_build_file(build_dir, &report_path).context("external stop lacks its Unblock report")?;
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
    state.feedback.push(FeedbackRef {
        path: report_path,
        purpose: "operator confirmed external condition resolved; Unblock guidance".into(),
    });
    state.unblock = None;
    state.worker_session = None;
    state.reviewer_session = None;
    state.current_action_id = None;
    state.stop = None;
    state.status = Status::Ready;
    save_state(state_path, state)?;
    Ok(())
}

pub fn run(store: &Store, request: BuildRequest) -> Result<BuildResult> {
    run_with_invoker(store, request, &adapter::ProcessInvocationApi)
}

fn run_with_invoker(
    store: &Store,
    request: BuildRequest,
    invoker: &dyn adapter::InvocationApi,
) -> Result<BuildResult> {
    let effort = select_effort(store, &request)?;
    let project = store.project_for(&effort)?;
    ensure!(
        fs::canonicalize(&request.project).ok().as_ref() == Some(&project.canonical_locator),
        "current repository does not match selected effort"
    );
    let build_dir = store.phase_dir(&effort, "build")?;
    let state_path = build_dir.join(STATE_FILE);
    if !state_path.exists() {
        initialize(store, &effort, &project.canonical_locator, &build_dir)?;
    }
    let mut state = load_state(&state_path)?;
    match state.status {
        Status::Complete => {
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
                .is_some_and(|stop| stop.kind == StopKind::ExternalRequirement)
            {
                prepare_continuation(
                    &project.canonical_locator,
                    &build_dir,
                    &state_path,
                    &mut state,
                )?;
            } else {
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
            return Ok(BuildResult::Blocked {
                detail: state.stop.as_ref().unwrap().detail.clone(),
                state: state_path,
            });
        }
        Status::Ready => {}
    }

    let inputs = load_execution_inputs(&build_dir, &state)?;

    loop {
        if state.status == Status::Complete {
            return Ok(BuildResult::Completed(
                state
                    .completion
                    .clone()
                    .context("complete Build lacks completion artifacts")?,
            ));
        }
        if state.status == Status::Stopped {
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
            &inputs,
            invoker,
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
            return Ok(BuildResult::Blocked {
                detail: state.stop.as_ref().unwrap().detail.clone(),
                state: state_path,
            });
        }
    }
}

struct ExecutionInputs {
    plan: BuildPlan,
    detailed: String,
    config: BuildConfig,
}

fn load_execution_inputs(build_dir: &Path, state: &BuildState) -> Result<ExecutionInputs> {
    let plan_path = build_dir.join(PLAN_FILE);
    let plan = load_plan(&plan_path)?;
    let detailed = read_build_file(build_dir, &plan.detailed_plan)
        .context("cannot read referenced detailed plan")?;
    let plan_digest = combined_digest(&fs::read(&plan_path)?, &detailed);
    ensure!(
        plan_digest == state.plan_digest && plan.reconciled == state.reconciled,
        "Build plan or detailed plan changed after initialization; restore the accepted files or start a new Build according to current authority"
    );
    let detailed = String::from_utf8(detailed).context("detailed plan must be UTF-8")?;
    let config = load_config(&build_dir.join(CONFIG_FILE))?;
    Ok(ExecutionInputs {
        plan,
        detailed,
        config,
    })
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
    ensure!(
        safe_relative_path(&plan.detailed_plan),
        "detailed plan path must be relative and safe"
    );
    let detailed = read_build_file(build_dir, &plan.detailed_plan)
        .context("cannot read referenced detailed plan")?;
    let plan_bytes = fs::read(&plan_path)?;
    let plan_digest = combined_digest(&plan_bytes, &detailed);
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
    validate_plan_requirements(&plan, &reconciled)?;
    let adoption = adopt_for_build(
        store,
        effort,
        plan.reconciled.clone(),
        provenance("build-adoption", None),
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
        worker_session: None,
        reviewer_session: None,
        implementation: None,
        completion: None,
        stop: None,
    };
    save_state(&build_dir.join(STATE_FILE), &state)?;
    eprintln!(
        "INITIALIZED checkpoint={} gate=work",
        state.checkpoint_commit
    );
    Ok(())
}

fn validate_plan_requirements(plan: &BuildPlan, reconciled: &ReconciledDiscovery) -> Result<()> {
    let known = reconciled
        .requirements
        .iter()
        .map(|item| item.requirement.id.as_str())
        .collect::<std::collections::HashSet<_>>();
    let mut phase_ids = std::collections::HashSet::new();
    for phase in &plan.phases {
        ensure!(
            !phase.id.trim().is_empty() && phase_ids.insert(phase.id.as_str()),
            "phase IDs must be nonempty and unique"
        );
        ensure!(
            !phase.tasks.is_empty() && phase.tasks.iter().all(|task| !task.trim().is_empty()),
            "phase {} must have nonempty tasks",
            phase.id
        );
        ensure!(
            !phase.requirement_ids.is_empty(),
            "phase {} must reference at least one requirement",
            phase.id
        );
        let mut local = std::collections::HashSet::new();
        for id in &phase.requirement_ids {
            ensure!(
                known.contains(id.as_str()),
                "phase {} references unknown requirement {id}",
                phase.id
            );
            ensure!(
                local.insert(id.as_str()),
                "phase {} repeats requirement {id}",
                phase.id
            );
        }
    }
    Ok(())
}

fn execute_gate(
    store: &Store,
    effort: &Effort,
    repo: &Path,
    build_dir: &Path,
    state: &mut BuildState,
    inputs: &ExecutionInputs,
    invoker: &dyn adapter::InvocationApi,
) -> Result<()> {
    if state.gate == Gate::Audit {
        ensure_audit_implementation(store, effort, repo, state)?;
    }
    ensure_repo_at_checkpoint(repo, &state.checkpoint_commit)?;
    let action_id = action_id();
    let gate = state.gate.clone();
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
    let packet = create_action_packet(
        store,
        effort,
        build_dir,
        &inputs.plan,
        state,
        &action_id,
        &inputs.detailed,
    )?;
    let role = role_for_gate(&inputs.config, &gate);
    let session = match gate {
        Gate::Work => state.worker_session.as_ref(),
        Gate::Review | Gate::Audit => state.reviewer_session.as_ref(),
        Gate::Unblock => None,
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
    eprintln!(
        "RUNNING gate={gate:?} action={action_id} checkpoint={}",
        state.checkpoint_commit
    );

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
            update_session(state, &gate, role, process.observed_session);
            if result.outcome == "blocked" {
                if state.unblock.is_some() {
                    bail!(
                        "retried gate blocked after Unblock; automatic second Unblock is forbidden"
                    );
                }
                reset_checkout(repo, &state.checkpoint_commit)?;
                state.feedback.push(feedback_ref);
                state.unblock = Some(UnblockContext {
                    gate: Gate::Work,
                    scope: state.scope.clone(),
                });
                state.gate = Gate::Unblock;
                state.status = Status::Ready;
            } else {
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
            update_session(state, &gate, role, process.observed_session);
            ensure!(
                result.inspected_commit == state.checkpoint_commit,
                "Review inspected commit does not match the exact checkpoint"
            );
            match result.outcome.as_str() {
                "pass" => {
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
                    state.feedback = vec![feedback_ref];
                    state.gate = Gate::Work;
                    state.unblock = None;
                    state.status = Status::Ready;
                }
                "blocked" => enter_unblock(state, Gate::Review, feedback_ref)?,
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
            update_session(state, &gate, role, process.observed_session);
            match result.outcome.as_str() {
                "blocked" => {
                    if state.unblock.is_some() {
                        bail!(
                            "retried Audit blocked after Unblock; automatic second Unblock is forbidden"
                        );
                    }
                    enter_unblock(state, Gate::Audit, feedback_ref)?;
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
                        provenance(role.adapter.as_str(), role.model.as_deref()),
                        format!("build-audit-{action_id}"),
                    )?;
                    let (_, report): (_, AuditReport) =
                        store.load_json(effort, &audit, "audit.json")?;
                    match report.verdict {
                        Verdict::Pass => {
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
                            if state.unblock.is_some() {
                                bail!(
                                    "retried Audit has unknown or missing coverage after Unblock; automatic second Unblock is forbidden"
                                );
                            }
                            state.feedback = vec![
                                feedback_ref,
                                FeedbackRef {
                                    path: format!("actions/{action_id}/assessment.json"),
                                    purpose: "Audit unknown or incomplete requirement coverage"
                                        .into(),
                                },
                            ];
                            state.unblock = Some(UnblockContext {
                                gate: Gate::Audit,
                                scope: state.scope.clone(),
                            });
                            state.gate = Gate::Unblock;
                            state.status = Status::Ready;
                        }
                    }
                }
                _ => unreachable!(),
            }
        }
        Gate::Unblock => {
            let result: UnblockResult = parse_result(response, &action_id)?;
            ensure!(
                matches!(result.outcome.as_str(), "retry" | "external_requirement"),
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
            match result.outcome.as_str() {
                "retry" => {
                    let context = state.unblock.as_ref().unwrap().clone();
                    reset_checkout(repo, &state.checkpoint_commit)?;
                    state.feedback.push(feedback_ref);
                    state.scope = context.scope;
                    state.gate = context.gate;
                    state.status = Status::Ready;
                }
                "external_requirement" => {
                    state.status = Status::Stopped;
                    state.stop = Some(Stop {
                        kind: StopKind::ExternalRequirement,
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
    eprintln!(
        "ROUTED gate={:?} scope={:?} status={:?} checkpoint={}",
        state.gate, state.scope, state.status, state.checkpoint_commit
    );
    Ok(())
}

fn role_for_gate<'a>(config: &'a BuildConfig, gate: &Gate) -> &'a RoleConfig {
    match gate {
        Gate::Work => &config.worker,
        Gate::Review | Gate::Audit => &config.reviewer,
        Gate::Unblock => config.unblocker.as_ref().unwrap_or(&config.reviewer),
    }
}

fn update_session(
    state: &mut BuildState,
    gate: &Gate,
    role: &RoleConfig,
    observed: Option<String>,
) {
    let target = match gate {
        Gate::Work => &mut state.worker_session,
        Gate::Review | Gate::Audit => &mut state.reviewer_session,
        Gate::Unblock => return,
    };
    if let Some(id) = observed.filter(|id| !id.trim().is_empty()) {
        *target = Some(Session {
            adapter: role.adapter.clone(),
            id,
        });
    } else if target
        .as_ref()
        .is_some_and(|session| session.adapter != role.adapter)
    {
        *target = None;
    }
}

fn enter_unblock(state: &mut BuildState, gate: Gate, feedback: FeedbackRef) -> Result<()> {
    ensure!(
        state.unblock.is_none(),
        "automatic second Unblock is forbidden"
    );
    state.feedback.push(feedback);
    state.unblock = Some(UnblockContext {
        gate,
        scope: state.scope.clone(),
    });
    state.gate = Gate::Unblock;
    state.status = Status::Ready;
    Ok(())
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
            "complete" | "blocked" | "pass" | "changes_required" | "retry" | "external_requirement"
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
                    provenance("build", None),
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

fn provenance(provider: &str, model: Option<&str>) -> Provenance {
    let guide = orchestrate_guides::FINAL_AUDIT;
    Provenance {
        host: "orchestrate-build".into(),
        provider: Some(provider.into()),
        model: model.map(str::to_owned),
        model_effort: None,
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

fn combined_digest(plan: &[u8], detailed: &[u8]) -> String {
    let mut bytes = Vec::with_capacity(16 + plan.len() + detailed.len());
    bytes.extend_from_slice(&(plan.len() as u64).to_be_bytes());
    bytes.extend_from_slice(plan);
    bytes.extend_from_slice(&(detailed.len() as u64).to_be_bytes());
    bytes.extend_from_slice(detailed);
    digest_bytes(&bytes)
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
mod tests {
    use super::*;
    use crate::adapter::InvocationOutcome;
    use orchestrate_contracts::{
        ArtifactKind, ArtifactRef, DiscoverySourceRef, ReconciledRequirement, RequestKind,
        Requirement,
    };
    use std::{
        collections::{BTreeMap, VecDeque},
        sync::{
            Mutex,
            atomic::{AtomicU64, Ordering},
        },
        time::{SystemTime, UNIX_EPOCH},
    };

    struct Fixture {
        root: PathBuf,
        repo: PathBuf,
        effort: Effort,
        store: Store,
        build_dir: PathBuf,
        reconciled: ArtifactRef,
    }

    static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    fn make_fixture(phases: Vec<crate::state::PlanPhase>) -> Fixture {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "build-controller-{nonce}-{}-{}",
            std::process::id(),
            FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        let repo = root.join("product");
        fs::create_dir_all(&repo).unwrap();
        git_ok(&repo, &["init", "-q"]);
        git_ok(&repo, &["config", "user.name", "Build Test"]);
        git_ok(
            &repo,
            &["config", "user.email", "build-test@example.invalid"],
        );
        fs::write(repo.join(".gitignore"), "keep.ignored\n").unwrap();
        fs::write(repo.join("product.txt"), "baseline\n").unwrap();
        git_ok(&repo, &["add", "."]);
        git_ok(&repo, &["commit", "-m", "baseline"]);
        let baseline_commit = git_text(&repo, &["rev-parse", "HEAD"]).unwrap();
        let baseline_tree = git_text(&repo, &["rev-parse", "HEAD^{tree}"]).unwrap();
        let store = Store::open(root.join("store")).unwrap();
        let effort = store
            .init_effort(
                &repo,
                "test-effort",
                RequestKind::Freeform,
                "Build test".into(),
                Vec::new(),
            )
            .unwrap();
        let requirement_ids = phases
            .iter()
            .flat_map(|phase| phase.requirement_ids.iter().cloned())
            .collect::<std::collections::BTreeSet<_>>();
        let reconciled = ReconciledDiscovery {
            reconciled_id: "test-reconciled".into(),
            context_id: effort.context.id.clone(),
            baseline_commit,
            baseline_tree,
            goal: "Test Build routing".into(),
            core_result: "A checkpointed gate runner".into(),
            problem: "Exercise transitions deterministically".into(),
            product_behavior_changed: vec!["Build implementation".into()],
            product_behavior_unchanged: Vec::new(),
            technical_behavior_changed: Vec::new(),
            technical_behavior_unchanged: Vec::new(),
            requirements: requirement_ids
                .into_iter()
                .map(|id| ReconciledRequirement {
                    requirement: Requirement {
                        id,
                        text: "Required behavior".into(),
                        acceptance: "Verified".into(),
                        condition: None,
                        governing: false,
                    },
                    source_refs: vec![DiscoverySourceRef {
                        discovery_artifact_id: "discovery-fixture".into(),
                        node_id: None,
                    }],
                    user_clarification: None,
                    frozen_user_constraint: false,
                })
                .collect(),
            discovery_attribution: Vec::new(),
            evidence_synthesis: Vec::new(),
            disagreements: Vec::new(),
            rejected_alternatives: Vec::new(),
            implementation_risks: Vec::new(),
            compatibility_concerns: Vec::new(),
            caveats: Vec::new(),
            technical_suggestions: Vec::new(),
            blocking_issues: Vec::new(),
        };
        let mut files = BTreeMap::new();
        files.insert(
            "reconciled-discovery.json".into(),
            encode(&reconciled).unwrap(),
        );
        let reconciled_ref = store
            .publish_bundle(
                &effort,
                "reconcile",
                ArtifactKind::ReconciledDiscovery,
                "test-reconcile".into(),
                "IMPLEMENTATION_READY".into(),
                Vec::new(),
                provenance("reconcile-test", None),
                files,
            )
            .unwrap();
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        fs::write(
            build_dir.join("implementation-plan.md"),
            "# Detailed plan\n\nImplement the listed requirements.\n",
        )
        .unwrap();
        let plan = BuildPlan {
            schema_version: crate::state::PLAN_VERSION,
            reconciled: reconciled_ref.clone(),
            detailed_plan: "implementation-plan.md".into(),
            phases,
        };
        fs::write(build_dir.join(PLAN_FILE), encode(&plan).unwrap()).unwrap();
        write_config(&build_dir.join(CONFIG_FILE), "codex", "codex");
        Fixture {
            root,
            repo,
            effort,
            store,
            build_dir,
            reconciled: reconciled_ref,
        }
    }

    fn one_phase() -> Vec<crate::state::PlanPhase> {
        vec![crate::state::PlanPhase {
            id: "P1".into(),
            tasks: vec!["T1".into()],
            requirement_ids: vec!["R-1".into()],
        }]
    }

    fn write_config(path: &Path, worker: &str, reviewer: &str) {
        let config = BuildConfig {
            schema_version: crate::state::CONFIG_VERSION,
            worker: RoleConfig {
                adapter: worker.into(),
                model: Some("native-model".into()),
                args: Some(vec!["--search".into()]),
            },
            reviewer: RoleConfig {
                adapter: reviewer.into(),
                model: None,
                args: None,
            },
            unblocker: None,
        };
        fs::write(path, toml::to_string(&config).unwrap()).unwrap();
    }

    fn git_ok(repo: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn id_from_prompt(plan: &adapter::InvocationPlan) -> String {
        let prompt = plan.record.argv.last().unwrap();
        prompt
            .split("action_id: ")
            .nth(1)
            .unwrap()
            .split(|ch: char| ch.is_whitespace() || ch == '.')
            .next()
            .unwrap()
            .to_owned()
    }

    fn packet_from_record(record: &adapter::InvocationRecord) -> serde_json::Value {
        let prompt = record.argv.last().unwrap();
        let packet = prompt.split_once("ACTION PACKET:\n").unwrap().1.trim();
        serde_json::from_str(packet).unwrap()
    }

    #[derive(Clone, Copy, Debug)]
    enum Step {
        WorkComplete,
        WorkBlocked,
        WorkInvalidCommit,
        WorkWrongHead,
        WorkDirtyAfterCommit,
        ReviewPass,
        ReviewChanges,
        ReviewBlocked,
        AuditPass,
        AuditFail,
        AuditUnknown,
        AuditBlocked,
        UnblockRetry,
        UnblockExternal,
        Malformed,
        StaleAction,
        ProviderFailure,
        InvocationError,
        MissingResponse,
    }

    #[derive(Default)]
    struct FakeInvoker {
        steps: Mutex<VecDeque<Step>>,
        records: Mutex<Vec<adapter::InvocationRecord>>,
        repo: Mutex<Option<PathBuf>>,
    }

    impl FakeInvoker {
        fn new(steps: impl IntoIterator<Item = Step>, repo: &Path) -> Self {
            Self {
                steps: Mutex::new(steps.into_iter().collect()),
                records: Mutex::new(Vec::new()),
                repo: Mutex::new(Some(repo.to_path_buf())),
            }
        }
        fn records(&self) -> Vec<adapter::InvocationRecord> {
            self.records.lock().unwrap().clone()
        }
        fn remaining(&self) -> usize {
            self.steps.lock().unwrap().len()
        }
    }

    impl adapter::InvocationApi for FakeInvoker {
        fn invoke(
            &self,
            plan: &adapter::InvocationPlan,
            cwd: &Path,
            _action_dir: &Path,
        ) -> Result<InvocationOutcome> {
            self.records.lock().unwrap().push(plan.record.clone());
            let step = self
                .steps
                .lock()
                .unwrap()
                .pop_front()
                .context("fake adapter has no queued response")?;
            if matches!(step, Step::InvocationError) {
                bail!("fake provider could not be spawned");
            }
            let prompt = plan.record.argv.last().unwrap();
            let action_id = id_from_prompt(plan);
            let gate = if prompt.contains("Gate: Work;") {
                "Work"
            } else if prompt.contains("Gate: Review;") {
                "Review"
            } else if prompt.contains("Gate: Audit;") {
                "Audit"
            } else {
                "Unblock"
            };
            if matches!(gate, "Review" | "Audit" | "Unblock") {
                let packet: serde_json::Value =
                    serde_json::from_slice(&fs::read(cwd.parent().unwrap().join("action.json"))?)?;
                assert_eq!(
                    git_text(cwd, &["rev-parse", "HEAD"])?,
                    packet["checkpoint_commit"].as_str().unwrap()
                );
                assert_eq!(git_text(cwd, &["status", "--porcelain"])?, "");
                assert_ne!(cwd, self.repo.lock().unwrap().as_ref().unwrap());
            }
            let response = match step {
                Step::WorkComplete => {
                    let count = self.records.lock().unwrap().iter().filter(|record| record.argv.last().is_some_and(|prompt| prompt.contains("Gate: Work;"))).count();
                    let filename = format!("work-step-{count}.txt");
                    fs::write(cwd.join(filename), format!("step {count}\n"))?;
                    git_ok(cwd, &["add", "-A"]);
                    git_ok(cwd, &["commit", "-m", &format!("work step {count}")]);
                    json!({"action_id": action_id, "outcome": "complete", "report": "Scoped work is committed and verified.", "commit": git_text(cwd, &["rev-parse", "HEAD"])?}).to_string()
                }
                Step::WorkBlocked => {
                    fs::write(cwd.join("partial-untracked.txt"), "discard this partial work\n")?;
                    json!({"action_id": action_id, "outcome": "blocked", "report": "Work needs a missing external requirement."}).to_string()
                }
                Step::WorkInvalidCommit => json!({"action_id": action_id, "outcome": "complete", "report": "Claimed commit is invalid.", "commit": "not-a-commit"}).to_string(),
                Step::WorkWrongHead | Step::WorkDirtyAfterCommit => {
                    let count = self.records.lock().unwrap().iter().filter(|record| record.argv.last().is_some_and(|prompt| prompt.contains("Gate: Work;"))).count();
                    fs::write(cwd.join(format!("wrong-head-{count}.txt")), "committed\n")?;
                    git_ok(cwd, &["add", "-A"]);
                    git_ok(cwd, &["commit", "-m", "work commit"]);
                    let reported_commit = if matches!(step, Step::WorkWrongHead) { git_text(cwd, &["rev-parse", "HEAD^"])? } else { git_text(cwd, &["rev-parse", "HEAD"])? };
                    if matches!(step, Step::WorkDirtyAfterCommit) { fs::write(cwd.join("left-untracked.txt"), "must be committed\n")?; }
                    json!({"action_id": action_id, "outcome": "complete", "report": "Work commit claimed.", "commit": reported_commit}).to_string()
                }
                Step::ReviewPass => json!({"action_id": action_id, "outcome": "pass", "report": "Review passed the exact checkpoint.", "inspected_commit": git_text(cwd, &["rev-parse", "HEAD"])?}).to_string(),
                Step::ReviewChanges => json!({"action_id": action_id, "outcome": "changes_required", "report": "Correct the reported issue, then repeat review.", "inspected_commit": git_text(cwd, &["rev-parse", "HEAD"])?}).to_string(),
                Step::ReviewBlocked => json!({"action_id": action_id, "outcome": "blocked", "report": "Review lacks an external dependency.", "inspected_commit": git_text(cwd, &["rev-parse", "HEAD"])?}).to_string(),
                Step::AuditPass | Step::AuditFail | Step::AuditUnknown => {
                    let packet: serde_json::Value = serde_json::from_slice(&fs::read(cwd.parent().unwrap().join("action.json"))?)?;
                    let refs = [&packet["reconciled"], &packet["adoption"], &packet["implementation"]];
                    let ids = packet["binding_reconciled"]["requirements"].as_array().unwrap();
                    let mut coverage = ids.iter().enumerate().map(|(index, item)| {
                        let state = match step { Step::AuditFail if index == 0 => "fail", Step::AuditUnknown => "unknown", _ => "pass" };
                        json!({"requirement_id": item["requirement"]["id"], "state": state, "rationale": "Assessment from exact checkpoint", "evidence": ["inspection: exact checkpoint"], "correction": if state == "fail" { "Fix the failed requirement." } else { "" }})
                    }).collect::<Vec<_>>();
                    if matches!(step, Step::AuditUnknown) { coverage.clear(); }
                    let assessment = json!({"reconciled": refs[0], "adoption": refs[1], "implementation": refs[2], "coverage": coverage, "assessor_context": "Inspected exact checkpoint."});
                    json!({"action_id": action_id, "outcome": "complete", "report": "Audit assessment is complete.", "assessment": assessment}).to_string()
                }
                Step::AuditBlocked => json!({"action_id": action_id, "outcome": "blocked", "report": "Audit cannot assess without an external requirement."}).to_string(),
                Step::UnblockRetry => json!({"action_id": action_id, "outcome": "retry", "report": "Retry the same gate after the checkpoint reset."}).to_string(),
                Step::UnblockExternal => json!({"action_id": action_id, "outcome": "external_requirement", "report": "An operator must supply the missing prerequisite."}).to_string(),
                Step::Malformed => "this is not JSON".into(),
                Step::StaleAction => json!({"action_id": "stale-action", "outcome": "blocked", "report": "Stale response."}).to_string(),
                Step::ProviderFailure | Step::InvocationError | Step::MissingResponse => String::new(),
            };
            let failure = matches!(step, Step::ProviderFailure);
            let missing = matches!(step, Step::MissingResponse);
            let response = if matches!(step, Step::StaleAction) {
                // Keep the stale action result structurally valid for the current gate.
                if gate == "Work" {
                    json!({"action_id":"stale-action","outcome":"blocked","report":"Stale response."}).to_string()
                } else {
                    response
                }
            } else {
                response
            };
            Ok(InvocationOutcome {
                success: !failure,
                exit_code: Some(if failure { 7 } else { 0 }),
                final_response: if failure || missing {
                    None
                } else {
                    Some(response.clone())
                },
                observed_session: Some(format!("session-{gate}")),
                stdout: response,
                stderr: if failure {
                    "provider failed".into()
                } else {
                    String::new()
                },
            })
        }
    }

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
    fn plan_digest_binds_both_exact_byte_sequences() {
        assert_ne!(combined_digest(b"ab", b"c"), combined_digest(b"a", b"bc"));
        assert_ne!(
            combined_digest(b"plan", b"detail"),
            combined_digest(b"PLAN", b"detail")
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
    fn initialization_routes_two_phases_then_final_audit_and_reuses_reviewer_session() {
        let phases = vec![
            crate::state::PlanPhase {
                id: "P1".into(),
                tasks: vec!["T1".into()],
                requirement_ids: vec!["R-1".into()],
            },
            crate::state::PlanPhase {
                id: "P2".into(),
                tasks: vec!["T2".into()],
                requirement_ids: vec!["R-2".into()],
            },
        ];
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
        assert_eq!(records[4].session_in.as_deref(), Some("session-Review"));
        for (record, scoped) in [(&records[0], "R-1"), (&records[2], "R-2")] {
            let packet = packet_from_record(record);
            assert_eq!(
                packet["binding_reconciled"]["requirements"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
            assert_eq!(packet["phase"]["requirement_ids"][0], scoped);
        }
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
        assert_eq!(state.stop.unwrap().kind, StopKind::ResetRequired);
    }

    #[test]
    fn review_block_and_unknown_audit_each_use_one_unblock_detour() {
        let fixture = make_fixture(one_phase());
        let fake = FakeInvoker::new(
            [
                Step::WorkComplete,
                Step::ReviewBlocked,
                Step::UnblockRetry,
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
                "Work", "Review", "Unblock", "Review", "Audit", "Unblock", "Audit"
            ]
        );
        assert!(records[2].session_in.is_none() && records[5].session_in.is_none());
        assert_eq!(records[3].session_in.as_deref(), Some("session-Review"));
        assert_eq!(records[6].session_in.as_deref(), Some("session-Audit"));
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
    fn next_explicit_launch_continues_an_external_stop_in_one_action() {
        let fixture = make_fixture(one_phase());
        let fake = FakeInvoker::new(
            [
                Step::WorkBlocked,
                Step::UnblockExternal,
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
                Step::UnblockExternal,
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
        let fake = FakeInvoker::new([Step::WorkBlocked, Step::UnblockExternal], &fixture.repo);
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
    fn reset_restores_tracked_state_deletes_untracked_preserves_ignored_and_clears_sessions() {
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
        state.worker_session = Some(Session {
            adapter: "codex".into(),
            id: "w".into(),
        });
        state.reviewer_session = Some(Session {
            adapter: "codex".into(),
            id: "r".into(),
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
        assert!(reset_state.worker_session.is_none() && reset_state.reviewer_session.is_none());
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
                    fixture.build_dir.join("implementation-plan.md"),
                    "changed after initialization\n",
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

    fn request(fixture: &Fixture) -> BuildRequest {
        BuildRequest {
            effort: Some(fixture.effort.id.clone()),
            project: fixture.repo.clone(),
        }
    }
}
