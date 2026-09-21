//! The small deterministic controller behind `orchestrate build`.
//!
//! This crate deliberately owns transitions and durable controller state, but
//! not engineering judgement.  Provider conversations write one receipt per
//! action; the controller validates that receipt before selecting the next
//! fixed action.

use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail, ensure};
use orchestrate_contracts::{
    ArtifactKind, ArtifactRef, AuditAssessment, ImplementationStatus, Independence, Provenance,
    Verdict, decode, digest_bytes, safe_relative_path,
};
use orchestrate_core::{Effort, Project, Store, write_bytes_sync};
use serde::{Deserialize, Serialize};

static ACTION_COUNTER: AtomicU64 = AtomicU64::new(0);
pub const BUILD_PLAN_VERSION: u32 = 1;
pub const BUILD_CONFIG_VERSION: u32 = 1;
pub const BUILD_STATE_VERSION: u32 = 1;

const WORK_GUIDE: &str = include_str!("../resources/work.md");
const REVIEW_GUIDE: &str = include_str!("../resources/review.md");
const FOLLOW_UP_GUIDE: &str = include_str!("../resources/work-follow-up.md");
const REVIEW_FOLLOW_UP_GUIDE: &str = include_str!("../resources/review-follow-up.md");
const UNBLOCK_GUIDE: &str = include_str!("../resources/unblock.md");

