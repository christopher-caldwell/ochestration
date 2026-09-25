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
    ArtifactKind, ArtifactRef, AuditAssessment, CoverageState, ImplementationStatus, Independence,
    Provenance, Verdict, decode, digest_bytes, safe_relative_path,
};
use orchestrate_core::{Effort, Project, Store, write_bytes_sync};
use serde::{Deserialize, Serialize};

static ACTION_COUNTER: AtomicU64 = AtomicU64::new(0);
pub const BUILD_PLAN_VERSION: u32 = 2;
pub const BUILD_CONFIG_VERSION: u32 = 2;
pub const BUILD_STATE_VERSION: u32 = 3;
const BUILD_STATE_MIGRATION: u32 = 1;
const RESOLUTION_SCHEMA_VERSION: u32 = 1;
const CONFIG_OVERLAY_SCHEMA_VERSION: u32 = 1;

/// Scope of the post-phase Audit turn, which is not one of the plan's delivery phases.
const FINAL_SCOPE: &str = "final";
/// Controller-owned Build directories that hold immutable records only.
const RESOLUTIONS_DIR: &str = "resolutions";
const CONFIG_HISTORY_DIR: &str = "config-history";

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
    Blocked {
        detail: String,
        state: PathBuf,
        /// Additive trigger data; existing fields keep their meaning.
        trigger: Option<String>,
        stopped_action: Option<String>,
        stop_record: Option<serde_json::Value>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildPlan {
    pub schema_version: u32,
    pub reconciled: ArtifactRef,
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
    discovery_baseline_commit: String,
    discovery_baseline_tree: String,
    build_start_commit: String,
    build_start_tree: String,
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
    /// Set only on the action an operator resolution authorized, so its packet
    /// can carry the recorded intervention instead of an implied one.
    #[serde(default)]
    resolution_id: Option<String>,
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
    #[serde(default)]
    migration_version: u32,
    #[serde(default)]
    stop: Option<StopRecord>,
    #[serde(default)]
    stop_history: Vec<StopRecord>,
    #[serde(default)]
    current_audit: Option<ArtifactRef>,
    #[serde(default)]
    applied_resolution_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum StopTrigger {
    SpawnFailure,
    ProviderExecutionFailure,
    UncertainAcceptance,
    IncompleteWork,
    InvalidReceipt,
    PhaseBlocker,
    AuditBlocked,
    ExternalRequirement,
    RecoveryExhausted,
    FrozenInputMismatch,
    ControllerFailure,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct StopRecord {
    pub action: CurrentAction,
    pub trigger: StopTrigger,
    #[serde(default)]
    pub action_failure: Option<StopTrigger>,
    pub stopped_at_ms: u128,
    pub controller_pid: u32,
    pub detail: String,
    pub process_completion: String,
    pub receipt_validation: String,
    pub recovery_remaining: Vec<String>,
    #[serde(default)]
    pub audit: Option<ArtifactRef>,
    #[serde(default)]
    pub assessment: Option<String>,
    #[serde(default)]
    pub unresolved_requirement_ids: Vec<String>,
    /// The action a pending automatic diagnosis was about, so a later
    /// resolution still knows what the stop interrupted.
    #[serde(default)]
    pub interrupted: Option<Box<CurrentAction>>,
}

/// A typed stop condition.  It travels as an error so the durable record never
/// has to infer the controller's reason from message text.
#[derive(Debug)]
struct BuildStop {
    trigger: StopTrigger,
    action_failure: Option<StopTrigger>,
    detail: String,
}

impl std::fmt::Display for BuildStop {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for BuildStop {}

fn stop_error(
    trigger: StopTrigger,
    action_failure: Option<StopTrigger>,
    detail: impl Into<String>,
) -> anyhow::Error {
    anyhow::Error::new(BuildStop {
        trigger,
        action_failure,
        detail: detail.into(),
    })
}

/// How the controller treats a role outcome that does not advance the Build.
#[derive(Clone, Debug)]
enum RecoveryCall {
    /// The role itself reported the work blocked.
    Blocked,
    /// The action failed before producing an advancing receipt.
    Failed(StopTrigger),
}

impl RecoveryCall {
    fn action_failure(&self) -> StopTrigger {
        match self {
            RecoveryCall::Blocked => StopTrigger::PhaseBlocker,
            RecoveryCall::Failed(trigger) => trigger.clone(),
        }
    }
}

/// What the operator asserts when resolving a stopped Build.
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionKind {
    /// The stopped role already had authority; the note is the clarification.
    ExistingAuthorityClarification,
    /// Access, environment or tooling was repaired outside the Build.
    EnvironmentRepair,
    /// New verification evidence for a blocked final Audit acceptance.
    NewVerificationEvidence,
    /// A proposed change to the adopted product authority, which Build refuses.
    AuthorityChange,
}

impl ResolutionKind {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "existing_authority_clarification" => Ok(ResolutionKind::ExistingAuthorityClarification),
            "environment_repair" => Ok(ResolutionKind::EnvironmentRepair),
            "new_verification_evidence" => Ok(ResolutionKind::NewVerificationEvidence),
            "authority_change" => Ok(ResolutionKind::AuthorityChange),
            other => bail!("unknown resolution kind {other}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ResolutionRequest {
    pub effort: String,
    /// Exact stopped action, as reported by the stopped Build.
    pub action: String,
    pub kind: ResolutionKind,
    pub note: String,
    pub evidence: Vec<PathBuf>,
    /// Explicit recorded confirmation that ambiguous in-flight work is no longer running.
    pub confirm_not_running: bool,
    /// New role configuration for future invocations, applied as an immutable overlay.
    pub config: Option<PathBuf>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum ResolutionOutcome {
    Resolved {
        resolution: PathBuf,
        resolution_id: String,
        kind: ResolutionKind,
        stopped_action: String,
        continuation_action: String,
        continuation_kind: String,
        config_version: Option<u32>,
    },
    Refused {
        resolution: PathBuf,
        resolution_id: String,
        reason: String,
        successor_guidance: String,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
enum ResolutionStatus {
    Resolved,
    Refused,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ResolutionRecord {
    schema_version: u32,
    resolution_id: String,
    kind: ResolutionKind,
    status: ResolutionStatus,
    created_at_ms: u128,
    effort_id: String,
    note: String,
    evidence: Vec<ResolutionEvidence>,
    binding: ResolutionBinding,
    transition: Option<ResolutionTransition>,
    config_overlay: Option<ConfigOverlay>,
    refusal: Option<ResolutionRefusal>,
}

/// Operator-supplied evidence, bound by digest so a later reader can verify it.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct ResolutionEvidence {
    path: String,
    sha256: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ResolutionBinding {
    stop: StopRecord,
    trigger: StopTrigger,
    action: CurrentAction,
    reconciled: ArtifactRef,
    plan_digest: String,
    detailed_plan_digest: String,
    config_digest: String,
    head_commit: String,
    checkout_status: String,
    in_flight_confirmation: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ResolutionTransition {
    /// The distinct continuation boundary this resolution authorizes.
    action: CurrentAction,
    /// The first action governed by this resolution, which the record also owns.
    governed_action: String,
    /// Whether the intervention restarted the bounded recovery budget.
    recovery_reset: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
struct ResolutionRefusal {
    reason: String,
    successor_guidance: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct ConfigOverlay {
    schema_version: u32,
    version: u32,
    config_digest: String,
    config_text: String,
    resolution_id: String,
    first_action_id: String,
    created_at_ms: u128,
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

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct BindingRequirements {
    schema_version: u32,
    source: ArtifactRef,
    source_json_sha256: String,
    requirements: Vec<orchestrate_contracts::ReconciledRequirement>,
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
    let config = effective_config(&build_dir)?;
    let plan = load_plan(store, &effort, &build_dir)?;
    materialize_role_guides(&build_dir)?;
    let state_path = build_dir.join("state.json");
    let (mut state, frozen_mismatch) = if state_path.exists() {
        load_state(&state_path, &plan, &build_dir)?
    } else {
        (initialize_state(store, &effort, &build_dir, &plan)?, None)
    };
    migrate_state(store, &effort, &build_dir, &state_path, &mut state)?;
    if let Some(detail) = frozen_mismatch {
        record_stop(
            store,
            &effort,
            &build_dir,
            &mut state,
            &stop_error(StopTrigger::FrozenInputMismatch, None, &detail),
        )?;
        save_state(&state_path, &state)?;
        return Ok(blocked_result(state_path, &state, detail));
    }
    // A resolution recorded before its state transition is the authority for
    // that transition, so an interrupted resolution completes exactly once.
    apply_pending_resolution(store, &effort, &build_dir, &state_path, &mut state)?;
    if let Some(done) = &state.terminal {
        return Ok(BuildResult::Completed(done.clone()));
    }
    if let Some(stop) = &state.stop {
        // A durable stop blocks a bare restart unless it left the single
        // bounded automatic continuation — one diagnostic Unblock turn — unrun.
        if !pending_diagnosis(&state, stop) {
            return Ok(blocked_result(state_path, &state, stop.detail.clone()));
        }
    }

    loop {
        let prior_action = state.action.clone();
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
                if state.action.id != prior_action.id {
                    journal_action_transition(store, &effort, &prior_action, &state.action)?;
                }
                save_state(&state_path, &state)?;
                if let Some(done) = &state.terminal {
                    return Ok(BuildResult::Completed(done.clone()));
                }
            }
            Err(error) => {
                record_stop(store, &effort, &build_dir, &mut state, &error)?;
                save_state(&state_path, &state)?;
                return Ok(blocked_result(
                    state_path,
                    &state,
                    format!("{error:#}"),
                ));
            }
        }
    }
}

fn blocked_result(state_path: PathBuf, state: &BuildState, detail: String) -> BuildResult {
    let stop = state.stop.as_ref();
    BuildResult::Blocked {
        detail,
        state: state_path,
        trigger: stop.map(|stop| trigger_name(&stop.trigger).to_owned()),
        stopped_action: stop.map(|stop| stop.action.id.clone()),
        stop_record: stop.and_then(|stop| serde_json::to_value(stop).ok()),
    }
}

fn trigger_name(trigger: &StopTrigger) -> &'static str {
    match trigger {
        StopTrigger::SpawnFailure => "spawn_failure",
        StopTrigger::ProviderExecutionFailure => "provider_execution_failure",
        StopTrigger::UncertainAcceptance => "uncertain_acceptance",
        StopTrigger::IncompleteWork => "incomplete_work",
        StopTrigger::InvalidReceipt => "invalid_receipt",
        StopTrigger::PhaseBlocker => "phase_blocker",
        StopTrigger::AuditBlocked => "audit_blocked",
        StopTrigger::ExternalRequirement => "external_requirement",
        StopTrigger::RecoveryExhausted => "recovery_exhausted",
        StopTrigger::FrozenInputMismatch => "frozen_input_mismatch",
        StopTrigger::ControllerFailure => "controller_failure",
    }
}

/// True when the current action is the one diagnostic Unblock turn the stop authorized.
fn pending_diagnosis(state: &BuildState, stop: &StopRecord) -> bool {
    matches!(state.action.kind, ActionKind::Unblock)
        && state
            .interrupted
            .as_deref()
            .is_some_and(|action| action.id == stop.action.id)
}

fn journal_action_transition(
    store: &Store,
    effort: &Effort,
    prior: &CurrentAction,
    next: &CurrentAction,
) -> Result<()> {
    let event = if matches!(next.kind, ActionKind::Unblock) {
        "build_unblock_started"
    } else if matches!(prior.kind, ActionKind::Unblock) {
        "build_unblock_resolved"
    } else {
        "build_action_transition"
    };
    append_journal_once(
        store,
        effort,
        event,
        &format!("{}->{}", prior.id, next.id),
        serde_json::json!({"from_action": prior, "to_action": next}),
    )
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
        let build = store.effort_dir(&effort).join("build");
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
    parse_config(&text)
}

fn parse_config(text: &str) -> Result<BuildConfig> {
    let config: BuildConfig = toml::from_str(text).context("invalid build/config.toml")?;
    ensure!(
        config.schema_version == BUILD_CONFIG_VERSION,
        "unsupported Build config version"
    );
    validate_role("worker", &config.worker)?;
    validate_role("reviewer", &config.reviewer)?;
    Ok(config)
}

/// The role configuration for the next invocation.  Version 1 is always the
/// frozen `config.toml`; higher versions only exist as immutable overlays an
/// operator resolution recorded.
struct EffectiveConfig {
    version: u32,
    config: BuildConfig,
}

fn effective_config(build_dir: &Path) -> Result<EffectiveConfig> {
    let mut effective = EffectiveConfig {
        version: 1,
        config: load_config(build_dir)?,
    };
    for overlay in config_overlays(build_dir)? {
        if overlay.version > effective.version {
            effective = EffectiveConfig {
                version: overlay.version,
                config: parse_config(&overlay.config_text)?,
            };
        }
    }
    Ok(effective)
}

/// Every recorded overlay, verified against its own bytes.  A corrupt or
/// conflicting history entry is a hard failure rather than a silent fallback.
fn config_overlays(build_dir: &Path) -> Result<Vec<ConfigOverlay>> {
    let dir = build_dir.join(CONFIG_HISTORY_DIR);
    let mut overlays = Vec::new();
    if !dir.is_dir() {
        return Ok(overlays);
    }
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let overlay: ConfigOverlay = read_json(&path)?;
        ensure!(
            overlay.schema_version == CONFIG_OVERLAY_SCHEMA_VERSION,
            "unsupported Build config overlay version in {}",
            path.display()
        );
        ensure!(
            digest_bytes(overlay.config_text.as_bytes()) == overlay.config_digest,
            "Build config overlay {} does not match its recorded digest",
            path.display()
        );
        parse_config(&overlay.config_text)?;
        overlays.push(overlay);
    }
    overlays.sort_by_key(|overlay| overlay.version);
    Ok(overlays)
}

fn next_config_version(build_dir: &Path) -> Result<u32> {
    Ok(config_overlays(build_dir)?
        .last()
        .map_or(2, |overlay| overlay.version + 1))
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
        plan.reconciled.kind == ArtifactKind::ReconciledDiscovery,
        "Build plan needs a Reconciled Discovery reference"
    );
    let (envelope, reconciled): (_, orchestrate_contracts::ReconciledDiscovery) =
        store.load_json(effort, &plan.reconciled, "reconciled-discovery.json")?;
    ensure!(
        envelope.outcome == "IMPLEMENTATION_READY"
            && reconciled.context_id == effort.context.id
            && reconciled.baseline_commit == effort.baseline_commit
            && reconciled.baseline_tree == effort.baseline_tree,
        "Build plan references an ineligible Reconciled Discovery"
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
    plan: &BuildPlan,
) -> Result<BuildState> {
    let (_, reconciled): (_, orchestrate_contracts::ReconciledDiscovery) =
        store.load_json(effort, &plan.reconciled, "reconciled-discovery.json")?;
    let project = store.project_for(effort)?;
    ensure!(
        git(
            &project.canonical_locator,
            ["status", "--porcelain=v1", "--untracked-files=all"]
        )?
        .is_empty(),
        "cannot start a new Build: the product checkout has staged, unstaged, or untracked changes; commit, move, or remove them before starting Build"
    );
    let adoption = orchestrate_audit::adopt_for_build(
        store,
        effort,
        plan.reconciled.clone(),
        build_provenance("build", orchestrate_guides::BUILD),
    )?;
    let build_start = store.materialize_snapshot(&project.canonical_locator, "HEAD")?;
    let ancestry = Command::new("git")
        .args([
            "merge-base",
            "--is-ancestor",
            &reconciled.baseline_commit,
            &build_start.commit,
        ])
        .current_dir(&project.canonical_locator)
        .status()?;
    ensure!(
        ancestry.success(),
        "current Build HEAD is not descended from the Discovery baseline"
    );
    let phase = &plan.delivery_phases[0].id;
    let state = BuildState {
        schema_version: BUILD_STATE_VERSION,
        frozen: FrozenInputs {
            adoption,
            reconciled: plan.reconciled.clone(),
            discovery_baseline_commit: reconciled.baseline_commit,
            discovery_baseline_tree: reconciled.baseline_tree,
            build_start_commit: build_start.commit,
            build_start_tree: build_start.tree,
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
        migration_version: BUILD_STATE_MIGRATION,
        stop: None,
        stop_history: Vec::new(),
        current_audit: None,
        applied_resolution_id: None,
    };
    save_state(&build_dir.join("state.json"), &state)?;
    Ok(state)
}

fn migrate_state(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    state_path: &Path,
    state: &mut BuildState,
) -> Result<()> {
    if state.migration_version >= BUILD_STATE_MIGRATION {
        return Ok(());
    }
    let original = fs::read(state_path)?;
    let evidence_dir = build_dir.join("evidence");
    fs::create_dir_all(&evidence_dir)?;
    let original_path = evidence_dir.join("state-v3-original.json");
    match OpenOptions::new().write(true).create_new(true).open(&original_path) {
        Ok(mut file) => {
            file.write_all(&original)?;
            file.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            ensure!(fs::read(&original_path)? == original, "preserved state-v3 bytes differ from current migration source");
        }
        Err(error) => return Err(error.into()),
    }
    state.migration_version = BUILD_STATE_MIGRATION;
    let original_digest = digest_bytes(&original);
    append_journal_once(
        store,
        effort,
        "build_state_migrated",
        &original_digest,
        serde_json::json!({"from": BUILD_STATE_VERSION, "to": BUILD_STATE_VERSION, "migration": BUILD_STATE_MIGRATION, "original_sha256": original_digest, "original_state": original_path}),
    )?;
    save_state(state_path, state)?;
    Ok(())
}

fn append_journal_once(
    store: &Store,
    effort: &Effort,
    event: &str,
    operation_id: &str,
    details: serde_json::Value,
) -> Result<()> {
    if store.read_journal(effort)?.iter().any(|entry| {
        entry.event == event && entry.details["operation_id"] == operation_id
    }) {
        return Ok(());
    }
    let mut details = details;
    details["operation_id"] = serde_json::json!(operation_id);
    store.append_journal(effort, event, Some(operation_id), details)
}

/// Load one Build state and report, rather than raise, a frozen input that
/// changed after execution began.  The caller records that as a typed stop.
fn load_state(
    path: &Path,
    plan: &BuildPlan,
    build_dir: &Path,
) -> Result<(BuildState, Option<String>)> {
    let state: BuildState = read_json(path)?;
    ensure!(
        state.schema_version == BUILD_STATE_VERSION,
        "unsupported Build state version"
    );
    let inputs = [
        ("plan.json", "Build plan", state.frozen.plan_digest.as_str()),
        (
            "config.toml",
            "Build config",
            state.frozen.config_digest.as_str(),
        ),
        (
            plan.detailed_plan.as_str(),
            "detailed implementation plan",
            state.frozen.detailed_plan_digest.as_str(),
        ),
    ];
    let mut mismatch = None;
    for (name, label, expected) in inputs {
        let actual = fs::read(build_dir.join(name)).map(|bytes| digest_bytes(&bytes));
        if actual.as_deref().ok() != Some(expected) {
            mismatch.get_or_insert(format!("{label} changed after execution began"));
        }
    }
    Ok((state, mismatch))
}

#[allow(clippy::too_many_arguments)]
fn perform_action(
    store: &Store,
    effort: &Effort,
    project: &Project,
    build_dir: &Path,
    config: &EffectiveConfig,
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
                state.frozen.build_start_commit.clone(),
                state.frozen.build_start_tree.clone(),
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
        &config.config.worker
    } else {
        &config.config.reviewer
    };
    let action_dir = build_dir.join("artifacts").join(&state.action.id);
    fs::create_dir_all(&action_dir)?;
    let completion_marker = action_dir.join("transport.completed");
    if state.action.dispatch == DispatchState::Running {
        if completion_marker.is_file() {
            state.action.dispatch = DispatchState::Completed;
            save_state(state_path, state)?;
        } else {
            return Err(stop_error(
                StopTrigger::UncertainAcceptance,
                None,
                format!(
                    "provider invocation for action {} may still be running; it will not be resent automatically",
                    state.action.id
                ),
            ));
        }
    }
    // Generated role instructions are refreshed immediately before dispatch so an
    // earlier role cannot alter the authority used by a later role.
    if state.action.dispatch == DispatchState::Prepared {
        materialize_role_guide(build_dir, &state.action.kind)?;
    }
    let cwd = match state.action.kind {
        ActionKind::Review => contained_checkout(
            project,
            &action_dir,
            state
                .action
                .target_commit
                .as_deref()
                .context("review lacks target commit")?,
            "source",
        )?,
        ActionKind::FinalAudit => contained_checkout(
            project,
            &action_dir,
            state
                .action
                .target_commit
                .as_deref()
                .context("final Audit lacks target commit")?,
            "verification",
        )?,
        _ => project.canonical_locator.clone(),
    };
    write_action_file(
        store,
        effort,
        build_dir,
        plan,
        state,
        &action_dir,
        &cwd,
        config.version,
    )?;
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
            InvocationCompletion::FailedBeforeAcceptance { detail } => {
                recover_or_block(
                    store,
                    effort,
                    build_dir,
                    state,
                    RecoveryCall::Failed(StopTrigger::SpawnFailure),
                    role,
                    &detail,
                )?;
                return Ok(());
            }
            InvocationCompletion::AcceptedButIncomplete { detail } => {
                // Without a transport identity the acceptance state is unknown: the invocation
                // may or may not have started, so it is never resent.
                ensure!(
                    state.action.transport_session_id.is_some(),
                    stop_error(
                        StopTrigger::UncertainAcceptance,
                        None,
                        format!(
                            "action {} ended without a resumable transport identity ({detail}); its acceptance state is unknown and it will not be resent automatically. Inspect {} before resuming.",
                            state.action.id,
                            action_dir.join("transport.jsonl").display()
                        )
                    )
                );
                recover_or_block(
                    store,
                    effort,
                    build_dir,
                    state,
                    RecoveryCall::Failed(StopTrigger::ProviderExecutionFailure),
                    role,
                    &detail,
                )?;
                return Ok(());
            }
        }
    }
    let receipt: Receipt = read_json(&action_dir.join("result.json"))
        .context("provider completed without a valid Build result receipt")
        .map_err(receipt_failure)?;
    consume_receipt(
        store,
        effort,
        build_dir,
        plan,
        state,
        &receipt,
        &action_dir,
    )
    .map_err(receipt_failure)
}

/// A receipt that cannot be validated stops the Build as a receipt problem
/// unless it already carries a more specific typed stop.
fn receipt_failure(error: anyhow::Error) -> anyhow::Error {
    match error.downcast::<BuildStop>() {
        Ok(stop) => anyhow::Error::new(stop),
        Err(error) => stop_error(
            StopTrigger::InvalidReceipt,
            Some(StopTrigger::InvalidReceipt),
            format!("{error:#}"),
        ),
    }
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

#[allow(clippy::too_many_arguments)]
fn write_action_file(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    plan: &BuildPlan,
    state: &BuildState,
    action_dir: &Path,
    cwd: &Path,
    config_version: u32,
) -> Result<()> {
    let requirement_path =
        ensure_requirement_projection(store, effort, action_dir, &state.frozen.reconciled)?;
    let binding_requirements: BindingRequirements = read_json(&requirement_path)?;
    let phase_data = if state.action.scope == FINAL_SCOPE {
        Some((Vec::new(), String::new(), String::new()))
    } else {
        let phase_tasks = plan
            .delivery_phases
            .iter()
            .find(|phase| phase.id == state.action.scope)
            .map(|phase| phase.tasks.as_slice())
            .unwrap_or(&[]);
        phase_authority(
            &fs::read_to_string(build_dir.join(&plan.detailed_plan))?,
            phase_tasks,
        )
    }
    .with_context(|| {
        format!(
            "detailed plan does not document authority scope {}",
            state.action.scope
        )
    })?;
    let (phase_requirements, completion_evidence, exclusions) = phase_data;
    let requirement_ids = binding_requirements
        .requirements
        .iter()
        .map(|item| item.requirement.id.as_str())
        .collect::<HashSet<_>>();
    ensure!(
        phase_requirements
            .iter()
            .all(|id| requirement_ids.contains(id.as_str())),
        "phase authority for {} references an unknown binding requirement",
        state.action.scope
    );
    let tasks = plan
        .delivery_phases
        .iter()
        .find(|phase| phase.id == state.action.scope)
        .map(|phase| phase.tasks.clone())
        .unwrap_or_default();
    let instruction = build_dir
        .join("instructions")
        .join(instruction_name(&state.action.kind));
    let role = match state.action.kind {
        ActionKind::Work => "worker",
        ActionKind::Review | ActionKind::FinalAudit | ActionKind::Unblock => "reviewer",
    };
    let mut value = serde_json::json!({
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
        "reconciled_discovery": store
            .artifact_dir(effort, &state.frozen.reconciled.artifact_id)?
            .join("reconciled-discovery.json"),
        "binding_requirements": requirement_path,
        "phase_authority": { "requirements": phase_requirements, "completion_evidence": completion_evidence, "exclusions": exclusions },
        "adoption": state.frozen.adoption,
        "result": action_dir.join("result.json"),
        "report": action_dir.join("report.md"),
        "detailed_plan": build_dir.join(&plan.detailed_plan),
        "working_directory": cwd,
        "config_version": config_version,
    });
    if let Some(resolution_id) = &state.action.resolution_id {
        let resolution = build_dir
            .join(RESOLUTIONS_DIR)
            .join(format!("{resolution_id}.json"));
        let record: ResolutionRecord = read_json(&resolution)?;
        value["resolution"] = serde_json::json!(resolution);
        value["resolution_kind"] = serde_json::to_value(record.kind)?;
        value["resolution_note"] = serde_json::json!(record.note);
        value["resolution_evidence"] = serde_json::json!(record.evidence);
    }
    if matches!(state.action.kind, ActionKind::Unblock) {
        let interrupted = state
            .interrupted
            .as_deref()
            .context("unblock action has no interrupted action")?;
        let interrupted_dir = build_dir.join("artifacts").join(&interrupted.id);
        value["interrupted_action"] = serde_json::json!({
            "kind": interrupted.kind,
            "scope": interrupted.scope,
            "target_commit": interrupted.target_commit,
        });
        value["interrupted_report"] = serde_json::json!(interrupted_dir.join("report.md"));
        value["interrupted_result"] = serde_json::json!(interrupted_dir.join("result.json"));
        value["prior_feedback"] = serde_json::json!(interrupted.feedback_path);
        if let Some(stop) = &state.stop {
            value["trigger"] = serde_json::to_value(&stop.trigger)?;
            value["stop_record"] = serde_json::to_value(stop)?;
            value["process_completion"] = serde_json::json!(stop.process_completion);
            value["receipt_validation"] = serde_json::json!(stop.receipt_validation);
            value["recovery_remaining"] = serde_json::json!(stop.recovery_remaining);
            value["current_audit"] = serde_json::json!(stop.audit);
            value["assessment"] = serde_json::json!(stop.assessment);
            value["unresolved_requirement_ids"] = serde_json::json!(stop.unresolved_requirement_ids);
        }
    }
    if matches!(state.action.kind, ActionKind::FinalAudit) {
        let implementation = state
            .implementation
            .as_ref()
            .context("final Audit implementation was not registered")?;
        let target_commit = state
            .action
            .target_commit
            .as_deref()
            .context("final Audit lacks target commit")?;
        value["adoption_receipt"] = serde_json::json!(
            store
                .artifact_dir(effort, &state.frozen.adoption.artifact_id)?
                .join("adoption.json")
        );
        value["implementation_record"] = serde_json::json!(
            store
                .artifact_dir(effort, &implementation.artifact_id)?
                .join("implementation.json")
        );
        value["registered_snapshot"] = serde_json::json!(
            store
                .root()
                .join("snapshots")
                .join(target_commit)
                .join("source")
        );
        value["verification_checkout"] = serde_json::json!(cwd);
        value["assessment"] = serde_json::json!(action_dir.join("assessment.json"));
        value["audit_attempt_id"] = serde_json::json!(state.action.id);
        value["current_audit"] = serde_json::json!(state.current_audit);
    }
    write_bytes_sync(
        &action_dir.join("action.json"),
        &serde_json::to_vec_pretty(&value)?,
    )
}

fn ensure_requirement_projection(
    store: &Store,
    effort: &Effort,
    action_dir: &Path,
    reference: &ArtifactRef,
) -> Result<PathBuf> {
    let artifact_dir = store.artifact_dir(effort, &reference.artifact_id)?;
    let source = artifact_dir.join("reconciled-discovery.json");
    let source_bytes = fs::read(&source).with_context(|| format!("read {}", source.display()))?;
    let (_, reconciled): (_, orchestrate_contracts::ReconciledDiscovery) =
        store.load_json(effort, reference, "reconciled-discovery.json")?;
    let projection = BindingRequirements {
        schema_version: 1,
        source: reference.clone(),
        source_json_sha256: digest_bytes(&source_bytes),
        requirements: reconciled.requirements,
    };
    let path = action_dir.join("binding-requirements.json");
    let bytes = serde_json::to_vec_pretty(&projection)?;
    if path.exists() {
        let existing: BindingRequirements = read_json(&path)?;
        ensure!(
            existing == projection,
            "binding requirement projection differs from its immutable Reconciled Discovery source"
        );
    } else {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_bytes_sync(&path, &bytes)?;
    }
    Ok(path)
}

fn phase_authority(plan: &str, tasks: &[String]) -> Option<(Vec<String>, String, String)> {
    if tasks.is_empty() {
        return None;
    }
    let headings = plan
        .match_indices("## Delivery phase ")
        .map(|(start, _)| start)
        .collect::<Vec<_>>();
    let section_start = headings.iter().enumerate().find_map(|(index, start)| {
        let end = headings.get(index + 1).copied().unwrap_or(plan.len());
        let candidate = &plan[*start..end];
        tasks
            .iter()
            .all(|task| {
                candidate.lines().any(|line| {
                    let item = line.trim_start().strip_prefix("- ").unwrap_or("").trim();
                    task_line(item, task)
                })
            })
            .then_some(*start)
    })?;
    let tail = &plan[section_start..];
    let body = tail.split_once('\n').map(|(_, body)| body).unwrap_or("");
    let body = body.split("\n## Delivery phase ").next().unwrap_or(body);
    let section = |label: &str| -> String {
        let prefix = format!("**{label}:**");
        body.lines()
            .find_map(|line| line.strip_prefix(&prefix).map(str::trim).map(str::to_owned))
            .unwrap_or_default()
    };
    let requirements = section("Requirements")
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    Some((
        requirements,
        section("Completion evidence"),
        section("Deliberate later-phase exclusions"),
    ))
}

/// A documented task line, in any of the forms a detailed plan may use.  The
/// separator keeps `D1` from matching a `D10` line.
fn task_line(item: &str, task: &str) -> bool {
    item == task
        || [" ", ":", " —"].iter().any(|separator| {
            item.strip_prefix(task)
                .is_some_and(|rest| rest.starts_with(separator))
        })
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
            } else if receipt.outcome == "blocked" {
                recover_or_block(
                    store,
                    effort,
                    build_dir,
                    state,
                    RecoveryCall::Blocked,
                    "worker",
                    "worker receipt reported blocked",
                )?;
            } else {
                recover_or_block(
                    store,
                    effort,
                    build_dir,
                    state,
                    RecoveryCall::Failed(StopTrigger::IncompleteWork),
                    "worker",
                    "worker receipt reported incomplete work",
                )?;
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
                recover_or_block(
                    store,
                    effort,
                    build_dir,
                    state,
                    RecoveryCall::Blocked,
                    "reviewer",
                    "review receipt reported blocked",
                )?;
            }
        }
        ActionKind::FinalAudit => {
            ensure!(
                matches!(receipt.outcome.as_str(), "complete" | "blocked"),
                "Audit action receipt has invalid outcome"
            );
            if receipt.outcome == "blocked" {
                recover_or_block(
                    store,
                    effort,
                    build_dir,
                    state,
                    RecoveryCall::Blocked,
                    "reviewer",
                    "Audit receipt reported blocked",
                )?;
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
            // This attempt's exact identity is its operation identity: a replay
            // republishes the same bundle, and a genuinely new attempt at the same
            // implementation publishes separately.  The Build consumes the attempt
            // it just published, never an Audit chosen by implementation or recency.
            let audit_run_id = format!("build-audit-{}", state.action.id);
            let audit = orchestrate_audit::finalize_audit_with_run_id(
                store,
                effort,
                assessment.clone(),
                build_provenance("audit", canonical_role_guide(&ActionKind::FinalAudit)),
                audit_run_id,
            )?;
            state.current_audit = Some(audit.clone());
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
                Verdict::Blocked => {
                    let ids = unresolved_requirement_ids(
                        store,
                        effort,
                        &state.frozen.reconciled,
                        &report.assessment,
                    )?;
                    // A completed assessment is never retried by continuation or replacement:
                    // at most one Unblock diagnosis may run, and a remedy may authorize one
                    // fresh reassessment.  A second completed BLOCKED assessment stops here.
                    if state.recovery.unblock_used {
                        return Err(stop_error(
                            StopTrigger::AuditBlocked,
                            Some(StopTrigger::AuditBlocked),
                            format!(
                                "a second completed final Audit remains BLOCKED; unresolved requirements: {}",
                                ids.join(", ")
                            ),
                        ));
                    }
                    let stopped = state.action.clone();
                    let stop = stop_record(
                        store,
                        effort,
                        build_dir,
                        state,
                        &stopped,
                        StopTrigger::AuditBlocked,
                        Some(StopTrigger::AuditBlocked),
                        format!("completed formal Audit {} derived BLOCKED", audit.artifact_id),
                        vec!["one unblock diagnosis".into()],
                        Some(Box::new(stopped.clone())),
                    )?;
                    push_stop(store, effort, state, stop)?;
                    state.interrupted = Some(Box::new(state.action.clone()));
                    state.recovery.unblock_used = true;
                    state.action = new_action(
                        build_dir,
                        ActionKind::Unblock,
                        FINAL_SCOPE,
                        state.action.target_commit.clone(),
                        None,
                    );
                }
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
            // Both outcomes must name the authorized condition that changes on
            // retry, or the missing evidence and capability.  Regenerating an
            // unchanged completed assessment is not a remedy.
            ensure!(
                fs::read_to_string(action_dir.join("report.md"))
                    .map(|report| !report.trim().is_empty())
                    .unwrap_or(false),
                "unblock receipt claimed {} without a non-empty diagnosis report",
                receipt.outcome
            );
            if receipt.outcome == "remedy_available" {
                let interrupted = state
                    .interrupted
                    .take()
                    .context("unblock action has no interrupted action to resume")?;
                state.recovery.after_remedy();
                state.stop = None;
                state.action = new_action(
                    build_dir,
                    interrupted.kind,
                    &interrupted.scope,
                    interrupted.target_commit,
                    Some(action_dir.join("report.md")),
                );
                return Ok(());
            }
            return Err(stop_error(
                StopTrigger::ExternalRequirement,
                Some(StopTrigger::ExternalRequirement),
                format!(
                    "unblocker identified an external requirement; see {}",
                    action_dir.join("report.md").display()
                ),
            ));
        }
    }
    Ok(())
}

/// A failed or blocked turn retries in a fixed order: same session, then a replacement session,
/// then one fresh session-less unblocker diagnosis.  Each flag is consumed once, so an
/// unattended run always reaches a decision instead of looping.
fn recover_or_block(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    state: &mut BuildState,
    reason: RecoveryCall,
    role: &str,
    detail: &str,
) -> Result<()> {
    let action_failure = reason.action_failure();
    if matches!(reason, RecoveryCall::Blocked) && !state.recovery.unblock_used {
        return start_unblock(
            store,
            effort,
            build_dir,
            state,
            StopTrigger::PhaseBlocker,
            action_failure,
            detail,
        );
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
        return start_unblock(
            store,
            effort,
            build_dir,
            state,
            StopTrigger::RecoveryExhausted,
            action_failure,
            detail,
        );
    }
    Err(stop_error(
        StopTrigger::RecoveryExhausted,
        Some(action_failure),
        format!("recovery is exhausted for scope {} ({detail})", state.action.scope),
    ))
}

fn record_stop(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    state: &mut BuildState,
    error: &anyhow::Error,
) -> Result<()> {
    let typed = error.downcast_ref::<BuildStop>();
    let trigger = typed.map_or_else(
        || fallback_trigger(&state.action),
        |stop| stop.trigger.clone(),
    );
    let action = state.action.clone();
    let record = stop_record(
        store,
        effort,
        build_dir,
        state,
        &action,
        trigger,
        typed.and_then(|stop| stop.action_failure.clone()),
        format!("{error:#}"),
        recovery_remaining(state),
        state.interrupted.clone(),
    )?;
    push_stop(store, effort, state, record)
}

/// Conditions the controller can still name without a typed stop, for example an
/// unexpected I/O failure around an action.
fn fallback_trigger(action: &CurrentAction) -> StopTrigger {
    match action.dispatch {
        DispatchState::Running => StopTrigger::UncertainAcceptance,
        DispatchState::Completed => StopTrigger::InvalidReceipt,
        DispatchState::Prepared => StopTrigger::ControllerFailure,
    }
}

/// The remaining bounded recovery budget, as durable recovery facts.
fn recovery_remaining(state: &BuildState) -> Vec<String> {
    [
        (!state.recovery.continuation_used).then_some("one continuation of the stopped action"),
        (!state.recovery.replacement_used).then_some("one replacement session"),
        (!state.recovery.unblock_used).then_some("one unblock diagnosis"),
    ]
    .into_iter()
    .flatten()
    .map(str::to_owned)
    .collect()
}

#[allow(clippy::too_many_arguments)]
fn stop_record(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    state: &BuildState,
    action: &CurrentAction,
    trigger: StopTrigger,
    action_failure: Option<StopTrigger>,
    detail: String,
    recovery_remaining: Vec<String>,
    interrupted: Option<Box<CurrentAction>>,
) -> Result<StopRecord> {
    let result_path = build_dir.join("artifacts").join(&action.id).join("result.json");
    let (audit, assessment, unresolved_requirement_ids) =
        if let Some(reference) = &state.current_audit {
            let (_, report): (_, orchestrate_contracts::AuditReport) =
                store.load_json(effort, reference, "audit.json")?;
            let unresolved = unresolved_requirement_ids(
                store,
                effort,
                &state.frozen.reconciled,
                &report.assessment,
            )?;
            let path = store
                .artifact_dir(effort, &reference.artifact_id)?
                .join("audit.json");
            (
                Some(reference.clone()),
                Some(path.to_string_lossy().into_owned()),
                unresolved,
            )
        } else {
            (None, None, Vec::new())
        };
    let receipt_validation = match (action_failure.as_ref(), result_path.is_file()) {
        (Some(StopTrigger::InvalidReceipt), true) => {
            "receipt present; its validation failed".to_owned()
        }
        (Some(StopTrigger::InvalidReceipt), false) => "receipt missing".to_owned(),
        (Some(StopTrigger::IncompleteWork), _) => {
            "receipt present; the role reported incomplete work".to_owned()
        }
        (Some(StopTrigger::ProviderExecutionFailure), _) => {
            "no valid receipt; the provider process did not complete".to_owned()
        }
        (Some(StopTrigger::SpawnFailure), _) => {
            "no receipt; the provider action was never accepted".to_owned()
        }
        (Some(StopTrigger::PhaseBlocker), _) => {
            "receipt present; the role reported the work blocked".to_owned()
        }
        (Some(StopTrigger::AuditBlocked), _) => {
            "complete receipt and assessment lineage validated".to_owned()
        }
        (_, true) => "receipt present; the transition did not complete".to_owned(),
        (_, false) => "receipt missing; the action produced no result".to_owned(),
    };
    Ok(StopRecord {
        action: action.clone(),
        trigger,
        action_failure,
        stopped_at_ms: now_ms(),
        controller_pid: std::process::id(),
        detail,
        process_completion: match action.dispatch {
            DispatchState::Prepared => {
                "the provider action was not accepted; it never started or its start is unknown".into()
            }
            DispatchState::Running => "accepted; provider completion is uncertain".into(),
            DispatchState::Completed => "provider process completed".into(),
        },
        receipt_validation,
        recovery_remaining,
        audit,
        assessment,
        unresolved_requirement_ids,
        interrupted,
    })
}

/// Record one durable stop, once per stopped action.
fn push_stop(
    store: &Store,
    effort: &Effort,
    state: &mut BuildState,
    record: StopRecord,
) -> Result<()> {
    if state
        .stop_history
        .iter()
        .any(|entry| entry.action.id == record.action.id)
    {
        return Ok(());
    }
    append_journal_once(
        store,
        effort,
        "build_stopped",
        &record.action.id,
        serde_json::to_value(&record)?,
    )?;
    state.stop = Some(record.clone());
    state.stop_history.push(record);
    Ok(())
}

fn unresolved_requirement_ids(
    store: &Store,
    effort: &Effort,
    reconciled_ref: &ArtifactRef,
    assessment: &AuditAssessment,
) -> Result<Vec<String>> {
    let (_, reconciled): (_, orchestrate_contracts::ReconciledDiscovery) =
        store.load_json(effort, reconciled_ref, "reconciled-discovery.json")?;
    let coverage = assessment
        .coverage
        .iter()
        .map(|row| (row.requirement_id.as_str(), row.state.clone()))
        .collect::<std::collections::HashMap<_, _>>();
    Ok(reconciled
        .requirements
        .iter()
        .filter_map(|item| match coverage.get(item.requirement.id.as_str()) {
            Some(CoverageState::Unknown) | None => Some(item.requirement.id.clone()),
            _ => None,
        })
        .collect())
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
fn start_unblock(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    state: &mut BuildState,
    trigger: StopTrigger,
    action_failure: StopTrigger,
    detail: &str,
) -> Result<()> {
    state.recovery.unblock_used = true;
    let interrupted = state.action.clone();
    let record = stop_record(
        store,
        effort,
        build_dir,
        state,
        &interrupted,
        trigger,
        Some(action_failure),
        detail.into(),
        recovery_remaining(state),
        Some(Box::new(interrupted.clone())),
    )?;
    push_stop(store, effort, state, record)?;
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
        resolution_id: None,
    }
}

/// The continuation boundary a resolution authorizes.  Its identity derives from
/// the resolution record, so re-applying an interrupted resolution yields the
/// same distinct action instead of a second, uncontrolled one.
fn continuation_action(
    resolution_id: &str,
    kind: ActionKind,
    scope: &str,
    target_commit: Option<String>,
    feedback_path: Option<String>,
) -> CurrentAction {
    CurrentAction {
        id: format!(
            "act-{}",
            &digest_bytes(format!("{resolution_id}:continuation").as_bytes())[..24]
        ),
        kind,
        scope: scope.into(),
        target_commit,
        feedback_path,
        transport_session_id: None,
        dispatch: DispatchState::Prepared,
        resolution_id: Some(resolution_id.into()),
    }
}

fn action_kind_name(kind: &ActionKind) -> &'static str {
    match kind {
        ActionKind::Work => "work",
        ActionKind::Review => "review",
        ActionKind::FinalAudit => "final_audit",
        ActionKind::Unblock => "unblock",
    }
}

fn resolution_kind_name(kind: ResolutionKind) -> &'static str {
    match kind {
        ResolutionKind::ExistingAuthorityClarification => "existing_authority_clarification",
        ResolutionKind::EnvironmentRepair => "environment_repair",
        ResolutionKind::NewVerificationEvidence => "new_verification_evidence",
        ResolutionKind::AuthorityChange => "authority_change",
    }
}

// ---------------------------------------------------------------------------
// Operator resolution of a stopped Build
// ---------------------------------------------------------------------------

/// Resolve a stopped Build: record the exact operator intervention, then create
/// the one continuation boundary it authorizes.  Resolution itself dispatches no
/// provider action; the next Build invocation is still the dispatch entry point.
pub fn resolve(store: &Store, request: ResolutionRequest) -> Result<ResolutionOutcome> {
    let effort = store.load_effort(&request.effort)?;
    let project = store.project_for(&effort)?;
    let _lock = ProjectLock::acquire(store, &project)?;
    let build_dir = store.phase_dir(&effort, "build")?;
    let state_path = build_dir.join("state.json");
    ensure!(
        state_path.is_file(),
        "no Build state exists for effort {}",
        effort.id
    );
    let plan = load_plan(store, &effort, &build_dir)?;
    let (mut state, frozen_mismatch) = load_state(&state_path, &plan, &build_dir)?;
    if let Some(detail) = frozen_mismatch {
        bail!(
            "cannot resolve: {detail}; frozen Build input changes need their own successor path, not a resolution"
        );
    }
    ensure!(
        !request.note.trim().is_empty(),
        "a resolution requires a note recording the intervention"
    );
    let evidence = read_evidence(&request.evidence)?;
    let config_text = match &request.config {
        Some(path) => {
            ensure!(
                matches!(request.kind, ResolutionKind::EnvironmentRepair),
                "role configuration changes require an environment_repair resolution"
            );
            let text = fs::read_to_string(path)
                .with_context(|| format!("cannot read {}", path.display()))?;
            parse_config(&text).context("the supplied role configuration is not a valid Build config")?;
            Some(text)
        }
        None => None,
    };
    let resolution_id = resolution_id(
        &request.action,
        request.kind,
        &request.note,
        &evidence,
        config_text.as_deref(),
    );
    let record_path = resolution_path(&build_dir, &resolution_id);
    if record_path.is_file() {
        // An identical submission is idempotent: the record already owns its transition.
        let existing: ResolutionRecord = read_json(&record_path)?;
        ensure!(
            existing.resolution_id == resolution_id,
            "resolution record {} is inconsistent",
            record_path.display()
        );
        return Ok(resolution_outcome(&record_path, &existing));
    }
    // A different submission for a stop that an applied resolution already
    // continued is stale, even though the Build has moved past that stop.
    if let Some(applied) = applied_resolution_for_stop(&build_dir, &request.action)? {
        bail!(
            "stopped action {} was already resolved by {}; the same stop accepts no second resolution",
            request.action,
            applied.resolution_id
        );
    }
    ensure!(
        state.terminal.is_none(),
        "Build {} already completed; there is no stopped action to resolve",
        effort.id
    );
    let stop = state
        .stop
        .clone()
        .context("this Build is not stopped; there is nothing to resolve")?;
    ensure!(
        stop.action.id == request.action,
        "action {} is not the stopped action {} of effort {}",
        request.action,
        stop.action.id,
        effort.id
    );
    // Ambiguous in-flight provider work is never resent; it needs an explicit
    // recorded confirmation that it is no longer running.
    let in_flight = stop.action.dispatch == DispatchState::Running
        || state.action.dispatch == DispatchState::Running;
    ensure!(
        !in_flight || request.confirm_not_running,
        "action {} was accepted and its completion is uncertain; confirm the provider is no longer running before a fresh continuation boundary is created",
        if stop.action.dispatch == DispatchState::Running { stop.action.id.as_str() } else { state.action.id.as_str() }
    );
    let binding = ResolutionBinding {
        stop: stop.clone(),
        trigger: stop.trigger.clone(),
        action: stop.action.clone(),
        reconciled: state.frozen.reconciled.clone(),
        plan_digest: state.frozen.plan_digest.clone(),
        detailed_plan_digest: state.frozen.detailed_plan_digest.clone(),
        config_digest: state.frozen.config_digest.clone(),
        head_commit: git(&project.canonical_locator, ["rev-parse", "HEAD"])?,
        checkout_status: checkout_status(&project)?,
        in_flight_confirmation: request.confirm_not_running,
    };
    if matches!(request.kind, ResolutionKind::AuthorityChange) {
        let refusal = ResolutionRefusal {
            reason: "a Build resolution cannot amend the adopted product authority".into(),
            successor_guidance: format!(
                "take the change through a new Discovery and Reconcile, then start a successor Build that links this effort's exact Reconciled Discovery {} and its commits; nothing in this Build changes",
                state.frozen.reconciled.artifact_id
            ),
        };
        let record = ResolutionRecord {
            schema_version: RESOLUTION_SCHEMA_VERSION,
            resolution_id: resolution_id.clone(),
            kind: request.kind,
            status: ResolutionStatus::Refused,
            created_at_ms: now_ms(),
            effort_id: effort.id.clone(),
            note: request.note.clone(),
            evidence,
            binding,
            transition: None,
            config_overlay: None,
            refusal: Some(refusal.clone()),
        };
        write_immutable(&record_path, &orchestrate_contracts::encode(&record)?)?;
        append_journal_once(
            store,
            &effort,
            "build_resolution_refused",
            &resolution_id,
            serde_json::json!({
                "resolution": resolution_id,
                "kind": resolution_kind_name(request.kind),
                "stopped_action": stop.action.id,
                "record": record_path,
                "reason": refusal.reason,
                "successor_guidance": refusal.successor_guidance,
            }),
        )?;
        return Ok(ResolutionOutcome::Refused {
            resolution: record_path,
            resolution_id,
            reason: refusal.reason,
            successor_guidance: refusal.successor_guidance,
        });
    }
    let (next_kind, scope, target_commit, feedback) = continuation_for(&state, &stop, request.kind)?;
    let continuation = continuation_action(&resolution_id, next_kind, &scope, target_commit, feedback);
    let mut config_overlay = config_text.map(|text| ConfigOverlay {
        schema_version: CONFIG_OVERLAY_SCHEMA_VERSION,
        version: 0,
        config_digest: digest_bytes(text.as_bytes()),
        config_text: text,
        resolution_id: resolution_id.clone(),
        first_action_id: continuation.id.clone(),
        created_at_ms: now_ms(),
    });
    if let Some(overlay) = config_overlay.as_mut() {
        overlay.version = next_config_version(&build_dir)?;
    }
    let record = ResolutionRecord {
        schema_version: RESOLUTION_SCHEMA_VERSION,
        resolution_id: resolution_id.clone(),
        kind: request.kind,
        status: ResolutionStatus::Resolved,
        created_at_ms: now_ms(),
        effort_id: effort.id.clone(),
        note: request.note.clone(),
        evidence,
        binding,
        transition: Some(ResolutionTransition {
            governed_action: continuation.id.clone(),
            action: continuation.clone(),
            recovery_reset: true,
        }),
        config_overlay,
        refusal: None,
    };
    // The immutable record and note exist before the bounded transition, so an
    // interruption in between leaves a record the Build can apply exactly once.
    write_immutable(&record_path, &orchestrate_contracts::encode(&record)?)?;
    apply_resolution(store, &effort, &build_dir, &state_path, &mut state, &record)?;
    Ok(resolution_outcome(&record_path, &record))
}

fn resolution_outcome(record_path: &Path, record: &ResolutionRecord) -> ResolutionOutcome {
    match record.status {
        ResolutionStatus::Refused => {
            let refusal = record
                .refusal
                .clone()
                .unwrap_or_else(|| ResolutionRefusal {
                    reason: "refused".into(),
                    successor_guidance: String::new(),
                });
            ResolutionOutcome::Refused {
                resolution: record_path.to_path_buf(),
                resolution_id: record.resolution_id.clone(),
                reason: refusal.reason,
                successor_guidance: refusal.successor_guidance,
            }
        }
        ResolutionStatus::Resolved => {
            let transition = record.transition.as_ref().expect("applied records transition");
            ResolutionOutcome::Resolved {
                resolution: record_path.to_path_buf(),
                resolution_id: record.resolution_id.clone(),
                kind: record.kind,
                stopped_action: record.binding.action.id.clone(),
                continuation_action: transition.action.id.clone(),
                continuation_kind: action_kind_name(&transition.action.kind).into(),
                config_version: record.config_overlay.as_ref().map(|overlay| overlay.version),
            }
        }
    }
}

/// Which action the resolution authorizes next.
fn continuation_for(
    state: &BuildState,
    stop: &StopRecord,
    kind: ResolutionKind,
) -> Result<(ActionKind, String, Option<String>, Option<String>)> {
    let subject = stop.interrupted.as_deref().unwrap_or(&stop.action);
    if matches!(kind, ResolutionKind::NewVerificationEvidence) {
        ensure!(
            stop.trigger == StopTrigger::AuditBlocked && subject.scope == FINAL_SCOPE,
            "new verification evidence only applies to a final-scope Audit acceptance that completed and derived BLOCKED; this stop is {}",
            trigger_name(&stop.trigger)
        );
        ensure!(
            state.implementation.is_some(),
            "this Build has no registered implementation to reassess"
        );
        return Ok((
            ActionKind::FinalAudit,
            FINAL_SCOPE.into(),
            subject.target_commit.clone(),
            subject.feedback_path.clone(),
        ));
    }
    ensure!(
        !matches!(subject.kind, ActionKind::Unblock),
        "the stopped action is itself a diagnosis; resolve the action it interrupted instead"
    );
    Ok((
        subject.kind.clone(),
        subject.scope.clone(),
        subject.target_commit.clone(),
        subject.feedback_path.clone(),
    ))
}

/// Apply one resolution record to Build state.  Every input comes from the
/// immutable record, so re-applying an interrupted resolution is idempotent.
fn apply_resolution(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    state_path: &Path,
    state: &mut BuildState,
    record: &ResolutionRecord,
) -> Result<()> {
    ensure!(
        record.status == ResolutionStatus::Resolved,
        "resolution {} was refused and cannot transition a Build",
        record.resolution_id
    );
    let transition = record
        .transition
        .as_ref()
        .context("applied resolution carries no transition")?;
    ensure!(
        state
            .stop
            .as_ref()
            .is_some_and(|stop| stop.action.id == record.binding.action.id),
        "resolution {} does not belong to the current stopped action",
        record.resolution_id
    );
    if let Some(overlay) = &record.config_overlay {
        ensure!(
            overlay.resolution_id == record.resolution_id
                && overlay.first_action_id == transition.action.id
                && digest_bytes(overlay.config_text.as_bytes()) == overlay.config_digest,
            "config overlay does not belong to resolution {}",
            record.resolution_id
        );
        write_immutable(
            &config_history_path(build_dir, overlay),
            &orchestrate_contracts::encode(overlay)?,
        )?;
    }
    state.action = transition.action.clone();
    state.interrupted = None;
    state.stop = None;
    if transition.recovery_reset {
        state.recovery = Recovery::default();
    }
    state.applied_resolution_id = Some(record.resolution_id.clone());
    save_state(state_path, state)?;
    append_journal_once(
        store,
        effort,
        "build_resolved",
        &record.resolution_id,
        serde_json::json!({
            "resolution": record.resolution_id,
            "record": resolution_path(build_dir, &record.resolution_id),
            "kind": resolution_kind_name(record.kind),
            "stopped_action": record.binding.action.id,
            "stopped_trigger": trigger_name(&record.binding.trigger),
            "head_commit": record.binding.head_commit,
            "checkout_status": record.binding.checkout_status,
            "in_flight_confirmation": record.binding.in_flight_confirmation,
            "continuation_action": transition.action.id,
            "continuation_kind": action_kind_name(&transition.action.kind),
            "continuation_scope": transition.action.scope,
            "recovery_reset": transition.recovery_reset,
            "config_version": record.config_overlay.as_ref().map(|overlay| overlay.version),
            "configured_action": record.config_overlay.as_ref().map(|overlay| overlay.first_action_id.clone()),
            "evidence": record.evidence,
            "note": record.note,
        }),
    )
}

/// Complete a resolution that recorded its transition but died before saving
/// state, so an interrupted record-before-state operation takes effect once.
fn apply_pending_resolution(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    state_path: &Path,
    state: &mut BuildState,
) -> Result<()> {
    let Some(stop) = state.stop.as_ref() else {
        return Ok(());
    };
    let Some(record) = applied_resolution_for_stop(build_dir, &stop.action.id)? else {
        return Ok(());
    };
    if state.applied_resolution_id.as_deref() == Some(record.resolution_id.as_str()) {
        return Ok(());
    }
    apply_resolution(store, effort, build_dir, state_path, state, &record)
}

fn resolution_records(build_dir: &Path) -> Result<Vec<ResolutionRecord>> {
    let dir = build_dir.join(RESOLUTIONS_DIR);
    let mut records = Vec::new();
    if !dir.is_dir() {
        return Ok(records);
    }
    for entry in fs::read_dir(&dir)? {
        let path = entry?.path();
        if path.extension().and_then(|value| value.to_str()) != Some("json") {
            continue;
        }
        let record: ResolutionRecord = read_json(&path)?;
        ensure!(
            record.schema_version == RESOLUTION_SCHEMA_VERSION,
            "unsupported resolution record version in {}",
            path.display()
        );
        records.push(record);
    }
    records.sort_by_key(|record| record.created_at_ms);
    Ok(records)
}

fn applied_resolution_for_stop(
    build_dir: &Path,
    stopped_action: &str,
) -> Result<Option<ResolutionRecord>> {
    let mut found: Option<ResolutionRecord> = None;
    for record in resolution_records(build_dir)? {
        if record.status != ResolutionStatus::Resolved
            || record.binding.action.id != stopped_action
        {
            continue;
        }
        ensure!(
            found
                .as_ref()
                .is_none_or(|existing| existing.resolution_id == record.resolution_id),
            "more than one resolution claims stopped action {stopped_action}"
        );
        found = Some(record);
    }
    Ok(found)
}

fn resolution_path(build_dir: &Path, resolution_id: &str) -> PathBuf {
    build_dir.join(RESOLUTIONS_DIR).join(format!("{resolution_id}.json"))
}

fn config_history_path(build_dir: &Path, overlay: &ConfigOverlay) -> PathBuf {
    build_dir.join(CONFIG_HISTORY_DIR).join(format!(
        "v{:04}-{}.json",
        overlay.version, overlay.resolution_id
    ))
}

fn resolution_id(
    stop_action: &str,
    kind: ResolutionKind,
    note: &str,
    evidence: &[ResolutionEvidence],
    config: Option<&str>,
) -> String {
    let mut material = format!("{stop_action}|{}|{note}", resolution_kind_name(kind));
    for item in evidence {
        material.push_str(&format!("|{}:{}", item.path, item.sha256));
    }
    if let Some(text) = config {
        material.push_str(&format!("|config:{}", digest_bytes(text.as_bytes())));
    }
    format!("res-{}", &digest_bytes(material.as_bytes())[..24])
}

fn read_evidence(paths: &[PathBuf]) -> Result<Vec<ResolutionEvidence>> {
    let mut evidence = Vec::new();
    for path in paths {
        let bytes = fs::read(path)
            .with_context(|| format!("cannot read resolution evidence {}", path.display()))?;
        evidence.push(ResolutionEvidence {
            path: path.to_string_lossy().into_owned(),
            sha256: digest_bytes(&bytes),
        });
    }
    Ok(evidence)
}

fn checkout_status(project: &Project) -> Result<String> {
    let status = git(
        &project.canonical_locator,
        ["status", "--porcelain=v1", "--untracked-files=all"],
    )?;
    Ok(if status.is_empty() {
        "clean".into()
    } else {
        status
    })
}

/// Publish immutable controller bytes once; identical bytes are a replay, and
/// different bytes under the same identity are a conflict.
fn write_immutable(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    match OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(mut file) => {
            file.write_all(bytes)?;
            file.sync_all()?;
            Ok(())
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            ensure!(
                fs::read(path)? == bytes,
                "{} already exists with different bytes",
                path.display()
            );
            Ok(())
        }
        Err(error) => Err(error.into()),
    }
}

fn contained_checkout(
    project: &Project,
    action_dir: &Path,
    commit: &str,
    directory: &str,
) -> Result<PathBuf> {
    let source = action_dir.join(directory);
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
        let path = store.project_dir(project).join(".build-controller.lock");
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

    /// A detailed plan that documents the authority scope of every phase in the
    /// test plan, which the controller requires before dispatching a role.
    const DETAILED_PLAN: &str = "## Delivery phase D1 — First phase\n\n**Requirements:** R-1\n\n**Completion evidence:** D1 is complete.\n\n**Deliberate later-phase exclusions:** none.\n\nTasks:\n- T1 Implement the first phase.\n\n## Delivery phase D2 — Second phase\n\n**Requirements:** R-1\n\n**Completion evidence:** D2 is complete.\n\n**Deliberate later-phase exclusions:** none.\n\nTasks:\n- T2 Implement the second phase.\n";

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
    fn requirement_projection_is_digest_bound_ordered_and_immutable() {
        let (store, effort, _, initial_ref, _) = prepared();
        let (_, mut source): (_, ReconciledDiscovery) = store
            .load_json(&effort, &initial_ref, "reconciled-discovery.json")
            .unwrap();
        let mut conditional = source.requirements[0].clone();
        conditional.requirement.id = "R-2".into();
        conditional.requirement.condition = Some("when the optional feature is enabled".into());
        source.requirements.push(conditional);
        let reconciled = store
            .publish_bundle(
                &effort,
                "reconcile",
                ArtifactKind::ReconciledDiscovery,
                "ready-conditional".into(),
                "IMPLEMENTATION_READY".into(),
                vec![],
                build_provenance("test", orchestrate_guides::BUILD),
                BTreeMap::from([(
                    "reconciled-discovery.json".into(),
                    orchestrate_contracts::encode(&source).unwrap(),
                )]),
            )
            .unwrap();
        let action_dir = temp("projection");
        let projection_path =
            ensure_requirement_projection(&store, &effort, &action_dir, &reconciled).unwrap();
        let projection: BindingRequirements = read_json(&projection_path).unwrap();
        let source = store
            .artifact_dir(&effort, &reconciled.artifact_id)
            .unwrap()
            .join("reconciled-discovery.json");
        assert_eq!(projection.source, reconciled);
        assert_eq!(
            projection.source_json_sha256,
            digest_bytes(&fs::read(source).unwrap())
        );
        assert_eq!(projection.requirements.len(), 2);
        assert_eq!(projection.requirements[0].requirement.id, "R-1");
        assert_eq!(projection.requirements[1].requirement.id, "R-2");
        assert_eq!(
            projection.requirements[1].requirement.condition.as_deref(),
            Some("when the optional feature is enabled")
        );
        let mut tampered: serde_json::Value = read_json(&projection_path).unwrap();
        tampered["requirements"].as_array_mut().unwrap().pop();
        fs::write(&projection_path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert!(ensure_requirement_projection(&store, &effort, &action_dir, &reconciled).is_err());
        tampered["requirements"] = serde_json::to_value(&projection.requirements).unwrap();
        tampered["requirements"].as_array_mut().unwrap().reverse();
        fs::write(&projection_path, serde_json::to_vec(&tampered).unwrap()).unwrap();
        assert!(ensure_requirement_projection(&store, &effort, &action_dir, &reconciled).is_err());
    }

    #[test]
    fn phase_authority_keeps_mapping_evidence_and_exclusions_together() {
        let plan = "## Delivery phase A — Authority\n\n**Requirements:** R-1, R-2\n\n**Completion evidence:** full and ordered.\n\n**Deliberate later-phase exclusions:** recovery, UI.\n\nTasks:\n- A1 Inspect authority.\n- A2 Test projection.\n\n## Delivery phase D — Status\n\n**Requirements:** R-3\n\n**Completion evidence:** status transitions.\n\n**Deliberate later-phase exclusions:** export.\n\nTasks:\n- D1 Implement status.";
        assert_eq!(
            phase_authority(plan, &["A1".into(), "A2".into()]).unwrap(),
            (
                vec!["R-1".to_string(), "R-2".to_string()],
                "full and ordered.".into(),
                "recovery, UI.".into(),
            )
        );
        assert_eq!(
            phase_authority(plan, &["D1".into()]).unwrap(),
            (
                vec!["R-3".to_string()],
                "status transitions.".into(),
                "export.".into(),
            )
        );
        assert_eq!(phase_authority(plan, &[]), None);
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
            problem: "test problem".into(),
            product_behavior_changed: vec!["works".into()],
            product_behavior_unchanged: vec![],
            technical_behavior_changed: vec!["implementation changes".into()],
            technical_behavior_unchanged: vec![],
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
            discovery_attribution: vec![],
            evidence_synthesis: vec![],
            disagreements: vec![],
            rejected_alternatives: vec![],
            implementation_risks: vec![],
            compatibility_concerns: vec![],
            caveats: vec![],
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
        let build = store.phase_dir(&effort, "build").unwrap();
        fs::write(build.join("implementation-plan.md"), DETAILED_PLAN).unwrap();
        fs::write(
            build.join("config.toml"),
            "schema_version = 2\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"cursor\"\n",
        )
        .unwrap();
        let plan = BuildPlan {
            schema_version: 2,
            reconciled: reconciled_ref.clone(),
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
        // The fifth value preserves the compact test-helper shape. It is deliberately not an
        // Adoption: every controller test must exercise Adoption creation at the Build boundary.
        (store, effort, repo, reconciled_ref.clone(), reconciled_ref)
    }

    #[test]
    fn build_start_may_be_a_descendant_and_is_recorded_separately() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        fs::write(repo.join("advanced.txt"), "advanced\n").unwrap();
        git_ok(&repo, &["add", "."]);
        git_ok(&repo, &["commit", "-m", "advance before build"]);
        let expected_start = git(&repo, ["rev-parse", "HEAD"]).unwrap();
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let plan = load_plan(&store, &effort, &build_dir).unwrap();
        let state = initialize_state(&store, &effort, &build_dir, &plan).unwrap();
        assert_eq!(
            state.frozen.discovery_baseline_commit,
            effort.baseline_commit
        );
        assert_eq!(state.frozen.discovery_baseline_tree, effort.baseline_tree);
        assert_eq!(state.frozen.build_start_commit, expected_start);
        assert_ne!(
            state.frozen.build_start_commit,
            state.frozen.discovery_baseline_commit
        );
        assert!(state.frozen.adoption.kind == ArtifactKind::Adoption);
    }

    #[test]
    fn build_start_rejects_a_head_unrelated_to_the_discovery_baseline() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        git_ok(&repo, &["checkout", "--orphan", "unrelated"]);
        fs::write(repo.join("unrelated.txt"), "unrelated\n").unwrap();
        git_ok(&repo, &["add", "."]);
        git_ok(&repo, &["commit", "-m", "unrelated history"]);
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let plan = load_plan(&store, &effort, &build_dir).unwrap();
        assert!(
            initialize_state(&store, &effort, &build_dir, &plan)
                .unwrap_err()
                .to_string()
                .contains("not descended from the Discovery baseline")
        );
    }

    #[test]
    fn new_build_rejects_staged_unstaged_and_untracked_checkout_state() {
        for kind in ["staged", "unstaged", "untracked"] {
            let (store, effort, repo, _reconciled, _adoption) = prepared();
            match kind {
                "staged" => {
                    fs::write(repo.join("source.txt"), "staged before Build\n").unwrap();
                    git_ok(&repo, &["add", "source.txt"]);
                }
                "unstaged" => {
                    fs::write(repo.join("source.txt"), "unstaged before Build\n").unwrap();
                }
                "untracked" => {
                    fs::write(repo.join("foreign.txt"), "untracked before Build\n").unwrap();
                }
                _ => unreachable!(),
            }
            let build_dir = store.phase_dir(&effort, "build").unwrap();
            let host = ScenarioHost::new(Scenario::Basic);
            let error = run_with_adapter(&store, request(&repo), &host).unwrap_err();
            assert!(
                error.to_string().contains("cannot start a new Build"),
                "{kind}: {error:#}"
            );
            assert!(
                !build_dir.join("state.json").exists(),
                "{kind} created Build state"
            );
            assert_eq!(host.call_count("work"), 0, "{kind} reached the worker");
            assert_eq!(artifact_count(&store, &effort, ArtifactKind::Adoption), 0);
        }
    }

    #[test]
    fn new_build_allows_ignored_environment_state() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        fs::write(repo.join(".gitignore"), "local-cache/\n").unwrap();
        git_ok(&repo, &["add", ".gitignore"]);
        git_ok(&repo, &["commit", "-m", "ignore local cache"]);
        fs::create_dir(repo.join("local-cache")).unwrap();
        fs::write(repo.join("local-cache").join("state.txt"), "local\n").unwrap();
        assert!(
            git(&repo, ["status", "--porcelain=v1", "--untracked-files=all"])
                .unwrap()
                .is_empty()
        );
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let plan = load_plan(&store, &effort, &build_dir).unwrap();
        initialize_state(&store, &effort, &build_dir, &plan).unwrap();
        assert!(build_dir.join("state.json").exists());
    }

    #[test]
    fn resumed_build_does_not_repeat_the_initial_checkout_check() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let plan = load_plan(&store, &effort, &build_dir).unwrap();
        initialize_state(&store, &effort, &build_dir, &plan).unwrap();
        fs::write(repo.join("worker-scratch.txt"), "worker state\n").unwrap();
        fs::write(repo.join("source.txt"), "work in progress\n").unwrap();
        let host = StaleGuideHost::default();
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(result, BuildResult::Blocked { .. }));
        assert_eq!(host.invocations.lock().unwrap().len(), 1);
    }
    /// Each role must be driven through its own effective adapter, not a shared
    /// default and not a superseded config version.
    fn assert_role_adapter(invocation: &Invocation) {
        let config = effective_config(&invocation.build_dir).unwrap();
        let expected = if invocation.role == "worker" {
            &config.config.worker.adapter
        } else {
            &config.config.reviewer.adapter
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
        SecondPhaseExternalRequirement,
        FailedBeforeAcceptance,
        AcceptedButIncomplete,
        InterruptedTwice,
        Uncertain,
        InvalidReview,
        NeverUnblocked,
        AlwaysBlockedAudit,
        BlockedAuditThenNewEvidence,
    }

    struct ScenarioHost {
        scenario: Scenario,
        calls: Mutex<Vec<String>>,
        adapters: Mutex<Vec<String>>,
    }

    impl ScenarioHost {
        fn new(scenario: Scenario) -> Self {
            Self {
                scenario,
                calls: Mutex::new(Vec::new()),
                adapters: Mutex::new(Vec::new()),
            }
        }

        fn call_count(&self, kind: &str) -> usize {
            let prefix = format!("{kind}:");
            self.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|call| call.starts_with(&prefix))
                .count()
        }

        /// The adapter each invocation of one role was driven through.
        fn adapters(&self, kind: &str) -> Vec<String> {
            let prefix = format!("{kind}:");
            self.adapters
                .lock()
                .unwrap()
                .iter()
                .filter_map(|call| call.strip_prefix(&prefix).map(str::to_owned))
                .collect()
        }

        fn call_scopes(&self, kind: &str) -> Vec<String> {
            let prefix = format!("{kind}:");
            self.calls
                .lock()
                .unwrap()
                .iter()
                .filter_map(|call| call.strip_prefix(&prefix).map(str::to_owned))
                .collect()
        }

        fn scoped_count(&self, kind: &str, scope: &str) -> usize {
            self.call_scopes(kind)
                .iter()
                .filter(|seen| seen.as_str() == scope)
                .count()
        }

        fn write_assessment(
            &self,
            invocation: &Invocation,
            action: &serde_json::Value,
            state: CoverageState,
        ) -> Result<()> {
            let (rationale, evidence, correction) = match state {
                CoverageState::Pass => ("ok".to_owned(), vec!["test".to_owned()], String::new()),
                CoverageState::Fail => (
                    "needs correction".to_owned(),
                    vec!["test".to_owned()],
                    "fix it".to_owned(),
                ),
                _ => (
                    "live evidence was unavailable".to_owned(),
                    Vec::new(),
                    String::new(),
                ),
            };
            let assessment = AuditAssessment {
                reconciled: serde_json::from_value(action["reconciled"].clone())?,
                adoption: serde_json::from_value(action["adoption"].clone())?,
                implementation: serde_json::from_value(action["implementation"].clone())?,
                coverage: vec![Coverage {
                    requirement_id: "R-1".into(),
                    state,
                    rationale,
                    evidence,
                    correction,
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
            self.calls.lock().unwrap().push(format!("{kind}:{scope}"));
            self.adapters
                .lock()
                .unwrap()
                .push(format!("{kind}:{}", invocation.adapter));
            // A recovery regression would otherwise spin forever instead of failing.
            assert!(
                self.calls.lock().unwrap().len() <= 64,
                "the controller stopped making progress"
            );
            let count = self.call_count(kind);
            if matches!(kind, "unblock" | "final_audit") {
                assert!(invocation.session_id.is_none());
            }
            if kind == "unblock" {
                let interrupted = &action["interrupted_action"];
                assert!(interrupted["kind"].is_string());
                assert_eq!(interrupted["scope"], action["scope"]);
                assert!(action["interrupted_report"].as_str().is_some());
                assert!(action["interrupted_result"].as_str().is_some());
                assert!(action.get("prior_feedback").is_some());
            }
            if kind == "final_audit" {
                for path in [
                    "reconciled_discovery",
                    "adoption_receipt",
                    "implementation_record",
                    "registered_snapshot",
                    "verification_checkout",
                    "assessment",
                    "report",
                    "result",
                ] {
                    assert!(
                        action[path].as_str().is_some(),
                        "missing final Audit {path}"
                    );
                }
                let verification_checkout = invocation.cwd.to_string_lossy();
                assert_eq!(
                    action["verification_checkout"].as_str(),
                    Some(verification_checkout.as_ref())
                );
                assert!(invocation.cwd.ends_with("verification"));
                assert!(Path::new(action["registered_snapshot"].as_str().unwrap()).is_dir());
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
                    let blocked = self.scenario == Scenario::NeverUnblocked
                        || matches!(
                            self.scenario,
                            Scenario::Remedy | Scenario::ExternalRequirement
                        ) && count == 1
                        || self.scenario == Scenario::SecondPhaseExternalRequirement
                            && scope == "D2"
                            && self.scoped_count("work", "D2") == 1;
                    if blocked {
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
                "unblock" => {
                    let outcome = if matches!(
                        self.scenario,
                        Scenario::ExternalRequirement | Scenario::SecondPhaseExternalRequirement
                    ) {
                        "external_requirement"
                    } else {
                        "remedy_available"
                    };
                    fs::write(
                        invocation.action.join("report.md"),
                        format!(
                            "diagnosis: the interrupted {} needs {} before it can be retried\n",
                            action["interrupted_action"]["kind"], outcome
                        ),
                    )?;
                    serde_json::json!({
                        "action_id": action["action_id"],
                        "scope": scope,
                        "outcome": outcome
                    })
                }
                "final_audit" => {
                    let corrections_still_required = self.scenario == Scenario::Corrections
                        && count == 1
                        || self.scenario == Scenario::RepeatedCorrections && count <= 2;
                    let coverage = match self.scenario {
                        Scenario::AlwaysBlockedAudit => CoverageState::Unknown,
                        Scenario::BlockedAuditThenNewEvidence if count <= 2 => CoverageState::Unknown,
                        _ if corrections_still_required => CoverageState::Fail,
                        _ => CoverageState::Pass,
                    };
                    self.write_assessment(invocation, &action, coverage)?;
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
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let result = run_with_adapter(
            &store,
            BuildRequest {
                effort: None,
                project: repo.clone(),
            },
            &FakeHost,
        )
        .unwrap();
        let BuildResult::Completed(done) = result else {
            panic!("Build did not complete")
        };
        let (_, implementation): (_, orchestrate_contracts::Implementation) = store
            .load_json(&effort, &done.implementation, "implementation.json")
            .unwrap();
        assert_eq!(
            implementation.discovery_baseline_commit,
            effort.baseline_commit
        );
        assert_eq!(implementation.discovery_baseline_tree, effort.baseline_tree);
        assert!(!implementation.build_start_commit.is_empty());
        assert!(!implementation.build_start_tree.is_empty());
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
        // The stop and both edges of the diagnosis are journaled.
        assert!(journal_count(&store, &_effort, "build_stopped") >= 1);
        assert_eq!(journal_count(&store, &_effort, "build_unblock_started"), 1);
        assert_eq!(journal_count(&store, &_effort, "build_unblock_resolved"), 1);
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

    fn build_state_path(store: &Store, effort: &Effort) -> PathBuf {
        store
            .phase_dir(effort, "build")
            .unwrap()
            .join("state.json")
    }

    fn read_state(store: &Store, effort: &Effort) -> BuildState {
        read_json(&build_state_path(store, effort)).unwrap()
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
        WorkerUntracked,
        WorkerTracked,
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
                    if self.mode == Tamper::WorkerUntracked {
                        fs::write(invocation.cwd.join("worker-scratch.txt"), "scratch\n")?;
                    } else if self.mode == Tamper::WorkerTracked {
                        fs::write(invocation.cwd.join(format!("{scope}.txt")), "leftover\n")?;
                    }
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
                    let bytes = if matches!(self.mode, Tamper::Malformed | Tamper::WorkerUntracked)
                    {
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
    fn worker_owned_untracked_file_does_not_block_commit_handoff() {
        let (store, _effort, repo, _reconciled, _adoption) = prepared();
        let host = TamperHost::new(Tamper::WorkerUntracked);
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(result, BuildResult::Blocked { .. }));
        assert_eq!(host.call_count("work"), 1);
        assert_eq!(
            host.call_count("review"),
            1,
            "valid work never reached Review"
        );
        assert!(repo.join("worker-scratch.txt").exists());
    }

    #[test]
    fn worker_completion_still_rejects_leftover_tracked_changes() {
        let (store, _effort, repo, _reconciled, _adoption) = prepared();
        let host = TamperHost::new(Tamper::WorkerTracked);
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        let BuildResult::Blocked { detail, .. } = result else {
            panic!("leftover tracked changes were accepted")
        };
        assert!(detail.contains("worker left tracked changes outside submitted commit"));
        assert_eq!(host.call_count("work"), 1);
        assert_eq!(host.call_count("review"), 0);
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

        // The host never reported acceptance, so the stopped action is uncertain
        // and a bare restart must not resend it.
        let bare = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(bare, BuildResult::Blocked { .. }));
        assert_eq!(host.invocations.lock().unwrap().len(), 1);

        // The operator confirms the provider is gone, which authorizes one
        // distinct continuation boundary instead of a resend.
        let state = read_state(&store, &effort);
        let stop = state.stop.as_ref().unwrap();
        assert_eq!(stop.trigger, StopTrigger::UncertainAcceptance);
        let resolution = resolve(
            &store,
            ResolutionRequest {
                effort: effort.id.clone(),
                action: stop.action.id.clone(),
                kind: ResolutionKind::EnvironmentRepair,
                note: "provider confirmed stopped; environment repaired".into(),
                evidence: Vec::new(),
                confirm_not_running: true,
                config: None,
            },
        )
        .unwrap();
        assert!(matches!(resolution, ResolutionOutcome::Resolved { .. }));

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

    // -----------------------------------------------------------------------
    // Phase B fixtures: durable stops, guarded recovery, exact Audit attempts
    // -----------------------------------------------------------------------

    /// How a role action fails, without any of them completing.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum FailureMode {
        NeverAccepted,
        ExitFailure,
        IncompleteReceipt,
    }

    struct FailureHost {
        mode: FailureMode,
        calls: Mutex<Vec<String>>,
    }

    impl FailureHost {
        fn new(mode: FailureMode) -> Self {
            Self {
                mode,
                calls: Mutex::new(Vec::new()),
            }
        }

        fn action_ids(&self, kind: &str) -> Vec<String> {
            let prefix = format!("{kind}:");
            self.calls
                .lock()
                .unwrap()
                .iter()
                .filter_map(|call| call.strip_prefix(&prefix).map(str::to_owned))
                .collect()
        }
    }

    impl HostAdapter for FailureHost {
        fn invoke(
            &self,
            invocation: &Invocation,
            observer: &mut dyn InvocationObserver,
        ) -> Result<InvocationResult> {
            let action: serde_json::Value = read_json(&invocation.action.join("action.json"))?;
            let kind = action["kind"].as_str().unwrap().to_owned();
            assert_role_adapter(invocation);
            self.calls.lock().unwrap().push(format!(
                "{kind}:{}",
                action["action_id"].as_str().unwrap()
            ));
            assert!(
                self.calls.lock().unwrap().len() <= 64,
                "the controller stopped making progress"
            );
            match kind.as_str() {
                "work" => match self.mode {
                    FailureMode::NeverAccepted => {
                        return Ok(InvocationResult {
                            completion: InvocationCompletion::FailedBeforeAcceptance {
                                detail: "cannot launch codex: No such file or directory".into(),
                            },
                        });
                    }
                    FailureMode::ExitFailure => {
                        observer.accepted()?;
                        observer.session_id("worker-session")?;
                        return Ok(InvocationResult {
                            completion: InvocationCompletion::AcceptedButIncomplete {
                                detail: "provider exited with 1".into(),
                            },
                        });
                    }
                    FailureMode::IncompleteReceipt => {
                        observer.accepted()?;
                        observer.session_id("worker-session")?;
                        write_bytes_sync(
                            &invocation.action.join("result.json"),
                            &serde_json::to_vec(&serde_json::json!({
                                "action_id": action["action_id"],
                                "scope": action["scope"],
                                "outcome": "incomplete"
                            }))?,
                        )?;
                    }
                },
                "unblock" => {
                    fs::write(
                        invocation.action.join("report.md"),
                        "diagnosis: the interrupted role can resume once the provider launches\n",
                    )?;
                    write_bytes_sync(
                        &invocation.action.join("result.json"),
                        &serde_json::to_vec(&serde_json::json!({
                            "action_id": action["action_id"],
                            "scope": action["scope"],
                            "outcome": "remedy_available"
                        }))?,
                    )?;
                }
                other => bail!("unexpected failure-host action {other}"),
            }
            Ok(InvocationResult {
                completion: InvocationCompletion::Completed,
            })
        }
    }

    fn stop_of(store: &Store, effort: &Effort) -> StopRecord {
        read_state(store, effort)
            .stop
            .expect("the Build should hold a durable stop")
    }

    /// A resolution request naming the current stopped action.
    fn resolve_stop(
        store: &Store,
        effort: &Effort,
        kind: ResolutionKind,
        note: &str,
        confirm_not_running: bool,
        config: Option<PathBuf>,
        evidence: Vec<PathBuf>,
    ) -> Result<ResolutionOutcome> {
        let action = stop_of(store, effort).action.id;
        resolve_action(
            store,
            effort,
            &action,
            kind,
            note,
            confirm_not_running,
            config,
            evidence,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn resolve_action(
        store: &Store,
        effort: &Effort,
        action: &str,
        kind: ResolutionKind,
        note: &str,
        confirm_not_running: bool,
        config: Option<PathBuf>,
        evidence: Vec<PathBuf>,
    ) -> Result<ResolutionOutcome> {
        resolve(
            store,
            ResolutionRequest {
                effort: effort.id.clone(),
                action: action.into(),
                kind,
                note: note.into(),
                evidence,
                confirm_not_running,
                config,
            },
        )
    }

    fn resolution_files(store: &Store, effort: &Effort) -> Vec<PathBuf> {
        let dir = store
            .phase_dir(effort, "build")
            .unwrap()
            .join(RESOLUTIONS_DIR);
        if !dir.is_dir() {
            return Vec::new();
        }
        let mut paths = fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        paths.sort();
        paths
    }

    #[test]
    fn process_failures_stop_with_typed_durable_records() {
        for (mode, action_failure) in [
            (FailureMode::NeverAccepted, StopTrigger::SpawnFailure),
            (FailureMode::ExitFailure, StopTrigger::ProviderExecutionFailure),
            (
                FailureMode::IncompleteReceipt,
                StopTrigger::IncompleteWork,
            ),
        ] {
            let (store, effort, repo, _reconciled, _adoption) = prepared();
            let host = FailureHost::new(mode);
            let result = run_with_adapter(&store, request(&repo), &host).unwrap();
            assert!(
                matches!(result, BuildResult::Blocked { .. }),
                "{mode:?} did not stop"
            );
            let state = read_state(&store, &effort);
            let stop = state
                .stop
                .as_ref()
                .unwrap_or_else(|| panic!("{mode:?} left no durable stop"));
            assert_eq!(stop.trigger, StopTrigger::RecoveryExhausted);
            assert_eq!(stop.action_failure, Some(action_failure.clone()));
            assert!(
                stop.recovery_remaining.is_empty(),
                "{mode:?} still claims recovery: {:?}",
                stop.recovery_remaining
            );
            // Every attempt was a distinct action; no uncertain action was resent.
            let attempts = host.action_ids("work");
            assert_eq!(attempts.len(), 6, "{mode:?}");
            let distinct = attempts.iter().collect::<HashSet<_>>();
            assert_eq!(distinct.len(), attempts.len(), "{mode:?} resent an action");
            assert_eq!(host.action_ids("unblock").len(), 1, "{mode:?}");
            assert_eq!(
                journal_count(&store, &effort, "build_stopped"),
                state.stop_history.len()
            );
            // The first record is the one the ladder wrote before diagnosing.
            assert_eq!(state.stop_history[0].trigger, StopTrigger::RecoveryExhausted);
            assert_eq!(state.stop_history[0].action_failure, Some(action_failure));
            assert!(stop.process_completion.contains("provider"));
        }
    }

    #[test]
    fn frozen_input_changes_record_a_stop_instead_of_a_bare_error() {
        for (name, suffix) in [
            ("plan.json", "\n"),
            ("config.toml", "\n# edited after execution began\n"),
            ("implementation-plan.md", "\n# edited after execution began\n"),
        ] {
            let (store, effort, repo, _reconciled, _adoption) = prepared();
            let build_dir = store.phase_dir(&effort, "build").unwrap();
            let plan = load_plan(&store, &effort, &build_dir).unwrap();
            let state = initialize_state(&store, &effort, &build_dir, &plan).unwrap();
            let before = read_state(&store, &effort);
            let input = build_dir.join(name);
            let mut bytes = fs::read(&input).unwrap();
            bytes.extend_from_slice(suffix.as_bytes());
            fs::write(&input, bytes).unwrap();

            let host = ScenarioHost::new(Scenario::Basic);
            let result = run_with_adapter(&store, request(&repo), &host).unwrap();
            let BuildResult::Blocked { trigger, .. } = result else {
                panic!("{name} did not stop the Build")
            };
            assert_eq!(trigger.as_deref(), Some("frozen_input_mismatch"), "{name}");
            assert_eq!(host.call_count("work"), 0, "{name} dispatched a role");
            let after = read_state(&store, &effort);
            assert_eq!(after.action.id, before.action.id, "{name}");
            assert_eq!(after.phase_index, before.phase_index, "{name}");
            assert_eq!(after.frozen.reconciled, before.frozen.reconciled, "{name}");
            let stop = after.stop.clone().unwrap();
            assert_eq!(stop.trigger, StopTrigger::FrozenInputMismatch);
            assert_eq!(state.action.id, before.action.id);

            // A frozen input that changed in place is not resolvable either.
            let error = resolve_action(
                &store,
                &effort,
                &stop.action.id,
                ResolutionKind::EnvironmentRepair,
                "repair the environment",
                false,
                None,
                Vec::new(),
            )
            .unwrap_err();
            assert!(error.to_string().contains("cannot resolve"), "{name}: {error:#}");
            assert!(resolution_files(&store, &effort).is_empty(), "{name}");
        }
    }

    #[test]
    fn a_blocked_audit_stops_on_the_exact_unresolved_requirements() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::AlwaysBlockedAudit);
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(result, BuildResult::Blocked { .. }));
        // One Unblock diagnosis and no more than two completed Audit attempts.
        assert_eq!(host.call_count("final_audit"), 2);
        assert_eq!(host.call_count("unblock"), 1);
        assert_eq!(artifact_count(&store, &effort, ArtifactKind::Audit), 2);

        let state = read_state(&store, &effort);
        let stop = state.stop.as_ref().unwrap();
        assert_eq!(stop.trigger, StopTrigger::AuditBlocked);
        assert_eq!(stop.unresolved_requirement_ids, vec!["R-1".to_owned()]);
        assert_eq!(state.current_audit, stop.audit);
        let first = state
            .stop_history
            .iter()
            .find(|record| record.trigger == StopTrigger::AuditBlocked)
            .unwrap();
        assert_ne!(first.audit, stop.audit, "the second attempt reused the first");
        assert_eq!(first.unresolved_requirement_ids, vec!["R-1".to_owned()]);

        // R-041: the formal truth table is unchanged — unknown coverage is BLOCKED.
        let (_, report): (_, orchestrate_contracts::AuditReport) =
            store.load_json(&effort, stop.audit.as_ref().unwrap(), "audit.json").unwrap();
        assert_eq!(report.verdict, Verdict::Blocked);
    }

    #[test]
    fn resolution_resumes_the_interrupted_scope_and_dispatches_nothing() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let stopped = stop_of(&store, &effort);
        assert_eq!(stopped.trigger, StopTrigger::ExternalRequirement);
        let host_calls = host.call_count("work");
        // Partial work in the checkout is surfaced by the resolution, not discarded.
        fs::write(repo.join("partial-work.txt"), "unfinished attempt\n").unwrap();

        let outcome = resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "the missing deployment access was granted",
            false,
            None,
            Vec::new(),
        )
        .unwrap();
        let ResolutionOutcome::Resolved {
            resolution,
            stopped_action,
            continuation_action,
            continuation_kind,
            ..
        } = outcome
        else {
            panic!("environment repair was refused")
        };
        // Resolution is records and transitions only: no provider action ran.
        assert_eq!(host.call_count("work"), host_calls);
        assert_eq!(stopped_action, stopped.action.id);
        assert_eq!(continuation_kind, "work");

        let record: ResolutionRecord = read_json(&resolution).unwrap();
        assert_eq!(record.binding.action.id, stopped.action.id);
        assert_eq!(record.binding.trigger, StopTrigger::ExternalRequirement);
        assert_eq!(record.binding.reconciled, stopped_action_reconciled(&store, &effort));
        assert_eq!(record.binding.plan_digest, read_state(&store, &effort).frozen.plan_digest);
        assert_eq!(record.binding.detailed_plan_digest, read_state(&store, &effort).frozen.detailed_plan_digest);
        assert_eq!(record.binding.config_digest, read_state(&store, &effort).frozen.config_digest);
        assert_eq!(record.binding.head_commit, git(&repo, ["rev-parse", "HEAD"]).unwrap());
        assert!(
            record.binding.checkout_status.contains("partial-work.txt"),
            "dirty work was not surfaced: {}",
            record.binding.checkout_status
        );
        assert!(repo.join("partial-work.txt").exists());
        assert_eq!(record.transition.as_ref().unwrap().governed_action, continuation_action);
        assert!(record.evidence.is_empty());

        let state = read_state(&store, &effort);
        assert!(state.stop.is_none());
        assert_eq!(state.applied_resolution_id.as_deref(), Some(record.resolution_id.as_str()));
        assert_eq!(state.action.id, continuation_action);
        assert_eq!(state.action.scope, "D2");
        assert!(state.interrupted.is_none());

        // The next Build resumes the interrupted scope with the accepted phase and
        // its session retained, then finishes normally.
        let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(resumed, BuildResult::Completed(_)));
        assert_eq!(host.call_scopes("work"), vec!["D1", "D2", "D2"]);
        assert_eq!(
            host.adapters("work").len(),
            3,
            "the continuation was not the only resumed action"
        );
        assert_eq!(state.phase_index, 1);
        assert_no_build_files_leaked(&repo);
    }

    fn stopped_action_reconciled(store: &Store, effort: &Effort) -> ArtifactRef {
        read_state(store, effort).frozen.reconciled
    }

    #[test]
    fn duplicate_resolutions_are_idempotent_and_conflicts_are_stale() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let stopped_action = stop_of(&store, &effort).action.id;
        let first = resolve_stop(
            &store,
            &effort,
            ResolutionKind::ExistingAuthorityClarification,
            "the role already had authority for this phase",
            false,
            None,
            Vec::new(),
        )
        .unwrap();
        let ResolutionOutcome::Resolved { resolution_id, continuation_action, .. } = &first else {
            panic!("clarification was refused")
        };
        let after_first = fs::read(build_state_path(&store, &effort)).unwrap();
        assert_eq!(resolution_files(&store, &effort).len(), 1);
        assert_eq!(journal_count(&store, &effort, "build_resolved"), 1);

        // An identical submission reuses the record and changes nothing.
        let repeat = resolve_action(
            &store,
            &effort,
            &stopped_action,
            ResolutionKind::ExistingAuthorityClarification,
            "the role already had authority for this phase",
            false,
            None,
            Vec::new(),
        )
        .unwrap();
        let ResolutionOutcome::Resolved { resolution_id: repeat_id, continuation_action: repeat_action, .. } = &repeat else {
            panic!("repeat was refused")
        };
        assert_eq!(repeat_id, resolution_id);
        assert_eq!(repeat_action, continuation_action);
        assert_eq!(resolution_files(&store, &effort).len(), 1);
        assert_eq!(journal_count(&store, &effort, "build_resolved"), 1);
        assert_eq!(fs::read(build_state_path(&store, &effort)).unwrap(), after_first);

        // A different submission for an already-resolved stop is stale.
        let error = resolve_action(
            &store,
            &effort,
            &stopped_action,
            ResolutionKind::EnvironmentRepair,
            "a different intervention",
            false,
            None,
            Vec::new(),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("already resolved"),
            "{error:#}"
        );
        assert_eq!(resolution_files(&store, &effort).len(), 1);
        assert_eq!(fs::read(build_state_path(&store, &effort)).unwrap(), after_first);

        // The single authorized continuation runs exactly once.
        let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(resumed, BuildResult::Completed(_)));
        assert_eq!(host.call_scopes("work"), vec!["D1", "D2", "D2"]);
    }

    #[test]
    fn resolution_rejects_stale_foreign_terminal_and_unstopped_targets() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let before = fs::read(build_state_path(&store, &effort)).unwrap();

        // A foreign action id names no stopped action of this effort.
        let error = resolve(
            &store,
            ResolutionRequest {
                effort: effort.id.clone(),
                action: "act-forged".into(),
                kind: ResolutionKind::EnvironmentRepair,
                note: "repair".into(),
                evidence: Vec::new(),
                confirm_not_running: false,
                config: None,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("is not the stopped action"), "{error:#}");
        assert_eq!(fs::read(build_state_path(&store, &effort)).unwrap(), before);
        assert!(resolution_files(&store, &effort).is_empty());

        // A live controller lock refuses the resolution outright.
        let lock = store.project_dir(&store.project_for(&effort).unwrap()).join(".build-controller.lock");
        fs::write(&lock, format!("{}\n", std::process::id())).unwrap();
        let error = resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "repair",
            false,
            None,
            Vec::new(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("another Build driver"), "{error:#}");
        fs::remove_file(&lock).unwrap();

        // An empty note is not a recorded intervention.
        let error = resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "   ",
            false,
            None,
            Vec::new(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("requires a note"), "{error:#}");
        assert_eq!(fs::read(build_state_path(&store, &effort)).unwrap(), before);

        // An unstopped Build has nothing to resolve.
        let (fresh_store, fresh_effort, _fresh_repo, _reconciled, _adoption) = prepared();
        let error = resolve(
            &fresh_store,
            ResolutionRequest {
                effort: fresh_effort.id.clone(),
                action: "act-none".into(),
                kind: ResolutionKind::EnvironmentRepair,
                note: "repair".into(),
                evidence: Vec::new(),
                confirm_not_running: false,
                config: None,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("no Build state exists"), "{error:#}");

        // A terminal Build cannot be resolved.
        let (done_store, done_effort, done_repo, _r, _a) = prepared();
        let done_host = ScenarioHost::new(Scenario::Basic);
        assert!(matches!(
            run_with_adapter(&done_store, request(&done_repo), &done_host).unwrap(),
            BuildResult::Completed(_)
        ));
        let error = resolve(
            &done_store,
            ResolutionRequest {
                effort: done_effort.id.clone(),
                action: "act-none".into(),
                kind: ResolutionKind::EnvironmentRepair,
                note: "repair".into(),
                evidence: Vec::new(),
                confirm_not_running: false,
                config: None,
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("already completed"), "{error:#}");
        assert!(resolution_files(&done_store, &done_effort).is_empty());
    }

    #[test]
    fn in_flight_work_needs_a_recorded_confirmation_before_continuation() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::Uncertain);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let stopped = stop_of(&store, &effort);
        assert_eq!(stopped.trigger, StopTrigger::UncertainAcceptance);
        assert_eq!(stopped.process_completion, "accepted; provider completion is uncertain");
        let before = fs::read(build_state_path(&store, &effort)).unwrap();
        let uncertain_id = stopped.action.id.clone();
        let action_dir = store.phase_dir(&effort, "build").unwrap().join("artifacts").join(&uncertain_id);
        assert!(action_dir.is_dir());

        // Without a recorded confirmation the uncertain action blocks resolution.
        let error = resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "the provider is gone",
            false,
            None,
            Vec::new(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("confirm the provider"), "{error:#}");
        assert_eq!(fs::read(build_state_path(&store, &effort)).unwrap(), before);
        assert!(resolution_files(&store, &effort).is_empty());

        let outcome = resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "the provider is gone",
            true,
            None,
            Vec::new(),
        )
        .unwrap();
        let ResolutionOutcome::Resolved {
            resolution,
            continuation_action,
            ..
        } = outcome
        else {
            panic!("confirmed repair was refused")
        };
        let record: ResolutionRecord = read_json(&resolution).unwrap();
        assert!(record.binding.in_flight_confirmation);
        // The continuation is a distinct boundary, not a resend of the old action.
        assert_ne!(continuation_action, uncertain_id);
        assert!(action_dir.is_dir(), "the uncertain action's artifacts were discarded");
        let state = read_state(&store, &effort);
        assert_eq!(state.stop_history.len(), 1);
        assert_eq!(state.stop_history[0].action.id, uncertain_id);

        let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(resumed, BuildResult::Completed(_)));
        assert_eq!(host.call_count("work"), 3);
        assert_eq!(host.call_scopes("work"), vec!["D1", "D1", "D2"]);
    }

    #[test]
    fn authority_changes_are_refused_with_successor_guidance_and_leave_state_alone() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let before = fs::read(build_state_path(&store, &effort)).unwrap();
        let host_calls = host.call_count("work");

        let outcome = resolve_stop(
            &store,
            &effort,
            ResolutionKind::AuthorityChange,
            "R-003 should not require a typed stop record",
            false,
            None,
            Vec::new(),
        )
        .unwrap();
        let ResolutionOutcome::Refused {
            resolution,
            successor_guidance,
            reason,
            ..
        } = outcome
        else {
            panic!("an authority amendment was accepted")
        };
        assert!(reason.contains("cannot amend"));
        assert!(successor_guidance.contains("Discovery"));
        assert!(successor_guidance.contains("Reconcile"));
        assert!(successor_guidance.contains("successor Build"));
        let record: ResolutionRecord = read_json(&resolution).unwrap();
        assert_eq!(record.status, ResolutionStatus::Refused);
        assert!(record.transition.is_none());
        // Execution state is untouched and no continuation was authorized.
        assert_eq!(fs::read(build_state_path(&store, &effort)).unwrap(), before);
        assert_eq!(host.call_count("work"), host_calls);
        assert_eq!(journal_count(&store, &effort, "build_resolution_refused"), 1);
        // A later Build still refuses to progress the old action.
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        assert_eq!(host.call_count("work"), host_calls);
    }

    #[test]
    fn new_evidence_publishes_a_distinct_audit_at_the_unchanged_implementation() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::BlockedAuditThenNewEvidence);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        assert_eq!(host.call_count("final_audit"), 2);
        assert_eq!(host.call_count("unblock"), 1);
        let stopped = stop_of(&store, &effort);
        assert_eq!(stopped.trigger, StopTrigger::AuditBlocked);
        let blocked_audit = stopped.audit.clone().unwrap();
        let implementation = read_state(&store, &effort).implementation.clone().unwrap();

        let evidence = temp("new-evidence");
        fs::write(evidence.join("deployment.txt"), "live deployment verified\n").unwrap();
        let evidence_file = evidence.join("deployment.txt");
        let outcome = resolve_stop(
            &store,
            &effort,
            ResolutionKind::NewVerificationEvidence,
            "live deployment evidence is now available",
            false,
            None,
            vec![evidence_file.clone()],
        )
        .unwrap();
        let ResolutionOutcome::Resolved {
            resolution,
            continuation_action,
            continuation_kind,
            ..
        } = outcome
        else {
            panic!("new evidence was refused")
        };
        assert_eq!(continuation_kind, "final_audit");
        let record: ResolutionRecord = read_json(&resolution).unwrap();
        assert_eq!(continuation_action, record.transition.as_ref().unwrap().governed_action);
        assert_eq!(record.evidence.len(), 1);
        assert_eq!(record.evidence[0].path, evidence_file.to_string_lossy());
        assert_eq!(record.evidence[0].sha256, digest_bytes(&fs::read(&evidence_file).unwrap()));

        let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
        let BuildResult::Completed(done) = resumed else {
            panic!("the new evidence did not produce a Build verdict")
        };
        assert_eq!(host.call_count("final_audit"), 3);
        assert_eq!(artifact_count(&store, &effort, ArtifactKind::Audit), 3);
        // The new attempt is at the same implementation and the old Audit survives.
        assert_eq!(done.implementation, implementation);
        assert_ne!(done.audit, blocked_audit);
        let (_, old_report): (_, orchestrate_contracts::AuditReport) =
            store.load_json(&effort, &blocked_audit, "audit.json").unwrap();
        assert_eq!(old_report.verdict, Verdict::Blocked);
        assert_eq!(old_report.assessment.implementation, implementation);
        let (_, new_report): (_, orchestrate_contracts::AuditReport) =
            store.load_json(&effort, &done.audit, "audit.json").unwrap();
        assert_eq!(new_report.verdict, Verdict::Pass);
        assert_eq!(new_report.assessment.implementation, implementation);

        // The new attempt's packet carries the recorded evidence context.
        let new_packet: serde_json::Value = read_json(
            &store
                .phase_dir(&effort, "build")
                .unwrap()
                .join("artifacts")
                .join(&record.transition.as_ref().unwrap().governed_action)
                .join("action.json"),
        )
        .unwrap();
        assert_eq!(new_packet["resolution_kind"], "new_verification_evidence");
        assert_eq!(new_packet["resolution_evidence"][0]["sha256"], record.evidence[0].sha256);
        assert_eq!(new_packet["current_audit"], serde_json::json!(blocked_audit));
    }

    #[test]
    fn new_evidence_is_refused_outside_a_blocked_final_audit() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let before = fs::read(build_state_path(&store, &effort)).unwrap();
        let error = resolve_stop(
            &store,
            &effort,
            ResolutionKind::NewVerificationEvidence,
            "live evidence",
            false,
            None,
            Vec::new(),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("final-scope Audit"),
            "{error:#}"
        );
        assert_eq!(fs::read(build_state_path(&store, &effort)).unwrap(), before);
        assert!(resolution_files(&store, &effort).is_empty());
    }

    #[test]
    fn an_interrupted_record_before_state_resolution_takes_effect_exactly_once() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let before = fs::read(build_state_path(&store, &effort)).unwrap();
        let overlay = temp("overlay");
        fs::write(
            overlay.join("config.toml"),
            "schema_version = 2\n[worker]\nadapter = \"cursor\"\n[reviewer]\nadapter = \"codex\"\n",
        )
        .unwrap();

        let outcome = resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "the worker environment was repaired",
            false,
            Some(overlay.join("config.toml")),
            Vec::new(),
        )
        .unwrap();
        let ResolutionOutcome::Resolved { continuation_action, .. } = &outcome else {
            panic!("repair was refused")
        };
        let continuation_action = continuation_action.clone();

        // Simulate dying after the record was written but before its state save.
        write_bytes_sync(&build_state_path(&store, &effort), &before).unwrap();
        let recovered = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(recovered, BuildResult::Completed(_)));

        let state = read_state(&store, &effort);
        assert!(state.applied_resolution_id.is_some());
        assert_eq!(journal_count(&store, &effort, "build_resolved"), 1);
        // The config overlay was published once, not once per application.
        let history = store.phase_dir(&effort, "build").unwrap().join(CONFIG_HISTORY_DIR);
        assert_eq!(fs::read_dir(history).unwrap().count(), 1);
        assert!(state.action.id != continuation_action || state.terminal.is_some());
        assert_eq!(host.call_scopes("work"), vec!["D1", "D2", "D2"]);
    }

    #[test]
    fn config_overlays_are_versioned_and_govern_only_future_actions() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let original_config = fs::read(build_dir.join("config.toml")).unwrap();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        assert_eq!(host.adapters("work"), vec!["codex", "codex"]);

        // A config overlay outside an environment repair is not a resolution path.
        let overlay = temp("overlay");
        fs::write(
            overlay.join("config.toml"),
            "schema_version = 2\n[worker]\nadapter = \"cursor\"\n[reviewer]\nadapter = \"codex\"\n",
        )
        .unwrap();
        let error = resolve_stop(
            &store,
            &effort,
            ResolutionKind::ExistingAuthorityClarification,
            "clarified",
            false,
            Some(overlay.join("config.toml")),
            Vec::new(),
        )
        .unwrap_err();
        assert!(
            error.to_string().contains("environment_repair"),
            "{error:#}"
        );

        // A malformed overlay is rejected before anything is recorded.
        fs::write(overlay.join("bad.toml"), "schema_version = 9\n").unwrap();
        let error = resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "repaired",
            false,
            Some(overlay.join("bad.toml")),
            Vec::new(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("valid Build config") || error.to_string().contains("unsupported"), "{error:#}");
        assert!(resolution_files(&store, &effort).is_empty());

        let outcome = resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "the worker adapter moved to cursor",
            false,
            Some(overlay.join("config.toml")),
            Vec::new(),
        )
        .unwrap();
        let ResolutionOutcome::Resolved { resolution_id, continuation_action, config_version, .. } = &outcome else {
            panic!("overlay repair was refused")
        };
        assert_eq!(*config_version, Some(2));

        // The original config bytes and frozen digest are untouched.
        assert_eq!(fs::read(build_dir.join("config.toml")).unwrap(), original_config);
        let state = read_state(&store, &effort);
        assert_eq!(state.frozen.config_digest, digest_bytes(&original_config));
        let effective = effective_config(&build_dir).unwrap();
        assert_eq!(effective.version, 2);
        assert_eq!(effective.config.worker.adapter, "cursor");
        let record: ResolutionRecord = read_json(&resolution_files(&store, &effort)[0]).unwrap();
        let overlay_record = record.config_overlay.as_ref().unwrap();
        assert_eq!(overlay_record.version, 2);
        assert_eq!(overlay_record.resolution_id, *resolution_id);
        assert_eq!(overlay_record.first_action_id, *continuation_action);
        assert!(overlay_record.first_action_id == read_state(&store, &effort).action.id);

        // Prior records keep the settings they ran under; the continuation uses the overlay.
        let stopped_action = record.binding.action.id.clone();
        let prior_packet: serde_json::Value = read_json(
            &build_dir.join("artifacts").join(&stopped_action).join("action.json"),
        )
        .unwrap();
        assert_eq!(prior_packet["config_version"], 1);

        let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(resumed, BuildResult::Completed(_)));
        assert_eq!(host.adapters("work"), vec!["codex", "codex", "cursor"]);
        let continuation_packet: serde_json::Value = read_json(
            &build_dir.join("artifacts").join(continuation_action).join("action.json"),
        )
        .unwrap();
        assert_eq!(continuation_packet["config_version"], 2);
    }

    #[test]
    fn state_v3_migration_preserves_original_bytes_and_lineage() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let modern = read_state(&store, &effort);
        assert_eq!(modern.phase_index, 1, "the first phase should be accepted");
        let accepted_commit = git(&repo, ["rev-parse", "HEAD"]).unwrap();

        // Rewrite the state as the pre-migration schema: the same version without
        // any of the additive stop/resolution fields.
        let state_path = build_state_path(&store, &effort);
        let mut legacy: serde_json::Value = read_json(&state_path).unwrap();
        for field in ["migration_version", "stop", "stop_history", "current_audit", "applied_resolution_id"] {
            legacy.as_object_mut().unwrap().remove(field);
        }
        legacy["action"].as_object_mut().unwrap().remove("resolution_id");
        let legacy_bytes = serde_json::to_vec_pretty(&legacy).unwrap();
        write_bytes_sync(&state_path, &legacy_bytes).unwrap();

        let blocked = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(blocked, BuildResult::Blocked { .. }));
        // Original bytes are retained as evidence exactly once.
        let preserved = build_dir.join("evidence").join("state-v3-original.json");
        assert_eq!(fs::read(&preserved).unwrap(), legacy_bytes);
        assert_eq!(journal_count(&store, &effort, "build_state_migrated"), 1);
        let migrated = read_state(&store, &effort);
        assert_eq!(migrated.migration_version, BUILD_STATE_MIGRATION);
        assert_eq!(migrated.phase_index, modern.phase_index);
        assert_eq!(migrated.action.id, modern.action.id);
        assert_eq!(migrated.frozen.reconciled, modern.frozen.reconciled);
        assert_eq!(migrated.frozen.config_digest, modern.frozen.config_digest);
        assert_eq!(host.call_scopes("work"), vec!["D1", "D2"]);
        assert_eq!(git(&repo, ["rev-parse", "HEAD"]).unwrap(), accepted_commit);

        // The migrated Build resolves and continues without repeating accepted work.
        let outcome = resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "the missing access was supplied",
            false,
            None,
            Vec::new(),
        )
        .unwrap();
        assert!(matches!(outcome, ResolutionOutcome::Resolved { .. }));
        let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(resumed, BuildResult::Completed(_)));
        assert_eq!(host.call_scopes("work"), vec!["D1", "D2", "D2"]);
        assert_eq!(journal_count(&store, &effort, "build_state_migrated"), 1);
    }

    #[test]
    fn a_restart_runs_the_bounded_diagnosis_a_stop_left_pending() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let plan = load_plan(&store, &effort, &build_dir).unwrap();
        let mut state = initialize_state(&store, &effort, &build_dir, &plan).unwrap();
        // The crash window between recording a stop and dispatching its one
        // diagnostic turn: the diagnosis is prepared but was never accepted.
        let stopped = state.action.clone();
        let record = stop_record(
            &store,
            &effort,
            &build_dir,
            &state,
            &stopped,
            StopTrigger::PhaseBlocker,
            Some(StopTrigger::PhaseBlocker),
            "role reported blocked".into(),
            recovery_remaining(&state),
            Some(Box::new(stopped.clone())),
        )
        .unwrap();
        push_stop(&store, &effort, &mut state, record).unwrap();
        state.interrupted = Some(Box::new(state.action.clone()));
        state.recovery.unblock_used = true;
        state.action = new_action(
            &build_dir,
            ActionKind::Unblock,
            &state.action.scope,
            None,
            None,
        );
        save_state(&build_state_path(&store, &effort), &state).unwrap();

        let host = ScenarioHost::new(Scenario::NeverUnblocked);
        let outcome = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(outcome, BuildResult::Blocked { .. }));
        assert_eq!(host.call_count("unblock"), 1, "the diagnosis did not run");
        assert_eq!(host.call_scopes("work"), vec!["D1", "D1", "D1"]);
        // The bounded stop is durable and names the exhausted recovery.
        let stopped = stop_of(&store, &effort);
        assert_eq!(stopped.trigger, StopTrigger::RecoveryExhausted);
        assert_eq!(stopped.action_failure, Some(StopTrigger::PhaseBlocker));
        assert!(stopped.recovery_remaining.is_empty());
    }

    #[test]
    fn migrated_final_scope_blocked_recovery_continues_with_new_evidence() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let host = ScenarioHost::new(Scenario::BlockedAuditThenNewEvidence);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        assert_eq!(host.call_count("final_audit"), 2);
        let migrated_action = read_state(&store, &effort).action.id.clone();

        // The same additive migration applies to a final-scope exhausted BLOCKED recovery.
        let state_path = build_state_path(&store, &effort);
        let mut legacy: serde_json::Value = read_json(&state_path).unwrap();
        for field in [
            "migration_version",
            "stop",
            "stop_history",
            "current_audit",
            "applied_resolution_id",
        ] {
            legacy.as_object_mut().unwrap().remove(field);
        }
        legacy["action"].as_object_mut().unwrap().remove("resolution_id");
        let legacy_bytes = serde_json::to_vec_pretty(&legacy).unwrap();
        write_bytes_sync(&state_path, &legacy_bytes).unwrap();

        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        assert_eq!(
            host.call_count("final_audit"),
            2,
            "migration dispatched a provider action"
        );
        assert_eq!(
            fs::read(build_dir.join("evidence").join("state-v3-original.json")).unwrap(),
            legacy_bytes
        );
        let migrated = read_state(&store, &effort);
        assert_eq!(migrated.action.id, migrated_action);
        assert_eq!(migrated.migration_version, BUILD_STATE_MIGRATION);
        assert_eq!(journal_count(&store, &effort, "build_state_migrated"), 1);

        let evidence = temp("migrated-evidence");
        fs::write(evidence.join("deployment.txt"), "live deployment verified\n").unwrap();
        let outcome = resolve_stop(
            &store,
            &effort,
            ResolutionKind::NewVerificationEvidence,
            "live deployment evidence is now available",
            false,
            None,
            vec![evidence.join("deployment.txt")],
        )
        .unwrap();
        assert!(matches!(outcome, ResolutionOutcome::Resolved { .. }));
        let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(resumed, BuildResult::Completed(_)));
        assert_eq!(host.call_count("final_audit"), 3);
        assert_eq!(artifact_count(&store, &effort, ArtifactKind::Audit), 3);
        assert_eq!(journal_count(&store, &effort, "build_state_migrated"), 1);
    }

    #[test]
    fn standalone_audits_neither_ambigify_nor_supply_the_build_verdict() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::Basic);
        let BuildResult::Completed(done) =
            run_with_adapter(&store, request(&repo), &host).unwrap()
        else {
            panic!("Build did not complete")
        };
        let implementation = done.implementation.clone();
        let (_, implementation_record): (_, orchestrate_contracts::Implementation) =
            store.load_json(&effort, &implementation, "implementation.json").unwrap();
        assert_eq!(implementation_record.target_commit, git(&repo, ["rev-parse", "HEAD"]).unwrap());

        // A standalone Audit for the same implementation, with a different verdict.
        let assessment = AuditAssessment {
            reconciled: read_state(&store, &effort).frozen.reconciled,
            adoption: read_state(&store, &effort).frozen.adoption,
            implementation: implementation.clone(),
            coverage: vec![Coverage {
                requirement_id: "R-1".into(),
                state: CoverageState::Fail,
                rationale: "standalone assessment".into(),
                evidence: vec!["standalone".into()],
                correction: "standalone correction".into(),
            }],
            assessor_context: "standalone".into(),
        };
        let standalone = orchestrate_audit::finalize_audit(
            &store,
            &effort,
            assessment,
            build_provenance("audit", canonical_role_guide(&ActionKind::FinalAudit)),
        )
        .unwrap();
        assert_ne!(standalone, done.audit);
        assert_eq!(artifact_count(&store, &effort, ArtifactKind::Audit), 2);
        let (_, standalone_report): (_, orchestrate_contracts::AuditReport) =
            store.load_json(&effort, &standalone, "audit.json").unwrap();
        assert_eq!(standalone_report.verdict, Verdict::ChangesRequired);
        assert_eq!(standalone_report.assessment.implementation, implementation);

        // The Build keeps its own attempt, its own verdict, and its own event.
        let replay = run_with_adapter(&store, request(&repo), &host).unwrap();
        let BuildResult::Completed(replayed) = replay else {
            panic!("standalone Audit changed the Build verdict")
        };
        assert_eq!(replayed.audit, done.audit);
        assert_eq!(journal_count(&store, &effort, "audit_finalized"), 2);
        assert_eq!(host.call_count("final_audit"), 1);
    }

    #[test]
    fn conflicting_bytes_under_one_audit_attempt_identity_are_rejected() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::Basic);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let state = read_state(&store, &effort);
        let audit_action = state.action.id.clone();

        // A replay that would publish different bytes under the same operation
        // identity is a conflict, not a second publication.
        let assessment_path = store
            .phase_dir(&effort, "build")
            .unwrap()
            .join("artifacts")
            .join(&audit_action)
            .join("assessment.json");
        let mut assessment: serde_json::Value = read_json(&assessment_path).unwrap();
        assessment["assessor_context"] = serde_json::json!("rewritten after publication");
        write_bytes_sync(&assessment_path, &serde_json::to_vec(&assessment).unwrap()).unwrap();
        rewrite_state(&build_state_path(&store, &effort), |state| {
            state["terminal"] = serde_json::Value::Null
        });

        let replay = run_with_adapter(&store, request(&repo), &host).unwrap();
        let BuildResult::Blocked { detail, .. } = replay else {
            panic!("conflicting bytes were published again")
        };
        assert!(detail.contains("operation identity conflicts"), "{detail}");
        assert_eq!(artifact_count(&store, &effort, ArtifactKind::Audit), 1);
        assert_eq!(journal_count(&store, &effort, "audit_finalized"), 1);
    }
}
