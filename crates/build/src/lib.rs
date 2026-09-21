//! The small deterministic controller behind `orchestrate build`.
//!
//! This crate deliberately owns transitions and durable controller state, but
//! not engineering judgement.  Provider conversations write one receipt per
//! action; the controller validates that receipt before selecting the next
//! fixed action.

use std::{
    collections::HashSet,
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
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
pub const BUILD_CONFIG_VERSION: u32 = 2;
pub const BUILD_STATE_VERSION: u32 = 2;

/// Scope of the post-phase Audit turn, which is not one of the plan's delivery phases.
const FINAL_SCOPE: &str = "final";

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
    pub model: Option<String>,
}

/// The four things a role can be asked to do.  A correction is not a kind of its own: the scope
/// names the phase and a referenced `feedback` marks the turn as a correction.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ActionKind {
    Work,
    Review,
    FinalAudit,
    Unblock,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum DispatchState {
    Prepared,
    Running,
    Completed,
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
    #[serde(default)]
    feedback_path: Option<String>,
    #[serde(default)]
    transport_session_id: Option<String>,
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

impl Recovery {
    /// A remedy restarts the retry ladder for the interrupted action but deliberately keeps the
    /// unblock flag, so a scope that blocks again after every remedy stops instead of looping.
    fn after_remedy(&mut self) {
        self.continuation_used = false;
        self.replacement_used = false;
    }
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
    interrupted: Option<Box<CurrentAction>>,
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
    fn invoke(
        &self,
        request: &Invocation,
        observer: &mut dyn InvocationObserver,
    ) -> Result<InvocationResult>;
}

/// Persistence callback used by adapters while a provider command is running.
/// The controller owns the transition; adapters only report opaque transport facts.
pub trait InvocationObserver {
    fn accepted(&mut self) -> Result<()>;
    fn session_id(&mut self, session_id: &str) -> Result<()>;
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
    pub completion: InvocationCompletion,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InvocationCompletion {
    Completed,
    FailedBeforeAcceptance { detail: String },
    AcceptedButIncomplete { detail: String },
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
        matches!(config.adapter.as_str(), "codex" | "claude" | "cursor"),
        "unsupported {role} adapter {}",
        config.adapter
    );
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
    _config: &BuildConfig,
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
        sessions: Sessions::default(),
        recovery: Recovery::default(),
        interrupted: None,
        implementation: None,
        terminal: None,
    };
    save_state(&build_dir.join("state.json"), &state)?;
    Ok(state)
}

fn load_state(
    path: &Path,
    _config: &BuildConfig,
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
    Ok(state)
}

#[allow(clippy::too_many_arguments)]
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
        // A restart may find the implementation already published; registering again would
        // duplicate the journal record, so only publish when reconciliation finds nothing.
        let implementation = match orchestrate_audit::find_implementation(
            store,
            effort,
            &state.frozen.adoption,
            &commit,
        )? {
            Some(found) => found,
            None => orchestrate_audit::register_implementation_with_run_id(
                store,
                effort,
                state.frozen.adoption.clone(),
                &commit,
                "unattended build".into(),
                ImplementationStatus::Submitted,
                build_provenance("build", canonical_role_guide(&ActionKind::Work)),
                format!(
                    "build-{}",
                    digest_bytes(format!("{}:{commit}", state.frozen.adoption.digest).as_bytes())
                ),
            )?,
        };
        state.implementation = Some(implementation);
        state.action.target_commit = Some(commit);
    }
    let role = match state.action.kind {
        ActionKind::Work => "worker",
        ActionKind::Review | ActionKind::FinalAudit | ActionKind::Unblock => "reviewer",
    };
    let role_config = if role == "worker" {
        &config.worker
    } else {
        &config.reviewer
    };
    let action_dir = build_dir.join("artifacts").join(&state.action.id);
    fs::create_dir_all(&action_dir)?;
    let completion_marker = action_dir.join("transport.completed");
    if state.action.dispatch == DispatchState::Running {
        if completion_marker.is_file() {
            state.action.dispatch = DispatchState::Completed;
            save_state(state_path, state)?;
        } else {
            bail!(
                "provider invocation for action {} may still be running; it will not be resent automatically",
                state.action.id
            );
        }
    }
    // Generated role instructions are refreshed immediately before dispatch so an
    // earlier role cannot alter the authority used by a later role.
    if state.action.dispatch == DispatchState::Prepared {
        materialize_role_guide(build_dir, &state.action.kind)?;
    }
    let cwd = match state.action.kind {
        ActionKind::Review => review_checkout(
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
    let persistent_session = match state.action.kind {
        ActionKind::Work => PersistentSession::Worker,
        ActionKind::Review => PersistentSession::Reviewer,
        ActionKind::FinalAudit | ActionKind::Unblock => PersistentSession::None,
    };
    let session = match persistent_session {
        PersistentSession::Worker => state.sessions.worker.clone(),
        PersistentSession::Reviewer => state.sessions.reviewer.clone(),
        PersistentSession::None => None,
    };
    let invocation = Invocation {
        adapter: role_config.adapter.clone(),
        role: role.into(),
        config: role_config.clone(),
        cwd,
        build_dir: build_dir.to_path_buf(),
        action: action_dir.clone(),
        session_id: session.clone(),
        prompt: invocation_prompt(build_dir, &action_dir, &state.action),
    };
    if state.action.dispatch == DispatchState::Prepared {
        state.action.transport_session_id = session;
        save_state(state_path, state)?;
        let outcome = {
            let mut observer = StateObserver {
                state_path,
                state,
                persistent_session,
            };
            adapter.invoke(&invocation, &mut observer)?
        };
        match outcome.completion {
            InvocationCompletion::Completed => {
                state.action.dispatch = DispatchState::Completed;
                write_bytes_sync(&completion_marker, b"completed\n")?;
                save_state(state_path, state)?;
            }
            InvocationCompletion::FailedBeforeAcceptance { .. } => {
                recover_or_block(build_dir, state, "incomplete", role)?;
                return Ok(());
            }
            InvocationCompletion::AcceptedButIncomplete { detail } => {
                // Without a transport identity the acceptance state is unknown: the invocation
                // may or may not have started, so it is never resent.
                ensure!(
                    state.action.transport_session_id.is_some(),
                    "action {} ended without a resumable transport identity ({detail}); its acceptance state is unknown and it will not be resent automatically. Inspect {} before resuming.",
                    state.action.id,
                    action_dir.join("transport.jsonl").display()
                );
                recover_or_block(build_dir, state, "incomplete", role)?;
                return Ok(());
            }
        }
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

#[derive(Clone, Copy)]
enum PersistentSession {
    Worker,
    Reviewer,
    None,
}

struct StateObserver<'a> {
    state_path: &'a Path,
    state: &'a mut BuildState,
    persistent_session: PersistentSession,
}

impl InvocationObserver for StateObserver<'_> {
    fn accepted(&mut self) -> Result<()> {
        self.state.action.dispatch = DispatchState::Running;
        save_state(self.state_path, self.state)
    }

    fn session_id(&mut self, session_id: &str) -> Result<()> {
        self.state.action.transport_session_id = Some(session_id.into());
        match self.persistent_session {
            PersistentSession::Worker if self.state.sessions.worker.is_none() => {
                self.state.sessions.worker = Some(session_id.into())
            }
            PersistentSession::Reviewer if self.state.sessions.reviewer.is_none() => {
                self.state.sessions.reviewer = Some(session_id.into())
            }
            _ => {}
        }
        save_state(self.state_path, self.state)
    }
}

fn write_action_file(
    build_dir: &Path,
    plan: &BuildPlan,
    state: &BuildState,
    action_dir: &Path,
    role: &str,
    cwd: &Path,
) -> Result<()> {
    let tasks = plan
        .delivery_phases
        .iter()
        .find(|phase| phase.id == state.action.scope)
        .map(|phase| phase.tasks.clone())
        .unwrap_or_default();
    let instruction = build_dir
        .join("instructions")
        .join(instruction_name(&state.action.kind));
    let value = serde_json::json!({
        "action_id": state.action.id,
        "kind": state.action.kind,
        "scope": state.action.scope,
        "tasks": tasks,
        "role": role,
        "instruction": instruction,
        "target_commit": state.action.target_commit,
        "feedback": state.action.feedback_path,
        "implementation": state.implementation,
        // The Audit needs these two lineage references to build its assessment; supplying them
        // directly keeps the action self-contained instead of pointing at controller state.
        "reconciled": state.frozen.reconciled,
        "adoption": state.frozen.adoption,
        "result": action_dir.join("result.json"),
        "report": action_dir.join("report.md"),
        "detailed_plan": build_dir.join(&plan.detailed_plan),
        "working_directory": cwd,
    });
    write_bytes_sync(
        &action_dir.join("action.json"),
        &serde_json::to_vec_pretty(&value)?,
    )
}

fn invocation_prompt(build_dir: &Path, action_dir: &Path, action: &CurrentAction) -> String {
    format!(
        "Read {} and {}. Follow the instruction document for this action.",
        action_dir.join("action.json").display(),
        build_dir
            .join("instructions")
            .join(instruction_name(&action.kind))
            .display(),
    )
}

fn instruction_name(kind: &ActionKind) -> &'static str {
    match kind {
        ActionKind::Work => "work.md",
        ActionKind::Review => "review.md",
        ActionKind::FinalAudit => "final-audit.md",
        ActionKind::Unblock => "unblock.md",
    }
}