#[derive(Clone, Debug)]
pub struct BuildRequest {
    pub effort: Option<String>,
    pub project: PathBuf,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BuildCompletion {
    pub implementation: ArtifactRef,
    pub audit: ArtifactRef,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum BuildResult {
    Completed(BuildCompletion),
    Blocked { detail: String, state: PathBuf },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildPlan {
    pub schema_version: u32,
    pub adoption: ArtifactRef,
    pub detailed_plan: String,
    pub delivery_phases: Vec<DeliveryPhase>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DeliveryPhase {
    pub id: String,
    pub tasks: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildConfig {
    pub schema_version: u32,
    pub worker: RoleConfig,
    pub reviewer: RoleConfig,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RoleConfig {
    pub adapter: String,
    #[serde(default)]
    pub provider: Option<String>,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub profile: Option<String>,
    #[serde(default)]
    pub session_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ActionKind {
    Work,
    Review,
    WorkFollowUp,
    ReviewFollowUp,
    FinalAudit,
    FinalFollowUp,
    Unblock,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DispatchState {
    Prepared,
    Sent,
}

fn prepared_dispatch() -> DispatchState {
    DispatchState::Prepared
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct FrozenInputs {
    adoption: ArtifactRef,
    reconciled: ArtifactRef,
    plan_digest: String,
    detailed_plan_digest: String,
    config_digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct CurrentAction {
    id: String,
    kind: ActionKind,
    scope: String,
    #[serde(default)]
    target_commit: Option<String>,
    result_path: String,
    report_path: String,
    #[serde(default)]
    feedback_path: Option<String>,
    #[serde(default = "prepared_dispatch")]
    dispatch: DispatchState,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct Sessions {
    #[serde(default)]
    worker: Option<String>,
    #[serde(default)]
    reviewer: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
struct Recovery {
    #[serde(default)]
    continuation_used: bool,
    #[serde(default)]
    replacement_used: bool,
    #[serde(default)]
    unblock_used: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct BuildState {
    schema_version: u32,
    frozen: FrozenInputs,
    phase_index: usize,
    action: CurrentAction,
    #[serde(default)]
    sessions: Sessions,
    #[serde(default)]
    recovery: Recovery,
    #[serde(default)]
    implementation: Option<ArtifactRef>,
    #[serde(default)]
    terminal: Option<BuildCompletion>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    action_id: String,
    scope: String,
    outcome: String,
    #[serde(default)]
    commit: Option<String>,
}

/// Narrow adapter boundary.  Concrete adapters only carry transport and session
/// concerns; they cannot influence controller routing.
pub trait HostAdapter {
    fn invoke(&self, request: &Invocation) -> Result<InvocationResult>;
}

#[derive(Clone, Debug)]
pub struct Invocation {
    pub adapter: String,
    pub role: String,
    pub config: RoleConfig,
    pub cwd: PathBuf,
    pub build_dir: PathBuf,
    pub action: PathBuf,
    pub session_id: Option<String>,
    pub prompt: String,
}

#[derive(Clone, Debug)]
pub struct InvocationResult {
    pub session_id: Option<String>,
}

/// Run a prepared Build using concrete local CLI adapters.
pub fn run(store: &Store, request: BuildRequest) -> Result<BuildResult> {
    run_with_adapter(store, request, &CommandAdapter)
}

/// Dependency-injected entry point used by deterministic tests.  It is public
/// so downstream hosts can test their own supported transport without a model.
pub fn run_with_adapter(
    store: &Store,
    request: BuildRequest,
    adapter: &dyn HostAdapter,
) -> Result<BuildResult> {
    let project_path = fs::canonicalize(&request.project)
        .with_context(|| format!("cannot canonicalize project {}", request.project.display()))?;
    let project = store
        .project_by_locator(&project_path)?
        .context("no Orchestrate project matches the current canonical repository")?;
    let effort = select_effort(store, &project, request.effort.as_deref())?;
    let _lock = ProjectLock::acquire(store, &project)?;
    let build_dir = store.phase_dir(&effort, "build")?;
    let config = load_config(&build_dir)?;
    let plan = load_plan(store, &effort, &build_dir)?;
    materialize_role_guides(&build_dir)?;
    let state_path = build_dir.join("state.json");
    let mut state = if state_path.exists() {
        load_state(&state_path, &config, &plan, &build_dir)?
    } else {
        initialize_state(store, &effort, &build_dir, &config, &plan)?
    };
    if let Some(done) = &state.terminal {
        return Ok(BuildResult::Completed(done.clone()));
    }

    loop {
        let result = perform_action(
            store,
            &effort,
            &project,
            &build_dir,
            &config,
            &plan,
            &state_path,
            &mut state,
            adapter,
        );
        match result {
            Ok(()) => {
                save_state(&state_path, &state)?;
                if let Some(done) = &state.terminal {
                    return Ok(BuildResult::Completed(done.clone()));
                }
            }
            Err(error) => {
                // Preserve a typed stopped position.  We do not overwrite the
                // receipt or invent a succeeding transition.
                save_state(&state_path, &state)?;
                return Ok(BuildResult::Blocked {
                    detail: format!("{error:#}"),
                    state: state_path,
                });
            }
        }
    }
}

fn select_effort(store: &Store, project: &Project, explicit: Option<&str>) -> Result<Effort> {
    if let Some(id) = explicit {
        let effort = store.load_effort(id)?;
        ensure!(
            effort.project_id == project.id,
            "effort {id} belongs to another project"
        );
        return Ok(effort);
    }
    let mut active = Vec::new();
    let mut completed = Vec::new();
    for effort in store.efforts_for_project(project)? {
        let build = store
            .effort_dir(&effort.project_id, &effort.id)
            .join("build");
        if !build.join("config.toml").is_file() || !build.join("plan.json").is_file() {
            continue;
        }
        if let Ok(state) = read_json::<BuildState>(&build.join("state.json")) {
            if state.terminal.is_some() {
                completed.push(effort);
            } else {
                active.push(effort);
            }
        } else {
            active.push(effort);
        }
    }
    match active.as_slice() {
        [only] => Ok(only.clone()),
        [] => match completed.as_slice() {
            [only] => Ok(only.clone()),
            [] => bail!("no prepared Build exists for this repository"),
            many => bail!(
                "multiple completed Build efforts exist: {}; pass --effort",
                effort_ids(many)
            ),
        },
        many => bail!(
            "multiple prepared or active Build efforts exist: {}; pass --effort",
            effort_ids(many)
        ),
    }
}

fn effort_ids(efforts: &[Effort]) -> String {
    efforts
        .iter()
        .map(|e| e.id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

fn load_config(build_dir: &Path) -> Result<BuildConfig> {
    let text = fs::read_to_string(build_dir.join("config.toml"))?;
    let config: BuildConfig = toml::from_str(&text).context("invalid build/config.toml")?;
    ensure!(
        config.schema_version == BUILD_CONFIG_VERSION,
        "unsupported Build config version"
    );
    validate_role("worker", &config.worker)?;
    validate_role("reviewer", &config.reviewer)?;
    Ok(config)
}

fn validate_role(role: &str, config: &RoleConfig) -> Result<()> {
    ensure!(!config.adapter.is_empty(), "{role} adapter is required");
    ensure!(
        matches!(
            config.adapter.as_str(),
            "codex" | "claude" | "cursor" | "opencode"
        ),
        "unsupported {role} adapter {}",
        config.adapter
    );
    if config.adapter == "opencode" {
        ensure!(
            config.provider.as_deref().is_some_and(|v| !v.is_empty()),
            "OpenCode {role} requires provider"
        );
        ensure!(
            config.model.as_deref().is_some_and(|v| !v.is_empty()),
            "OpenCode {role} requires model"
        );
    }
    Ok(())
}

fn load_plan(store: &Store, effort: &Effort, build_dir: &Path) -> Result<BuildPlan> {
    let plan: BuildPlan = read_json(&build_dir.join("plan.json"))?;
    ensure!(
        plan.schema_version == BUILD_PLAN_VERSION,
        "unsupported Build plan version"
    );
    ensure!(
        safe_relative_path(&plan.detailed_plan),
        "detailed_plan must be a safe relative Build path"
    );
    ensure!(
        build_dir.join(&plan.detailed_plan).is_file(),
        "referenced detailed plan does not exist"
    );
    ensure!(
        plan.adoption.kind == ArtifactKind::Adoption,
        "Build plan needs an Adoption reference"
    );
    let (_, adoption): (_, orchestrate_contracts::Adoption) =
        store.load_json(effort, &plan.adoption, "adoption.json")?;
    ensure!(
        adoption.reconciled.kind == ArtifactKind::ReconciledDiscovery,
        "Adoption has invalid reconciled authority"
    );
    let mut phases = HashSet::new();
    let mut tasks = HashSet::new();
    ensure!(
        !plan.delivery_phases.is_empty(),
        "Build plan needs at least one delivery phase"
    );
    for phase in &plan.delivery_phases {
        ensure!(
            valid_id(&phase.id),
            "invalid delivery phase id {}",
            phase.id
        );
        ensure!(
            phases.insert(&phase.id),
            "duplicate delivery phase id {}",
            phase.id
        );
        ensure!(
            !phase.tasks.is_empty(),
            "delivery phase {} has no tasks",
            phase.id
        );
        for task in &phase.tasks {
            ensure!(valid_id(task), "invalid task id {task}");
            ensure!(
                tasks.insert(task),
                "task {task} occurs in more than one delivery phase"
            );
        }
    }
    Ok(plan)
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn initialize_state(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    config: &BuildConfig,
    plan: &BuildPlan,
) -> Result<BuildState> {
    let (_, adoption): (_, orchestrate_contracts::Adoption) =
        store.load_json(effort, &plan.adoption, "adoption.json")?;
    let phase = &plan.delivery_phases[0].id;
    let state = BuildState {
        schema_version: BUILD_STATE_VERSION,
        frozen: FrozenInputs {
            adoption: plan.adoption.clone(),
            reconciled: adoption.reconciled,
            plan_digest: digest_file(&build_dir.join("plan.json"))?,
            detailed_plan_digest: digest_file(&build_dir.join(&plan.detailed_plan))?,
            config_digest: digest_file(&build_dir.join("config.toml"))?,
        },
        phase_index: 0,
        action: new_action(build_dir, ActionKind::Work, phase, None, None),
        sessions: Sessions {
            worker: config.worker.session_id.clone(),
            reviewer: config.reviewer.session_id.clone(),
        },
        recovery: Recovery::default(),
        implementation: None,
        terminal: None,
    };
    save_state(&build_dir.join("state.json"), &state)?;
    Ok(state)
}

fn load_state(
    path: &Path,
    config: &BuildConfig,
    plan: &BuildPlan,
    build_dir: &Path,
) -> Result<BuildState> {
    let state: BuildState = read_json(path)?;
    ensure!(
        state.schema_version == BUILD_STATE_VERSION,
        "unsupported Build state version"
    );
    ensure!(
        state.frozen.plan_digest == digest_file(&build_dir.join("plan.json"))?,
        "Build plan changed after execution began"
    );
    ensure!(
        state.frozen.config_digest == digest_file(&build_dir.join("config.toml"))?,
        "Build config changed after execution began"
    );
    ensure!(
        state.frozen.detailed_plan_digest == digest_file(&build_dir.join(&plan.detailed_plan))?,
        "detailed implementation plan changed after execution began"
    );
    ensure!(
        state.sessions.worker.as_deref() == config.worker.session_id.as_deref()
            || config.worker.session_id.is_none(),
        "worker session differs from frozen runtime state"
    );
    ensure!(
        state.sessions.reviewer.as_deref() == config.reviewer.session_id.as_deref()
            || config.reviewer.session_id.is_none(),
        "reviewer session differs from frozen runtime state"
    );
    Ok(state)
}

fn perform_action(
    store: &Store,
    effort: &Effort,
    project: &Project,
    build_dir: &Path,
    config: &BuildConfig,
    plan: &BuildPlan,
    state_path: &Path,
    state: &mut BuildState,
    adapter: &dyn HostAdapter,
) -> Result<()> {
    if matches!(state.action.kind, ActionKind::FinalAudit) && state.implementation.is_none() {
        let commit = git(&project.canonical_locator, ["rev-parse", "HEAD"])?;
        let implementation =
            orchestrate_audit::find_implementation(store, effort, &state.frozen.adoption, &commit)?
                .unwrap_or(orchestrate_audit::register_implementation_with_run_id(
                    store,
                    effort,
                    state.frozen.adoption.clone(),
                    &commit,
                    "unattended build".into(),
                    ImplementationStatus::Submitted,
                    build_provenance("build"),
                    format!(
                        "build-{}",
                        digest_bytes(
                            format!("{}:{commit}", state.frozen.adoption.digest).as_bytes()
                        )
                    ),
                )?);
        state.implementation = Some(implementation);
        state.action.target_commit = Some(commit);
    }
    let role = match state.action.kind {
        ActionKind::Work | ActionKind::WorkFollowUp | ActionKind::FinalFollowUp => "worker",
        ActionKind::Review
        | ActionKind::ReviewFollowUp
        | ActionKind::FinalAudit
        | ActionKind::Unblock => "reviewer",
    };
    let role_config = if role == "worker" {
        &config.worker
    } else {
        &config.reviewer
    };
    let action_dir = build_dir.join("artifacts").join(&state.action.id);
    fs::create_dir_all(&action_dir)?;
    let completion_marker = action_dir.join("transport.completed");
    if state.action.dispatch == DispatchState::Sent && !completion_marker.is_file() {
        bail!(
            "provider acceptance for action {} is uncertain; it will not be resent automatically",
            state.action.id
        );
    }
    let cwd = match state.action.kind {
        ActionKind::Review | ActionKind::ReviewFollowUp => review_checkout(
            project,
            &action_dir,
            state
                .action
                .target_commit
                .as_deref()
                .context("review lacks target commit")?,
        )?,
        _ => project.canonical_locator.clone(),
    };
    write_action_file(build_dir, plan, state, &action_dir, role, &cwd)?;
    let session = if role == "worker" {
        state.sessions.worker.clone()
    } else {
        state.sessions.reviewer.clone()
    };
    let invocation = Invocation {
        adapter: role_config.adapter.clone(),
        role: role.into(),
        config: role_config.clone(),
        cwd,
        build_dir: build_dir.to_path_buf(),
        action: action_dir.clone(),
        session_id: session,
        prompt: invocation_prompt(&state.action),
    };
    if state.action.dispatch == DispatchState::Prepared {
        state.action.dispatch = DispatchState::Sent;
        save_state(state_path, state)?;
        let invoked = adapter.invoke(&invocation)?;
        if role == "worker" && state.sessions.worker.is_none() {
            state.sessions.worker = invoked.session_id.clone();
        }
        if role == "reviewer"
            && !matches!(
                state.action.kind,
                ActionKind::FinalAudit | ActionKind::Unblock
            )
            && state.sessions.reviewer.is_none()
        {
            state.sessions.reviewer = invoked.session_id;
        }
        write_bytes_sync(&completion_marker, b"completed\n")?;
    }
    let receipt: Receipt = read_json(&action_dir.join("result.json"))
        .context("provider completed without a valid Build result receipt")?;
    consume_receipt(
        store,
        effort,
        build_dir,
        config,
        plan,
        state,
        &receipt,
        &action_dir,
    )
}

fn write_action_file(
    build_dir: &Path,
    plan: &BuildPlan,
    state: &BuildState,
    action_dir: &Path,
    role: &str,
    cwd: &Path,
) -> Result<()> {
    let value = serde_json::json!({
        "action_id": state.action.id,
        "kind": state.action.kind,
        "scope": state.action.scope,
        "role": role,
        "target_commit": state.action.target_commit,
        "feedback": state.action.feedback_path,
        "implementation": state.implementation,
        "result": action_dir.join("result.json"),
        "report": action_dir.join("report.md"),
        "detailed_plan": build_dir.join(&plan.detailed_plan),
        "working_directory": cwd,
        "controller_state": build_dir.join("state.json"),
    });
    write_bytes_sync(
        &action_dir.join("action.json"),
        &serde_json::to_vec_pretty(&value)?,
    )
}

fn invocation_prompt(action: &CurrentAction) -> String {
    let trigger = match action.kind {
        ActionKind::Work => "$work",
        ActionKind::Review => "$review",
        ActionKind::WorkFollowUp | ActionKind::FinalFollowUp => "$work-follow-up",
        ActionKind::ReviewFollowUp => "$review-follow-up",
        ActionKind::FinalAudit => "$audit",
        ActionKind::Unblock => "Read the unblock guide and diagnose only the current blocker.",
    };
    format!(
        "{trigger}\nRead the controller-generated action.json in the Build artifacts directory. Follow its role guide, write report.md and result.json exactly there, and do not edit state.json."
    )
}

fn consume_receipt(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    _config: &BuildConfig,
    plan: &BuildPlan,
    state: &mut BuildState,
    receipt: &Receipt,
    action_dir: &Path,
) -> Result<()> {
    ensure!(
        receipt.action_id == state.action.id,
        "stale or foreign action receipt"
    );
    ensure!(
        receipt.scope == state.action.scope,
        "receipt scope does not match current action"
    );
    match state.action.kind {
        ActionKind::Work | ActionKind::WorkFollowUp | ActionKind::FinalFollowUp => {
            ensure!(
                matches!(
                    receipt.outcome.as_str(),
                    "complete" | "incomplete" | "blocked"
                ),
                "worker receipt has invalid outcome"
            );
            if receipt.outcome == "complete" {
                let commit = receipt
                    .commit
                    .as_deref()
                    .context("complete worker receipt needs commit")?;
                ensure!(
                    git(
                        &store.project_for(effort)?.canonical_locator,
                        ["rev-parse", "HEAD"]
                    )? == commit,
                    "worker receipt commit is not current HEAD"
                );
                ensure!(
                    git(
                        &store.project_for(effort)?.canonical_locator,
                        ["status", "--porcelain", "--untracked-files=no"]
                    )?
                    .is_empty(),
                    "worker left tracked changes outside submitted commit"
                );
                state.recovery = Recovery::default();
                let was_final_follow_up = matches!(state.action.kind, ActionKind::FinalFollowUp);
                let next_kind = if was_final_follow_up {
                    ActionKind::FinalAudit
                } else if matches!(state.action.kind, ActionKind::WorkFollowUp) {
                    ActionKind::ReviewFollowUp
                } else {
                    ActionKind::Review
                };
                if was_final_follow_up {
                    state.implementation = None;
                }
                state.action = new_action(
                    build_dir,
                    next_kind,
                    &state.action.scope,
                    Some(commit.into()),
                    Some(action_dir.join("report.md")),
                );
            } else {
                recover_or_block(build_dir, state, &receipt.outcome, "worker")?;
            }
        }
        ActionKind::Review | ActionKind::ReviewFollowUp => {
            ensure!(
                matches!(
                    receipt.outcome.as_str(),
                    "pass" | "changes_required" | "blocked"
                ),
                "review receipt has invalid outcome"
            );
            let expected = state
                .action
                .target_commit
                .as_deref()
                .context("review lacks target commit")?;
            ensure!(
                receipt.commit.as_deref() == Some(expected),
                "review receipt did not inspect exact submitted commit"
            );
            if receipt.outcome == "pass" {
                state.recovery = Recovery::default();
                if state.phase_index + 1 == plan.delivery_phases.len() {
                    state.action = new_action(
                        build_dir,
                        ActionKind::FinalAudit,
                        "final",
                        Some(expected.into()),
                        Some(action_dir.join("report.md")),
                    );
                } else {
                    state.phase_index += 1;
                    let scope = &plan.delivery_phases[state.phase_index].id;
                    state.action = new_action(build_dir, ActionKind::Work, scope, None, None);
                }
            } else if receipt.outcome == "changes_required" {
                state.action = new_action(
                    build_dir,
                    ActionKind::WorkFollowUp,
                    &state.action.scope,
                    Some(expected.into()),
                    Some(action_dir.join("report.md")),
                );
            } else {
                recover_or_block(build_dir, state, "blocked", "reviewer")?;
            }
        }
        ActionKind::FinalAudit => {
            ensure!(
                matches!(receipt.outcome.as_str(), "complete" | "blocked"),
                "Audit action receipt has invalid outcome"
            );
            if receipt.outcome == "blocked" {
                recover_or_block(build_dir, state, "blocked", "reviewer")?;
                return Ok(());
            }
            let implementation = state
                .implementation
                .clone()
                .context("final Audit implementation was not registered")?;
            let assessment: AuditAssessment = read_json(&action_dir.join("assessment.json"))
                .context("final Audit must write assessment.json beside its receipt")?;
            ensure!(
                assessment.reconciled == state.frozen.reconciled
                    && assessment.adoption == state.frozen.adoption
                    && assessment.implementation == implementation,
                "Audit assessment lineage does not match current Build"
            );
            let audit = orchestrate_audit::finalize_audit_with_run_id(
                store,
                effort,
                assessment.clone(),
                build_provenance("audit"),
                format!("build-audit-{}", state.action.id),
            )?;
            let (_, report): (_, orchestrate_contracts::AuditReport) =
                store.load_json(effort, &audit, "audit.json")?;
            match report.verdict {
                Verdict::Pass => {
                    state.terminal = Some(BuildCompletion {
                        implementation,
                        audit,
                    })
                }
                Verdict::ChangesRequired => {
                    state.action = new_action(
                        build_dir,
                        ActionKind::FinalFollowUp,
                        "final",
                        state.action.target_commit.clone(),
                        Some(action_dir.join("assessment.json")),
                    )
                }
                Verdict::Blocked => recover_or_block(build_dir, state, "blocked", "reviewer")?,
            }
        }
        ActionKind::Unblock => {
            ensure!(
                matches!(receipt.outcome.as_str(), "complete" | "blocked"),
                "unblock receipt has invalid outcome"
            );
            if receipt.outcome == "complete" {
                bail!(
                    "unblocker recorded a remedy; rerun `orchestrate build` to retry the preserved action"
                )
            }
            bail!(
                "unblocker identified an external requirement; see {}",
                action_dir.join("report.md").display()
            )
        }
    }
    Ok(())
}

fn recover_or_block(
    build_dir: &Path,
    state: &mut BuildState,
    outcome: &str,
    role: &str,
) -> Result<()> {
    if outcome == "blocked" && !state.recovery.unblock_used {
        state.recovery.unblock_used = true;
        state.action = new_action(
            build_dir,
            ActionKind::Unblock,
            &state.action.scope,
            state.action.target_commit.clone(),
            state.action.feedback_path.as_ref().map(PathBuf::from),
        );
        return Ok(());
    }
    if !state.recovery.continuation_used {
        state.recovery.continuation_used = true;
        state.action = new_action(
            build_dir,
            state.action.kind.clone(),
            &state.action.scope,
            state.action.target_commit.clone(),
            state.action.feedback_path.as_ref().map(PathBuf::from),
        );
        return Ok(());
    }
    if !state.recovery.replacement_used {
        state.recovery.replacement_used = true;
        if role == "worker" {
            state.sessions.worker = None;
        } else {
            state.sessions.reviewer = None;
        }
        state.action = new_action(
            build_dir,
            state.action.kind.clone(),
            &state.action.scope,
            state.action.target_commit.clone(),
            state.action.feedback_path.as_ref().map(PathBuf::from),
        );
        return Ok(());
    }
    if !state.recovery.unblock_used {
        state.recovery.unblock_used = true;
        state.action = new_action(
            build_dir,
            ActionKind::Unblock,
            &state.action.scope,
            state.action.target_commit.clone(),
            state.action.feedback_path.as_ref().map(PathBuf::from),
        );
        return Ok(());
    }
    bail!("recovery is exhausted for scope {}", state.action.scope)
}

fn new_action(
    build_dir: &Path,
    kind: ActionKind,
    scope: &str,
    target_commit: Option<String>,
    feedback: Option<PathBuf>,
) -> CurrentAction {
    let id = format!(
        "act-{}-{}-{}",
        now_ms(),
        std::process::id(),
        ACTION_COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    let root = build_dir.join("artifacts").join(&id);
    CurrentAction {
        id,
        kind,
        scope: scope.into(),
        target_commit,
        result_path: root.join("result.json").to_string_lossy().into_owned(),
        report_path: root.join("report.md").to_string_lossy().into_owned(),
        feedback_path: feedback.map(|p| p.to_string_lossy().into_owned()),
        dispatch: DispatchState::Prepared,
    }
}

fn review_checkout(project: &Project, action_dir: &Path, commit: &str) -> Result<PathBuf> {
    let source = action_dir.join("source");
    if source.exists() {
        return Ok(source);
    }
    let clone = Command::new("git")
        .args(["clone", "--shared", "--no-checkout"])
        .arg(&project.canonical_locator)
        .arg(&source)
        .output()?;
    ensure!(
        clone.status.success(),
        "cannot create contained review checkout: {}",
        String::from_utf8_lossy(&clone.stderr)
    );
    let checkout = Command::new("git")
        .args(["checkout", "--detach", commit])
        .current_dir(&source)
        .output()?;
    ensure!(
        checkout.status.success(),
        "cannot check out review target: {}",
        String::from_utf8_lossy(&checkout.stderr)
    );
    Ok(source)
}

fn materialize_role_guides(build_dir: &Path) -> Result<()> {
    for (name, body) in [
        ("work.md", WORK_GUIDE),
        ("review.md", REVIEW_GUIDE),
        ("work-follow-up.md", FOLLOW_UP_GUIDE),
        ("review-follow-up.md", REVIEW_FOLLOW_UP_GUIDE),
        ("unblock.md", UNBLOCK_GUIDE),
    ] {
        let path = build_dir.join("instructions").join(name);
        if !path.exists() {
            write_bytes_sync(&path, body.as_bytes())?;
        }
    }
    Ok(())
}

fn build_provenance(host: &str) -> Provenance {
    Provenance {
        host: host.into(),
        provider: None,
        model: None,
        model_effort: None,
        guide_digest: digest_bytes(b"orchestrate-build"),
        independence: Independence::InputExcluded,
    }
}

fn save_state(path: &Path, state: &BuildState) -> Result<()> {
    write_bytes_sync(path, &serde_json::to_vec_pretty(state)?)
}
fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T> {
    decode(&fs::read(path).with_context(|| format!("cannot read {}", path.display()))?)
}
fn digest_file(path: &Path) -> Result<String> {
    Ok(digest_bytes(&fs::read(path)?))
}
fn now_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis())
}
fn git<const N: usize>(repo: &Path, args: [&str; N]) -> Result<String> {
    let output = Command::new("git").args(args).current_dir(repo).output()?;
    ensure!(
        output.status.success(),
        "git command failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8(output.stdout)?.trim().into())
}

struct ProjectLock {
    path: PathBuf,
}
impl ProjectLock {
    fn acquire(store: &Store, project: &Project) -> Result<Self> {
        let path = store
            .root()
            .join("projects")
            .join(&project.id)
            .join(".build-controller.lock");
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(mut file) => {
                writeln!(file, "{}", std::process::id())?;
                file.sync_all()?;
                Ok(Self { path })
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                let pid = fs::read_to_string(&path)
                    .ok()
                    .and_then(|v| v.trim().parse::<u32>().ok());
                if pid.is_some_and(process_alive) {
                    bail!("another Build driver controls this checkout")
                }
                fs::remove_file(&path).context("cannot clear stale Build controller lock")?;
                Self::acquire(store, project)
            }
            Err(error) => Err(error.into()),
        }
    }
}
impl Drop for ProjectLock {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.path);
    }
}
fn process_alive(pid: u32) -> bool {
    Command::new("kill")
        .args(["-0", &pid.to_string()])
        .status()
        .is_ok_and(|s| s.success())
}

struct CommandAdapter;
impl HostAdapter for CommandAdapter {
    fn invoke(&self, request: &Invocation) -> Result<InvocationResult> {
        let mut command = provider_command(request)?;
        let output = command
            .output()
            .with_context(|| format!("cannot launch {}", request.adapter))?;
        let log = request.action.join("transport.jsonl");
        write_bytes_sync(&log, &output.stdout)?;
        ensure!(
            output.status.success(),
            "{} invocation failed: {}",
            request.adapter,
            String::from_utf8_lossy(&output.stderr)
        );
        let session_id = parse_session_id(&output.stdout);
        Ok(InvocationResult { session_id })
    }
}

fn provider_command(request: &Invocation) -> Result<Command> {
    let mut command = match request.adapter.as_str() {
        "codex" => {
            let mut c = Command::new("codex");
            c.args(["exec", "--json", "-C"])
                .arg(&request.cwd)
                .arg("--add-dir")
                .arg(&request.build_dir);
            if let Some(session) = &request.session_id {
                c.args(["resume", session]);
            }
            c
        }
        "claude" => {
            let mut c = Command::new("claude");
            c.args(["-p", "--output-format", "stream-json"]);
            if let Some(session) = &request.session_id {
                c.args(["--resume", session]);
            }
            c.current_dir(&request.cwd);
            c
        }
        "cursor" => {
            let mut c = Command::new("cursor-agent");
            c.args(["-p", "--output-format", "stream-json"]);
            if let Some(session) = &request.session_id {
                c.args(["--resume", session]);
            }
            c.current_dir(&request.cwd);
            c
        }
        "opencode" => {
            let mut c = Command::new("opencode");
            c.args(["run", "--format", "json", "--dir"])
                .arg(&request.cwd);
            if let Some(session) = &request.session_id {
                c.args(["--session", session]);
            }
            if let (Some(provider), Some(model)) = (&request.config.provider, &request.config.model)
            {
                c.args(["--model", &format!("{provider}/{model}")]);
            }
            c
        }
        other => bail!("unsupported adapter {other}"),
    };
    if let Some(model) = &request.config.model {
        if request.adapter != "opencode" {
            command.args(["--model", model]);
        }
    }
    command.arg(&request.prompt);
    Ok(command)
}

fn parse_session_id(bytes: &[u8]) -> Option<String> {
    for line in String::from_utf8_lossy(bytes).lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        for key in ["session_id", "thread_id", "chat_id"] {
            if let Some(value) = value.get(key).and_then(|v| v.as_str()) {
                return Some(value.into());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use orchestrate_contracts::{
        AuditAssessment, Coverage, CoverageState, ReconciledDiscovery, ReconciledRequirement,
        RequestKind, Requirement,
    };
    use std::{collections::BTreeMap, process::Command};

    #[test]
    fn session_ids_are_read_from_provider_events() {
        assert_eq!(
            parse_session_id(br#"{"type":"thread.started","thread_id":"t-1"}"#),
            Some("t-1".into())
        );
        assert_eq!(
            parse_session_id(br#"{"session_id":"s-1"}"#),
            Some("s-1".into())
        );
    }
    #[test]
    fn plan_ids_are_restricted() {
        assert!(valid_id("D-1"));
        assert!(!valid_id("D 1"));
    }
    #[test]
    fn named_adapters_build_supported_local_commands() {
        let root = temp("commands");
        for (adapter, program) in [
            ("codex", "codex"),
            ("claude", "claude"),
            ("cursor", "cursor-agent"),
            ("opencode", "opencode"),
        ] {
            let request = Invocation {
                adapter: adapter.into(),
                role: "worker".into(),
                config: RoleConfig {
                    adapter: adapter.into(),
                    provider: (adapter == "opencode").then(|| "openai".into()),
                    model: Some("model".into()),
                    profile: None,
                    session_id: None,
                },
                cwd: root.clone(),
                build_dir: root.clone(),
                action: root.clone(),
                session_id: Some("session".into()),
                prompt: "work".into(),
            };
            assert_eq!(provider_command(&request).unwrap().get_program(), program);
        }
    }

    fn temp(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("orchestrate-build-{name}-{}", now_ms()));
        fs::create_dir_all(&path).unwrap();
        path
    }
    fn git_ok(repo: &Path, args: &[&str]) {
        assert!(
            Command::new("git")
                .args(args)
                .current_dir(repo)
                .status()
                .unwrap()
                .success()
        );
    }
    fn prepared() -> (Store, Effort, PathBuf, ArtifactRef, ArtifactRef) {
        let root = temp("store");
        let repo = temp("repo");
        git_ok(&repo, &["init"]);
        git_ok(&repo, &["config", "user.email", "test@example.com"]);
        git_ok(&repo, &["config", "user.name", "Test"]);
        fs::write(repo.join("source.txt"), "baseline\n").unwrap();
        git_ok(&repo, &["add", "."]);
        git_ok(&repo, &["commit", "-m", "baseline"]);
        let store = Store::open(&root).unwrap();
        let effort = store
            .init_effort(&repo, "build", RequestKind::Freeform, "work".into(), vec![])
            .unwrap();
        let reconciled = ReconciledDiscovery {
            reconciled_id: "r".into(),
            context_id: effort.context.id.clone(),
            baseline_commit: effort.baseline_commit.clone(),
            baseline_tree: effort.baseline_tree.clone(),
            goal: "test".into(),
            core_result: "test".into(),
            requirements: vec![ReconciledRequirement {
                requirement: Requirement {
                    id: "R-1".into(),
                    text: "works".into(),
                    acceptance: "test".into(),
                    condition: None,
                    governing: false,
                },
                source_refs: vec![],
                user_clarification: Some("test".into()),
                frozen_user_constraint: false,
            }],
            technical_suggestions: vec![],
            blocking_issues: vec![],
        };
        let mut files = BTreeMap::new();
        files.insert(
            "reconciled-discovery.json".into(),
            orchestrate_contracts::encode(&reconciled).unwrap(),
        );
        let reconciled_ref = store
            .publish_bundle(
                &effort,
                "reconcile",
                ArtifactKind::ReconciledDiscovery,
                "ready".into(),
                "IMPLEMENTATION_READY".into(),
                vec![],
                build_provenance("test"),
                files,
            )
            .unwrap();
        let adoption = orchestrate_audit::adopt(
            &store,
            &effort,
            reconciled_ref.clone(),
            "test".into(),
            build_provenance("test"),
        )
        .unwrap();
        let build = store.phase_dir(&effort, "build").unwrap();
        fs::write(build.join("implementation-plan.md"), "# Plan\n").unwrap();
        fs::write(
            build.join("config.toml"),
            "schema_version = 1\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"cursor\"\n",
        )
        .unwrap();
        let plan = BuildPlan {
            schema_version: 1,
            adoption: adoption.clone(),
            detailed_plan: "implementation-plan.md".into(),
            delivery_phases: vec![
                DeliveryPhase {
                    id: "D1".into(),
                    tasks: vec!["T1".into()],
                },
                DeliveryPhase {
                    id: "D2".into(),
                    tasks: vec!["T2".into()],
                },
            ],
        };
        write_bytes_sync(
            &build.join("plan.json"),
            &orchestrate_contracts::encode(&plan).unwrap(),
        )
        .unwrap();
        (store, effort, repo, reconciled_ref, adoption)
    }
    struct FakeHost;
    impl HostAdapter for FakeHost {
        fn invoke(&self, invocation: &Invocation) -> Result<InvocationResult> {
            let action: serde_json::Value = read_json(&invocation.action.join("action.json"))?;
            let kind = action["kind"].as_str().unwrap();
            let action_id = action["action_id"].as_str().unwrap();
            let scope = action["scope"].as_str().unwrap();
            let receipt = match kind {
                "work" => {
                    fs::write(invocation.cwd.join(format!("{scope}.txt")), scope)?;
                    git_ok(&invocation.cwd, &["add", "."]);
                    git_ok(&invocation.cwd, &["commit", "-m", scope]);
                    serde_json::json!({"action_id": action_id, "scope": scope, "outcome": "complete", "commit": git(&invocation.cwd, ["rev-parse", "HEAD"])?})
                }
                "review" | "review_follow_up" => {
                    serde_json::json!({"action_id": action_id, "scope": scope, "outcome": "pass", "commit": action["target_commit"]})
                }
                "final_audit" => {
                    let state: serde_json::Value =
                        read_json(Path::new(action["controller_state"].as_str().unwrap()))?;
                    let assessment = AuditAssessment {
                        reconciled: serde_json::from_value(state["frozen"]["reconciled"].clone())?,
                        adoption: serde_json::from_value(state["frozen"]["adoption"].clone())?,
                        implementation: serde_json::from_value(action["implementation"].clone())?,
                        coverage: vec![Coverage {
                            requirement_id: "R-1".into(),
                            state: CoverageState::Pass,
                            rationale: "ok".into(),
                            evidence: vec!["test".into()],
                            correction: String::new(),
                        }],
                        assessor_context: "test".into(),
                    };
                    write_bytes_sync(
                        &invocation.action.join("assessment.json"),
                        &orchestrate_contracts::encode(&assessment)?,
                    )?;
                    serde_json::json!({"action_id": action_id, "scope": scope, "outcome": "complete"})
                }
                other => bail!("unexpected fake action {other}"),
            };
            write_bytes_sync(
                &invocation.action.join("result.json"),
                &serde_json::to_vec_pretty(&receipt)?,
            )?;
            Ok(InvocationResult {
                session_id: Some(format!("{}-session", invocation.role)),
            })
        }
    }
    #[test]
    fn one_run_crosses_delivery_phases_and_final_audit() {
        let (store, _effort, repo, _reconciled, _adoption) = prepared();
        let result = run_with_adapter(
            &store,
            BuildRequest {
                effort: None,
                project: repo.clone(),
            },
            &FakeHost,
        )
        .unwrap();
        assert!(matches!(result, BuildResult::Completed(_)));
        assert!(repo.join("D1.txt").exists() && repo.join("D2.txt").exists());
        let replay = run_with_adapter(
            &store,
            BuildRequest {
                effort: None,
                project: repo,
            },
            &FakeHost,
        )
        .unwrap();
        assert!(matches!(replay, BuildResult::Completed(_)));
    }
}