#[allow(clippy::too_many_arguments)]
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
        ActionKind::Work => {
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
                let repo = store.project_for(effort)?.canonical_locator;
                ensure!(
                    git(&repo, ["rev-parse", "HEAD"])? == commit,
                    "worker receipt commit is not current HEAD"
                );
                ensure!(
                    git(&repo, ["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
                    "worker left tracked changes outside submitted commit"
                );
                state.recovery = Recovery::default();
                let scope = state.action.scope.clone();
                // Work for the Audit scope is a correction, so it returns to a fresh Audit
                // instead of to a phase review.
                let next = if scope == FINAL_SCOPE {
                    state.implementation = None;
                    ActionKind::FinalAudit
                } else {
                    ActionKind::Review
                };
                state.action = new_action(
                    build_dir,
                    next,
                    &scope,
                    Some(commit.into()),
                    Some(action_dir.join("report.md")),
                );
            } else {
                recover_or_block(build_dir, state, &receipt.outcome, "worker")?;
            }
        }
        ActionKind::Review => {
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
                .clone()
                .context("review lacks target commit")?;
            ensure!(
                receipt.commit.as_deref() == Some(expected.as_str()),
                "review receipt did not inspect exact submitted commit"
            );
            let review_source = action_dir.join("source");
            ensure!(
                git(&review_source, ["rev-parse", "HEAD"])? == expected,
                "reviewer changed the detached review checkout"
            );
            ensure!(
                git(
                    &review_source,
                    ["status", "--porcelain", "--untracked-files=all"]
                )?
                .is_empty(),
                "reviewer left changes in the detached review checkout"
            );
            if receipt.outcome == "pass" {
                state.recovery = Recovery::default();
                if state.phase_index + 1 == plan.delivery_phases.len() {
                    state.action = new_action(
                        build_dir,
                        ActionKind::FinalAudit,
                        FINAL_SCOPE,
                        Some(expected),
                        Some(action_dir.join("report.md")),
                    );
                } else {
                    state.phase_index += 1;
                    let scope = plan.delivery_phases[state.phase_index].id.clone();
                    state.action = new_action(build_dir, ActionKind::Work, &scope, None, None);
                }
            } else if receipt.outcome == "changes_required" {
                let scope = state.action.scope.clone();
                state.action = new_action(
                    build_dir,
                    ActionKind::Work,
                    &scope,
                    Some(expected),
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
            // Only publish when no Audit already covers this exact implementation, so a restart
            // after publication reuses it instead of appending a second immutable record.
            let audit = match orchestrate_audit::find_audit(store, effort, &implementation)? {
                Some(existing) => existing,
                None => orchestrate_audit::finalize_audit_with_run_id(
                    store,
                    effort,
                    assessment.clone(),
                    build_provenance("audit", canonical_role_guide(&ActionKind::FinalAudit)),
                    format!("build-audit-{}", state.action.id),
                )?,
            };
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
                        ActionKind::Work,
                        FINAL_SCOPE,
                        state.action.target_commit.clone(),
                        Some(action_dir.join("assessment.json")),
                    )
                }
                Verdict::Blocked => recover_or_block(build_dir, state, "blocked", "reviewer")?,
            }
        }
        ActionKind::Unblock => {
            ensure!(
                matches!(
                    receipt.outcome.as_str(),
                    "remedy_available" | "external_requirement"
                ),
                "unblock receipt has invalid outcome"
            );
            if receipt.outcome == "remedy_available" {
                let interrupted = state
                    .interrupted
                    .take()
                    .context("unblock action has no interrupted action to resume")?;
                state.recovery.after_remedy();
                state.action = new_action(
                    build_dir,
                    interrupted.kind,
                    &interrupted.scope,
                    interrupted.target_commit,
                    Some(action_dir.join("report.md")),
                );
                return Ok(());
            }
            bail!(
                "unblocker identified an external requirement; see {}",
                action_dir.join("report.md").display()
            )
        }
    }
    Ok(())
}

/// A failed or blocked turn retries in a fixed order: same session, then a replacement session,
/// then one fresh session-less unblocker diagnosis.  Each flag is consumed once, so an
/// unattended run always reaches a decision instead of looping.
fn recover_or_block(
    build_dir: &Path,
    state: &mut BuildState,
    outcome: &str,
    role: &str,
) -> Result<()> {
    if outcome == "blocked" && !state.recovery.unblock_used {
        return start_unblock(build_dir, state);
    }
    if !state.recovery.continuation_used {
        state.recovery.continuation_used = true;
        let retry = retry_action(build_dir, state);
        state.action = retry;
        return Ok(());
    }
    if !state.recovery.replacement_used {
        state.recovery.replacement_used = true;
        if role == "worker" {
            state.sessions.worker = None;
        } else {
            state.sessions.reviewer = None;
        }
        let retry = retry_action(build_dir, state);
        state.action = retry;
        return Ok(());
    }
    if !state.recovery.unblock_used {
        return start_unblock(build_dir, state);
    }
    bail!("recovery is exhausted for scope {}", state.action.scope)
}

/// Re-run the same action once more, keeping its scope, target commit, and feedback.
fn retry_action(build_dir: &Path, state: &BuildState) -> CurrentAction {
    new_action(
        build_dir,
        state.action.kind.clone(),
        &state.action.scope,
        state.action.target_commit.clone(),
        state.action.feedback_path.as_ref().map(PathBuf::from),
    )
}

/// Preserve the interrupted action and ask a session-less unblocker to diagnose it.
fn start_unblock(build_dir: &Path, state: &mut BuildState) -> Result<()> {
    state.recovery.unblock_used = true;
    state.interrupted = Some(Box::new(state.action.clone()));
    state.action = new_action(
        build_dir,
        ActionKind::Unblock,
        &state.action.scope,
        state.action.target_commit.clone(),
        state.action.feedback_path.as_ref().map(PathBuf::from),
    );
    Ok(())
}

fn new_action(
    _build_dir: &Path,
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
    CurrentAction {
        id,
        kind,
        scope: scope.into(),
        target_commit,
        feedback_path: feedback.map(|p| p.to_string_lossy().into_owned()),
        transport_session_id: None,
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

fn canonical_role_guide(kind: &ActionKind) -> &'static str {
    match kind {
        ActionKind::Work => orchestrate_guides::WORK,
        ActionKind::Review => orchestrate_guides::REVIEW,
        ActionKind::FinalAudit => orchestrate_guides::FINAL_AUDIT,
        ActionKind::Unblock => orchestrate_guides::UNBLOCK,
    }
}

fn materialize_role_guide(build_dir: &Path, kind: &ActionKind) -> Result<()> {
    write_bytes_sync(
        &build_dir.join("instructions").join(instruction_name(kind)),
        canonical_role_guide(kind).as_bytes(),
    )
}

/// Role guides are generated from the running CLI. Rewrite them on every start
/// and resume so a newer binary cannot leave a model reading the previous version.
fn materialize_role_guides(build_dir: &Path) -> Result<()> {
    for kind in [
        ActionKind::Work,
        ActionKind::Review,
        ActionKind::Unblock,
        ActionKind::FinalAudit,
    ] {
        materialize_role_guide(build_dir, &kind)?;
    }
    Ok(())
}

fn build_provenance(host: &str, guide: &str) -> Provenance {
    Provenance {
        host: host.into(),
        provider: None,
        model: None,
        model_effort: None,
        guide_digest: digest_bytes(guide.as_bytes()),
        independence: Independence::InputExcluded,
    }
}

/// Write the current Build templates into the effort Build directory.
/// Existing files are left untouched.
pub fn scaffold(store: &Store, effort_id: &str) -> Result<PathBuf> {
    let effort = store.load_effort(effort_id)?;
    let build_dir = store.phase_dir(&effort, "build")?;
    for (name, body) in [
        ("plan.json", orchestrate_guides::templates::PLAN_JSON),
        ("config.toml", orchestrate_guides::templates::CONFIG_TOML),
    ] {
        let path = build_dir.join(name);
        ensure!(
            !path.exists(),
            "{} already exists; refusing to overwrite",
            path.display()
        );
        write_bytes_sync(&path, body.as_bytes())?;
    }
    Ok(build_dir)
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
    fn invoke(
        &self,
        request: &Invocation,
        observer: &mut dyn InvocationObserver,
    ) -> Result<InvocationResult> {
        let mut command = provider_command(request)?;
        let log = request.action.join("transport.jsonl");
        let mut child = match command
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                return Ok(InvocationResult {
                    completion: InvocationCompletion::FailedBeforeAcceptance {
                        detail: format!("cannot launch {}: {error}", request.adapter),
                    },
                });
            }
        };
        observer.accepted()?;
        let stdout = child
            .stdout
            .take()
            .context("provider stdout was not captured")?;
        let mut log_file = OpenOptions::new().create(true).append(true).open(log)?;
        for line in BufReader::new(stdout).split(b'\n') {
            let mut line = line?;
            line.push(b'\n');
            log_file.write_all(&line)?;
            log_file.sync_data()?;
            if let Some(session_id) = parse_session_id(&line) {
                observer.session_id(&session_id)?;
            }
        }
        let status = child.wait()?;
        Ok(InvocationResult {
            completion: if status.success() {
                InvocationCompletion::Completed
            } else {
                InvocationCompletion::AcceptedButIncomplete {
                    detail: format!("{} exited with {status}", request.adapter),
                }
            },
        })
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
            c.args([
                "-p",
                "--verbose",
                "--output-format",
                "stream-json",
                "--add-dir",
            ])
            .arg(&request.build_dir);
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
        other => bail!("unsupported adapter {other}"),
    };
    if let Some(model) = &request.config.model {
        command.args(["--model", model]);
    }
    if request.adapter == "claude" {
        // Claude's `--add-dir` accepts a variadic argument list, so without a
        // separator it consumes the action prompt as another directory.
        command.arg("--");
    }
    command.arg(&request.prompt);
    Ok(command)
}

fn parse_session_id(bytes: &[u8]) -> Option<String> {
    for line in String::from_utf8_lossy(bytes).lines() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        for key in [
            "session_id",
            "thread_id",
            "chat_id",
            "sessionId",
            "threadId",
            "chatId",
        ] {
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
    use std::{collections::BTreeMap, process::Command, sync::Mutex};

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
    fn scaffold_writes_the_embedded_templates_once() {
        let root = temp("scaffold-store");
        let repo = temp("scaffold-repo");
        git_ok(&repo, &["init"]);
        git_ok(&repo, &["config", "user.email", "test@example.com"]);
        git_ok(&repo, &["config", "user.name", "Test"]);
        fs::write(repo.join("source.txt"), "baseline\n").unwrap();
        git_ok(&repo, &["add", "."]);
        git_ok(&repo, &["commit", "-m", "baseline"]);
        let store = Store::open(&root).unwrap();
        let effort = store
            .init_effort(
                &repo,
                "scaffold",
                RequestKind::Freeform,
                "work".into(),
                vec![],
            )
            .unwrap();
        let dir = scaffold(&store, &effort.id).unwrap();
        assert_eq!(
            fs::read_to_string(dir.join("plan.json")).unwrap(),
            orchestrate_guides::templates::PLAN_JSON
        );
        assert_eq!(
            fs::read_to_string(dir.join("config.toml")).unwrap(),
            orchestrate_guides::templates::CONFIG_TOML
        );
        assert!(scaffold(&store, &effort.id).is_err());
    }
    #[test]
    fn named_adapters_build_supported_local_commands() {
        let root = temp("commands");
        for (adapter, program) in [
            ("codex", "codex"),
            ("claude", "claude"),
            ("cursor", "cursor-agent"),
        ] {
            let request = Invocation {
                adapter: adapter.into(),
                role: "worker".into(),
                config: RoleConfig {
                    adapter: adapter.into(),
                    model: Some("model".into()),
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

    #[test]
    fn codex_and_claude_receive_the_external_build_directory() {
        let root = temp("commands");
        for adapter in ["codex", "claude"] {
            let request = Invocation {
                adapter: adapter.into(),
                role: "worker".into(),
                config: RoleConfig {
                    adapter: adapter.into(),
                    model: None,
                },
                cwd: root.clone(),
                build_dir: root.clone(),
                action: root.clone(),
                session_id: None,
                prompt: "work".into(),
            };
            let command = provider_command(&request).unwrap();
            let args = command
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>();
            assert!(
                args.windows(2)
                    .any(|pair| { pair[0] == "--add-dir" && pair[1] == root.to_string_lossy() })
            );
        }
    }

    #[test]
    fn opencode_is_rejected_until_its_cli_contract_is_verified() {
        assert!(
            validate_role(
                "worker",
                &RoleConfig {
                    adapter: "opencode".into(),
                    model: None,
                }
            )
            .is_err()
        );
    }

    fn temp(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "orchestrate-build-{name}-{}-{}",
            now_ms(),
            ACTION_COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
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
                build_provenance("test", orchestrate_guides::BUILD),
                files,
            )
            .unwrap();
        let adoption = orchestrate_audit::adopt(
            &store,
            &effort,
            reconciled_ref.clone(),
            "test".into(),
            build_provenance("test", orchestrate_guides::BUILD),
        )
        .unwrap();
        let build = store.phase_dir(&effort, "build").unwrap();
        fs::write(build.join("implementation-plan.md"), "# Plan\n").unwrap();
        fs::write(
            build.join("config.toml"),
            "schema_version = 2\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"cursor\"\n",
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
    /// Each role must be driven through its own configured adapter, not a shared default.
    fn assert_role_adapter(invocation: &Invocation) {
        let config: BuildConfig =
            toml::from_str(&fs::read_to_string(invocation.build_dir.join("config.toml")).unwrap())
                .unwrap();
        let expected = if invocation.role == "worker" {
            &config.worker.adapter
        } else {
            &config.reviewer.adapter
        };
        assert_eq!(
            &invocation.adapter, expected,
            "role {} was driven through adapter {}",
            invocation.role, invocation.adapter
        );
    }

    struct FakeHost;
    impl HostAdapter for FakeHost {
        fn invoke(
            &self,
            invocation: &Invocation,
            observer: &mut dyn InvocationObserver,
        ) -> Result<InvocationResult> {
            observer.accepted()?;
            observer.session_id(&format!("{}-session", invocation.role))?;
            let action: serde_json::Value = read_json(&invocation.action.join("action.json"))?;
            assert_role_adapter(invocation);
            let kind = action["kind"].as_str().unwrap();
            let action_id = action["action_id"].as_str().unwrap();
            let scope = action["scope"].as_str().unwrap();
            assert!(
                invocation.prompt.contains(
                    invocation
                        .action
                        .join("action.json")
                        .to_string_lossy()
                        .as_ref()
                )
            );
            assert!(
                invocation
                    .prompt
                    .contains(action["instruction"].as_str().unwrap())
            );
            if let Some(phase) = scope.strip_prefix('D') {
                assert_eq!(action["tasks"], serde_json::json!([format!("T{phase}")]));
            } else {
                // The post-phase Audit scope owns no delivery-phase task IDs.
                assert_eq!(scope, FINAL_SCOPE);
                assert_eq!(action["tasks"], serde_json::json!([]));
            }
            match (kind, scope) {
                ("work", "D2") => {
                    assert_eq!(invocation.session_id.as_deref(), Some("worker-session"))
                }
                ("review", "D2") => {
                    assert_eq!(invocation.session_id.as_deref(), Some("reviewer-session"))
                }
                ("final_audit", _) => assert!(invocation.session_id.is_none()),
                _ => {}
            }
            let receipt = match kind {
                "work" => {
                    fs::write(invocation.cwd.join(format!("{scope}.txt")), scope)?;
                    git_ok(&invocation.cwd, &["add", "."]);
                    git_ok(&invocation.cwd, &["commit", "-m", scope]);
                    serde_json::json!({"action_id": action_id, "scope": scope, "outcome": "complete", "commit": git(&invocation.cwd, ["rev-parse", "HEAD"])?})
                }
                "review" => {
                    serde_json::json!({"action_id": action_id, "scope": scope, "outcome": "pass", "commit": action["target_commit"]})
                }
                "final_audit" => {
                    let assessment = AuditAssessment {
                        reconciled: serde_json::from_value(action["reconciled"].clone())?,
                        adoption: serde_json::from_value(action["adoption"].clone())?,
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
                completion: InvocationCompletion::Completed,
            })
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Scenario {
        Basic,
        Corrections,
        RepeatedCorrections,
        Remedy,
        ExternalRequirement,
        FailedBeforeAcceptance,
        AcceptedButIncomplete,
        InterruptedTwice,
        Uncertain,
        InvalidReview,
        NeverUnblocked,
    }

    struct ScenarioHost {
        scenario: Scenario,
        calls: Mutex<Vec<String>>,
    }

    impl ScenarioHost {
        fn new(scenario: Scenario) -> Self {
            Self {
                scenario,
                calls: Mutex::new(Vec::new()),
            }
        }

        fn call_count(&self, kind: &str) -> usize {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|call| call.as_str() == kind)
                .count()
        }

        fn write_assessment(
            &self,
            invocation: &Invocation,
            action: &serde_json::Value,
            pass: bool,
        ) -> Result<()> {
            let assessment = AuditAssessment {
                reconciled: serde_json::from_value(action["reconciled"].clone())?,
                adoption: serde_json::from_value(action["adoption"].clone())?,
                implementation: serde_json::from_value(action["implementation"].clone())?,
                coverage: vec![Coverage {
                    requirement_id: "R-1".into(),
                    state: if pass {
                        CoverageState::Pass
                    } else {
                        CoverageState::Fail
                    },
                    rationale: if pass {
                        "ok".into()
                    } else {
                        "needs correction".into()
                    },
                    evidence: vec!["test".into()],
                    correction: if pass { String::new() } else { "fix it".into() },
                }],
                assessor_context: "test".into(),
            };
            write_bytes_sync(
                &invocation.action.join("assessment.json"),
                &orchestrate_contracts::encode(&assessment)?,
            )
        }

        fn complete_work(
            &self,
            invocation: &Invocation,
            kind: &str,
            scope: &str,
        ) -> Result<serde_json::Value> {
            let file = invocation
                .cwd
                .join(format!("{kind}-{}.txt", self.call_count(kind)));
            fs::write(file, format!("{kind} {scope}\n"))?;
            git_ok(&invocation.cwd, &["add", "."]);
            git_ok(&invocation.cwd, &["commit", "-m", kind]);
            Ok(serde_json::json!({
                "action_id": read_json::<serde_json::Value>(&invocation.action.join("action.json"))?["action_id"],
                "scope": scope,
                "outcome": "complete",
                "commit": git(&invocation.cwd, ["rev-parse", "HEAD"])?
            }))
        }
    }

    impl HostAdapter for ScenarioHost {
        fn invoke(
            &self,
            invocation: &Invocation,
            observer: &mut dyn InvocationObserver,
        ) -> Result<InvocationResult> {
            let action: serde_json::Value = read_json(&invocation.action.join("action.json"))?;
            let kind = action["kind"].as_str().unwrap();
            let scope = action["scope"].as_str().unwrap();
            assert_role_adapter(invocation);
            self.calls.lock().unwrap().push(kind.into());
            // A recovery regression would otherwise spin forever instead of failing.
            assert!(
                self.calls.lock().unwrap().len() <= 64,
                "the controller stopped making progress"
            );
            let count = self.call_count(kind);
            if matches!(kind, "unblock" | "final_audit") {
                assert!(invocation.session_id.is_none());
            }
            if self.scenario == Scenario::FailedBeforeAcceptance && kind == "work" && count == 1 {
                return Ok(InvocationResult {
                    completion: InvocationCompletion::FailedBeforeAcceptance {
                        detail: "offline".into(),
                    },
                });
            }
            if self.scenario == Scenario::InterruptedTwice && kind == "work" {
                // The first retry continues the accepted session; the second must have dropped
                // it, so the provider is never asked twice about an uncertain session.
                match count {
                    1 => assert!(invocation.session_id.is_none()),
                    2 => assert_eq!(invocation.session_id.as_deref(), Some("worker-session")),
                    3 => assert!(invocation.session_id.is_none()),
                    _ => {}
                }
            }
            observer.accepted()?;
            if !(self.scenario == Scenario::Uncertain && kind == "work" && count == 1) {
                observer.session_id(&format!("{}-session", invocation.role))?;
            }
            if matches!(
                self.scenario,
                Scenario::AcceptedButIncomplete | Scenario::InterruptedTwice
            ) && kind == "work"
                && if self.scenario == Scenario::InterruptedTwice {
                    count <= 2
                } else {
                    count == 1
                }
            {
                return Ok(InvocationResult {
                    completion: InvocationCompletion::AcceptedButIncomplete {
                        detail: "interrupted".into(),
                    },
                });
            }
            if self.scenario == Scenario::Uncertain && kind == "work" && count == 1 {
                return Ok(InvocationResult {
                    completion: InvocationCompletion::AcceptedButIncomplete {
                        detail: "unknown".into(),
                    },
                });
            }
            let receipt = match kind {
                "work" => {
                    if self.scenario == Scenario::NeverUnblocked
                        || matches!(
                            self.scenario,
                            Scenario::Remedy | Scenario::ExternalRequirement
                        ) && count == 1
                    {
                        serde_json::json!({"action_id": action["action_id"], "scope": scope, "outcome": "blocked"})
                    } else {
                        self.complete_work(invocation, kind, scope)?
                    }
                }
                "review" => {
                    let outcome = if self.scenario == Scenario::Corrections && count == 1
                        || self.scenario == Scenario::RepeatedCorrections && count <= 2
                    {
                        "changes_required"
                    } else {
                        "pass"
                    };
                    serde_json::json!({
                        "action_id": action["action_id"],
                        "scope": scope,
                        "outcome": outcome,
                        "commit": if self.scenario == Scenario::InvalidReview { serde_json::json!("stale") } else { action["target_commit"].clone() }
                    })
                }
                "unblock" => serde_json::json!({
                    "action_id": action["action_id"],
                    "scope": scope,
                    "outcome": if self.scenario == Scenario::ExternalRequirement { "external_requirement" } else { "remedy_available" }
                }),
                "final_audit" => {
                    let corrections_still_required = self.scenario == Scenario::Corrections
                        && count == 1
                        || self.scenario == Scenario::RepeatedCorrections && count <= 2;
                    self.write_assessment(invocation, &action, !corrections_still_required)?;
                    serde_json::json!({"action_id": action["action_id"], "scope": scope, "outcome": "complete"})
                }
                other => bail!("unexpected scenario action {other}"),
            };
            write_bytes_sync(
                &invocation.action.join("result.json"),
                &serde_json::to_vec_pretty(&receipt)?,
            )?;
            Ok(InvocationResult {
                completion: InvocationCompletion::Completed,
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
        assert_no_build_files_leaked(&repo);
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

    #[test]
    fn corrections_repeat_work_review_and_final_audit_automatically() {
        let (store, _effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::Corrections);
        let result = run_with_adapter(
            &store,
            BuildRequest {
                effort: None,
                project: repo,
            },
            &host,
        )
        .unwrap();
        assert!(matches!(result, BuildResult::Completed(_)));
        // Phase work, its correction, the second phase, and the Audit-scope correction.
        assert_eq!(host.call_count("work"), 4);
        assert_eq!(host.call_count("review"), 3);
        assert_eq!(host.call_count("final_audit"), 2);
    }

    #[test]
    fn remedy_unblock_resumes_the_interrupted_action() {
        let (store, _effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::Remedy);
        let result = run_with_adapter(
            &store,
            BuildRequest {
                effort: None,
                project: repo,
            },
            &host,
        )
        .unwrap();
        assert!(matches!(result, BuildResult::Completed(_)));
        assert_eq!(host.call_count("unblock"), 1);
        assert_eq!(host.call_count("work"), 3);
    }

    #[test]
    fn external_requirement_stops_without_retrying_work() {
        let (store, _effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::ExternalRequirement);
        let result = run_with_adapter(
            &store,
            BuildRequest {
                effort: None,
                project: repo,
            },
            &host,
        )
        .unwrap();
        assert!(matches!(result, BuildResult::Blocked { .. }));
        assert_eq!(host.call_count("unblock"), 1);
        assert_eq!(host.call_count("work"), 1);
    }

    #[test]
    fn definitive_provider_failures_resume_then_replace_without_user_action() {
        for scenario in [
            Scenario::FailedBeforeAcceptance,
            Scenario::AcceptedButIncomplete,
        ] {
            let (store, _effort, repo, _reconciled, _adoption) = prepared();
            let host = ScenarioHost::new(scenario);
            let result = run_with_adapter(
                &store,
                BuildRequest {
                    effort: None,
                    project: repo,
                },
                &host,
            )
            .unwrap();
            assert!(matches!(result, BuildResult::Completed(_)));
            assert_eq!(host.call_count("work"), 3);
        }
    }

    #[test]
    fn an_incomplete_action_continues_its_session_then_replaces_it() {
        let (store, _effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::InterruptedTwice);
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(result, BuildResult::Completed(_)));
        // Two interrupted attempts for the first phase plus one work turn per phase.
        assert_eq!(host.call_count("work"), 4);
    }

    #[test]
    fn uncertain_provider_acceptance_never_retries() {
        let (store, _effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::Uncertain);
        let result = run_with_adapter(
            &store,
            BuildRequest {
                effort: None,
                project: repo,
            },
            &host,
        )
        .unwrap();
        assert!(matches!(result, BuildResult::Blocked { .. }));
        assert_eq!(host.call_count("work"), 1);
    }

    #[test]
    fn stale_review_receipts_do_not_advance_a_phase() {
        let (store, _effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::InvalidReview);
        let result = run_with_adapter(
            &store,
            BuildRequest {
                effort: None,
                project: repo,
            },
            &host,
        )
        .unwrap();
        assert!(matches!(result, BuildResult::Blocked { .. }));
        assert_eq!(host.call_count("work"), 1);
        assert_eq!(host.call_count("review"), 1);
    }

    fn request(repo: &Path) -> BuildRequest {
        BuildRequest {
            effort: None,
            project: repo.to_path_buf(),
        }
    }

    fn artifact_count(store: &Store, effort: &Effort, kind: ArtifactKind) -> usize {
        store
            .list_artifacts(effort)
            .unwrap()
            .into_iter()
            .filter(|reference| reference.kind == kind)
            .count()
    }

    fn journal_count(store: &Store, effort: &Effort, event: &str) -> usize {
        store
            .read_journal(effort)
            .unwrap()
            .into_iter()
            .filter(|entry| entry.event == event)
            .count()
    }

    fn rewrite_state(state_path: &Path, mutate: impl Fn(&mut serde_json::Value)) {
        let mut state: serde_json::Value = read_json(state_path).unwrap();
        mutate(&mut state);
        write_bytes_sync(state_path, &serde_json::to_vec_pretty(&state).unwrap()).unwrap();
    }

    /// Nothing the controller owns may appear in the product repository.
    fn assert_no_build_files_leaked(repo: &Path) {
        let status = git(repo, ["status", "--porcelain", "--untracked-files=all"]).unwrap();
        assert!(status.is_empty(), "product repository is dirty: {status}");
        let mut stack = vec![repo.to_path_buf()];
        while let Some(dir) = stack.pop() {
            for entry in fs::read_dir(&dir).unwrap() {
                let entry = entry.unwrap();
                if entry.file_name() == ".git" {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().into_owned();
                assert!(
                    !matches!(
                        name.as_str(),
                        "state.json"
                            | "action.json"
                            | "result.json"
                            | "report.md"
                            | "assessment.json"
                            | "transport.jsonl"
                            | "artifacts"
                            | "instructions"
                            | "build"
                    ),
                    "Build file {name} leaked into the product repository"
                );
                if entry.file_type().unwrap().is_dir() {
                    stack.push(entry.path());
                }
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum Tamper {
        ActionId,
        Scope,
        Malformed,
        DirtyCheckout,
        PrematureReceipt,
    }

    struct TamperHost {
        mode: Tamper,
        calls: Mutex<Vec<String>>,
    }

    impl TamperHost {
        fn new(mode: Tamper) -> Self {
            Self {
                mode,
                calls: Mutex::new(Vec::new()),
            }
        }

        fn call_count(&self, kind: &str) -> usize {
            self.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|call| call.as_str() == kind)
                .count()
        }
    }

    impl HostAdapter for TamperHost {
        fn invoke(
            &self,
            invocation: &Invocation,
            observer: &mut dyn InvocationObserver,
        ) -> Result<InvocationResult> {
            let action: serde_json::Value = read_json(&invocation.action.join("action.json"))?;
            let kind = action["kind"].as_str().unwrap().to_owned();
            let scope = action["scope"].as_str().unwrap().to_owned();
            let action_id = action["action_id"].as_str().unwrap().to_owned();
            assert_role_adapter(invocation);
            self.calls.lock().unwrap().push(kind.clone());
            observer.accepted()?;
            match kind.as_str() {
                "work" => {
                    // A premature receipt is only meaningful if the invocation cannot be
                    // resumed, so this mode never exposes a transport identity.
                    if self.mode != Tamper::PrematureReceipt {
                        observer.session_id("worker-session")?;
                    }
                    fs::write(
                        invocation.cwd.join(format!("{scope}.txt")),
                        format!("{scope}\n"),
                    )?;
                    git_ok(&invocation.cwd, &["add", "."]);
                    git_ok(&invocation.cwd, &["commit", "-m", &scope]);
                    let receipt = serde_json::json!({
                        "action_id": action_id,
                        "scope": scope,
                        "outcome": "complete",
                        "commit": git(&invocation.cwd, ["rev-parse", "HEAD"])?
                    });
                    write_bytes_sync(
                        &invocation.action.join("result.json"),
                        &serde_json::to_vec(&receipt)?,
                    )?;
                    if self.mode == Tamper::PrematureReceipt {
                        return Ok(InvocationResult {
                            completion: InvocationCompletion::AcceptedButIncomplete {
                                detail: "transport never completed".into(),
                            },
                        });
                    }
                }
                "review" => {
                    observer.session_id("reviewer-session")?;
                    if self.mode == Tamper::DirtyCheckout {
                        fs::write(
                            invocation.cwd.join("stray.txt"),
                            "reviewer edited the checkout\n",
                        )?;
                    }
                    let body = match self.mode {
                        Tamper::ActionId => serde_json::json!({
                            "action_id": "act-forged",
                            "scope": scope,
                            "outcome": "pass",
                            "commit": action["target_commit"]
                        }),
                        Tamper::Scope => serde_json::json!({
                            "action_id": action_id,
                            "scope": "D9",
                            "outcome": "pass",
                            "commit": action["target_commit"]
                        }),
                        _ => serde_json::json!({
                            "action_id": action_id,
                            "scope": scope,
                            "outcome": "pass",
                            "commit": action["target_commit"]
                        }),
                    };
                    let bytes = if self.mode == Tamper::Malformed {
                        b"{not json".to_vec()
                    } else {
                        serde_json::to_vec(&body)?
                    };
                    write_bytes_sync(&invocation.action.join("result.json"), &bytes)?;
                }
                other => bail!("unexpected tamper action {other}"),
            }
            Ok(InvocationResult {
                completion: InvocationCompletion::Completed,
            })
        }
    }

    #[test]
    fn forged_receipts_never_advance_the_controller() {
        for mode in [Tamper::ActionId, Tamper::Scope, Tamper::Malformed] {
            let (store, _effort, repo, _reconciled, _adoption) = prepared();
            let host = TamperHost::new(mode);
            let result = run_with_adapter(&store, request(&repo), &host).unwrap();
            assert!(
                matches!(result, BuildResult::Blocked { .. }),
                "{mode:?} advanced the controller"
            );
            assert_eq!(host.call_count("work"), 1);
            assert_eq!(host.call_count("review"), 1);
        }
    }

    #[test]
    fn a_receipt_written_before_transport_completed_is_ignored() {
        let (store, _effort, repo, _reconciled, _adoption) = prepared();
        let host = TamperHost::new(Tamper::PrematureReceipt);
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(result, BuildResult::Blocked { .. }));
        assert_eq!(host.call_count("work"), 1);
        assert_eq!(
            host.call_count("review"),
            0,
            "a premature receipt advanced the phase"
        );
    }

    #[test]
    fn a_mutated_review_checkout_is_rejected() {
        let (store, _effort, repo, _reconciled, _adoption) = prepared();
        let host = TamperHost::new(Tamper::DirtyCheckout);
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(result, BuildResult::Blocked { .. }));
        assert_eq!(host.call_count("work"), 1);
        assert_eq!(host.call_count("review"), 1);
    }

    #[test]
    fn repeated_correction_cycles_stay_automatic() {
        let (store, _effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::RepeatedCorrections);
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(result, BuildResult::Completed(_)));
        // Two correction cycles and the second phase, then two Audit-scope corrections.
        assert_eq!(host.call_count("work"), 6);
        assert_eq!(host.call_count("review"), 4);
        assert_eq!(host.call_count("final_audit"), 3);
    }

    #[test]
    fn a_scope_that_keeps_blocking_stops_instead_of_looping() {
        let (store, _effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::NeverUnblocked);
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(result, BuildResult::Blocked { .. }));
        // One unblocker diagnosis, then the retry ladder, then a bounded stop.
        assert_eq!(host.call_count("unblock"), 1);
        assert_eq!(host.call_count("work"), 4);
    }

    #[test]
    fn restart_after_audit_publication_reuses_the_published_audit() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::Basic);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let implementations = artifact_count(&store, &effort, ArtifactKind::Implementation);
        let audits = artifact_count(&store, &effort, ArtifactKind::Audit);
        assert_eq!((implementations, audits), (1, 1));

        // A crash between publishing the Audit and persisting terminal state leaves the action
        // transport-completed with its receipt on disk but no recorded completion.
        let state_path = store
            .phase_dir(&effort, "build")
            .unwrap()
            .join("state.json");
        rewrite_state(&state_path, |state| {
            state["terminal"] = serde_json::Value::Null
        });

        let replay = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(replay, BuildResult::Completed(_)));
        assert_eq!(
            artifact_count(&store, &effort, ArtifactKind::Implementation),
            implementations
        );
        assert_eq!(artifact_count(&store, &effort, ArtifactKind::Audit), audits);
        assert_eq!(journal_count(&store, &effort, "audit_finalized"), 1);
        assert_eq!(host.call_count("final_audit"), 1);
    }

    #[test]
    fn restart_before_the_implementation_was_recorded_reuses_it() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::Basic);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let implementations = artifact_count(&store, &effort, ArtifactKind::Implementation);
        let audits = artifact_count(&store, &effort, ArtifactKind::Audit);

        let state_path = store
            .phase_dir(&effort, "build")
            .unwrap()
            .join("state.json");
        rewrite_state(&state_path, |state| {
            state["terminal"] = serde_json::Value::Null;
            state["implementation"] = serde_json::Value::Null;
        });

        let replay = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(replay, BuildResult::Completed(_)));
        assert_eq!(
            artifact_count(&store, &effort, ArtifactKind::Implementation),
            implementations
        );
        assert_eq!(artifact_count(&store, &effort, ArtifactKind::Audit), audits);
        assert_eq!(
            journal_count(&store, &effort, "implementation_registered"),
            1
        );
    }

    #[test]
    fn distinct_worker_and_reviewer_adapters_are_both_driven() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        fs::write(
            store
                .phase_dir(&effort, "build")
                .unwrap()
                .join("config.toml"),
            "schema_version = 2\n[worker]\nadapter = \"cursor\"\n[reviewer]\nadapter = \"codex\"\n",
        )
        .unwrap();
        let host = ScenarioHost::new(Scenario::Basic);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        assert_no_build_files_leaked(&repo);
    }

    #[test]
    fn resumed_build_refreshes_stale_role_guides() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let build = store.phase_dir(&effort, "build").unwrap();
        let instructions = build.join("instructions");
        let user_inputs = ["plan.json", "config.toml", "implementation-plan.md"];
        let preserved: Vec<_> = user_inputs
            .iter()
            .map(|name| fs::read(build.join(name)).unwrap())
            .collect();

        let host = StaleGuideHost::default();
        let started = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(started, BuildResult::Blocked { .. }));
        assert_role_guides_match_canonical(&instructions);
        assert_eq!(host.invocations.lock().unwrap().len(), 1);

        let stale = b"STALE GUIDE FROM AN OLDER CLI\n";
        for kind in role_kinds() {
            fs::write(instructions.join(instruction_name(&kind)), stale).unwrap();
        }

        let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(resumed, BuildResult::Blocked { .. }));
        assert_role_guides_match_canonical(&instructions);
        for (name, before) in user_inputs.iter().zip(preserved) {
            assert_eq!(
                fs::read(build.join(name)).unwrap(),
                before,
                "{name} is a Build input and must survive guide refresh"
            );
        }

        let invocations = host.invocations.lock().unwrap();
        assert_eq!(invocations.len(), 2);
        let resume = &invocations[1];
        assert_eq!(resume.body, canonical_role_guide(&ActionKind::Work));
        assert_eq!(
            resume.prompt,
            format!(
                "Read {} and {}. Follow the instruction document for this action.",
                resume.action_json.display(),
                resume.instruction.display(),
            )
        );
        assert_eq!(
            build_provenance("build", canonical_role_guide(&ActionKind::Work)).guide_digest,
            digest_bytes(resume.body.as_bytes())
        );
        assert_eq!(
            build_provenance("audit", canonical_role_guide(&ActionKind::FinalAudit)).guide_digest,
            digest_bytes(canonical_role_guide(&ActionKind::FinalAudit).as_bytes())
        );
        assert_eq!(
            fs::read(instructions.join("final-audit.md")).unwrap(),
            canonical_role_guide(&ActionKind::FinalAudit).as_bytes()
        );
    }

    #[test]
    fn a_role_cannot_poison_the_next_roles_guide() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = PoisoningHost::default();
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(result, BuildResult::Blocked { .. }));

        let seen = host.review.lock().unwrap();
        let review = seen
            .as_ref()
            .expect("review was invoked in the same uninterrupted run");
        let review_guide = store
            .phase_dir(&effort, "build")
            .unwrap()
            .join("instructions")
            .join("review.md");
        assert_eq!(review.instruction, review_guide);
        assert_eq!(review.body, canonical_role_guide(&ActionKind::Review));
        assert_ne!(review.body, "POISONED BY PREVIOUS ROLE");
        assert_eq!(
            review.prompt,
            format!(
                "Read {} and {}. Follow the instruction document for this action.",
                review.action_json.display(),
                review.instruction.display(),
            )
        );
        assert!(host.poisoned.lock().unwrap().is_some());
    }

    #[derive(Default)]
    struct PoisoningHost {
        poisoned: Mutex<Option<String>>,
        review: Mutex<Option<SeenInvocation>>,
    }

    impl HostAdapter for PoisoningHost {
        fn invoke(
            &self,
            invocation: &Invocation,
            _observer: &mut dyn InvocationObserver,
        ) -> Result<InvocationResult> {
            let action: serde_json::Value = read_json(&invocation.action.join("action.json"))?;
            let kind = action["kind"].as_str().unwrap();
            match kind {
                "work" => {
                    let review_guide = invocation.build_dir.join("instructions").join("review.md");
                    let poison = "POISONED BY PREVIOUS ROLE";
                    fs::write(&review_guide, poison)?;
                    assert_eq!(fs::read_to_string(&review_guide)?, poison);
                    *self.poisoned.lock().unwrap() = Some(poison.into());
                    fs::write(invocation.cwd.join("work.txt"), "work\n")?;
                    git_ok(&invocation.cwd, &["add", "."]);
                    git_ok(&invocation.cwd, &["commit", "-m", "work"]);
                    let receipt = serde_json::json!({
                        "action_id": action["action_id"],
                        "scope": action["scope"],
                        "outcome": "complete",
                        "commit": git(&invocation.cwd, ["rev-parse", "HEAD"])?
                    });
                    write_bytes_sync(
                        &invocation.action.join("result.json"),
                        &serde_json::to_vec_pretty(&receipt)?,
                    )?;
                    Ok(InvocationResult {
                        completion: InvocationCompletion::Completed,
                    })
                }
                "review" => {
                    let instruction = PathBuf::from(action["instruction"].as_str().unwrap());
                    let body = fs::read_to_string(&instruction)?;
                    *self.review.lock().unwrap() = Some(SeenInvocation {
                        prompt: invocation.prompt.clone(),
                        action_json: invocation.action.join("action.json"),
                        instruction,
                        body,
                    });
                    Ok(InvocationResult {
                        completion: InvocationCompletion::AcceptedButIncomplete {
                            detail: "stop after observing review".into(),
                        },
                    })
                }
                other => bail!("poisoning host expected work then review, saw {other}"),
            }
        }
    }

    fn role_kinds() -> [ActionKind; 4] {
        [
            ActionKind::Work,
            ActionKind::Review,
            ActionKind::FinalAudit,
            ActionKind::Unblock,
        ]
    }

    fn assert_role_guides_match_canonical(instructions: &Path) {
        for kind in role_kinds() {
            let path = instructions.join(instruction_name(&kind));
            let body = fs::read(&path).unwrap();
            let canonical = canonical_role_guide(&kind);
            assert_eq!(
                body,
                canonical.as_bytes(),
                "{} drifted from the embedded role guide",
                path.display()
            );
            assert_eq!(
                build_provenance("build", canonical).guide_digest,
                digest_bytes(&body),
                "provenance must hash the same guide the model reads"
            );
        }
    }

    #[derive(Default)]
    struct StaleGuideHost {
        invocations: Mutex<Vec<SeenInvocation>>,
    }

    struct SeenInvocation {
        prompt: String,
        action_json: PathBuf,
        instruction: PathBuf,
        body: String,
    }

    impl HostAdapter for StaleGuideHost {
        fn invoke(
            &self,
            invocation: &Invocation,
            _observer: &mut dyn InvocationObserver,
        ) -> Result<InvocationResult> {
            let action: serde_json::Value = read_json(&invocation.action.join("action.json"))?;
            let instruction = PathBuf::from(action["instruction"].as_str().unwrap());
            let body = fs::read_to_string(&instruction)?;
            self.invocations.lock().unwrap().push(SeenInvocation {
                prompt: invocation.prompt.clone(),
                action_json: invocation.action.join("action.json"),
                instruction,
                body,
            });
            Ok(InvocationResult {
                completion: InvocationCompletion::AcceptedButIncomplete {
                    detail: "stop before role work".into(),
                },
            })
        }
    }
}
