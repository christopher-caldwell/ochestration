//! The small deterministic controller behind `orchestrate build`.
//!
//! This crate deliberately owns transitions and durable controller state, but
//! not engineering judgement.  Provider conversations write one receipt per
//! action; the controller validates that receipt before selecting the next
//! fixed action.

use std::{
    collections::{BTreeMap, HashSet},
    fs::{self, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Mutex, OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail, ensure};
use orchestrate_contracts::{
    ArtifactKind, ArtifactRef, AuditAssessment, CoverageState, ImplementationStatus, Independence,
    Provenance, Verdict, decode, digest_bytes, safe_relative_path,
};
use orchestrate_core::{Effort, Project, Store, write_bytes_sync};

use crate::progress::Progress;
use serde::{Deserialize, Serialize};
use serde_json::json;

pub mod archive;
pub mod cleanup;
pub mod export;
pub mod preflight;
pub mod progress;
pub mod status;

static ACTION_COUNTER: AtomicU64 = AtomicU64::new(0);
pub const BUILD_PLAN_VERSION: u32 = 2;
pub const BUILD_CONFIG_VERSION: u32 = 3;
const LEGACY_BUILD_CONFIG_VERSION: u32 = 2;
pub const BUILD_STATE_VERSION: u32 = 3;
const BUILD_STATE_MIGRATION: u32 = 1;
const RESOLUTION_SCHEMA_VERSION: u32 = 1;
const CONFIG_OVERLAY_SCHEMA_VERSION: u32 = 1;

/// Scope of the post-phase Audit turn, which is not one of the plan's delivery phases.
const FINAL_SCOPE: &str = "final";
/// The shared guide that tells a read-only role how to state evidence its
/// permission stops it from writing.
const EVIDENCE_OUTPUT_GUIDE: &str = "evidence-output.md";
/// Controller-owned Build directories that hold immutable records only.
const RESOLUTIONS_DIR: &str = "resolutions";
const CONFIG_HISTORY_DIR: &str = "config-history";
const AUTHORITY_AMENDMENTS_DIR: &str = "authority-amendments";

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
    /// Set only on a successor Build that answers a refused authority-amendment
    /// resolution; the linkage is validated against the recorded refusal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub predecessor: Option<PlanPredecessor>,
}

/// The linkage a successor Build declares: the predecessor Build's exact adopted
/// contract and the authority-change refusal that directed this successor.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PlanPredecessor {
    pub effort_id: String,
    pub reconciled: ArtifactRef,
    /// ID of the predecessor's immutable authority refusal/amendment record.
    pub resolution_id: String,
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
    /// Optional.  Absent, an Unblock turn uses the reviewer's settings, so a
    /// config-v2 file keeps its exact meaning.
    #[serde(default)]
    pub unblocker: Option<RoleConfig>,
    /// Optional advisory whole-implementation once-over.  Absent means the role
    /// never runs.
    #[serde(default)]
    pub once_over: Option<RoleConfig>,
}

/// One role's requested attributes.  Every attribute is forwarded to the
/// provider exactly as configured or refused explicitly; the controller never
/// substitutes a model or effort, and an unset attribute sends no flag at all.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoleConfig {
    pub adapter: String,
    /// Provider-neutral model quality tier. Mapped at the adapter edge.
    #[serde(default)]
    pub model_strength: Option<String>,
    /// Provider-native model name, accepted only when reading a frozen v2 config.
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    /// Explicit provider permission value.  Absent adds no privilege flag.
    #[serde(default)]
    pub permission: Option<String>,
    /// Input schema version is runtime metadata, never serialized into config.toml.
    #[serde(skip)]
    pub(crate) config_schema_version: u32,
}

/// The explicit unrestricted permission.  A role must name it for itself in
/// both frozen v2 and current v3 configurations; the controller never selects
/// or widens it on a role's behalf.
const FULL_ACCESS: &str = "full_access";
const GENERIC_MODEL_STRENGTHS: &[&str] = &["standard", "strong"];
const GENERIC_REASONING_EFFORTS: &[&str] = &["low", "medium", "high"];
const GENERIC_PERMISSIONS: &[&str] = &["read_only", "workspace_write", FULL_ACCESS];

/// The five things a role can be asked to do.  A correction is not a kind of its own: the scope
/// names the phase and a referenced `feedback` marks the turn as a correction.
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
enum ActionKind {
    Work,
    Review,
    FinalAudit,
    Unblock,
    /// Advisory, non-gating whole-implementation review that runs once per new
    /// commit submitted to the formal Audit.
    OnceOver,
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
    /// How and from what this action continues: a continued, replaced,
    /// remedied or resolved turn always names its exact predecessor.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    handoff: Option<Box<Handoff>>,
    /// The Unblock diagnosis that authorized a remedy, kept separate from the
    /// original review/Audit correction the action must still address.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    remedy_path: Option<String>,
    /// When this action was accepted for dispatch, for truthful elapsed/activity
    /// status.  Prepared-but-never-dispatched time is not model time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    accepted_at_ms: Option<u128>,
}

/// Exact predecessor facts for a continued, replaced, remedied or resolved
/// action, so the role never scans unrelated action directories or infers which
/// report is current.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct Handoff {
    /// continuation, replacement, remedy, diagnosis or resolution.
    kind: String,
    action: CurrentAction,
    /// The observed product HEAD when this handoff was created, which may be
    /// the predecessor's partial or ending commit.
    ending_commit: Option<String>,
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
    /// The validated predecessor linkage of a successor Build, recorded at
    /// initialization from the plan and the predecessor's own refusal record.
    #[serde(default)]
    successor_of: Option<SuccessorLink>,
    /// Every contained checkout the controller created, with its exact commit
    /// and owning action.  Cleanup establishes ownership from these records and
    /// canonical containment, never from a directory name or path pattern.
    #[serde(default)]
    checkouts: Vec<CheckoutRecord>,
    /// Commits that already received the advisory once-over, so re-verifying
    /// the same commit runs it once.
    #[serde(default)]
    once_over: Vec<OnceOverRecord>,
    /// Accepted delivery-phase reviews, which the advisory once-over receives.
    #[serde(default)]
    phase_reviews: Vec<PhaseReview>,
    /// The latest cleanup result.  It is recorded separately from acceptance and
    /// never rewrites a stop or a verdict.
    #[serde(default)]
    cleanup: Option<serde_json::Value>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct CheckoutRecord {
    action_id: String,
    kind: String,
    path: String,
    commit: String,
    created_at_ms: u128,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct OnceOverRecord {
    commit: String,
    action_id: String,
    /// advisory_complete, blocked, invalid_receipt or uncertain; never an
    /// acceptance fact.
    outcome: String,
    report: String,
}

/// One accepted delivery-phase review, as the advisory once-over's context.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct PhaseReview {
    scope: String,
    commit: String,
    report: String,
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

/// A recorded resolution whose repository binding no longer matches.  It is a
/// rejection rather than a controller failure: nothing was applied or changed,
/// and the same stop still accepts a fresh submission.
#[derive(Debug)]
struct StaleBinding {
    detail: String,
}

impl std::fmt::Display for StaleBinding {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.detail)
    }
}

impl std::error::Error for StaleBinding {}

fn stale_binding(detail: impl Into<String>) -> anyhow::Error {
    anyhow::Error::new(StaleBinding {
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
            "existing_authority_clarification" => {
                Ok(ResolutionKind::ExistingAuthorityClarification)
            }
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

#[derive(Clone, Debug)]
pub struct AuthorityAmendmentRequest {
    pub effort: String,
    pub action: String,
    pub note: String,
    pub evidence: Vec<PathBuf>,
    pub confirm_not_running: bool,
}

#[derive(Clone, Debug, Serialize)]
pub struct AuthorityAmendmentOutcome {
    pub record: PathBuf,
    pub amendment_id: String,
    pub predecessor_effort_id: String,
    pub predecessor_reconciled: ArtifactRef,
    pub action: String,
    pub reason: String,
    pub successor_guidance: String,
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

/// Additive authority refusal for a historical accepted-but-incomplete action
/// that has no durable stop record. It never changes state.json or frozen inputs.
#[derive(Clone, Debug, Deserialize, Serialize)]
struct AuthorityAmendmentRecord {
    schema_version: u32,
    amendment_id: String,
    created_at_ms: u128,
    effort_id: String,
    note: String,
    evidence: Vec<ResolutionEvidence>,
    prior_action: CurrentAction,
    predecessor_reconciled: ArtifactRef,
    plan_digest: String,
    detailed_plan_digest: String,
    config_digest: String,
    predecessor_commits: PredecessorCommits,
    checkout_status: String,
    in_flight_confirmation: bool,
    reason: String,
    successor_guidance: String,
    successor: SuccessorLink,
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
    /// Durable lineage for the directed successor, not prose alone.
    successor: SuccessorLink,
}

/// Durable lineage between a refused authority amendment and the successor Build
/// that answers it.  The refusal names the predecessor's exact adopted contract,
/// its commits and the refused resolution; `successor_reconciled` is filled only
/// by the successor's own plan, because no contract exists to name yet.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct SuccessorLink {
    resolution_id: String,
    predecessor_effort_id: String,
    predecessor_reconciled: ArtifactRef,
    predecessor_commits: PredecessorCommits,
    successor_reconciled: Option<ArtifactRef>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
struct PredecessorCommits {
    discovery_baseline_commit: String,
    discovery_baseline_tree: String,
    build_start_commit: String,
    build_start_tree: String,
    head_commit: String,
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

    /// Whether this adapter launches, on this machine, the provider executable
    /// the role configuration names.  The local CLI adapter does, so a missing
    /// or unlaunchable binary is a deterministic setup failure the controller
    /// must find before any product-changing dispatch.  An injected transport
    /// reports `false`: the configured binary is then not the execution path,
    /// and its absence says nothing about what this adapter would run.
    fn launches_configured_executable(&self) -> bool {
        false
    }

    /// Whether one provider program is available to this adapter's execution
    /// path.  The local CLI adapter answers from this machine's own PATH; a
    /// transport that launches nothing is never asked.
    fn provider_executable_available(&self, program: &str) -> bool {
        preflight::resolve_executable(program).is_some()
    }
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
    /// The provider process could not start at all, for example the executable
    /// is missing or not executable.  This is deterministic: the same launch
    /// fails the same way, so it stops instead of consuming recovery.
    SpawnFailed {
        detail: String,
    },
    /// The provider could not be reached or failed before it accepted the
    /// action.  It remains recoverable because another attempt can differ.
    FailedBeforeAcceptance {
        detail: String,
    },
    AcceptedButIncomplete {
        detail: String,
    },
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
    let state_path = build_dir.join("state.json");
    // An existing Build is loaded first: its frozen inputs are compared from the
    // identity saved in its state, so a malformed, missing or edited input is the
    // typed stop the controller already knows instead of a bare parse error.
    let (mut state, frozen_mismatch) = if state_path.exists() {
        load_state(&state_path, &build_dir)?
    } else {
        let plan = load_plan(store, &effort, &build_dir)?;
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
    let plan = load_plan(store, &effort, &build_dir)?;
    materialize_role_guides(&build_dir)?;
    let progress = Progress::new();
    // A resolution recorded before its state transition is the authority for
    // that transition, so an interrupted resolution completes exactly once.
    apply_pending_resolution(
        store,
        &effort,
        &project,
        &build_dir,
        &state_path,
        &mut state,
    )?;
    // The effective configuration is chosen after that replay: an overlay the
    // replayed transition just published governs the action it recorded, so
    // loading it earlier would drive the first future action with stale settings.
    let config = effective_config(&build_dir)?;
    progress.event(&format!(
        "effort {} · Build config v{} · {} delivery phases",
        effort.id,
        config.version,
        plan.delivery_phases.len()
    ));
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
    // Bounded, inference-free readiness of the effective configuration the
    // required roles would dispatch with, checked before any product-changing
    // work so a worker cannot change the product and only then discover that the
    // reviewer cannot be launched.  The check runs after any replayed overlay,
    // so it always sees the configuration the next action would use.
    if adapter.launches_configured_executable()
        && let Err(error) = static_readiness(&config.config, &|program| {
            adapter.provider_executable_available(program)
        })
    {
        record_stop(store, &effort, &build_dir, &mut state, &error)?;
        save_state(&state_path, &state)?;
        if let Some(stop) = &state.stop {
            progress.event(&format!(
                "STOPPED — {} on action {}: {}",
                trigger_name(&stop.trigger),
                stop.action.id,
                stop.detail
            ));
        }
        return Ok(blocked_result(state_path, &state, format!("{error:#}")));
    }

    loop {
        let prior_action = state.action.clone();
        let (role_name, role_config) = role_for(&config.config, &state.action.kind);
        // Only the optional once-over can lack a configuration, and it is not an
        // error: an operator overlay may have removed the role while its action
        // was pending.  The loop must not assume it exists; perform_action
        // records that advisory absence and hands the Build to the formal Audit.
        let role_description = match role_config {
            Some(role_config) => format!(
                "{role_name} through {}{}{}",
                role_config.adapter,
                role_config
                    .model
                    .as_deref()
                    .map(|model| format!("/{model}"))
                    .unwrap_or_default(),
                role_config
                    .reasoning_effort
                    .as_deref()
                    .map(|effort| format!(" (effort {effort})"))
                    .unwrap_or_default(),
            ),
            None => {
                "no configured role (the optional advisory once-over is not configured in the effective configuration)".to_owned()
            }
        };
        let dispatch_description = if role_config.is_none() {
            "recording the advisory absence; nothing is dispatched"
        } else {
            match state.action.dispatch {
                DispatchState::Prepared => "dispatching",
                DispatchState::Running => "checking an uncertain action",
                DispatchState::Completed => "consuming its receipt",
            }
        };
        progress.event(&format!(
            "{} · {} · {role_description} · {dispatch_description}",
            state.action.scope,
            action_kind_name(&state.action.kind),
        ));
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
            &progress,
        );
        match result {
            Ok(()) => {
                if state.action.id != prior_action.id {
                    journal_action_transition(store, &effort, &prior_action, &state.action)?;
                    progress.event(&format!(
                        "{} finished; next is {} · {} · scope {}",
                        prior_action.id,
                        action_kind_name(&state.action.kind),
                        state.action.role_name(),
                        state.action.scope
                    ));
                }
                save_state(&state_path, &state)?;
                if let Some(done) = &state.terminal {
                    let verdict = store
                        .load_json::<orchestrate_contracts::AuditReport>(
                            &effort,
                            &done.audit,
                            "audit.json",
                        )
                        .map(|(_, report)| format!("{:?}", report.verdict))
                        .unwrap_or_else(|_| "unknown".into());
                    progress.event(&format!(
                        "BUILD COMPLETE — delivery finished and formal Audit {} derived {verdict}",
                        done.audit.artifact_id
                    ));
                    return Ok(BuildResult::Completed(done.clone()));
                }
            }
            Err(error) => {
                record_stop(store, &effort, &build_dir, &mut state, &error)?;
                save_state(&state_path, &state)?;
                if let Some(stop) = &state.stop {
                    progress.event(&format!(
                        "STOPPED — {} on action {}: {}",
                        trigger_name(&stop.trigger),
                        stop.action.id,
                        stop.detail
                    ));
                }
                return Ok(blocked_result(state_path, &state, format!("{error:#}")));
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
        // An existing Build is selected by its saved state alone: an edited,
        // malformed or missing frozen input must reach its typed stop rather
        // than hide the effort from the controller.
        if build.join("state.json").is_file() {
            match read_json::<BuildState>(&build.join("state.json")) {
                Ok(state) if state.terminal.is_some() => completed.push(effort),
                _ => active.push(effort),
            }
            continue;
        }
        if !build.join("config.toml").is_file() || !build.join("plan.json").is_file() {
            continue;
        }
        active.push(effort);
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
    let mut config: BuildConfig = toml::from_str(text).context("invalid build/config.toml")?;
    ensure!(
        matches!(
            config.schema_version,
            LEGACY_BUILD_CONFIG_VERSION | BUILD_CONFIG_VERSION
        ),
        "unsupported Build config version {}; supported versions are {} (provider-neutral) and {} (frozen legacy)",
        config.schema_version,
        BUILD_CONFIG_VERSION,
        LEGACY_BUILD_CONFIG_VERSION
    );
    for role in [&mut config.worker, &mut config.reviewer]
        .into_iter()
        .chain(config.unblocker.iter_mut())
        .chain(config.once_over.iter_mut())
    {
        role.config_schema_version = config.schema_version;
    }
    validate_role("worker", &config.worker, config.schema_version)?;
    validate_role("reviewer", &config.reviewer, config.schema_version)?;
    if let Some(unblocker) = &config.unblocker {
        validate_role("unblocker", unblocker, config.schema_version)?;
    }
    if let Some(once_over) = &config.once_over {
        validate_role("once_over", once_over, config.schema_version)?;
    }
    Ok(config)
}

/// The role configuration for the next invocation.  Version 1 is always the
/// frozen `config.toml`; higher versions only exist as immutable overlays an
/// operator resolution recorded.
pub(crate) struct EffectiveConfig {
    pub version: u32,
    pub config: BuildConfig,
}

pub(crate) fn effective_config(build_dir: &Path) -> Result<EffectiveConfig> {
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

/// Load the configuration immediately preceding a recorded overlay version.
/// An overlay may already be on disk when resolution replay resumes after a
/// crash, so comparing against `effective_config` would compare the new config
/// with itself and retain sessions owned by the previous adapter.
fn config_before_overlay(build_dir: &Path, version: u32) -> Result<BuildConfig> {
    let mut config = load_config(build_dir)?;
    for overlay in config_overlays(build_dir)? {
        if overlay.version >= version {
            break;
        }
        config = parse_config(&overlay.config_text)?;
    }
    Ok(config)
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

fn validate_role(role: &str, config: &RoleConfig, schema_version: u32) -> Result<()> {
    ensure!(!config.adapter.is_empty(), "{role} adapter is required");
    ensure!(
        matches!(config.adapter.as_str(), "codex" | "claude" | "cursor"),
        "unsupported {role} adapter {}",
        config.adapter
    );
    if schema_version == LEGACY_BUILD_CONFIG_VERSION {
        ensure!(
            config.model_strength.is_none(),
            "unsupported {role} field model_strength for schema_version 2; create a schema_version 3 config or overlay"
        );
        if let Some(model) = &config.model {
            ensure!(
                !model.trim().is_empty(),
                "{role} field model has an empty value for adapter {}",
                config.adapter
            );
        }
        if let Some(effort) = &config.reasoning_effort {
            ensure!(
                config.adapter == "codex"
                    && ["minimal", "low", "medium", "high"].contains(&effort.as_str()),
                "unsupported {role} field reasoning_effort value {effort} for adapter {} in frozen schema_version 2",
                config.adapter
            );
        }
        if let Some(permission) = &config.permission {
            ensure!(
                permission == FULL_ACCESS,
                "unsupported {role} field permission value {permission} for adapter {} in frozen schema_version 2",
                config.adapter
            );
        }
        return Ok(());
    }
    ensure!(
        config.model.is_none(),
        "unsupported {role} field model value {} for adapter {}; schema_version 3 requires provider-neutral model_strength = standard|strong",
        config.model.as_deref().unwrap_or_default(),
        config.adapter
    );
    if let Some(strength) = &config.model_strength {
        ensure!(
            GENERIC_MODEL_STRENGTHS.contains(&strength.as_str()),
            "unsupported {role} field model_strength value {strength} for adapter {}; supported values are {}",
            config.adapter,
            GENERIC_MODEL_STRENGTHS.join(", ")
        );
    }
    if let Some(effort) = &config.reasoning_effort {
        ensure!(
            GENERIC_REASONING_EFFORTS.contains(&effort.as_str()),
            "unsupported {role} field reasoning_effort value {effort} for adapter {}; supported values are {}",
            config.adapter,
            GENERIC_REASONING_EFFORTS.join(", ")
        );
    }
    if let Some(permission) = &config.permission {
        ensure!(
            GENERIC_PERMISSIONS.contains(&permission.as_str()),
            "unsupported {role} field permission value {permission} for adapter {}; supported values are {}",
            config.adapter,
            GENERIC_PERMISSIONS.join(", ")
        );
    }
    // Verify all generic values have a concrete mapping before any provider is launched.
    attribute_arguments_for(role, config)?;
    Ok(())
}

/// The role name and exact configuration one action kind runs under.  Final
/// Audit always uses the reviewer's settings; an absent unblocker falls back to
/// them, and the once-over is only ever created when it is configured.
fn role_for<'a>(
    config: &'a BuildConfig,
    kind: &ActionKind,
) -> (&'static str, Option<&'a RoleConfig>) {
    match kind {
        ActionKind::Work => ("worker", Some(&config.worker)),
        ActionKind::Review | ActionKind::FinalAudit => ("reviewer", Some(&config.reviewer)),
        ActionKind::Unblock => match &config.unblocker {
            Some(unblocker) => ("unblocker", Some(unblocker)),
            None => ("reviewer", Some(&config.reviewer)),
        },
        // The once-over has no fallback: it runs only while it is configured,
        // and an operator overlay may remove it while its action is pending.
        ActionKind::OnceOver => ("once_over", config.once_over.as_ref()),
    }
}

/// Bounded, inference-free readiness of the required roles' execution path: the
/// executable each required role would launch and the adapter-native attribute
/// mapping it would pass.  It resolves names from this machine's own launch
/// environment and runs nothing, so it certifies no model, effort, permission or
/// sandbox capability; those stay unknown facts.
///
/// Only the required worker and reviewer roles gate here.  The optional
/// unblocker and the advisory once-over are reported by `build preflight` but
/// never block a Build: neither has to run before product work, and an
/// unlaunchable advisory role must stay advisory rather than stop delivery.
fn static_readiness(config: &BuildConfig, available: &dyn Fn(&str) -> bool) -> Result<()> {
    for (role, adapter, program) in required_role_programs(config)? {
        ensure!(
            available(program),
            stop_error(
                StopTrigger::SpawnFailure,
                Some(StopTrigger::SpawnFailure),
                format!(
                    "the required {role} role uses the {adapter} adapter, but the {program} executable is not on this launch environment's PATH; this is a deterministic setup failure, so no product-changing work was dispatched. Repair the environment, then resolve the stop and run again."
                ),
            )
        );
    }
    Ok(())
}

/// The program each required role would launch, and the adapter-native mapping
/// it would pass, in a stable order.  The optional unblocker and the advisory
/// once-over are deliberately absent: neither has to run before product work and
/// neither may gate delivery.
fn required_role_programs(
    config: &BuildConfig,
) -> Result<Vec<(&'static str, String, &'static str)>> {
    let mut programs: Vec<(&'static str, String, &'static str)> = Vec::new();
    for (role, role_config) in [("worker", &config.worker), ("reviewer", &config.reviewer)] {
        let program = preflight::program_for(&role_config.adapter)?;
        if programs.iter().any(|(_, _, checked)| *checked == program) {
            continue;
        }
        // The same mapping dispatch would build, so an invalid one is found here
        // rather than after a provider launch.
        attribute_arguments_for(role, role_config)?;
        programs.push((role, role_config.adapter.clone(), program));
    }
    Ok(programs)
}

/// The exact provider-native arguments one role configuration maps to, in the
/// order they are appended.  It is the single mapping used by dispatch and by
/// static preflight, so a reported mapping is the one that would run.
#[cfg(test)]
fn attribute_arguments(config: &RoleConfig) -> Result<Vec<String>> {
    attribute_arguments_for("role", config)
}

/// Translate provider-neutral attributes at the execution edge.  Frozen v2
/// configurations keep their original provider-native interpretation; v3
/// values use a versioned, explicit adapter table.
pub(crate) fn attribute_arguments_for(role: &str, config: &RoleConfig) -> Result<Vec<String>> {
    let mut args = Vec::new();
    if config.config_schema_version == LEGACY_BUILD_CONFIG_VERSION {
        if let Some(model) = &config.model {
            args.extend(["--model".into(), model.clone()]);
        }
        if let Some(effort) = &config.reasoning_effort {
            ensure!(
                config.adapter == "codex"
                    && ["minimal", "low", "medium", "high"].contains(&effort.as_str()),
                "unsupported {role} field reasoning_effort value {effort} for adapter {} in frozen schema_version 2",
                config.adapter
            );
            args.extend(["-c".into(), format!("model_reasoning_effort={effort}")]);
        }
        if config.permission.as_deref() == Some(FULL_ACCESS) {
            args.push(
                match config.adapter.as_str() {
                    "codex" => "--dangerously-bypass-approvals-and-sandbox",
                    "claude" => "--dangerously-skip-permissions",
                    "cursor" => "--force",
                    other => bail!("unsupported {role} adapter {other}"),
                }
                .into(),
            );
        }
        return Ok(args);
    }

    let effort = config.reasoning_effort.as_deref();
    if config.adapter == "codex" && config.permission.as_deref() == Some("workspace_write") {
        bail!(
            "unsupported {role} field permission value workspace_write for adapter codex: Codex protects Git metadata as read-only in workspace-write, so it cannot fulfill the commit contract; name the explicit unrestricted permission \"{FULL_ACCESS}\" instead if that is what this role needs"
        );
    }
    if let Some(strength) = config.model_strength.as_deref() {
        let model = match (config.adapter.as_str(), strength) {
            ("codex", "standard") => "gpt-5.5".to_owned(),
            ("codex", "strong") => "gpt-5.6-sol".to_owned(),
            ("claude", "standard") => "deepseek-flash".to_owned(),
            ("claude", "strong") => "deepseek-v4-pro".to_owned(),
            ("cursor", "standard") if effort.is_none() => "gpt-5.5".to_owned(),
            ("cursor", "strong") if effort.is_none() => "gpt-5.6-sol-high".to_owned(),
            ("cursor", "standard") => format!("gpt-5.5-{}", effort.unwrap()),
            ("cursor", "strong") => format!("gpt-5.6-sol-{}", effort.unwrap()),
            (adapter, value) => {
                bail!("unsupported {role} field model_strength value {value} for adapter {adapter}")
            }
        };
        args.extend(["--model".into(), model]);
    }
    if let Some(effort) = effort {
        match config.adapter.as_str() {
            "codex" => args.extend(["-c".into(), format!("model_reasoning_effort={effort}")]),
            "claude" => {
                let mapped = match effort {
                    "low" => "low",
                    "medium" => "high",
                    "high" => "max",
                    value => bail!(
                        "unsupported {role} field reasoning_effort value {value} for adapter claude"
                    ),
                };
                args.extend(["--effort".into(), mapped.into()]);
            }
            "cursor" if config.model_strength.is_some() => {}
            "cursor" => bail!(
                "unsupported {role} field reasoning_effort value {effort} for adapter cursor: set model_strength because Cursor selects reasoning through model IDs"
            ),
            adapter => bail!("unsupported {role} adapter {adapter}"),
        }
    }
    if let Some(permission) = config.permission.as_deref() {
        match (config.adapter.as_str(), permission) {
            ("codex", "read_only") => args.extend([
                "-c".into(),
                "sandbox_mode=\"read-only\"".into(),
                "-c".into(),
                "approval_policy=\"never\"".into(),
            ]),
            ("claude", "read_only") => args.extend([
                "--permission-mode".into(),
                "plan".into(),
                "--settings".into(),
                r#"{"sandbox":{"enabled":true,"allowUnsandboxedCommands":false}}"#.into(),
            ]),
            ("claude", "workspace_write") => args.extend([
                "--permission-mode".into(),
                "acceptEdits".into(),
                "--settings".into(),
                r#"{"sandbox":{"enabled":true,"allowUnsandboxedCommands":false}}"#.into(),
            ]),
            // Cursor's read-only modes are `plan` and `ask`.  `plan` exists to
            // propose a plan for approval and ends its turn requesting one,
            // which a role whose deliverable is its own report never receives;
            // `ask` is read-only without that approval turn.
            ("cursor", "read_only") => args.extend([
                "--trust".into(),
                "--mode".into(),
                "ask".into(),
                "--sandbox".into(),
                "enabled".into(),
            ]),
            ("cursor", "workspace_write") => args.extend([
                "--trust".into(),
                "--force".into(),
                "--sandbox".into(),
                "enabled".into(),
            ]),
            // The one explicit unrestricted mapping per adapter.  It is only ever
            // reached from an operator's own `permission = "full_access"`.
            (adapter, FULL_ACCESS) => args.push(
                match adapter {
                    "codex" => "--dangerously-bypass-approvals-and-sandbox",
                    "claude" => "--dangerously-skip-permissions",
                    "cursor" => "--force",
                    other => {
                        bail!("unsupported {role} field permission value {FULL_ACCESS} for adapter {other}")
                    }
                }
                .into(),
            ),
            (adapter, value) => {
                bail!("unsupported {role} field permission value {value} for adapter {adapter}")
            }
        }
    }
    Ok(args)
}

/// How each configured attribute is described for one role: the value applied,
/// or that the provider's own default governs and is not a frozen effective
/// value.
pub(crate) fn attribute_provenance(config: &RoleConfig) -> serde_json::Value {
    let describe = |name: &str, value: &Option<String>| match value {
        Some(value) => {
            serde_json::json!({"attribute": name, "configured": value, "source": "configured"})
        }
        None => {
            serde_json::json!({"attribute": name, "configured": null, "source": "provider default (not a frozen effective value)"})
        }
    };
    serde_json::json!({
        "adapter": {"attribute": "adapter", "configured": config.adapter, "source": "configured"},
        "model_strength": describe("model_strength", &config.model_strength),
        "legacy_model": describe("model", &config.model),
        "reasoning_effort": describe("reasoning_effort", &config.reasoning_effort),
        "permission": describe("permission", &config.permission),
    })
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
    let successor_of = successor_linkage(store, effort, plan)?;
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
        successor_of,
        checkouts: Vec::new(),
        once_over: Vec::new(),
        phase_reviews: Vec::new(),
        cleanup: None,
    };
    save_state(&build_dir.join("state.json"), &state)?;
    Ok(state)
}

/// Validate the predecessor linkage a successor Build declares against the
/// predecessor's own durable refusal, and return the linkage its state records.
/// The predecessor's authority, state and commits are read, never modified, and
/// only this successor's plan supplies its exact new contract.
fn successor_linkage(
    store: &Store,
    effort: &Effort,
    plan: &BuildPlan,
) -> Result<Option<SuccessorLink>> {
    let Some(declared) = &plan.predecessor else {
        return Ok(None);
    };
    ensure!(
        declared.effort_id != effort.id,
        "a Build cannot declare itself its own predecessor"
    );
    ensure!(
        plan.reconciled != declared.reconciled,
        "a successor Build needs its own Reconciled Discovery, not the predecessor's adopted contract"
    );
    let predecessor = store.load_effort(&declared.effort_id).with_context(|| {
        format!(
            "successor linkage names unknown predecessor effort {}",
            declared.effort_id
        )
    })?;
    let predecessor_build = store.phase_dir(&predecessor, "build")?;
    let predecessor_state_path = predecessor_build.join("state.json");
    ensure!(
        predecessor_state_path.is_file(),
        "successor linkage names predecessor effort {} which has no Build state",
        declared.effort_id
    );
    let predecessor_state: BuildState = read_json(&predecessor_state_path)?;
    ensure!(
        predecessor_state.schema_version == BUILD_STATE_VERSION,
        "unsupported Build state version in {}",
        predecessor_state_path.display()
    );
    ensure!(
        predecessor_state.frozen.reconciled == declared.reconciled,
        "successor linkage names a contract that is not predecessor effort {}'s adopted Reconciled Discovery",
        declared.effort_id
    );
    let resolution = resolution_path(&predecessor_build, &declared.resolution_id);
    if resolution.is_file() {
        let record: ResolutionRecord = read_json(&resolution)?;
        ensure!(
            record.resolution_id == declared.resolution_id
                && record.status == ResolutionStatus::Refused
                && record.kind == ResolutionKind::AuthorityChange,
            "successor linkage needs a recorded authority-change refusal of predecessor effort {}, not {}",
            declared.effort_id,
            declared.resolution_id
        );
        let refusal = record
            .refusal
            .as_ref()
            .context("refused resolution record carries no refusal")?;
        ensure!(
            refusal.successor.resolution_id == declared.resolution_id
                && refusal.successor.predecessor_effort_id == declared.effort_id
                && refusal.successor.predecessor_reconciled == declared.reconciled,
            "successor linkage does not match the predecessor's recorded refusal"
        );
        Ok(Some(SuccessorLink {
            resolution_id: declared.resolution_id.clone(),
            predecessor_effort_id: declared.effort_id.clone(),
            predecessor_reconciled: declared.reconciled.clone(),
            predecessor_commits: refusal.successor.predecessor_commits.clone(),
            successor_reconciled: Some(plan.reconciled.clone()),
        }))
    } else {
        let amendment = authority_amendment_path(&predecessor_build, &declared.resolution_id);
        ensure!(
            amendment.is_file(),
            "successor linkage names no recorded authority refusal {} of predecessor effort {}",
            declared.resolution_id,
            declared.effort_id
        );
        let record: AuthorityAmendmentRecord = read_json(&amendment)?;
        ensure!(
            record.schema_version == 1
                && record.amendment_id == declared.resolution_id
                && record.effort_id == declared.effort_id
                && record.in_flight_confirmation
                && record.prior_action.dispatch == DispatchState::Running,
            "successor linkage names an invalid authority-amendment refusal"
        );
        ensure!(
            record.successor.resolution_id == declared.resolution_id
                && record.successor.predecessor_effort_id == declared.effort_id
                && record.successor.predecessor_reconciled == declared.reconciled,
            "successor linkage does not match the predecessor's recorded authority amendment"
        );
        Ok(Some(SuccessorLink {
            resolution_id: declared.resolution_id.clone(),
            predecessor_effort_id: declared.effort_id.clone(),
            predecessor_reconciled: declared.reconciled.clone(),
            predecessor_commits: record.successor.predecessor_commits,
            successor_reconciled: Some(plan.reconciled.clone()),
        }))
    }
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
    match OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&original_path)
    {
        Ok(mut file) => {
            file.write_all(&original)?;
            file.sync_all()?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            ensure!(
                fs::read(&original_path)? == original,
                "preserved state-v3 bytes differ from current migration source"
            );
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
    if store
        .read_journal(effort)?
        .iter()
        .any(|entry| entry.event == event && entry.details["operation_id"] == operation_id)
    {
        return Ok(());
    }
    let mut details = details;
    details["operation_id"] = serde_json::json!(operation_id);
    store.append_journal(effort, event, Some(operation_id), details)
}

/// Load one Build state and report, rather than raise, a frozen input that
/// changed after execution began.  Every comparison uses a digest saved in the
/// state, so an edited, malformed or missing input is reported as the changed
/// frozen input it is.  The caller records that as a typed stop.
fn load_state(path: &Path, build_dir: &Path) -> Result<(BuildState, Option<String>)> {
    let state: BuildState = read_json(path)?;
    ensure!(
        state.schema_version == BUILD_STATE_VERSION,
        "unsupported Build state version"
    );
    let mut inputs = vec![
        (
            "plan.json".to_owned(),
            "Build plan",
            state.frozen.plan_digest.as_str(),
        ),
        (
            "config.toml".to_owned(),
            "Build config",
            state.frozen.config_digest.as_str(),
        ),
    ];
    // The detailed plan's relative path is read straight from plan.json, so a
    // missing detailed plan is a changed frozen input rather than a plan error.
    // An unreadable plan.json is already reported by its own digest above.
    if let Some(detailed_plan) = read_json::<BuildPlan>(&build_dir.join("plan.json"))
        .ok()
        .map(|plan| plan.detailed_plan)
        .filter(|path| safe_relative_path(path))
    {
        inputs.push((
            detailed_plan,
            "detailed implementation plan",
            state.frozen.detailed_plan_digest.as_str(),
        ));
    }
    let mut mismatch = None;
    for (name, label, expected) in inputs {
        let actual = fs::read(build_dir.join(&name)).map(|bytes| digest_bytes(&bytes));
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
    progress: &Progress,
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
                role_provenance(
                    "build",
                    canonical_role_guide(&ActionKind::Work),
                    &config.config.worker,
                ),
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
    let (role, role_config) = role_for(&config.config, &state.action.kind);
    let action_dir = build_dir.join("artifacts").join(&state.action.id);
    fs::create_dir_all(&action_dir)?;
    let Some(role_config) = role_config else {
        // Only the once-over can lack a configuration.  Its advisory rule
        // applies: record that it cannot run and hand the Build to the formal
        // Audit rather than dispatching under substituted settings.
        let action = state.action.clone();
        record_once_over(build_dir, state, &action, &action_dir, "not_configured");
        save_state(state_path, state)?;
        return Ok(());
    };
    let completion_marker = action_dir.join("transport.completed");
    if state.action.dispatch == DispatchState::Running {
        if completion_marker.is_file() {
            state.action.dispatch = DispatchState::Completed;
            save_state(state_path, state)?;
        } else if matches!(state.action.kind, ActionKind::OnceOver) {
            // The advisory once-over is non-mutating and never gated, so an
            // uncertain one is recorded as such and cannot block the formal
            // Audit.  It is not resent either.
            let action = state.action.clone();
            record_once_over(build_dir, state, &action, &action_dir, "uncertain");
            save_state(state_path, state)?;
            return Ok(());
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
        // The exact instruction bytes this invocation receives are retained with
        // the action, so a later CLI that regenerates the guides cannot change
        // what an earlier invocation was told.
        write_bytes_sync(
            &action_dir.join("instruction.md"),
            canonical_role_guide(&state.action.kind).as_bytes(),
        )?;
        // The durable-output contract travels with the action too, so the exact
        // evidence rules this invocation was given stay recoverable from it.
        write_bytes_sync(
            &action_dir.join("output-contract.md"),
            orchestrate_guides::EVIDENCE_OUTPUT.as_bytes(),
        )?;
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
        ActionKind::OnceOver => contained_checkout(
            project,
            &action_dir,
            state
                .action
                .target_commit
                .as_deref()
                .context("once-over lacks target commit")?,
            "once-over",
        )?,
        _ => project.canonical_locator.clone(),
    };
    if let (ActionKind::Review | ActionKind::FinalAudit | ActionKind::OnceOver, Some(commit)) =
        (&state.action.kind, state.action.target_commit.clone())
    {
        // Ownership of every contained checkout is recorded here, with its exact
        // commit, so nothing later has to infer ownership from a path or name.
        let action = state.action.clone();
        record_checkout(
            state,
            state_path,
            &action,
            action_kind_name(&state.action.kind),
            &cwd,
            &commit,
        )?;
    }
    write_action_file(
        store,
        effort,
        build_dir,
        plan,
        state,
        &action_dir,
        &cwd,
        &config.config,
        config.version,
    )?;
    let persistent_session = match state.action.kind {
        ActionKind::Work => PersistentSession::Worker,
        ActionKind::Review => PersistentSession::Reviewer,
        ActionKind::FinalAudit | ActionKind::Unblock | ActionKind::OnceOver => {
            PersistentSession::None
        }
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
        state.action.transport_session_id = session.clone();
        save_state(state_path, state)?;
        // The wall times of this dispatch are taken here, immediately before the
        // provider is launched, so an action prepared long before dispatch does
        // not report preparation time as model time.
        let dispatched_at_ms = now_ms();
        write_invocation_record(
            &invocation,
            &state.action,
            config.version,
            None,
            InvocationTiming {
                prepared_at_ms: Some(dispatched_at_ms),
                dispatched_at_ms: Some(dispatched_at_ms),
                ended_at_ms: None,
                monotonic_elapsed_ms: None,
            },
            None,
        )?;
        // While the provider runs, the operator still sees that the controller
        // is alive and when the provider last wrote anything.
        let dispatched_at = std::time::Instant::now();
        let heartbeat = progress.heartbeat(
            action_dir.clone(),
            format!(
                "{} {} ({role} through {})",
                state.action.id,
                action_kind_name(&state.action.kind),
                role_config.adapter
            ),
        );
        let mut observed_session = None;
        let outcome = {
            let mut observer = StateObserver {
                state_path,
                state,
                persistent_session,
                observed_session: &mut observed_session,
            };
            adapter.invoke(&invocation, &mut observer)?
        };
        drop(heartbeat);
        write_invocation_record(
            &invocation,
            &state.action,
            config.version,
            observed_session.as_deref(),
            InvocationTiming {
                prepared_at_ms: Some(dispatched_at_ms),
                dispatched_at_ms: Some(dispatched_at_ms),
                ended_at_ms: Some(now_ms()),
                monotonic_elapsed_ms: Some(dispatched_at.elapsed().as_millis()),
            },
            Some((&outcome.completion, completion_detail(&outcome.completion))),
        )?;
        match outcome.completion {
            InvocationCompletion::Completed => {
                state.action.dispatch = DispatchState::Completed;
                write_bytes_sync(&completion_marker, b"completed\n")?;
                save_state(state_path, state)?;
            }
            InvocationCompletion::SpawnFailed { detail } => {
                if matches!(state.action.kind, ActionKind::OnceOver) {
                    let action = state.action.clone();
                    record_once_over(build_dir, state, &action, &action_dir, "spawn_failed");
                    return Ok(());
                }
                // A deterministic OS spawn failure such as NotFound or
                // PermissionDenied cannot be changed by retrying the same launch,
                // so it stops immediately with no continuation, replacement or
                // Unblock diagnosis.
                return Err(stop_error(
                    StopTrigger::SpawnFailure,
                    Some(StopTrigger::SpawnFailure),
                    format!(
                        "{detail}; role {role} was never accepted and a deterministic spawn failure is not retried"
                    ),
                ));
            }
            InvocationCompletion::FailedBeforeAcceptance { detail } => {
                if matches!(state.action.kind, ActionKind::OnceOver) {
                    let action = state.action.clone();
                    record_once_over(build_dir, state, &action, &action_dir, "failed");
                    return Ok(());
                }
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
            InvocationCompletion::AcceptedButIncomplete { detail } => {
                if matches!(state.action.kind, ActionKind::OnceOver) {
                    let action = state.action.clone();
                    record_once_over(build_dir, state, &action, &action_dir, "incomplete");
                    return Ok(());
                }
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
    if !matches!(state.action.kind, ActionKind::Work) {
        // A read-only role cannot write its receipt, report or assessment, so
        // its final response carries them and the controller persists what
        // parses.  Work is excluded: a worker's contract is a commit, which no
        // read-only permission can perform.
        persist_transported_evidence(&action_dir, &state.action.kind)?;
    }
    if matches!(state.action.kind, ActionKind::OnceOver) {
        // Advisory only: a missing, malformed, stale or foreign once-over
        // receipt is recorded as that outcome and the formal Audit proceeds.
        // Nothing about the once-over may gate, stop or consume recovery.
        let expected = state.action.clone();
        let outcome = match read_json::<Receipt>(&action_dir.join("result.json")) {
            Ok(receipt)
                if receipt.action_id == expected.id
                    && receipt.scope == expected.scope
                    && matches!(receipt.outcome.as_str(), "advisory_complete" | "blocked") =>
            {
                receipt.outcome
            }
            _ => "invalid_receipt".into(),
        };
        record_once_over(build_dir, state, &expected, &action_dir, &outcome);
        return Ok(());
    }
    let receipt: Receipt = read_json(&action_dir.join("result.json"))
        .context("provider completed without a valid Build result receipt")
        .map_err(receipt_failure)?;
    consume_receipt(
        store,
        effort,
        build_dir,
        plan,
        config,
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
    /// The session the provider's own stream reported, kept apart from the
    /// session this dispatch requested so a fresh invocation is never recorded
    /// as a resumed one.
    observed_session: &'a mut Option<String>,
}

impl InvocationObserver for StateObserver<'_> {
    fn accepted(&mut self) -> Result<()> {
        self.state.action.dispatch = DispatchState::Running;
        self.state.action.accepted_at_ms = Some(now_ms());
        save_state(self.state_path, self.state)
    }

    fn session_id(&mut self, session_id: &str) -> Result<()> {
        *self.observed_session = Some(session_id.into());
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
    config: &BuildConfig,
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
    // Every identifier the entry names must exist.  An entry may be a bare id
    // or a sentence that names ids, so a frozen plan written either way reaches
    // the same check; ordinary prose words are not identifiers.
    for entry in &phase_requirements {
        for token in requirement_tokens(entry) {
            ensure!(
                requirement_ids.contains(token),
                "phase authority for {} references an unknown binding requirement {token}",
                state.action.scope
            );
        }
    }
    let tasks = plan
        .delivery_phases
        .iter()
        .find(|phase| phase.id == state.action.scope)
        .map(|phase| phase.tasks.clone())
        .unwrap_or_default();
    let instruction = build_dir
        .join("instructions")
        .join(instruction_name(&state.action.kind));
    let role = role_for(config, &state.action.kind).0;
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
        // Named for every role: a role whose permission prevents writing the
        // files above states the same evidence in its final response instead.
        "output_contract": output_contract_path(build_dir),
        "detailed_plan": build_dir.join(&plan.detailed_plan),
        "working_directory": cwd,
        "config_version": config_version,
    });
    if let Some(handoff) = state.action.handoff.as_deref() {
        // Exact predecessor facts: the action, its report/result/transport or
        // their explicit absence, the session, and the observed ending commit.
        // Nothing here turns a predecessor claim into verified completion.
        let predecessor_dir = build_dir.join("artifacts").join(&handoff.action.id);
        let presence = |name: &str| {
            let path = predecessor_dir.join(name);
            let present = path.is_file();
            (path, present)
        };
        let (report, report_present) = presence("report.md");
        let (result, result_present) = presence("result.json");
        let (transport, transport_present) = presence("transport.jsonl");
        let (provider_stderr, provider_stderr_present) = presence("provider-stderr.log");
        value["predecessor"] = serde_json::json!({
            "handoff": handoff.kind,
            "action_id": handoff.action.id,
            "kind": handoff.action.kind,
            "scope": handoff.action.scope,
            "dispatch": handoff.action.dispatch,
            "session_id": handoff.action.transport_session_id,
            "target_commit": handoff.action.target_commit,
            "ending_commit": handoff.ending_commit,
            "report": report,
            "report_present": report_present,
            "result": result,
            "result_present": result_present,
            "transport": transport,
            "transport_present": transport_present,
            "provider_stderr": provider_stderr,
            "provider_stderr_present": provider_stderr_present,
            "claims": "The predecessor's own claims, including any reported test run or command chain, remain unverified and are not completion. Determine recorded work and outstanding checks from the referenced report and result yourself; an early failure in the predecessor's conditional command chain means later commands were not executed.",
            "environment_note": "Any environment workaround or tool-path observation the predecessor recorded is re-checkable execution context, not a requirement.",
        });
    }
    if let Some(remedy) = &state.action.remedy_path {
        value["remedy"] = serde_json::json!({
            "report": remedy,
            "note": "the diagnosis that authorized this retry; it is recovery context and does not replace the original correction named by feedback",
        });
    }
    if matches!(state.action.kind, ActionKind::OnceOver) {
        let target_commit = state
            .action
            .target_commit
            .as_deref()
            .context("once-over lacks target commit")?;
        value["build_start_commit"] = serde_json::json!(state.frozen.build_start_commit);
        value["build_start_tree"] = serde_json::json!(state.frozen.build_start_tree);
        value["delivery_reviews"] = serde_json::json!(state.phase_reviews);
        value["once_over"] = serde_json::json!({
            "commit": target_commit,
            "advisory": "This review is advisory. Its report stays out of the worker and formal Audit inputs, it never gates acceptance, and the formal Audit decides the outcome.",
        });
    }
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
            value["unresolved_requirement_ids"] =
                serde_json::json!(stop.unresolved_requirement_ids);
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
        // A sentence-final period is prose punctuation, not part of the
        // identifier: a frozen plan that writes its list as a sentence must
        // resolve to the same requirement ids as one that writes a bare list.
        .map(|entry| entry.trim().trim_end_matches('.').trim())
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect();
    Some((
        requirements,
        section("Completion evidence"),
        section("Deliberate later-phase exclusions"),
    ))
}

/// Every identifier a phase requirement entry names, in the identifier shape
/// requirement ids are written in: letters, a hyphen, then a digit.  A frozen
/// plan may list ids or name them inside a sentence; text that is not written
/// in that shape is prose, not an identifier.
fn requirement_tokens(entry: &str) -> impl Iterator<Item = &str> {
    entry
        .split(|character: char| !(character.is_ascii_alphanumeric() || character == '-'))
        .filter(|token| match token.split_once('-') {
            Some((prefix, rest)) => {
                !prefix.is_empty()
                    && prefix
                        .chars()
                        .all(|character| character.is_ascii_alphabetic())
                    && rest.starts_with(|character: char| character.is_ascii_digit())
            }
            None => false,
        })
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
        ActionKind::OnceOver => "once-over.md",
    }
}

/// The step after final delivery Review passes or final-scope work completes:
/// the advisory once-over once per new commit submitted to Audit when the role
/// is configured, then that commit's formal Audit.  The formal Audit is never
/// gated by the once-over.
fn next_audit_step(
    build_dir: &Path,
    config: &BuildConfig,
    state: &BuildState,
    commit: &str,
    feedback: Option<PathBuf>,
) -> CurrentAction {
    if config.once_over.is_some() && !state.once_over.iter().any(|run| run.commit == commit) {
        return new_action(
            build_dir,
            ActionKind::OnceOver,
            FINAL_SCOPE,
            Some(commit.into()),
            feedback,
        );
    }
    new_action(
        build_dir,
        ActionKind::FinalAudit,
        FINAL_SCOPE,
        Some(commit.into()),
        feedback,
    )
}

/// Record one advisory once-over outcome and hand the Build to the formal Audit
/// at the same commit.  Nothing here consults or changes recovery, stops or the
/// verdict.
fn record_once_over(
    build_dir: &Path,
    state: &mut BuildState,
    action: &CurrentAction,
    action_dir: &Path,
    outcome: &str,
) {
    let commit = action.target_commit.clone().unwrap_or_default();
    let record = OnceOverRecord {
        commit: commit.clone(),
        action_id: action.id.clone(),
        outcome: outcome.into(),
        report: action_dir.join("report.md").to_string_lossy().into_owned(),
    };
    if !state.once_over.contains(&record) {
        state.once_over.push(record);
    }
    state.action = new_action(
        build_dir,
        ActionKind::FinalAudit,
        FINAL_SCOPE,
        Some(commit).filter(|value| !value.is_empty()),
        action.feedback_path.as_ref().map(PathBuf::from),
    );
}

/// The marker each durable artifact a role emits in its final response carries.
/// A read-only role cannot write the files itself, so it states them here and
/// the controller validates and persists them.
const RECEIPT_BLOCK: &str = "orchestrate-receipt";
const REPORT_BLOCK: &str = "orchestrate-report";
const ASSESSMENT_BLOCK: &str = "orchestrate-assessment";
const TRANSPORT_EVIDENCE_VERSION: u32 = 1;

/// The provider's own final-response messages, in order, from the
/// controller-owned transport log.  Codex reports them as completed agent
/// messages; Claude Code and Cursor report the final response on their terminal
/// `result` event.  Nothing else in the stream is a role's answer.
pub(crate) fn transport_messages(transport: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(transport) else {
        return Vec::new();
    };
    let mut messages = Vec::new();
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(event) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if event["type"] == "item.completed"
            && event["item"]["type"] == "agent_message"
            && let Some(message) = event["item"]["text"].as_str()
            && !message.trim().is_empty()
        {
            messages.push(message.to_owned());
        }
        if event["type"] == "result"
            && event["is_error"] == false
            && let Some(message) = event["result"].as_str()
            && !message.trim().is_empty()
        {
            messages.push(message.to_owned());
        }
    }
    messages
}

/// One fenced block a role emitted, identified by its exact info string.  A
/// closing fence must be at least as long as its opener, so a report that
/// contains ordinary code fences survives intact.  The last complete block wins:
/// a later dispatch of the same action is the later word on it.
fn fenced_block(text: &str, info: &str) -> Option<String> {
    let mut open: Option<usize> = None;
    let mut body = String::new();
    let mut found: Option<String> = None;
    for line in text.lines() {
        let trimmed = line.trim();
        let run = trimmed
            .chars()
            .take_while(|character| *character == '`')
            .count();
        match open {
            Some(length) => {
                if run >= length && trimmed[run..].trim().is_empty() {
                    open = None;
                    found = Some(body.clone());
                } else {
                    body.push_str(line);
                    body.push('\n');
                }
            }
            None => {
                if run >= 3 && trimmed[run..].trim() == info {
                    open = Some(run);
                    body.clear();
                }
            }
        }
    }
    found.map(|block| block.trim_end().to_owned())
}

/// Persist the durable evidence this role could not write for itself, taken
/// from the provider's own final response.
///
/// A role whose permission contract is read-only cannot create the files the
/// controller requires, so the response carries the same artifacts in marked
/// blocks instead.  Each one is written only when it parses as the artifact the
/// controller would otherwise read, and the file stays the authority whenever
/// the role wrote one: this adds a source for evidence, never a second
/// interpretation of it.  What the controller then validates — action, scope,
/// commit, checkout state and Audit lineage — is unchanged, and a response that
/// carries no valid artifact leaves the existing stop in place rather than
/// inventing one.
fn persist_transported_evidence(action_dir: &Path, kind: &ActionKind) -> Result<()> {
    let receipt_path = action_dir.join("result.json");
    let report_path = action_dir.join("report.md");
    let assessment_path = action_dir.join("assessment.json");
    let wants_assessment = matches!(kind, ActionKind::FinalAudit);
    if receipt_path.is_file()
        && report_path.is_file()
        && (!wants_assessment || assessment_path.is_file())
    {
        return Ok(());
    }
    let transport = action_dir.join("transport.jsonl");
    let messages = transport_messages(&transport);
    if messages.is_empty() {
        return Ok(());
    }
    let response = messages.join("\n\n");
    let mut persisted = Vec::new();
    let mut persist = |path: &Path, block: &str, artifact: &str| -> Result<()> {
        write_bytes_sync(path, block.as_bytes())?;
        persisted.push(json!({
            "artifact": artifact,
            "block": artifact_block(artifact),
            "sha256": digest_bytes(block.as_bytes()),
            "bytes": block.len(),
        }));
        Ok(())
    };
    if !receipt_path.is_file()
        && let Some(block) = fenced_block(&response, RECEIPT_BLOCK)
        && serde_json::from_str::<Receipt>(&block).is_ok()
    {
        persist(&receipt_path, &block, "result.json")?;
    }
    if !report_path.is_file()
        && let Some(block) = fenced_block(&response, REPORT_BLOCK)
        && !block.trim().is_empty()
    {
        persist(&report_path, &block, "report.md")?;
    }
    if wants_assessment
        && !assessment_path.is_file()
        && let Some(block) = fenced_block(&response, ASSESSMENT_BLOCK)
        && serde_json::from_str::<AuditAssessment>(&block).is_ok()
    {
        persist(&assessment_path, &block, "assessment.json")?;
    }
    if persisted.is_empty() {
        return Ok(());
    }
    // What was read from the response, and from which bytes, stays auditable
    // beside the artifacts it produced.
    write_bytes_sync(
        &action_dir.join("transport-evidence.json"),
        &serde_json::to_vec_pretty(&json!({
            "schema_version": TRANSPORT_EVIDENCE_VERSION,
            "transport": transport,
            "transport_sha256": digest_bytes(&fs::read(&transport)?),
            "persisted": persisted,
            "note": "these artifacts were taken from the provider's final response because the role's permission contract prevented it from writing them; each parsed as the artifact the controller would otherwise have read, and the controller's validation of it is unchanged",
        }))?,
    )?;
    Ok(())
}

fn artifact_block(artifact: &str) -> &'static str {
    match artifact {
        "result.json" => RECEIPT_BLOCK,
        "report.md" => REPORT_BLOCK,
        _ => ASSESSMENT_BLOCK,
    }
}

#[allow(clippy::too_many_arguments)]
fn consume_receipt(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    plan: &BuildPlan,
    config: &EffectiveConfig,
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
        // Normally consumed before the receipt is validated; if it ever reaches
        // here, the advisory rule still applies and nothing gates.
        ActionKind::OnceOver => {
            let action = state.action.clone();
            record_once_over(build_dir, state, &action, action_dir, "invalid_receipt");
        }
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
                    next_audit_step(
                        build_dir,
                        &config.config,
                        state,
                        commit,
                        Some(action_dir.join("report.md")),
                    )
                } else {
                    new_action(
                        build_dir,
                        ActionKind::Review,
                        &scope,
                        Some(commit.into()),
                        Some(action_dir.join("report.md")),
                    )
                };
                state.action = next;
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
                let review = PhaseReview {
                    scope: state.action.scope.clone(),
                    commit: expected.clone(),
                    report: action_dir.join("report.md").to_string_lossy().into_owned(),
                };
                if !state.phase_reviews.contains(&review) {
                    state.phase_reviews.push(review);
                }
                if state.phase_index + 1 == plan.delivery_phases.len() {
                    state.action = next_audit_step(
                        build_dir,
                        &config.config,
                        state,
                        &expected,
                        Some(action_dir.join("report.md")),
                    );
                } else {
                    state.phase_index += 1;
                    let scope = plan.delivery_phases[state.phase_index].id.clone();
                    state.action = new_action(build_dir, ActionKind::Work, &scope, None, None);
                }
            } else if receipt.outcome == "changes_required" {
                // The corrected action must receive the complete correction set,
                // so a changes_required review has to leave one.
                ensure!(
                    fs::read_to_string(action_dir.join("report.md"))
                        .map(|report| !report.trim().is_empty())
                        .unwrap_or(false),
                    "review requested changes without a correction report for the corrected action"
                );
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
                role_provenance(
                    "audit",
                    canonical_role_guide(&ActionKind::FinalAudit),
                    &config.config.reviewer,
                ),
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
                        format!(
                            "completed formal Audit {} derived BLOCKED",
                            audit.artifact_id
                        ),
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
                // The original review/Audit correction stays the action's
                // feedback; the diagnosis travels separately as recovery
                // context and never replaces the correction source.
                let mut resumed = new_action(
                    build_dir,
                    interrupted.kind.clone(),
                    &interrupted.scope,
                    interrupted.target_commit.clone(),
                    interrupted.feedback_path.as_ref().map(PathBuf::from),
                )
                .continuing("remedy", &interrupted, observed_head(store, effort));
                resumed.resolution_id = interrupted.resolution_id.clone();
                resumed.remedy_path =
                    Some(action_dir.join("report.md").to_string_lossy().into_owned());
                state.action = resumed;
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
        let retry = retry_action(
            build_dir,
            state,
            "continuation",
            observed_head(store, effort),
        );
        state.action = retry;
        return Ok(());
    }
    if !state.recovery.replacement_used {
        state.recovery.replacement_used = true;
        match role {
            "worker" => state.sessions.worker = None,
            "unblocker" | "once_over" => {}
            _ => state.sessions.reviewer = None,
        }
        let retry = retry_action(
            build_dir,
            state,
            "replacement",
            observed_head(store, effort),
        );
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
        format!(
            "recovery is exhausted for scope {} ({detail})",
            state.action.scope
        ),
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
    let result_path = build_dir
        .join("artifacts")
        .join(&action.id)
        .join("result.json");
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
                "the provider action was not accepted; it never started or its start is unknown"
                    .into()
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

/// Re-run the same action once more, keeping its scope, target commit, original
/// correction and recovery context, and carrying the exact predecessor facts.
fn retry_action(
    build_dir: &Path,
    state: &BuildState,
    handoff: &str,
    ending_commit: Option<String>,
) -> CurrentAction {
    let mut retry = new_action(
        build_dir,
        state.action.kind.clone(),
        &state.action.scope,
        state.action.target_commit.clone(),
        state.action.feedback_path.as_ref().map(PathBuf::from),
    )
    .continuing(handoff, &state.action, ending_commit);
    retry.remedy_path = state.action.remedy_path.clone();
    retry.resolution_id = state.action.resolution_id.clone();
    retry
}

/// The observed product HEAD at a handoff boundary.  It is an observation, not
/// a claim about what the predecessor achieved.
/// Record controller ownership of one contained checkout: the exact owning
/// action, its canonical path and its exact detached commit.
fn record_checkout(
    state: &mut BuildState,
    state_path: &Path,
    action: &CurrentAction,
    kind: &str,
    path: &Path,
    commit: &str,
) -> Result<()> {
    let canonical = fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    let record = CheckoutRecord {
        action_id: action.id.clone(),
        kind: kind.into(),
        path: canonical.to_string_lossy().into_owned(),
        commit: commit.into(),
        created_at_ms: now_ms(),
    };
    if state
        .checkouts
        .iter()
        .any(|existing| existing.action_id == record.action_id && existing.path == record.path)
    {
        return Ok(());
    }
    state.checkouts.push(record);
    save_state(state_path, state)
}

fn observed_head(store: &Store, effort: &Effort) -> Option<String> {
    store
        .project_for(effort)
        .ok()
        .and_then(|project| git(&project.canonical_locator, ["rev-parse", "HEAD"]).ok())
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
    let ending_commit = observed_head(store, effort);
    state.interrupted = Some(Box::new(state.action.clone()));
    state.action = new_action(
        build_dir,
        ActionKind::Unblock,
        &state.action.scope,
        state.action.target_commit.clone(),
        state.action.feedback_path.as_ref().map(PathBuf::from),
    )
    .continuing("diagnosis", &interrupted, ending_commit);
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
        handoff: None,
        remedy_path: None,
        accepted_at_ms: None,
    }
}

impl CurrentAction {
    /// Attach the exact predecessor facts this action continues from.
    fn continuing(
        mut self,
        handoff: &str,
        predecessor: &CurrentAction,
        ending_commit: Option<String>,
    ) -> Self {
        self.handoff = Some(Box::new(Handoff {
            kind: handoff.into(),
            action: predecessor.clone(),
            ending_commit,
        }));
        self
    }

    /// The role this action runs as, for status and diagnostics.
    fn role_name(&self) -> &'static str {
        match self.kind {
            ActionKind::Work => "worker",
            ActionKind::Review | ActionKind::FinalAudit => "reviewer",
            ActionKind::Unblock => "unblocker",
            ActionKind::OnceOver => "once_over",
        }
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
    predecessor: &CurrentAction,
    ending_commit: Option<String>,
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
        handoff: Some(Box::new(Handoff {
            kind: "resolution".into(),
            action: predecessor.clone(),
            ending_commit,
        })),
        remedy_path: None,
        accepted_at_ms: None,
    }
}

fn action_kind_name(kind: &ActionKind) -> &'static str {
    match kind {
        ActionKind::Work => "work",
        ActionKind::Review => "review",
        ActionKind::FinalAudit => "final_audit",
        ActionKind::Unblock => "unblock",
        ActionKind::OnceOver => "once_over",
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
    let (mut state, frozen_mismatch) = load_state(&state_path, &build_dir)?;
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
    let config_text = role_config_text(&request)?;
    let resolution_id = resolution_id(
        &request.action,
        request.kind,
        &request.note,
        &evidence,
        config_text.as_deref(),
    );
    let record_path = resolution_path(&build_dir, &resolution_id);
    if record_path.is_file() {
        // An identical submission is idempotent, and it also completes a
        // transition an interrupted earlier attempt recorded but never applied.
        let existing: ResolutionRecord = read_json(&record_path)?;
        ensure!(
            existing.resolution_id == resolution_id,
            "resolution record {} is inconsistent",
            record_path.display()
        );
        if existing.status == ResolutionStatus::Resolved {
            apply_resolution(
                store,
                &effort,
                &project,
                &build_dir,
                &state_path,
                &mut state,
                &existing,
            )?;
        }
        return resolution_outcome(&record_path, &existing);
    }
    // A different submission for a stop that an applied resolution already
    // continued is stale, even though the Build has moved past that stop.  A
    // record that was never applied does not close the stop, so a rejected or
    // interrupted record cannot permanently block a fresh submission.
    let resolved_this_stop = applied_resolution_record(&build_dir, &state)?
        .filter(|applied| applied.binding.action.id == request.action);
    if let Some(applied) = resolved_this_stop {
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
        if stop.action.dispatch == DispatchState::Running {
            stop.action.id.as_str()
        } else {
            state.action.id.as_str()
        }
    );
    let binding = resolution_binding(&state, &project, &stop, request.confirm_not_running)?;
    if matches!(request.kind, ResolutionKind::AuthorityChange) {
        let refusal = ResolutionRefusal {
            reason: "a Build resolution cannot amend the adopted product authority".into(),
            successor_guidance: format!(
                "take the change through a new Discovery and Reconcile, then start a successor Build whose plan links this effort's exact Reconciled Discovery {}, the refusal {}, and its commits; nothing in this Build changes",
                state.frozen.reconciled.artifact_id, resolution_id
            ),
            successor: SuccessorLink {
                resolution_id: resolution_id.clone(),
                predecessor_effort_id: effort.id.clone(),
                predecessor_reconciled: state.frozen.reconciled.clone(),
                predecessor_commits: PredecessorCommits {
                    discovery_baseline_commit: state.frozen.discovery_baseline_commit.clone(),
                    discovery_baseline_tree: state.frozen.discovery_baseline_tree.clone(),
                    build_start_commit: state.frozen.build_start_commit.clone(),
                    build_start_tree: state.frozen.build_start_tree.clone(),
                    head_commit: binding.head_commit.clone(),
                },
                successor_reconciled: None,
            },
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
    let record = prepared_resolution(
        store,
        &effort,
        &build_dir,
        &state,
        &stop,
        &request,
        binding,
        evidence,
        config_text,
    )?;
    // The immutable record and note exist before the bounded transition, so an
    // interruption in between leaves a record the Build can apply exactly once.
    write_immutable(&record_path, &orchestrate_contracts::encode(&record)?)?;
    apply_resolution(
        store,
        &effort,
        &project,
        &build_dir,
        &state_path,
        &mut state,
        &record,
    )?;
    resolution_outcome(&record_path, &record)
}

/// Record a requested authority change against a historical in-flight action
/// that never acquired a durable stop. The record is additive, refused, and
/// linked for a future successor; this operation never edits Build state or
/// resumes provider work.
pub fn amend_authority(
    store: &Store,
    request: AuthorityAmendmentRequest,
) -> Result<AuthorityAmendmentOutcome> {
    ensure!(
        request.confirm_not_running,
        "recording an authority amendment for an accepted-but-incomplete action requires explicit confirmation that it is no longer running"
    );
    ensure!(
        !request.note.trim().is_empty(),
        "an authority amendment requires a note"
    );
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
    let state: BuildState = read_json(&state_path)?;
    ensure!(
        state.schema_version == BUILD_STATE_VERSION,
        "unsupported Build state version in {}; no state migration was performed",
        state_path.display()
    );
    ensure!(
        state.terminal.is_none(),
        "Build {} already completed",
        effort.id
    );
    ensure!(
        state.stop.is_none(),
        "Build {} has a durable stop; use `build resolve --kind authority_change` for its stopped action",
        effort.id
    );
    ensure!(
        state.action.id == request.action,
        "action {} is not the current historical action {} of effort {}",
        request.action,
        state.action.id,
        effort.id
    );
    ensure!(
        state.action.dispatch == DispatchState::Running,
        "current action {} is not recorded as an accepted-but-incomplete provider action",
        request.action
    );
    let evidence = read_evidence(&request.evidence)?;
    let mut material = format!(
        "{}|{}|{}|{}",
        effort.id, request.action, request.note, state.frozen.reconciled.digest
    );
    for item in &evidence {
        material.push_str(&format!("|{}:{}", item.path, item.sha256));
    }
    let amendment_id = format!("amend-{}", &digest_bytes(material.as_bytes())[..24]);
    let record_path = authority_amendment_path(&build_dir, &amendment_id);
    let head_commit = git(&project.canonical_locator, ["rev-parse", "HEAD"])?;
    let predecessor_commits = PredecessorCommits {
        discovery_baseline_commit: state.frozen.discovery_baseline_commit.clone(),
        discovery_baseline_tree: state.frozen.discovery_baseline_tree.clone(),
        build_start_commit: state.frozen.build_start_commit.clone(),
        build_start_tree: state.frozen.build_start_tree.clone(),
        head_commit,
    };
    let successor_guidance = format!(
        "This amendment cannot change effort {}'s adopted Reconciled Discovery {} or continue its interrupted action. Take the updated authority through a new Discovery and Reconcile, then create a successor Build whose plan links predecessor effort {}, adopted artifact {}, and authority record {}. The prior Build state and config remain untouched.",
        effort.id,
        state.frozen.reconciled.artifact_id,
        effort.id,
        state.frozen.reconciled.artifact_id,
        amendment_id
    );
    let reason = "the prior Build has no durable stop record for its interrupted action, so the requested authority change is recorded as a refusal and directed to a successor".to_owned();
    let record = AuthorityAmendmentRecord {
        schema_version: 1,
        amendment_id: amendment_id.clone(),
        created_at_ms: now_ms(),
        effort_id: effort.id.clone(),
        note: request.note,
        evidence,
        prior_action: state.action.clone(),
        predecessor_reconciled: state.frozen.reconciled.clone(),
        plan_digest: state.frozen.plan_digest.clone(),
        detailed_plan_digest: state.frozen.detailed_plan_digest.clone(),
        config_digest: state.frozen.config_digest.clone(),
        predecessor_commits: predecessor_commits.clone(),
        checkout_status: checkout_status(&project)?,
        in_flight_confirmation: true,
        reason: reason.clone(),
        successor_guidance: successor_guidance.clone(),
        successor: SuccessorLink {
            resolution_id: amendment_id.clone(),
            predecessor_effort_id: effort.id.clone(),
            predecessor_reconciled: state.frozen.reconciled.clone(),
            predecessor_commits,
            successor_reconciled: None,
        },
    };
    write_immutable(&record_path, &orchestrate_contracts::encode(&record)?)?;
    append_journal_once(
        store,
        &effort,
        "build_authority_amendment_refused",
        &amendment_id,
        serde_json::json!({
            "operation_id": amendment_id.clone(),
            "kind": "authority_change",
            "action": request.action.clone(),
            "record": record_path.to_string_lossy().into_owned(),
            "reason": reason.clone(),
            "successor_guidance": successor_guidance.clone(),
        }),
    )?;
    Ok(AuthorityAmendmentOutcome {
        record: record_path,
        amendment_id,
        predecessor_effort_id: effort.id,
        predecessor_reconciled: state.frozen.reconciled,
        action: state.action.id,
        reason,
        successor_guidance,
    })
}

/// The exact effort, stop, authority, frozen digests, checkout and trigger a
/// resolution is bound to.  Re-application revalidates the repository half.
fn resolution_binding(
    state: &BuildState,
    project: &Project,
    stop: &StopRecord,
    confirm_not_running: bool,
) -> Result<ResolutionBinding> {
    Ok(ResolutionBinding {
        stop: stop.clone(),
        trigger: stop.trigger.clone(),
        action: stop.action.clone(),
        reconciled: state.frozen.reconciled.clone(),
        plan_digest: state.frozen.plan_digest.clone(),
        detailed_plan_digest: state.frozen.detailed_plan_digest.clone(),
        config_digest: state.frozen.config_digest.clone(),
        head_commit: git(&project.canonical_locator, ["rev-parse", "HEAD"])?,
        checkout_status: checkout_status(project)?,
        in_flight_confirmation: confirm_not_running,
    })
}

/// The role configuration an operator supplied, already validated.  Only an
/// environment repair may carry one.
fn role_config_text(request: &ResolutionRequest) -> Result<Option<String>> {
    let Some(path) = &request.config else {
        return Ok(None);
    };
    ensure!(
        matches!(request.kind, ResolutionKind::EnvironmentRepair),
        "role configuration changes require an environment_repair resolution"
    );
    let text =
        fs::read_to_string(path).with_context(|| format!("cannot read {}", path.display()))?;
    parse_config(&text).context("the supplied role configuration is not a valid Build config")?;
    Ok(Some(text))
}

/// The immutable record one accepted resolution produces: the exact stop, the
/// bindings the operator resolved, and the one continuation it authorizes.
#[allow(clippy::too_many_arguments)]
fn prepared_resolution(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    state: &BuildState,
    stop: &StopRecord,
    request: &ResolutionRequest,
    binding: ResolutionBinding,
    evidence: Vec<ResolutionEvidence>,
    config_text: Option<String>,
) -> Result<ResolutionRecord> {
    let resolution_id = resolution_id(
        &request.action,
        request.kind,
        &request.note,
        &evidence,
        config_text.as_deref(),
    );
    let (next_kind, scope, target_commit, feedback) =
        continuation_for(store, effort, state, stop, request.kind)?;
    let continuation = continuation_action(
        &resolution_id,
        next_kind,
        &scope,
        target_commit,
        feedback,
        &stop.action,
        Some(binding.head_commit.clone()),
    );
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
        overlay.version = next_config_version(build_dir)?;
    }
    Ok(ResolutionRecord {
        schema_version: RESOLUTION_SCHEMA_VERSION,
        resolution_id,
        kind: request.kind,
        status: ResolutionStatus::Resolved,
        created_at_ms: now_ms(),
        effort_id: effort.id.clone(),
        note: request.note.clone(),
        evidence,
        binding,
        transition: Some(ResolutionTransition {
            governed_action: continuation.id.clone(),
            action: continuation,
            recovery_reset: true,
        }),
        config_overlay,
        refusal: None,
    })
}

fn resolution_outcome(record_path: &Path, record: &ResolutionRecord) -> Result<ResolutionOutcome> {
    match record.status {
        ResolutionStatus::Refused => {
            let refusal = record
                .refusal
                .clone()
                .context("refused resolution record carries no refusal")?;
            Ok(ResolutionOutcome::Refused {
                resolution: record_path.to_path_buf(),
                resolution_id: record.resolution_id.clone(),
                reason: refusal.reason,
                successor_guidance: refusal.successor_guidance,
            })
        }
        ResolutionStatus::Resolved => {
            let transition = record
                .transition
                .as_ref()
                .context("applied resolution record carries no transition")?;
            Ok(ResolutionOutcome::Resolved {
                resolution: record_path.to_path_buf(),
                resolution_id: record.resolution_id.clone(),
                kind: record.kind,
                stopped_action: record.binding.action.id.clone(),
                continuation_action: transition.action.id.clone(),
                continuation_kind: action_kind_name(&transition.action.kind).into(),
                config_version: record
                    .config_overlay
                    .as_ref()
                    .map(|overlay| overlay.version),
            })
        }
    }
}

/// Which action the resolution authorizes next.
fn continuation_for(
    store: &Store,
    effort: &Effort,
    state: &BuildState,
    stop: &StopRecord,
    kind: ResolutionKind,
) -> Result<(ActionKind, String, Option<String>, Option<String>)> {
    let subject = stop.interrupted.as_deref().unwrap_or(&stop.action);
    if matches!(kind, ResolutionKind::NewVerificationEvidence) {
        // New evidence answers a final-scope acceptance that completed as
        // BLOCKED.  The stop may be that acceptance itself, or the
        // external-requirement stop its single Unblock diagnosis produced; both
        // retain the same completed Audit attempt, which is re-verified here so
        // an unrelated stop at a final scope cannot borrow this path.
        ensure!(
            matches!(
                stop.trigger,
                StopTrigger::AuditBlocked | StopTrigger::ExternalRequirement
            ) && matches!(subject.kind, ActionKind::FinalAudit)
                && subject.scope == FINAL_SCOPE,
            "new verification evidence only applies to a final-scope Audit acceptance that completed and derived BLOCKED; this stop is {}",
            trigger_name(&stop.trigger)
        );
        ensure!(
            state.implementation.is_some(),
            "this Build has no registered implementation to reassess"
        );
        ensure!(
            retained_blocked_audit(store, effort, subject)?,
            "new verification evidence needs the retained BLOCKED publication of the interrupted Audit attempt {}",
            subject.id
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

/// The Audit published for one exact Build attempt, found by the attempt's own
/// operation identity rather than by implementation, scope or recency.
fn published_audit_for_attempt(
    store: &Store,
    effort: &Effort,
    attempt_id: &str,
) -> Result<Option<ArtifactRef>> {
    let run_id = format!("build-audit-{attempt_id}");
    let mut found: Option<ArtifactRef> = None;
    for reference in store.list_artifacts(effort)? {
        if reference.kind != ArtifactKind::Audit {
            continue;
        }
        if store.load_envelope(effort, &reference)?.run_id != run_id {
            continue;
        }
        ensure!(
            found
                .as_ref()
                .is_none_or(|existing| existing.artifact_id == reference.artifact_id),
            "more than one Audit publication claims attempt {attempt_id}"
        );
        found = Some(reference);
    }
    Ok(found)
}

/// True when the retained publication of one Build attempt is a completed Audit
/// that derived BLOCKED.  It is the evidence a new-evidence resolution answers.
fn retained_blocked_audit(store: &Store, effort: &Effort, attempt: &CurrentAction) -> Result<bool> {
    let Some(reference) = published_audit_for_attempt(store, effort, &attempt.id)? else {
        return Ok(false);
    };
    let (_, report): (_, orchestrate_contracts::AuditReport) =
        store.load_json(effort, &reference, "audit.json")?;
    Ok(report.verdict == Verdict::Blocked)
}

/// Apply one resolution record to Build state.  Every input comes from the
/// immutable record, so re-applying an interrupted resolution is idempotent.
fn apply_resolution(
    store: &Store,
    effort: &Effort,
    project: &Project,
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
    if state.applied_resolution_id.as_deref() == Some(record.resolution_id.as_str()) {
        // The transition is already effective; only its journal record can still
        // be missing, and appending it again is a no-op.
        return journal_resolution(store, effort, build_dir, record);
    }
    ensure!(
        state
            .stop
            .as_ref()
            .is_some_and(|stop| stop.action.id == record.binding.action.id),
        "resolution {} does not belong to the current stopped action",
        record.resolution_id
    );
    // A pending transition is authorized for the repository state the operator
    // resolved, and nothing is applied until that still holds.
    revalidate_resolution_binding(project, state, &record.binding)?;
    if let Some(overlay) = &record.config_overlay {
        ensure!(
            overlay.resolution_id == record.resolution_id
                && overlay.first_action_id == transition.action.id
                && digest_bytes(overlay.config_text.as_bytes()) == overlay.config_digest,
            "config overlay does not belong to resolution {}",
            record.resolution_id
        );
        let previous_config = config_before_overlay(build_dir, overlay.version)?;
        let next_config = parse_config(&overlay.config_text)?;
        if previous_config.worker.adapter != next_config.worker.adapter {
            state.sessions.worker = None;
        }
        if previous_config.reviewer.adapter != next_config.reviewer.adapter {
            state.sessions.reviewer = None;
        }
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
    journal_resolution(store, effort, build_dir, record)
}

/// The immutable journal record of one applied resolution.  It is written after
/// the state save, so a crash in between leaves a missing event that re-application
/// appends exactly once.
fn journal_resolution(
    store: &Store,
    effort: &Effort,
    build_dir: &Path,
    record: &ResolutionRecord,
) -> Result<()> {
    let transition = record
        .transition
        .as_ref()
        .context("applied resolution carries no transition")?;
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

/// Re-check the repository bindings a resolution recorded.  Resolution is
/// authorized for the exact stopped repository state the operator saw, so a
/// checkout that moved in the meantime is rejected as stale instead of applied.
fn revalidate_resolution_binding(
    project: &Project,
    state: &BuildState,
    binding: &ResolutionBinding,
) -> Result<()> {
    if state.frozen.reconciled != binding.reconciled
        || state.frozen.plan_digest != binding.plan_digest
        || state.frozen.detailed_plan_digest != binding.detailed_plan_digest
        || state.frozen.config_digest != binding.config_digest
    {
        return Err(stale_binding(
            "this resolution was recorded for different frozen Build inputs; nothing was applied and the stop stands, so record a new resolution against the current Build",
        ));
    }
    // The frozen input files themselves were already compared with these digests
    // before any transition, so only the checkout the operator resolved remains.
    let head = git(&project.canonical_locator, ["rev-parse", "HEAD"])?;
    if head != binding.head_commit {
        return Err(stale_binding(format!(
            "the checkout moved from {} to {head} after this resolution was recorded; nothing was applied and any recorded partial work is retained, so record a new resolution against the current repository state",
            binding.head_commit
        )));
    }
    let status = checkout_status(project)?;
    if status != binding.checkout_status {
        return Err(stale_binding(format!(
            "the checkout changed after this resolution was recorded (recorded {:?}, now {:?}); nothing was applied and any partial work is retained, so record a new resolution against the current repository state",
            binding.checkout_status, status
        )));
    }
    Ok(())
}

/// Complete a resolution that recorded its transition but died before its
/// effects were fully durable, so an interrupted record-before-state operation
/// takes effect exactly once.  A record whose bindings no longer hold is
/// rejected and journaled: the stop stands, and the state is left untouched.
fn apply_pending_resolution(
    store: &Store,
    effort: &Effort,
    project: &Project,
    build_dir: &Path,
    state_path: &Path,
    state: &mut BuildState,
) -> Result<()> {
    if let Some(stop) = state.stop.clone() {
        for record in resolutions_for_stop(build_dir, &stop.action.id)? {
            match apply_resolution(
                store, effort, project, build_dir, state_path, state, &record,
            ) {
                Ok(()) => return Ok(()),
                Err(error) => match error.downcast::<StaleBinding>() {
                    Ok(stale) => {
                        journal_rejected_resolution(store, effort, &record, &stale.detail)?
                    }
                    Err(error) => return Err(error),
                },
            }
        }
    }
    if let Some(record) = applied_resolution_record(build_dir, state)? {
        return journal_resolution(store, effort, build_dir, &record);
    }
    Ok(())
}

/// Record, once, that a recorded resolution could not be applied because the
/// repository no longer matches its binding.  Nothing about execution changes.
fn journal_rejected_resolution(
    store: &Store,
    effort: &Effort,
    record: &ResolutionRecord,
    detail: &str,
) -> Result<()> {
    append_journal_once(
        store,
        effort,
        "build_resolution_replay_rejected",
        &record.resolution_id,
        serde_json::json!({
            "resolution": record.resolution_id,
            "record": resolution_path(&store.phase_dir(effort, "build")?, &record.resolution_id),
            "stopped_action": record.binding.action.id,
            "detail": detail,
        }),
    )
}

/// The record of the one resolution already applied to this Build, if the state
/// names one that is still on disk.
fn applied_resolution_record(
    build_dir: &Path,
    state: &BuildState,
) -> Result<Option<ResolutionRecord>> {
    let Some(applied) = state.applied_resolution_id.as_deref() else {
        return Ok(None);
    };
    let path = resolution_path(build_dir, applied);
    if !path.is_file() {
        return Ok(None);
    }
    let record: ResolutionRecord = read_json(&path)?;
    ensure!(
        record.resolution_id == applied,
        "resolution record {} is inconsistent",
        path.display()
    );
    Ok(Some(record))
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

/// Every resolved record bound to one stopped action, oldest first.  A record
/// rejected as stale stays on disk, so a stop may hold more than one candidate.
fn resolutions_for_stop(build_dir: &Path, stopped_action: &str) -> Result<Vec<ResolutionRecord>> {
    Ok(resolution_records(build_dir)?
        .into_iter()
        .filter(|record| {
            record.status == ResolutionStatus::Resolved
                && record.binding.action.id == stopped_action
        })
        .collect())
}

fn resolution_path(build_dir: &Path, resolution_id: &str) -> PathBuf {
    build_dir
        .join(RESOLUTIONS_DIR)
        .join(format!("{resolution_id}.json"))
}

fn authority_amendment_path(build_dir: &Path, amendment_id: &str) -> PathBuf {
    build_dir
        .join(AUTHORITY_AMENDMENTS_DIR)
        .join(format!("{amendment_id}.json"))
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
        let selected = if path.is_absolute() {
            path.clone()
        } else {
            std::env::current_dir()?.join(path)
        };
        let stable_path = fs::canonicalize(&selected)
            .with_context(|| format!("cannot resolve resolution evidence {}", path.display()))?;
        let bytes = fs::read(&stable_path)
            .with_context(|| format!("cannot read resolution evidence {}", path.display()))?;
        evidence.push(ResolutionEvidence {
            path: stable_path.to_string_lossy().into_owned(),
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
        ActionKind::OnceOver => orchestrate_guides::ONCE_OVER,
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
        ActionKind::OnceOver,
        ActionKind::FinalAudit,
    ] {
        materialize_role_guide(build_dir, &kind)?;
    }
    write_bytes_sync(
        &build_dir.join("instructions").join(EVIDENCE_OUTPUT_GUIDE),
        orchestrate_guides::EVIDENCE_OUTPUT.as_bytes(),
    )?;
    Ok(())
}

/// The durable-output contract a role reads when its permission prevents it
/// from writing its evidence files, kept as one file rather than restated in
/// every role guide.
fn output_contract_path(build_dir: &Path) -> PathBuf {
    build_dir.join("instructions").join(EVIDENCE_OUTPUT_GUIDE)
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
pub(crate) fn read_json<T: for<'a> Deserialize<'a>>(path: &Path) -> Result<T> {
    decode(&fs::read(path).with_context(|| format!("cannot read {}", path.display()))?)
}
fn digest_file(path: &Path) -> Result<String> {
    Ok(digest_bytes(&fs::read(path)?))
}
pub(crate) fn now_ms() -> u128 {
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
                // Expected stale-lock diagnostics stay quiet: the probe reports
                // its result here, and the reclaim is labelled instead of
                // letting a bare `kill` message through.
                match pid {
                    Some(pid) => eprintln!(
                        "[build] reclaiming stale controller lock at {}; recorded controller pid {pid} is not running",
                        path.display()
                    ),
                    None => eprintln!(
                        "[build] reclaiming stale controller lock at {}; the recorded controller identity is unreadable",
                        path.display()
                    ),
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
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

struct CommandAdapter;
impl HostAdapter for CommandAdapter {
    fn launches_configured_executable(&self) -> bool {
        true
    }

    fn invoke(
        &self,
        request: &Invocation,
        observer: &mut dyn InvocationObserver,
    ) -> Result<InvocationResult> {
        let mut command = provider_command(request)?;
        let executable = command.get_program().to_string_lossy().into_owned();
        let log = request.action.join("transport.jsonl");
        let stderr_log = request.action.join("provider-stderr.log");
        let mut child = match command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
        {
            Ok(child) => child,
            Err(error) => {
                return Ok(InvocationResult {
                    completion: InvocationCompletion::SpawnFailed {
                        detail: format!(
                            "cannot launch {executable} with the {} adapter for role {}: {error}",
                            request.adapter, request.role
                        ),
                    },
                });
            }
        };
        // Provider stderr is drained on its own thread so a chatty provider
        // cannot fill the pipe while stdout is being read here.
        let stderr_forwarder = child.stderr.take().map(|stderr| {
            let adapter = request.adapter.clone();
            let role = request.role.clone();
            std::thread::spawn(move || {
                forward_provider_stderr(&mut BufReader::new(stderr), &stderr_log, &adapter, &role)
            })
        });
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
        // Raw local process facts for the invocation record; they are transport
        // evidence and never a verdict.
        write_bytes_sync(
            &request.action.join("provider-exit.json"),
            &serde_json::to_vec_pretty(&json!({
                "code": status.code(),
                "success": status.success(),
                "detail": status.to_string(),
            }))?,
        )?;
        if let Some(forwarder) = stderr_forwarder {
            // The forwarder owns no controller state; a join failure is reported
            // as the missing evidence it is.
            let _ = forwarder.join();
        }
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

/// What one executable is, resolved once per controller process: its size, the
/// digest of its bytes when they are readable, and the version it reports.  A
/// local `--version` call and a full read of a large binary are both bounded but
/// not free, so per-dispatch records reuse the same observation instead of
/// repeating it for every action.
#[derive(Clone, Debug)]
struct ExecutableFacts {
    bytes: u64,
    sha256: Option<String>,
    version: Option<String>,
}

fn executable_facts(path: &Path) -> ExecutableFacts {
    static FACTS: OnceLock<Mutex<BTreeMap<PathBuf, ExecutableFacts>>> = OnceLock::new();
    let facts = FACTS.get_or_init(|| Mutex::new(BTreeMap::new()));
    let mut facts = facts
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    facts
        .entry(path.to_path_buf())
        .or_insert_with(|| {
            let Ok(metadata) = fs::metadata(path) else {
                return ExecutableFacts {
                    bytes: 0,
                    sha256: None,
                    version: None,
                };
            };
            let sha256 = if metadata.len() <= 512 * 1024 * 1024 {
                fs::read(path).ok().map(|bytes| digest_bytes(&bytes))
            } else {
                None
            };
            ExecutableFacts {
                bytes: metadata.len(),
                sha256,
                version: preflight::executable_version(path),
            }
        })
        .clone()
}

/// One passive invocation record per dispatch.  It describes the launch, the
/// exact instruction, the requested settings and their provenance, timing and
/// process outcome; it never records environment contents, credentials or
/// provider configuration, and it interprets nothing about the provider.
///
/// The facts this dispatch observed and the facts the provider later reported
/// stay separate: the session this invocation asked for is recorded under
/// `session.requested` even after the provider's stream reported its own
/// session.  A second write completes the same record, preserving the dispatch
/// facts of the first instead of restating them from completion-time state.
fn write_invocation_record(
    invocation: &Invocation,
    action: &CurrentAction,
    config_version: u32,
    observed_session: Option<&str>,
    started: InvocationTiming,
    completion: Option<(&InvocationCompletion, &str)>,
) -> Result<()> {
    let action_dir = invocation.action.as_path();
    let config = &invocation.config;
    let program = preflight::program_for(&config.adapter)?;
    let executable = preflight::resolve_executable(program);
    let facts = executable
        .as_deref()
        .map(executable_facts)
        .unwrap_or(ExecutableFacts {
            bytes: 0,
            sha256: None,
            version: None,
        });
    let instruction = action_dir.join("instruction.md");
    let (instruction_digest, instruction_bytes) = fs::read(&instruction)
        .map(|bytes| (Some(digest_bytes(&bytes)), bytes.len()))
        .unwrap_or((None, 0));
    let exit = fs::read(action_dir.join("provider-exit.json"))
        .ok()
        .and_then(|bytes| decode::<serde_json::Value>(&bytes).ok());
    let previous = fs::read(action_dir.join("invocation.json"))
        .ok()
        .and_then(|bytes| decode::<serde_json::Value>(&bytes).ok());
    // The exact argv this dispatch launched, with only the prompt elided.
    let mut command_form = std::iter::once(program.to_owned())
        .chain(provider_arguments(invocation)?)
        .collect::<Vec<_>>();
    if let Some(last) = command_form.last_mut() {
        *last = "<prompt elided>".to_owned();
    }
    let requested_session = &invocation.session_id;
    let observed_model =
        preflight::observed_provider_model(&config.adapter, &action_dir.join("transport.jsonl"));
    let mut record = json!({
        "schema_version": 2,
        "action_id": action.id,
        "action": {
            "id": action.id,
            "kind": action_kind_name(&action.kind),
            "scope": action.scope,
            "target_commit": action.target_commit,
        },
        "role": invocation.role,
        "adapter": config.adapter,
        "config_version": config_version,
        "mode": invocation_mode(action, requested_session),
        "build_action_continuation": action.handoff.is_some(),
        "session": {
            "requested": requested_session,
            "resumed": requested_session.is_some(),
            "observed": observed_session,
            "note": "requested is the session this dispatch asked the provider to resume, or null for a fresh invocation; observed is the session the provider's own stream reported, and the two are different facts. A fresh invocation given a new session by the provider is still recorded as fresh.",
        },
        "requested": attribute_provenance(config),
        "mapped": {
            "arguments": attribute_arguments_for(&invocation.role, config)?,
            // The controller injects no provider environment for any adapter, so
            // this dispatch runs against the backend the launching environment
            // and the provider's own configuration select.
            "environment": json!({}),
            "note": "mapped arguments are the execution-edge settings this dispatch passes; the controller adds no environment override of its own, and credential values are never recorded",
        },
        // Requested configuration and provider-observed values are different
        // facts. Keep an observed model label only when the transport reports
        // one; effort and permission remain unknown for these CLI adapters.
        "observed": {
            "model": observed_model,
            "reasoning_effort": serde_json::Value::Null,
            "session": observed_session,
            "note": "model is the provider-reported transport label when available; the transport does not report effective reasoning effort or permission, so those stay unknown. Requested values above are configuration, not observation",
        },
        "executable": {
            "program": program,
            "path": executable.as_ref().map(|path| path.to_string_lossy().into_owned()),
            "bytes": facts.bytes,
            "sha256": facts.sha256,
            "version": facts.version,
            "note": "the digest is the local binary's bytes when they could be read; it is not a claim about the provider service",
        },
        "command": {
            "form": serde_json::Value::Array(command_form.into_iter().map(serde_json::Value::from).collect()),
            "working_directory": invocation.cwd,
            "directory_grants": [invocation.build_dir],
            "prompt_sha256": digest_bytes(invocation.prompt.as_bytes()),
            "prompt_bytes": invocation.prompt.len(),
            "note": "the exact argv this dispatch launched, generated from the same construction the launch used; only the prompt is elided",
        },
        "instruction": {
            "path": instruction,
            "sha256": instruction_digest,
            "bytes": instruction_bytes,
            "note": "the exact instruction bytes supplied to this invocation, retained even when a later CLI regenerates the role guides",
        },
        "outputs": {
            "transport": action_dir.join("transport.jsonl"),
            "provider_stderr": action_dir.join("provider-stderr.log"),
            "result": action_dir.join("result.json"),
            "report": action_dir.join("report.md"),
        },
        // Present from the first write so a reader never has to tell an absent
        // field from an unknown value; a running dispatch has no exit yet, and
        // an action interrupted before completion keeps that fact explicit.
        "exit": serde_json::Value::Null,
        "privacy": "no environment values, credentials or provider-configuration content are recorded in this file",
    });
    record["timing"] = merged_timing(previous.as_ref(), &started);
    if let Some((completion, detail)) = completion {
        record["completion"] = json!(match completion {
            InvocationCompletion::Completed => "completed",
            InvocationCompletion::SpawnFailed { .. } => "spawn_failed",
            InvocationCompletion::FailedBeforeAcceptance { .. } => "failed_before_acceptance",
            InvocationCompletion::AcceptedButIncomplete { .. } => "accepted_but_incomplete",
        });
        record["completion_detail"] = json!(detail);
        record["exit"] = exit.unwrap_or(serde_json::Value::Null);
    }
    write_bytes_sync(
        &action_dir.join("invocation.json"),
        &serde_json::to_vec_pretty(&record)?,
    )
}

/// The dispatch, completion and elapsed facts of one invocation record.  A fact
/// this process did not observe — the dispatch times of an action another
/// process launched — is taken from the record that observed it rather than
/// restated from completion-time state or invented.
fn merged_timing(
    previous: Option<&serde_json::Value>,
    started: &InvocationTiming,
) -> serde_json::Value {
    let carried = |field: &str| {
        previous.and_then(|record| record.pointer(&format!("/timing/{field}")).cloned())
    };
    let prepared = started
        .prepared_at_ms
        .map(|value| json!(value))
        .or_else(|| carried("prepared_at_ms"))
        .unwrap_or(serde_json::Value::Null);
    let dispatched = started
        .dispatched_at_ms
        .map(|value| json!(value))
        .or_else(|| carried("dispatched_at_ms"))
        .unwrap_or(serde_json::Value::Null);
    let elapsed_observed = started.monotonic_elapsed_ms.is_some();
    json!({
        "prepared_at_ms": prepared,
        "dispatched_at_ms": dispatched,
        "ended_at_ms": started.ended_at_ms.map(|value| json!(value)).unwrap_or(serde_json::Value::Null),
        // The elapsed duration comes from a monotonic clock, so a wall-clock
        // adjustment cannot make it negative or absurd.
        "elapsed_ms": started.monotonic_elapsed_ms.map(|value| json!(value)).unwrap_or(serde_json::Value::Null),
        "elapsed_observed": elapsed_observed,
        "note": "elapsed time is dispatch-to-completion from a monotonic clock; preparing an action earlier does not add model time, and a completion recorded by a later process reports its elapsed duration as unknown",
    })
}

/// fresh, resumed or replacement, decided from the session this dispatch
/// actually requested rather than from the session the provider later reported.
fn invocation_mode(action: &CurrentAction, requested_session: &Option<String>) -> &'static str {
    match action
        .handoff
        .as_deref()
        .map(|handoff| handoff.kind.as_str())
    {
        Some("replacement") => "replacement",
        _ if requested_session.is_some() => "resumed",
        _ => "fresh",
    }
}

fn completion_detail(completion: &InvocationCompletion) -> &str {
    match completion {
        InvocationCompletion::Completed => "the provider process completed",
        InvocationCompletion::SpawnFailed { detail }
        | InvocationCompletion::FailedBeforeAcceptance { detail }
        | InvocationCompletion::AcceptedButIncomplete { detail } => detail,
    }
}

#[derive(Clone, Copy)]
struct InvocationTiming {
    /// None when this process did not prepare or dispatch the action; the
    /// record keeps the value written by the process that did.
    prepared_at_ms: Option<u128>,
    dispatched_at_ms: Option<u128>,
    ended_at_ms: Option<u128>,
    /// Measured with `Instant`, so it stays monotonic across clock changes.
    /// None when this process did not observe the dispatch.
    monotonic_elapsed_ms: Option<u128>,
}

/// Provenance for one published artifact, attributed to the exact role
/// configuration that produced it.  An unset model or effort stays unknown
/// rather than becoming a frozen effective value.
fn role_provenance(host: &str, guide: &str, config: &RoleConfig) -> Provenance {
    let arguments = attribute_arguments_for("provenance", config).unwrap_or_default();
    let selected_model = config.model.clone().or_else(|| {
        arguments
            .windows(2)
            .find(|pair| pair[0] == "--model")
            .map(|pair| pair[1].clone())
    });
    let native_effort = config.reasoning_effort.as_ref().map(|effort| {
        match (config.adapter.as_str(), effort.as_str()) {
            ("claude", "low") => "low",
            ("claude", "medium") => "high",
            ("claude", "high") => "max",
            _ => effort.as_str(),
        }
        .to_owned()
    });
    Provenance {
        host: host.into(),
        provider: Some(config.adapter.clone()),
        model: selected_model,
        model_effort: native_effort,
        guide_digest: digest_bytes(guide.as_bytes()),
        independence: Independence::InputExcluded,
    }
}

/// Retain one action's raw provider stderr byte-for-byte, then forward the same
/// bytes to the controller's stderr with role and adapter attribution.  Provider
/// stderr is transport evidence: an error line in it never becomes an
/// engineering verdict, and nothing here routes the Build.
fn forward_provider_stderr(
    stderr: &mut impl BufRead,
    log_path: &Path,
    adapter: &str,
    role: &str,
) -> Result<()> {
    let mut raw = OpenOptions::new()
        .create(true)
        .append(true)
        .open(log_path)
        .with_context(|| format!("cannot retain provider stderr at {}", log_path.display()))?;
    let mut buffer = Vec::new();
    loop {
        buffer.clear();
        if stderr.read_until(b'\n', &mut buffer)? == 0 {
            break;
        }
        raw.write_all(&buffer)?;
        raw.sync_data()?;
        let text = String::from_utf8_lossy(&buffer);
        for line in text.lines() {
            eprintln!("[provider stderr · {adapter} · {role}] {line}");
        }
    }
    Ok(())
}

/// The exact provider-native argv one invocation is launched with, ending in the
/// action prompt.  Dispatch and the invocation record both call this, so the
/// recorded prompt-elided command is the command that actually runs, including
/// the subcommand, resume identity, working directory and directory grants.
pub(crate) fn provider_arguments(request: &Invocation) -> Result<Vec<String>> {
    let mut args = match request.adapter.as_str() {
        "codex" => {
            let mut args = vec![
                "exec".to_owned(),
                "--json".into(),
                "-C".into(),
                request.cwd.to_string_lossy().into_owned(),
                "--add-dir".into(),
                request.build_dir.to_string_lossy().into_owned(),
            ];
            if let Some(session) = &request.session_id {
                args.push("resume".into());
                args.push(session.clone());
            }
            args
        }
        "claude" => {
            let mut args = vec![
                "-p".to_owned(),
                "--verbose".into(),
                "--output-format".into(),
                "stream-json".into(),
                "--add-dir".into(),
                request.build_dir.to_string_lossy().into_owned(),
            ];
            if let Some(session) = &request.session_id {
                args.push("--resume".into());
                args.push(session.clone());
            }
            args
        }
        "cursor" => {
            let mut args = vec![
                "-p".to_owned(),
                "--output-format".into(),
                "stream-json".into(),
            ];
            if let Some(session) = &request.session_id {
                args.push("--resume".into());
                args.push(session.clone());
            }
            args
        }
        other => bail!("unsupported adapter {other}"),
    };
    args.extend(attribute_arguments_for(&request.role, &request.config)?);
    if request.adapter == "claude" {
        // Claude's `--add-dir` accepts a variadic argument list, so without a
        // separator it consumes the action prompt as another directory.
        args.push("--".into());
    }
    args.push(request.prompt.clone());
    Ok(args)
}

/// The provider process one role is launched as.  The controller adds no
/// provider environment of its own: each adapter inherits the launching
/// environment, so a role runs against exactly the backend the host and the
/// provider's own configuration already select.  A Claude role therefore uses
/// whatever backend that host's Claude Code is configured for — including an
/// Anthropic-compatible gateway such as the DeepSeek endpoint — and the
/// controller neither redirects it nor records its credential.
fn provider_command(request: &Invocation) -> Result<Command> {
    let program = preflight::program_for(&request.adapter)?;
    let mut command = Command::new(program);
    command.args(provider_arguments(request)?);
    // Codex takes its working directory as `-C <dir>`; the other adapters take
    // it as the child process's own working directory.
    if request.adapter != "codex" {
        command.current_dir(&request.cwd);
    }
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

    /// A frozen plan may write its requirement list as a sentence.  A trailing
    /// period is punctuation, ids named inside a clause keep the plan's own
    /// wording, and an identifier that is written but does not exist still
    /// reaches the check.
    #[test]
    fn prose_requirement_lists_resolve_to_the_identifiers_they_name() {
        let plan = "## Delivery phase G — Docs\n\n**Requirements:** R-037, R-042 plus cross-cutting acceptance for R-001–R-036.\n\n**Completion evidence:** reviewed.\n\n**Deliberate later-phase exclusions:** none.\n\nTasks:\n- G1 Document.";
        assert_eq!(
            phase_authority(plan, &["G1".into()]).unwrap().0,
            vec![
                "R-037".to_string(),
                "R-042 plus cross-cutting acceptance for R-001–R-036".to_string(),
            ]
        );
        assert_eq!(
            requirement_tokens("R-042 plus cross-cutting acceptance for R-001–R-036")
                .collect::<Vec<_>>(),
            vec!["R-042", "R-001", "R-036"]
        );
        assert_eq!(
            requirement_tokens("R-999").collect::<Vec<_>>(),
            vec!["R-999"]
        );
        for prose in [
            "plus cross-cutting acceptance for the docs",
            "the schema v3 wording",
            "",
        ] {
            assert!(
                requirement_tokens(prose).next().is_none(),
                "{prose} was read as an identifier"
            );
        }
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
                    model_strength: None,
                    config_schema_version: 2,
                    model: Some("model".into()),
                    reasoning_effort: None,
                    permission: None,
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
                    model_strength: None,
                    config_schema_version: 2,
                    model: None,
                    reasoning_effort: None,
                    permission: None,
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

    /// R-019: fresh and resumed dispatches carry exactly the configured
    /// attributes, and an unsupported attribute fails instead of being dropped.
    #[test]
    fn fresh_and_resumed_dispatches_map_the_same_attributes_exactly() {
        let root = temp("mapping");
        let config = RoleConfig {
            adapter: "codex".into(),
            model_strength: None,
            config_schema_version: 2,
            model: Some("gpt-5-codex".into()),
            reasoning_effort: Some("high".into()),
            permission: Some("full_access".into()),
        };
        let command_for = |session: Option<&str>| {
            let request = Invocation {
                adapter: "codex".into(),
                role: "worker".into(),
                config: config.clone(),
                cwd: root.clone(),
                build_dir: root.clone(),
                action: root.clone(),
                session_id: session.map(str::to_owned),
                prompt: "do the work".into(),
            };
            provider_command(&request)
                .unwrap()
                .get_args()
                .map(|arg| arg.to_string_lossy().into_owned())
                .collect::<Vec<_>>()
        };
        for value in [
            "--model",
            "gpt-5-codex",
            "-c",
            "model_reasoning_effort=high",
            "--dangerously-bypass-approvals-and-sandbox",
        ] {
            assert!(
                command_for(None).contains(&value.to_owned()),
                "attribute {value} is missing from a fresh dispatch"
            );
            assert!(
                command_for(Some("session-1")).contains(&value.to_owned()),
                "attribute {value} is missing from a resumed dispatch"
            );
        }
        // The resume form keeps the session and still ends with the prompt.
        let resumed = command_for(Some("session-1"));
        assert_eq!(resumed[resumed.len() - 1], "do the work");
        assert!(
            resumed
                .windows(2)
                .any(|pair| pair == ["resume", "session-1"])
        );
        // An unsupported attribute is refused before any command exists.
        let unsupported = Invocation {
            adapter: "cursor".into(),
            role: "reviewer".into(),
            config: RoleConfig {
                adapter: "cursor".into(),
                model_strength: None,
                config_schema_version: 2,
                reasoning_effort: Some("high".into()),
                ..config.clone()
            },
            cwd: root.clone(),
            build_dir: root.clone(),
            action: root.clone(),
            session_id: None,
            prompt: "review".into(),
        };
        let error = provider_command(&unsupported).unwrap_err().to_string();
        assert!(error.contains("cursor") && error.contains("reasoning_effort"));
    }

    /// R-025: provider stderr is retained byte-for-byte and forwarded with
    /// attribution, which tests exercise through the same function dispatch uses.
    #[test]
    fn provider_stderr_is_retained_verbatim_and_forwarded_with_attribution() {
        let dir = temp("stderr");
        let log = dir.join("provider-stderr.log");
        let raw = b"ERROR: transient network failure\nno trailing newline";
        let mut reader = BufReader::new(std::io::Cursor::new(raw.to_vec()));
        forward_provider_stderr(&mut reader, &log, "codex", "worker").unwrap();
        assert_eq!(fs::read(&log).unwrap(), raw, "the raw log was rewritten");
    }

    #[test]
    fn opencode_is_rejected_until_its_cli_contract_is_verified() {
        assert!(
            validate_role(
                "worker",
                &RoleConfig {
                    adapter: "opencode".into(),
                    model_strength: None,
                    config_schema_version: 2,
                    model: None,
                    reasoning_effort: None,
                    permission: None,
                },
                2,
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
    /// Publish the Reconciled Discovery one test effort adopts.
    fn publish_reconciled(store: &Store, effort: &Effort, goal: &str) -> ArtifactRef {
        let reconciled = ReconciledDiscovery {
            reconciled_id: format!("r-{goal}"),
            context_id: effort.context.id.clone(),
            baseline_commit: effort.baseline_commit.clone(),
            baseline_tree: effort.baseline_tree.clone(),
            goal: goal.into(),
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
        store
            .publish_bundle(
                effort,
                "reconcile",
                ArtifactKind::ReconciledDiscovery,
                "ready".into(),
                "IMPLEMENTATION_READY".into(),
                vec![],
                build_provenance("test", orchestrate_guides::BUILD),
                files,
            )
            .unwrap()
    }

    fn publish_finalized_discovery_with_nested_graph(
        store: &Store,
        effort: &Effort,
    ) -> ArtifactRef {
        let (run_id, workspace) = orchestrate_discovery::prepare(
            store,
            effort,
            build_provenance("test", orchestrate_guides::DISCOVERY),
        )
        .unwrap();
        fs::write(
            workspace.join("technical-spec.md"),
            "A tested discovery spec.\n",
        )
        .unwrap();
        fs::create_dir_all(workspace.join("graph")).unwrap();
        fs::write(
            workspace.join("graph/F-1.md"),
            "---\nid: F-1\nkind: finding\nstatus: accepted\nsources:\n  - src/lib.rs:1\nverification: inspection\n---\n# Finding\nA checked discovery fact.\n",
        )
        .unwrap();
        orchestrate_discovery::finalize(store, effort, &run_id).unwrap()
    }

    /// Write one effort's frozen Build files, optionally declaring a predecessor.
    fn prepare_build_files(
        store: &Store,
        effort: &Effort,
        reconciled_ref: &ArtifactRef,
        predecessor: Option<PlanPredecessor>,
    ) {
        prepare_build_files_with_config(
            store,
            effort,
            reconciled_ref,
            predecessor,
            "schema_version = 2\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"cursor\"\n",
        );
    }

    fn prepare_build_files_with_config(
        store: &Store,
        effort: &Effort,
        reconciled_ref: &ArtifactRef,
        predecessor: Option<PlanPredecessor>,
        config_toml: &str,
    ) {
        let build = store.phase_dir(effort, "build").unwrap();
        fs::write(build.join("implementation-plan.md"), DETAILED_PLAN).unwrap();
        fs::write(build.join("config.toml"), config_toml).unwrap();
        let plan = BuildPlan {
            schema_version: 2,
            reconciled: reconciled_ref.clone(),
            detailed_plan: "implementation-plan.md".into(),
            predecessor,
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
    }

    fn fresh_repo(name: &str) -> PathBuf {
        let repo = temp(name);
        git_ok(&repo, &["init"]);
        git_ok(&repo, &["config", "user.email", "test@example.com"]);
        git_ok(&repo, &["config", "user.name", "Test"]);
        fs::write(repo.join("source.txt"), "baseline\n").unwrap();
        git_ok(&repo, &["add", "."]);
        git_ok(&repo, &["commit", "-m", "baseline"]);
        repo
    }

    fn prepared() -> (Store, Effort, PathBuf, ArtifactRef, ArtifactRef) {
        let root = temp("store");
        let repo = fresh_repo("repo");
        let store = Store::open(&root).unwrap();
        let effort = store
            .init_effort(&repo, "build", RequestKind::Freeform, "work".into(), vec![])
            .unwrap();
        let reconciled_ref = publish_reconciled(&store, &effort, "test");
        prepare_build_files(&store, &effort, &reconciled_ref, None);
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
        let action: serde_json::Value = read_json(&invocation.action.join("action.json")).unwrap();
        let kind: ActionKind = serde_json::from_value(action["kind"].clone()).unwrap();
        let expected = role_for(&config.config, &kind)
            .1
            .expect("this role is configured in the fixture")
            .adapter
            .clone();
        assert_eq!(
            invocation.adapter, expected,
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
        /// A valid blocked Audit transport/receipt that exits before writing an
        /// assessment, as the controller contract permits.
        BlockedAuditReceiptOnly,
        BlockedAuditThenNewEvidence,
        /// A completed BLOCKED Audit whose single Unblock diagnosis finds a
        /// genuine external requirement instead of a remedy.
        BlockedAuditExternalEvidence,
        /// Advisory once-over blocked outcome.
        OnceOverBlocked,
        /// Advisory once-over wrote an invalid receipt.
        OnceOverInvalidReceipt,
        /// Advisory once-over could not be launched at all.
        OnceOverSpawnFailure,
        /// Advisory once-over wrote no receipt at all.
        OnceOverMissingReceipt,
        /// Advisory once-over wrote a receipt for a different action.
        OnceOverStaleReceipt,
        /// A review asks for changes, the correction worker blocks, and the one
        /// Unblock diagnosis finds a remedy for the same action.
        CorrectionBlockedThenRemedy,
        /// The controller itself is interrupted while the advisory once-over is
        /// pending, leaving a durable stop on that exact action.
        OnceOverControllerInterrupted,
    }

    struct ScenarioHost {
        scenario: Scenario,
        calls: Mutex<Vec<String>>,
        adapters: Mutex<Vec<String>>,
        /// Every session the provider reported, in dispatch order.
        sessions: Mutex<Vec<String>>,
        /// Report a session unique to each dispatch instead of one reused label,
        /// so a test can tell the requested identity apart from the observed one.
        distinct_sessions: bool,
    }

    impl ScenarioHost {
        fn new(scenario: Scenario) -> Self {
            Self {
                scenario,
                calls: Mutex::new(Vec::new()),
                adapters: Mutex::new(Vec::new()),
                sessions: Mutex::new(Vec::new()),
                distinct_sessions: false,
            }
        }

        fn reporting_distinct_sessions(scenario: Scenario) -> Self {
            Self {
                distinct_sessions: true,
                ..Self::new(scenario)
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
            if kind == "once_over" && self.scenario == Scenario::OnceOverControllerInterrupted {
                // The controller is interrupted while the advisory action is
                // pending and never dispatched.
                bail!("fixture interruption while the once-over action was pending");
            }
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
            if matches!(kind, "unblock" | "final_audit" | "once_over") {
                assert!(invocation.session_id.is_none());
            }
            if kind == "once_over" {
                let target = action["target_commit"].as_str().unwrap();
                assert!(invocation.cwd.ends_with("once-over"));
                assert_eq!(git(&invocation.cwd, ["rev-parse", "HEAD"]).unwrap(), target);
                assert!(
                    action["delivery_reviews"].as_array().is_some(),
                    "the once-over lost the delivery reviews"
                );
                assert!(action["build_start_commit"].as_str().is_some());
                assert!(action["once_over"]["advisory"].is_string());
            }
            if kind == "unblock" {
                let interrupted = &action["interrupted_action"];
                assert!(interrupted["kind"].is_string());
                assert_eq!(interrupted["scope"], action["scope"]);
                assert!(action["interrupted_report"].as_str().is_some());
                assert!(action["interrupted_result"].as_str().is_some());
                assert!(action.get("prior_feedback").is_some());
                // R-003: the diagnosis receives the same trigger and the durable
                // facts the stop captured, not a re-derived summary.
                let stop = &action["stop_record"];
                for field in ["trigger", "process_completion", "receipt_validation"] {
                    assert_eq!(action[field], stop[field], "unblock lost stop {field}");
                }
                assert_eq!(action["recovery_remaining"], stop["recovery_remaining"]);
                assert_eq!(
                    action["unresolved_requirement_ids"],
                    stop["unresolved_requirement_ids"]
                );
                assert_eq!(action["current_audit"], stop["audit"]);
                if stop["trigger"] == "audit_blocked" {
                    // The completed BLOCKED assessment is supplied as the exact
                    // published Audit, its assessment path and the unresolved ids.
                    let audit = action["current_audit"]["artifact_id"].as_str().unwrap();
                    assert!(audit.starts_with("audit-"));
                    let assessment = action["assessment"].as_str().unwrap();
                    assert!(Path::new(assessment).is_file(), "missing {assessment}");
                    assert!(
                        !action["unresolved_requirement_ids"]
                            .as_array()
                            .unwrap()
                            .is_empty()
                    );
                }
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
                    2 => assert_eq!(
                        invocation.session_id,
                        self.sessions.lock().unwrap().first().cloned(),
                        "the continuation did not resume the session the provider reported"
                    ),
                    3 => assert!(invocation.session_id.is_none()),
                    _ => {}
                }
            }
            observer.accepted()?;
            if !(self.scenario == Scenario::Uncertain && kind == "work" && count == 1) {
                let session = {
                    let mut sessions = self.sessions.lock().unwrap();
                    let label = if self.distinct_sessions {
                        format!("{}-provider-session-{}", invocation.role, sessions.len())
                    } else {
                        format!("{}-session", invocation.role)
                    };
                    sessions.push(label.clone());
                    label
                };
                observer.session_id(&session)?;
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
                            && self.scoped_count("work", "D2") == 1
                        || self.scenario == Scenario::CorrectionBlockedThenRemedy
                            && scope == "D1"
                            && self.scoped_count("work", "D1") == 2;
                    if blocked {
                        serde_json::json!({"action_id": action["action_id"], "scope": scope, "outcome": "blocked"})
                    } else {
                        self.complete_work(invocation, kind, scope)?
                    }
                }
                "review" => {
                    let outcome = if self.scenario == Scenario::Corrections && count == 1
                        || self.scenario == Scenario::RepeatedCorrections && count <= 2
                        || self.scenario == Scenario::CorrectionBlockedThenRemedy && count == 1
                    {
                        "changes_required"
                    } else {
                        "pass"
                    };
                    fs::write(
                        invocation.action.join("report.md"),
                        format!(
                            "review of {scope} at {}: {outcome}\n\n1. Re-check the conditional command chain: the later commands were not executed.\n",
                            action["target_commit"]
                        ),
                    )?;
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
                        Scenario::ExternalRequirement
                            | Scenario::SecondPhaseExternalRequirement
                            | Scenario::BlockedAuditExternalEvidence
                            | Scenario::BlockedAuditReceiptOnly
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
                "once_over" => {
                    if self.scenario == Scenario::OnceOverSpawnFailure {
                        return Ok(InvocationResult {
                            completion: InvocationCompletion::SpawnFailed {
                                detail: "cannot launch claude with the claude adapter for role once_over: No such file or directory (os error 2)".into(),
                            },
                        });
                    }
                    if self.scenario == Scenario::OnceOverMissingReceipt {
                        fs::write(
                            invocation.action.join("report.md"),
                            "advisory report without any receipt\n",
                        )?;
                        return Ok(InvocationResult {
                            completion: InvocationCompletion::Completed,
                        });
                    }
                    if self.scenario == Scenario::OnceOverStaleReceipt {
                        fs::write(
                            invocation.action.join("report.md"),
                            "advisory report with a foreign receipt\n",
                        )?;
                        write_bytes_sync(
                            &invocation.action.join("result.json"),
                            &serde_json::to_vec(&serde_json::json!({
                                "action_id": "act-somewhere-else",
                                "scope": "final",
                                "outcome": "advisory_complete"
                            }))?,
                        )?;
                        return Ok(InvocationResult {
                            completion: InvocationCompletion::Completed,
                        });
                    }
                    let outcome = match self.scenario {
                        Scenario::OnceOverBlocked => "blocked",
                        Scenario::OnceOverInvalidReceipt => "nonsense",
                        _ => "advisory_complete",
                    };
                    fs::write(
                        invocation.action.join("report.md"),
                        format!("advisory: whole-implementation review outcome {outcome}\n"),
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
                        Scenario::BlockedAuditThenNewEvidence if count <= 2 => {
                            CoverageState::Unknown
                        }
                        Scenario::BlockedAuditExternalEvidence if count == 1 => {
                            CoverageState::Unknown
                        }
                        _ if corrections_still_required => CoverageState::Fail,
                        _ => CoverageState::Pass,
                    };
                    if self.scenario != Scenario::BlockedAuditReceiptOnly {
                        self.write_assessment(invocation, &action, coverage)?;
                    }
                    serde_json::json!({
                        "action_id": action["action_id"],
                        "scope": scope,
                        "outcome": if self.scenario == Scenario::BlockedAuditReceiptOnly {
                            "blocked"
                        } else {
                            "complete"
                        }
                    })
                }
                other => bail!("unexpected scenario action {other}"),
            };
            // A completed provider leaves its transport behind; the exporter
            // treats a completed action's transport as required evidence, so
            // this fixture writes the same fact a real adapter would.
            fs::write(
                invocation.action.join("transport.jsonl"),
                format!("{{\"fixture\":\"{kind}\",\"scope\":\"{scope}\"}}\n"),
            )?;
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
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::Uncertain);
        let result = run_with_adapter(
            &store,
            BuildRequest {
                effort: None,
                project: repo.clone(),
            },
            &host,
        )
        .unwrap();
        assert!(matches!(result, BuildResult::Blocked { .. }));
        assert_eq!(host.call_count("work"), 1);
        // R-032: an action whose provider never reported a usable ending keeps
        // the record of what was launched, with the unobserved facts explicit.
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let uncertain = read_state(&store, &effort).action.id.clone();
        let record: serde_json::Value = read_json(
            &build_dir
                .join("artifacts")
                .join(&uncertain)
                .join("invocation.json"),
        )
        .unwrap();
        assert_eq!(record["action"]["id"].as_str().unwrap(), uncertain);
        assert_eq!(record["mode"], "fresh");
        assert!(record["timing"]["dispatched_at_ms"].is_u64());
        assert!(record["exit"].is_null());
        assert_eq!(record["completion"], "accepted_but_incomplete");
        assert!(record["session"]["requested"].is_null());
        // The same action is never resent on a restart.
        let restart = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(restart, BuildResult::Blocked { .. }));
        assert_eq!(host.call_count("work"), 1);
    }

    /// A host that dies with the controller while the provider is running, so a
    /// dispatch can be interrupted at the exact point a killed process would be.
    struct PanickingHost;

    impl HostAdapter for PanickingHost {
        fn invoke(
            &self,
            _request: &Invocation,
            observer: &mut dyn InvocationObserver,
        ) -> Result<InvocationResult> {
            observer.accepted()?;
            panic!("fixture interruption while the provider was running")
        }
    }

    /// R-032: a dispatch interrupted in flight leaves the record of what was
    /// launched, with no invented completion or exit.
    #[test]
    fn an_interrupted_dispatch_keeps_its_launch_record_without_inventing_an_end() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let interrupted = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            run_with_adapter(&store, request(&repo), &PanickingHost)
        }));
        assert!(
            interrupted.is_err(),
            "the fixture host did not interrupt the run"
        );
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let state = read_state(&store, &effort);
        assert!(matches!(state.action.dispatch, DispatchState::Running));
        let record: serde_json::Value = read_json(
            &build_dir
                .join("artifacts")
                .join(&state.action.id)
                .join("invocation.json"),
        )
        .unwrap();
        assert_eq!(record["action"]["id"].as_str().unwrap(), state.action.id);
        assert!(record["timing"]["dispatched_at_ms"].is_u64());
        assert!(record["timing"]["ended_at_ms"].is_null());
        assert!(record["exit"].is_null());
        assert!(
            record.get("completion").is_none(),
            "an interrupted dispatch claimed a completion: {record:#?}"
        );
        assert!(
            record["command"]["form"]
                .as_array()
                .unwrap()
                .iter()
                .any(|part| part == "--add-dir")
        );
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
        store.phase_dir(effort, "build").unwrap().join("state.json")
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
        MissingReceipt,
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
                    if self.mode == Tamper::MissingReceipt {
                        fs::write(
                            invocation.action.join("transport.jsonl"),
                            b"{\"fixture\":\"completed without receipt\"}\n",
                        )?;
                        return Ok(InvocationResult {
                            completion: InvocationCompletion::Completed,
                        });
                    }
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
            fs::write(
                invocation.action.join("transport.jsonl"),
                format!("{{\"fixture\":\"{kind}\"}}\n"),
            )?;
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

    /// R-029/R-030: a transport-completed action whose recorded stop says the
    /// receipt was never written exports the stopped state successfully, while
    /// preserving an invalid receipt when the controller records that it saw
    /// and rejected it.
    #[test]
    fn export_describes_receipt_absence_and_validation_from_the_recorded_stop() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = TamperHost::new(Tamper::MissingReceipt);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let state = read_state(&store, &effort);
        let stopped_action = state.stop.as_ref().unwrap().action.id.clone();
        assert_eq!(
            state.stop.as_ref().unwrap().receipt_validation,
            "receipt missing"
        );
        let action_dir = store
            .phase_dir(&effort, "build")
            .unwrap()
            .join("artifacts")
            .join(&stopped_action);
        assert!(action_dir.join("transport.completed").is_file());
        assert!(!action_dir.join("result.json").exists());
        let out = temp("export-recorded-missing-receipt");
        let outcome = export::export(&store, &effort, &out.join("stopped.zip")).unwrap();
        assert!(
            outcome.complete,
            "recorded missing receipt prevented complete stopped-state export: {outcome:#?}"
        );
        let member = outcome
            .members
            .iter()
            .find(|member| member.name == format!("build/artifacts/{stopped_action}/result.json"))
            .unwrap();
        assert_eq!(member.class, export::MemberClass::Absent);
        assert!(!member.required);
        assert!(
            member
                .detail
                .as_deref()
                .unwrap()
                .contains("receipt missing")
        );

        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = TamperHost::new(Tamper::Malformed);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let state = read_state(&store, &effort);
        let stopped_action = state.stop.as_ref().unwrap().action.id.clone();
        assert_eq!(
            state.stop.as_ref().unwrap().receipt_validation,
            "receipt present; its validation failed"
        );
        let out = temp("export-recorded-invalid-receipt");
        let outcome = export::export(&store, &effort, &out.join("invalid.zip")).unwrap();
        assert!(
            outcome.complete,
            "the preserved invalid receipt was misclassified as consumed evidence: {outcome:#?}"
        );
        let member = outcome
            .members
            .iter()
            .find(|member| member.name == format!("build/artifacts/{stopped_action}/result.json"))
            .unwrap();
        assert_eq!(member.class, export::MemberClass::Copied);
        assert!(
            member
                .detail
                .as_deref()
                .unwrap()
                .contains("receipt present; its validation failed")
        );
        assert!(
            member
                .detail
                .as_deref()
                .unwrap()
                .contains("were not consumed")
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
        /// The provider could not be reached; another attempt can differ.
        Transient,
        ExitFailure,
        IncompleteReceipt,
        /// The provider executable can never be launched as configured.
        SpawnFailure,
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
            self.calls
                .lock()
                .unwrap()
                .push(format!("{kind}:{}", action["action_id"].as_str().unwrap()));
            assert!(
                self.calls.lock().unwrap().len() <= 64,
                "the controller stopped making progress"
            );
            match kind.as_str() {
                "work" => match self.mode {
                    FailureMode::Transient => {
                        return Ok(InvocationResult {
                            completion: InvocationCompletion::FailedBeforeAcceptance {
                                detail: "the provider could not be reached".into(),
                            },
                        });
                    }
                    FailureMode::SpawnFailure => {
                        return Ok(InvocationResult {
                            completion: InvocationCompletion::SpawnFailed {
                                detail: "cannot launch codex with the codex adapter for role worker: No such file or directory (os error 2)".into(),
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

    /// The exact interruption window a resolution leaves when it dies right
    /// after persisting its immutable record: the record and the still-stopped
    /// state exist, and no transition, config overlay or journal event does.
    fn record_pending_resolution(
        store: &Store,
        effort: &Effort,
        kind: ResolutionKind,
        note: &str,
        confirm_not_running: bool,
        config: Option<PathBuf>,
        evidence: Vec<PathBuf>,
    ) -> ResolutionRecord {
        let project = store.project_for(effort).unwrap();
        let build_dir = store.phase_dir(effort, "build").unwrap();
        let state = read_state(store, effort);
        let stop = state.stop.clone().expect("the Build should hold a stop");
        let request = ResolutionRequest {
            effort: effort.id.clone(),
            action: stop.action.id.clone(),
            kind,
            note: note.into(),
            evidence,
            confirm_not_running,
            config,
        };
        let evidence = read_evidence(&request.evidence).unwrap();
        let config_text = role_config_text(&request).unwrap();
        let binding = resolution_binding(&state, &project, &stop, confirm_not_running).unwrap();
        let record = prepared_resolution(
            store,
            effort,
            &build_dir,
            &state,
            &stop,
            &request,
            binding,
            evidence,
            config_text,
        )
        .unwrap();
        write_immutable(
            &resolution_path(&build_dir, &record.resolution_id),
            &orchestrate_contracts::encode(&record).unwrap(),
        )
        .unwrap();
        record
    }

    /// The window on the other side of the same interruption: the record, the
    /// config overlay and the state save happened, and the journal append did
    /// not.
    fn apply_recorded_resolution_state(store: &Store, effort: &Effort, record: &ResolutionRecord) {
        let build_dir = store.phase_dir(effort, "build").unwrap();
        if let Some(overlay) = &record.config_overlay {
            write_immutable(
                &config_history_path(&build_dir, overlay),
                &orchestrate_contracts::encode(overlay).unwrap(),
            )
            .unwrap();
        }
        let mut state = read_state(store, effort);
        let transition = record.transition.as_ref().unwrap();
        state.action = transition.action.clone();
        state.interrupted = None;
        state.stop = None;
        if transition.recovery_reset {
            state.recovery = Recovery::default();
        }
        state.applied_resolution_id = Some(record.resolution_id.clone());
        save_state(&build_state_path(store, effort), &state).unwrap();
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
            (
                FailureMode::Transient,
                StopTrigger::ProviderExecutionFailure,
            ),
            (
                FailureMode::ExitFailure,
                StopTrigger::ProviderExecutionFailure,
            ),
            (FailureMode::IncompleteReceipt, StopTrigger::IncompleteWork),
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
            assert_eq!(
                state.stop_history[0].trigger,
                StopTrigger::RecoveryExhausted
            );
            assert_eq!(state.stop_history[0].action_failure, Some(action_failure));
            assert!(stop.process_completion.contains("provider"));
        }
    }

    /// R-006: a deterministic OS spawn failure stops immediately with the role,
    /// adapter and executable named, and dispatches no continuation, replacement
    /// or Unblock diagnosis.
    #[test]
    fn deterministic_spawn_failure_stops_without_consuming_recovery() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = FailureHost::new(FailureMode::SpawnFailure);
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        let BuildResult::Blocked {
            trigger,
            stopped_action,
            stop_record,
            detail,
            ..
        } = result
        else {
            panic!("a spawn failure did not stop the Build")
        };
        assert_eq!(trigger.as_deref(), Some("spawn_failure"));
        assert_eq!(host.action_ids("work").len(), 1, "the launch was retried");
        assert_eq!(host.action_ids("unblock").len(), 0);
        assert_eq!(host.action_ids("review").len(), 0);
        let stop = stop_record.expect("the stop record travels with the result");
        assert_eq!(stop["trigger"], "spawn_failure");
        assert_eq!(stop["action_failure"], "spawn_failure");
        assert_eq!(
            stop["action"]["id"],
            serde_json::json!(stopped_action.clone().unwrap())
        );
        assert!(
            stop["recovery_remaining"]
                .as_array()
                .unwrap()
                .iter()
                .any(|entry| entry.as_str().unwrap().contains("continuation")),
            "a spawn failure must not consume the recovery budget: {stop}"
        );
        for fact in ["role worker", "codex adapter", "codex"] {
            assert!(
                detail.contains(fact),
                "the stop does not identify {fact}: {detail}"
            );
        }
        let state = read_state(&store, &effort);
        assert_eq!(
            state.stop.as_ref().unwrap().trigger,
            StopTrigger::SpawnFailure
        );
        assert_eq!(
            state.action.id,
            state.stop.as_ref().unwrap().action.id,
            "a spawn failure created a new action"
        );
        assert!(state.stop_history.iter().all(|record| {
            record
                .action_failure
                .as_ref()
                .is_none_or(|failure| *failure == StopTrigger::SpawnFailure)
        }));
    }

    /// R-018/R-019/R-020: roles route exactly, unsupported values fail clearly,
    /// and configured attributes appear in the exact provider arguments.
    #[test]
    fn role_attributes_route_exactly_and_unsupported_values_fail_clearly() {
        let base = |adapter: &str| RoleConfig {
            adapter: adapter.into(),
            model_strength: None,
            config_schema_version: 2,
            model: None,
            reasoning_effort: None,
            permission: None,
        };
        // Unset attributes send no overriding flag at all.
        assert!(attribute_arguments(&base("codex")).unwrap().is_empty());
        let worker = RoleConfig {
            model: Some("gpt-5-codex".into()),
            reasoning_effort: Some("high".into()),
            ..base("codex")
        };
        assert_eq!(
            attribute_arguments(&worker).unwrap(),
            vec![
                "--model".to_string(),
                "gpt-5-codex".into(),
                "-c".into(),
                "model_reasoning_effort=high".into(),
            ]
        );
        // An explicit full-access value is passed exactly and named as such.
        let privileged = RoleConfig {
            permission: Some("full_access".into()),
            ..base("codex")
        };
        assert_eq!(
            attribute_arguments(&privileged).unwrap(),
            vec!["--dangerously-bypass-approvals-and-sandbox".to_string()]
        );
        let claude_privileged = RoleConfig {
            permission: Some("full_access".into()),
            ..base("claude")
        };
        assert_eq!(
            attribute_arguments(&claude_privileged).unwrap(),
            vec!["--dangerously-skip-permissions".to_string()]
        );
        // Unsupported values fail with the role, attribute and adapter named.
        let cursor_effort = RoleConfig {
            reasoning_effort: Some("high".into()),
            ..base("cursor")
        };
        let error = validate_role("unblocker", &cursor_effort, 2)
            .unwrap_err()
            .to_string();
        for fact in ["unblocker", "reasoning_effort", "cursor"] {
            assert!(error.contains(fact), "{error} does not name {fact}");
        }
        let bad_effort = RoleConfig {
            reasoning_effort: Some("turbo".into()),
            ..base("codex")
        };
        assert!(
            validate_role("worker", &bad_effort, 2)
                .unwrap_err()
                .to_string()
                .contains("turbo")
        );
        let bad_permission = RoleConfig {
            permission: Some("workspace_write".into()),
            ..base("codex")
        };
        let error = validate_role("reviewer", &bad_permission, 2)
            .unwrap_err()
            .to_string();
        for fact in [
            "reviewer",
            "permission",
            "workspace_write",
            "schema_version 2",
        ] {
            assert!(error.contains(fact), "{error} does not name {fact}");
        }
        // Unknown keys in a role table fail instead of being dropped.
        assert!(
            parse_config(
                "schema_version = 2\n[worker]\nadapter = \"codex\"\ntemperature = 0.2\n[reviewer]\nadapter = \"codex\"\n"
            )
            .is_err()
        );
    }

    #[test]
    fn provider_neutral_v3_attributes_map_identically_on_fresh_and_resumed_calls() {
        let root = temp("neutral-mapping");
        let cases = [
            (
                "codex",
                "gpt-5.5",
                "read_only",
                vec![
                    "-c",
                    "model_reasoning_effort=medium",
                    "-c",
                    "sandbox_mode=\"read-only\"",
                    "-c",
                    "approval_policy=\"never\"",
                ],
            ),
            (
                "claude",
                "deepseek-flash",
                "workspace_write",
                vec![
                    "--effort",
                    "high",
                    "--permission-mode",
                    "acceptEdits",
                    "--settings",
                    r#"{"sandbox":{"enabled":true,"allowUnsandboxedCommands":false}}"#,
                ],
            ),
            (
                "cursor",
                "gpt-5.5-medium",
                "workspace_write",
                vec!["--force", "--sandbox", "enabled"],
            ),
        ];
        for (adapter, expected_model, permission, suffix) in cases {
            let config = RoleConfig {
                adapter: adapter.into(),
                model_strength: Some("standard".into()),
                config_schema_version: 3,
                model: None,
                reasoning_effort: Some("medium".into()),
                permission: Some(permission.into()),
            };
            let mapped = attribute_arguments_for("worker", &config).unwrap();
            assert!(
                mapped
                    .windows(2)
                    .any(|pair| pair == ["--model", expected_model])
            );
            for pair in suffix.chunks_exact(2) {
                assert!(
                    mapped.windows(2).any(|actual| actual == pair),
                    "{adapter}: {mapped:?} lacks {pair:?}"
                );
            }
            let args_for = |session_id: Option<&str>| {
                provider_arguments(&Invocation {
                    adapter: adapter.into(),
                    role: "worker".into(),
                    config: config.clone(),
                    cwd: root.clone(),
                    build_dir: root.clone(),
                    action: root.clone(),
                    session_id: session_id.map(str::to_owned),
                    prompt: "probe".into(),
                })
                .unwrap()
            };
            let fresh = args_for(None);
            let resumed = args_for(Some("same-session"));
            for argument in &mapped {
                assert!(
                    fresh.contains(argument),
                    "{adapter} fresh args omit {argument:?}: {fresh:?}"
                );
                assert!(
                    resumed.contains(argument),
                    "{adapter} resumed args omit {argument:?}: {resumed:?}"
                );
            }
            if let Some(session_index) = resumed.iter().position(|argument| argument == "--resume")
            {
                assert_eq!(
                    resumed.get(session_index + 1).map(String::as_str),
                    Some("same-session")
                );
            } else if adapter == "codex" {
                assert!(
                    resumed
                        .windows(2)
                        .any(|pair| pair == ["resume", "same-session"])
                );
            } else {
                panic!("{adapter} did not carry a resume id: {resumed:?}");
            }
        }
    }

    #[test]
    fn provider_reported_model_labels_are_separate_from_requested_values() {
        let root = temp("provider-reported-model");
        let claude_transport = root.join("claude-transport.jsonl");
        fs::write(
            &claude_transport,
            "{\"type\":\"assistant\",\"message\":{\"model\":\"deepseek-flash\"}}\n",
        )
        .unwrap();
        assert_eq!(
            preflight::observed_provider_model("claude", &claude_transport).as_deref(),
            Some("deepseek-flash")
        );

        let cursor_transport = root.join("cursor-transport.jsonl");
        fs::write(
            &cursor_transport,
            "{\"type\":\"system\",\"subtype\":\"init\",\"model\":\"GPT-5.5 272K Medium\"}\n",
        )
        .unwrap();
        assert_eq!(
            preflight::observed_provider_model("cursor", &cursor_transport).as_deref(),
            Some("GPT-5.5 272K Medium")
        );

        let codex_transport = root.join("codex-transport.jsonl");
        fs::write(&codex_transport, "{\"type\":\"thread.started\"}\n").unwrap();
        assert_eq!(
            preflight::observed_provider_model("codex", &codex_transport),
            None
        );
    }

    #[test]
    fn v3_unsupported_values_name_role_field_value_and_adapter() {
        let cursor_effort = RoleConfig {
            adapter: "cursor".into(),
            model_strength: None,
            config_schema_version: 3,
            model: None,
            reasoning_effort: Some("low".into()),
            permission: None,
        };
        let error = validate_role("reviewer", &cursor_effort, 3)
            .unwrap_err()
            .to_string();
        for fact in ["reviewer", "reasoning_effort", "low", "cursor"] {
            assert!(error.contains(fact), "{error} does not name {fact}");
        }
        let codex_workspace_write = RoleConfig {
            adapter: "codex".into(),
            model_strength: Some("standard".into()),
            config_schema_version: 3,
            model: None,
            reasoning_effort: Some("medium".into()),
            permission: Some("workspace_write".into()),
        };
        let error = validate_role("worker", &codex_workspace_write, 3)
            .unwrap_err()
            .to_string();
        for fact in [
            "worker",
            "permission",
            "workspace_write",
            "codex",
            "commit contract",
        ] {
            assert!(error.contains(fact), "{error} does not name {fact}");
        }
        let native_model = parse_config(
            "schema_version = 3\n[worker]\nadapter = \"codex\"\nmodel = \"gpt-5-codex\"\n[reviewer]\nadapter = \"codex\"\n",
        )
        .unwrap_err()
        .to_string();
        for fact in ["worker", "model", "gpt-5-codex", "codex", "model_strength"] {
            assert!(
                native_model.contains(fact),
                "{native_model} does not name {fact}"
            );
        }
    }

    /// R-018: an absent unblocker falls back to the reviewer; an absent once-over
    /// is never dispatched; a configured unblocker is used exactly.
    #[test]
    fn absent_roles_fall_back_and_configured_roles_route_exactly() {
        let config: BuildConfig = toml::from_str(
            "schema_version = 2\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"cursor\"\n",
        )
        .unwrap();
        assert_eq!(role_for(&config, &ActionKind::Work).0, "worker");
        assert_eq!(role_for(&config, &ActionKind::Review).0, "reviewer");
        assert_eq!(role_for(&config, &ActionKind::FinalAudit).0, "reviewer");
        assert_eq!(role_for(&config, &ActionKind::Unblock).0, "reviewer");
        assert_eq!(
            role_for(&config, &ActionKind::Unblock).1.unwrap().adapter,
            "cursor"
        );
        // An unconfigured once-over has no settings at all, so nothing can be
        // silently substituted for it.
        assert!(role_for(&config, &ActionKind::OnceOver).1.is_none());
        assert!(config.once_over.is_none());
        let configured: BuildConfig = toml::from_str(
            "schema_version = 2\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"codex\"\n[unblocker]\nadapter = \"claude\"\nmodel = \"sonnet\"\n[once_over]\nadapter = \"claude\"\n",
        )
        .unwrap();
        let (role, unblocker) = role_for(&configured, &ActionKind::Unblock);
        assert_eq!(role, "unblocker");
        let unblocker = unblocker.unwrap();
        assert_eq!(unblocker.adapter, "claude");
        assert_eq!(unblocker.model.as_deref(), Some("sonnet"));
        assert_eq!(role_for(&configured, &ActionKind::OnceOver).0, "once_over");
        assert_eq!(
            role_for(&configured, &ActionKind::OnceOver)
                .1
                .unwrap()
                .adapter,
            "claude"
        );
        // config-v2 files keep their exact meaning.
        assert!(parse_config(
            "schema_version = 2\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"codex\"\n"
        )
        .is_ok());
    }

    #[test]
    fn frozen_input_changes_record_a_stop_instead_of_a_bare_error() {
        for (name, suffix) in [
            ("plan.json", "\n"),
            ("config.toml", "\n# edited after execution began\n"),
            (
                "implementation-plan.md",
                "\n# edited after execution began\n",
            ),
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
            assert!(
                error.to_string().contains("cannot resolve"),
                "{name}: {error:#}"
            );
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
        assert_ne!(
            first.audit, stop.audit,
            "the second attempt reused the first"
        );
        assert_eq!(first.unresolved_requirement_ids, vec!["R-1".to_owned()]);

        // R-041: the formal truth table is unchanged — unknown coverage is BLOCKED.
        let (_, report): (_, orchestrate_contracts::AuditReport) = store
            .load_json(&effort, stop.audit.as_ref().unwrap(), "audit.json")
            .unwrap();
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
        assert_eq!(
            record.binding.reconciled,
            stopped_action_reconciled(&store, &effort)
        );
        assert_eq!(
            record.binding.plan_digest,
            read_state(&store, &effort).frozen.plan_digest
        );
        assert_eq!(
            record.binding.detailed_plan_digest,
            read_state(&store, &effort).frozen.detailed_plan_digest
        );
        assert_eq!(
            record.binding.config_digest,
            read_state(&store, &effort).frozen.config_digest
        );
        assert_eq!(
            record.binding.head_commit,
            git(&repo, ["rev-parse", "HEAD"]).unwrap()
        );
        assert!(
            record.binding.checkout_status.contains("partial-work.txt"),
            "dirty work was not surfaced: {}",
            record.binding.checkout_status
        );
        assert!(repo.join("partial-work.txt").exists());
        assert_eq!(
            record.transition.as_ref().unwrap().governed_action,
            continuation_action
        );
        assert!(record.evidence.is_empty());

        let state = read_state(&store, &effort);
        assert!(state.stop.is_none());
        assert_eq!(
            state.applied_resolution_id.as_deref(),
            Some(record.resolution_id.as_str())
        );
        assert_eq!(state.action.id, continuation_action);
        assert_eq!(state.action.scope, "D2");
        assert!(state.interrupted.is_none());
        // The bounded recovery budget restarts with the intervention, and that
        // reset is attributed to it in the journal.
        assert!(
            !state.recovery.continuation_used
                && !state.recovery.replacement_used
                && !state.recovery.unblock_used,
            "the intervention did not reset the recovery budget"
        );
        let resolved_event = store
            .read_journal(&effort)
            .unwrap()
            .into_iter()
            .find(|event| event.event == "build_resolved")
            .unwrap();
        assert_eq!(
            resolved_event.details["recovery_reset"],
            serde_json::json!(true)
        );

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
        let ResolutionOutcome::Resolved {
            resolution_id,
            continuation_action,
            ..
        } = &first
        else {
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
        let ResolutionOutcome::Resolved {
            resolution_id: repeat_id,
            continuation_action: repeat_action,
            ..
        } = &repeat
        else {
            panic!("repeat was refused")
        };
        assert_eq!(repeat_id, resolution_id);
        assert_eq!(repeat_action, continuation_action);
        assert_eq!(resolution_files(&store, &effort).len(), 1);
        assert_eq!(journal_count(&store, &effort, "build_resolved"), 1);
        assert_eq!(
            fs::read(build_state_path(&store, &effort)).unwrap(),
            after_first
        );

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
        assert!(error.to_string().contains("already resolved"), "{error:#}");
        assert_eq!(resolution_files(&store, &effort).len(), 1);
        assert_eq!(
            fs::read(build_state_path(&store, &effort)).unwrap(),
            after_first
        );

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
        assert!(
            error.to_string().contains("is not the stopped action"),
            "{error:#}"
        );
        assert_eq!(fs::read(build_state_path(&store, &effort)).unwrap(), before);
        assert!(resolution_files(&store, &effort).is_empty());

        // A live controller lock refuses the resolution outright.
        let lock = store
            .project_dir(&store.project_for(&effort).unwrap())
            .join(".build-controller.lock");
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
        assert!(
            error.to_string().contains("another Build driver"),
            "{error:#}"
        );
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
        assert!(
            error.to_string().contains("no Build state exists"),
            "{error:#}"
        );

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
        assert_eq!(
            stopped.process_completion,
            "accepted; provider completion is uncertain"
        );
        let before = fs::read(build_state_path(&store, &effort)).unwrap();
        let uncertain_id = stopped.action.id.clone();
        let action_dir = store
            .phase_dir(&effort, "build")
            .unwrap()
            .join("artifacts")
            .join(&uncertain_id);
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
        assert!(
            error.to_string().contains("confirm the provider"),
            "{error:#}"
        );
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
        assert!(
            action_dir.is_dir(),
            "the uncertain action's artifacts were discarded"
        );
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
        assert_eq!(
            journal_count(&store, &effort, "build_resolution_refused"),
            1
        );
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
        fs::write(
            evidence.join("deployment.txt"),
            "live deployment verified\n",
        )
        .unwrap();
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
        assert_eq!(
            continuation_action,
            record.transition.as_ref().unwrap().governed_action
        );
        assert_eq!(record.evidence.len(), 1);
        assert_eq!(
            record.evidence[0].path,
            evidence_file.canonicalize().unwrap().to_string_lossy()
        );
        assert_eq!(
            record.evidence[0].sha256,
            digest_bytes(&fs::read(&evidence_file).unwrap())
        );

        let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
        let BuildResult::Completed(done) = resumed else {
            panic!("the new evidence did not produce a Build verdict")
        };
        assert_eq!(host.call_count("final_audit"), 3);
        assert_eq!(artifact_count(&store, &effort, ArtifactKind::Audit), 3);
        // The new attempt is at the same implementation and the old Audit survives.
        assert_eq!(done.implementation, implementation);
        assert_ne!(done.audit, blocked_audit);
        let (_, old_report): (_, orchestrate_contracts::AuditReport) = store
            .load_json(&effort, &blocked_audit, "audit.json")
            .unwrap();
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
        assert_eq!(
            new_packet["resolution_evidence"][0]["sha256"],
            record.evidence[0].sha256
        );
        assert_eq!(
            new_packet["current_audit"],
            serde_json::json!(blocked_audit)
        );
    }

    #[test]
    fn relative_evidence_child_entry() {
        let Ok(mode) = std::env::var("ORCHESTRATE_B3_CHILD_MODE") else {
            return;
        };
        match mode.as_str() {
            "accept" => {
                let evidence = read_evidence(&[PathBuf::from("verification.md")]).unwrap();
                fs::write(
                    std::env::var_os("ORCHESTRATE_B3_RECORD").unwrap(),
                    orchestrate_contracts::encode(&evidence).unwrap(),
                )
                .unwrap();
            }
            "consume" => {
                let evidence: Vec<ResolutionEvidence> = read_json(Path::new(
                    &std::env::var_os("ORCHESTRATE_B3_RECORD").unwrap(),
                ))
                .unwrap();
                let record = &evidence[0];
                let bytes = fs::read(&record.path).unwrap();
                assert_eq!(bytes, b"accepted from directory A\n");
                assert_eq!(digest_bytes(&bytes), record.sha256);
            }
            "resolve" => {
                let store = Store::open(Path::new(
                    &std::env::var_os("ORCHESTRATE_B3_STORE").unwrap(),
                ))
                .unwrap();
                let effort = store
                    .load_effort(&std::env::var("ORCHESTRATE_B3_EFFORT").unwrap())
                    .unwrap();
                let state = read_state(&store, &effort);
                let outcome = resolve(
                    &store,
                    ResolutionRequest {
                        effort: effort.id,
                        action: state.stop.unwrap().action.id,
                        kind: ResolutionKind::NewVerificationEvidence,
                        note: "relative verification accepted from its selected directory".into(),
                        evidence: vec![PathBuf::from("verification.md")],
                        confirm_not_running: false,
                        config: None,
                    },
                )
                .unwrap();
                let ResolutionOutcome::Resolved { resolution, .. } = outcome else {
                    panic!("the selected verification evidence was refused");
                };
                fs::write(
                    std::env::var_os("ORCHESTRATE_B3_RESULT").unwrap(),
                    resolution.to_string_lossy().as_bytes(),
                )
                .unwrap();
            }
            "amend" => {
                let store = Store::open(Path::new(
                    &std::env::var_os("ORCHESTRATE_B3_STORE").unwrap(),
                ))
                .unwrap();
                let effort_id = std::env::var("ORCHESTRATE_B3_EFFORT").unwrap();
                let effort = store.load_effort(&effort_id).unwrap();
                let state = read_state(&store, &effort);
                let outcome = amend_authority(
                    &store,
                    AuthorityAmendmentRequest {
                        effort: effort.id,
                        action: state.action.id,
                        note: "relative authority evidence accepted from its selected directory"
                            .into(),
                        evidence: vec![PathBuf::from("verification.md")],
                        confirm_not_running: true,
                    },
                )
                .unwrap();
                fs::write(
                    std::env::var_os("ORCHESTRATE_B3_RESULT").unwrap(),
                    outcome.record.to_string_lossy().as_bytes(),
                )
                .unwrap();
            }
            "export" => {
                let store = Store::open(Path::new(
                    &std::env::var_os("ORCHESTRATE_B3_STORE").unwrap(),
                ))
                .unwrap();
                let effort = store
                    .load_effort(&std::env::var("ORCHESTRATE_B3_EFFORT").unwrap())
                    .unwrap();
                let archive_path =
                    PathBuf::from(std::env::var_os("ORCHESTRATE_B3_ARCHIVE").unwrap());
                let outcome = export::export(&store, &effort, &archive_path).unwrap();
                fs::write(
                    std::env::var_os("ORCHESTRATE_B3_RESULT").unwrap(),
                    serde_json::to_vec(&outcome).unwrap(),
                )
                .unwrap();
            }
            other => panic!("unexpected child mode {other}"),
        }
    }

    #[test]
    fn relative_operator_evidence_keeps_its_origin_and_digest_across_directories() {
        let first = temp("evidence-origin-a");
        let second = temp("evidence-origin-b");
        let record_path = first.join("accepted-record.json");
        fs::write(
            first.join("verification.md"),
            b"accepted from directory A\n",
        )
        .unwrap();
        fs::write(
            second.join("verification.md"),
            b"different bytes in directory B\n",
        )
        .unwrap();
        let executable = std::env::current_exe().unwrap();
        let child = |mode: &str, cwd: &Path| {
            Command::new(&executable)
                .args([
                    "--exact",
                    "tests::relative_evidence_child_entry",
                    "--nocapture",
                ])
                .current_dir(cwd)
                .env("ORCHESTRATE_B3_CHILD_MODE", mode)
                .env("ORCHESTRATE_B3_RECORD", &record_path)
                .output()
                .unwrap()
        };
        let accepted = child("accept", &first);
        assert!(
            accepted.status.success(),
            "acceptance child failed: {}",
            String::from_utf8_lossy(&accepted.stderr)
        );
        let evidence: Vec<ResolutionEvidence> = read_json(&record_path).unwrap();
        assert_eq!(evidence.len(), 1);
        assert_eq!(
            Path::new(&evidence[0].path),
            fs::canonicalize(first.join("verification.md")).unwrap()
        );
        assert_eq!(
            evidence[0].sha256,
            digest_bytes(b"accepted from directory A\n")
        );
        let repeated = read_evidence(&[first.join("./verification.md")]).unwrap();
        assert_eq!(
            repeated, evidence,
            "equivalent selections got different identities"
        );
        assert_eq!(
            resolution_id(
                "act-evidence-test",
                ResolutionKind::NewVerificationEvidence,
                "same accepted evidence",
                &repeated,
                None,
            ),
            resolution_id(
                "act-evidence-test",
                ResolutionKind::NewVerificationEvidence,
                "same accepted evidence",
                &evidence,
                None,
            )
        );

        let consumed = child("consume", &second);
        assert!(
            consumed.status.success(),
            "evidence was substituted from directory B: {}",
            String::from_utf8_lossy(&consumed.stderr)
        );
        fs::write(
            first.join("verification.md"),
            b"modified after acceptance\n",
        )
        .unwrap();
        let changed = child("consume", &second);
        assert!(
            !changed.status.success(),
            "modified accepted bytes passed the digest check"
        );
    }

    #[test]
    fn relative_evidence_stays_bound_through_both_public_acceptance_operations_and_export() {
        let executable = std::env::current_exe().unwrap();
        let run_child = |mode: &str,
                         cwd: &Path,
                         store: &Store,
                         effort: &Effort,
                         result_path: &Path,
                         archive_path: Option<&Path>| {
            let mut command = Command::new(&executable);
            command
                .args([
                    "--exact",
                    "tests::relative_evidence_child_entry",
                    "--nocapture",
                ])
                .current_dir(cwd)
                .env("ORCHESTRATE_B3_CHILD_MODE", mode)
                .env("ORCHESTRATE_B3_STORE", store.root())
                .env("ORCHESTRATE_B3_EFFORT", &effort.id)
                .env("ORCHESTRATE_B3_RESULT", result_path);
            if let Some(archive_path) = archive_path {
                command.env("ORCHESTRATE_B3_ARCHIVE", archive_path);
            }
            command.output().unwrap()
        };
        let a = temp("public-evidence-origin-a");
        let b = temp("public-evidence-origin-b");
        fs::write(a.join("verification.md"), b"accepted from directory A\n").unwrap();
        fs::write(
            b.join("verification.md"),
            b"different bytes in directory B\n",
        )
        .unwrap();

        let (store, effort, repo, _, _) = prepared();
        let host = ScenarioHost::new(Scenario::BlockedAuditThenNewEvidence);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let result_path = a.join("resolution-record-path.txt");
        let accepted = run_child("resolve", &a, &store, &effort, &result_path, None);
        assert!(
            accepted.status.success(),
            "resolve child failed: {}",
            String::from_utf8_lossy(&accepted.stderr)
        );
        let resolution_path = PathBuf::from(fs::read_to_string(&result_path).unwrap());
        let resolution: ResolutionRecord = read_json(&resolution_path).unwrap();
        assert_eq!(
            resolution.evidence[0].path,
            fs::canonicalize(a.join("verification.md"))
                .unwrap()
                .to_string_lossy()
        );
        assert_eq!(
            resolution.evidence[0].sha256,
            digest_bytes(b"accepted from directory A\n")
        );
        let archive_path = temp("public-resolution-export").join("resolution.zip");
        let export_result = b.join("resolution-export.json");
        let exported = run_child(
            "export",
            &b,
            &store,
            &effort,
            &export_result,
            Some(&archive_path),
        );
        assert!(
            exported.status.success(),
            "export child failed: {}",
            String::from_utf8_lossy(&exported.stderr)
        );
        let export_outcome: serde_json::Value = read_json(&export_result).unwrap();
        assert_eq!(export_outcome["complete"], true);
        let members = archive::members_by_name(&archive_path).unwrap();
        let evidence_name = format!(
            "resolutions/evidence/{}-0-verification.md",
            resolution.resolution_id
        );
        let evidence_member = members.get(&evidence_name).unwrap();
        let (bytes, digest) = archive::read_member(&archive_path, evidence_member).unwrap();
        assert_eq!(bytes, b"accepted from directory A\n");
        assert_eq!(digest, resolution.evidence[0].sha256);

        let (store, effort, _repo, _, _) = prepared();
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let plan = load_plan(&store, &effort, &build_dir).unwrap();
        let mut state = initialize_state(&store, &effort, &build_dir, &plan).unwrap();
        state.action.dispatch = DispatchState::Running;
        save_state(&build_dir.join("state.json"), &state).unwrap();
        let action_dir = build_dir.join("artifacts").join(&state.action.id);
        fs::create_dir_all(&action_dir).unwrap();
        write_bytes_sync(
            &action_dir.join("action.json"),
            &serde_json::to_vec_pretty(&json!({"action_id": state.action.id})).unwrap(),
        )
        .unwrap();
        let result_path = a.join("authority-record-path.txt");
        let amended = run_child("amend", &a, &store, &effort, &result_path, None);
        assert!(
            amended.status.success(),
            "amend child failed: {}",
            String::from_utf8_lossy(&amended.stderr)
        );
        let amendment_path = PathBuf::from(fs::read_to_string(&result_path).unwrap());
        let amendment: AuthorityAmendmentRecord = read_json(&amendment_path).unwrap();
        assert_eq!(
            amendment.evidence[0].path,
            fs::canonicalize(a.join("verification.md"))
                .unwrap()
                .to_string_lossy()
        );
        let archive_path = temp("public-amendment-export").join("amendment.zip");
        let export_result = b.join("amendment-export.json");
        let exported = run_child(
            "export",
            &b,
            &store,
            &effort,
            &export_result,
            Some(&archive_path),
        );
        assert!(
            exported.status.success(),
            "amendment export child failed: {}",
            String::from_utf8_lossy(&exported.stderr)
        );
        let export_outcome: serde_json::Value = read_json(&export_result).unwrap();
        assert_eq!(export_outcome["complete"], true);
        let members = archive::members_by_name(&archive_path).unwrap();
        let evidence_name = format!(
            "authority-amendments/evidence/{}-0-verification.md",
            amendment.amendment_id
        );
        let evidence_member = members.get(&evidence_name).unwrap();
        let (bytes, digest) = archive::read_member(&archive_path, evidence_member).unwrap();
        assert_eq!(bytes, b"accepted from directory A\n");
        assert_eq!(digest, amendment.evidence[0].sha256);
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
        assert!(error.to_string().contains("final-scope Audit"), "{error:#}");
        assert_eq!(fs::read(build_state_path(&store, &effort)).unwrap(), before);
        assert!(resolution_files(&store, &effort).is_empty());
    }

    #[test]
    fn a_recorded_but_unapplied_resolution_effectively_transitions_once() {
        // Window: the immutable record is durable and nothing else is — no
        // transition, no config overlay, no journal event.  Both entry points
        // must apply it once and dispatch the authorized continuation once.
        for entry in ["build", "resolution"] {
            let (store, effort, repo, _reconciled, _adoption) = prepared();
            let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
            assert!(matches!(
                run_with_adapter(&store, request(&repo), &host).unwrap(),
                BuildResult::Blocked { .. }
            ));
            let overlay = temp("overlay");
            fs::write(
                overlay.join("config.toml"),
                "schema_version = 2\n[worker]\nadapter = \"cursor\"\n[reviewer]\nadapter = \"codex\"\n",
            )
            .unwrap();
            let record = record_pending_resolution(
                &store,
                &effort,
                ResolutionKind::EnvironmentRepair,
                "the worker environment was repaired",
                false,
                Some(overlay.join("config.toml")),
                Vec::new(),
            );
            let continuation = record.transition.as_ref().unwrap().action.id.clone();
            assert_eq!(host.call_count("work"), 2, "{entry}");
            assert!(read_state(&store, &effort).stop.is_some(), "{entry}");
            assert_eq!(
                journal_count(&store, &effort, "build_resolved"),
                0,
                "{entry}"
            );

            if entry == "resolution" {
                let outcome = resolve_action(
                    &store,
                    &effort,
                    &record.binding.action.id,
                    ResolutionKind::EnvironmentRepair,
                    "the worker environment was repaired",
                    false,
                    Some(overlay.join("config.toml")),
                    Vec::new(),
                )
                .unwrap();
                assert!(
                    matches!(outcome, ResolutionOutcome::Resolved { .. }),
                    "{entry}"
                );
                // The retry observed the existing record instead of a second one,
                // and it completed the transition the earlier attempt never applied.
                assert_eq!(resolution_files(&store, &effort).len(), 1, "{entry}");
                let state = read_state(&store, &effort);
                assert_eq!(
                    state.applied_resolution_id.as_deref(),
                    Some(record.resolution_id.as_str()),
                    "{entry}"
                );
                assert!(state.stop.is_none(), "{entry}");
                assert_eq!(state.action.id, continuation, "{entry}");
                assert_eq!(
                    journal_count(&store, &effort, "build_resolved"),
                    1,
                    "{entry}"
                );
            }
            let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
            assert!(matches!(resumed, BuildResult::Completed(_)), "{entry}");
            assert_eq!(host.call_scopes("work"), vec!["D1", "D2", "D2"], "{entry}");
            assert_eq!(
                host.adapters("work"),
                vec!["codex", "codex", "cursor"],
                "{entry}"
            );

            let state = read_state(&store, &effort);
            assert_eq!(
                state.applied_resolution_id.as_deref(),
                Some(record.resolution_id.as_str()),
                "{entry}"
            );
            let resolved = store
                .read_journal(&effort)
                .unwrap()
                .into_iter()
                .filter(|event| event.event == "build_resolved")
                .collect::<Vec<_>>();
            assert_eq!(resolved.len(), 1, "{entry}");
            assert_eq!(
                resolved[0].details["continuation_action"],
                serde_json::json!(continuation),
                "{entry}"
            );
            // The overlay was published once, not once per application.
            let history = store
                .phase_dir(&effort, "build")
                .unwrap()
                .join(CONFIG_HISTORY_DIR);
            assert_eq!(fs::read_dir(history).unwrap().count(), 1, "{entry}");

            // A finished Build is terminal: replay is not dispatch eligibility.
            assert!(matches!(
                run_with_adapter(&store, request(&repo), &host).unwrap(),
                BuildResult::Completed(_)
            ));
            assert_eq!(host.call_count("work"), 3, "{entry}");
            assert_eq!(
                journal_count(&store, &effort, "build_resolved"),
                1,
                "{entry}"
            );
        }
    }

    #[test]
    fn a_resolution_whose_journal_append_was_lost_still_records_it_once() {
        // Window: the record, the overlay and the state save happened, and the
        // journal append did not.
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let record = record_pending_resolution(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "the worker environment was repaired",
            false,
            None,
            Vec::new(),
        );
        let continuation = record.transition.as_ref().unwrap().action.id.clone();
        apply_recorded_resolution_state(&store, &effort, &record);
        assert_eq!(journal_count(&store, &effort, "build_resolved"), 0);
        assert_eq!(
            read_state(&store, &effort).action.id,
            continuation,
            "the transition was not applied"
        );

        let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(resumed, BuildResult::Completed(_)));
        let resolved = store
            .read_journal(&effort)
            .unwrap()
            .into_iter()
            .filter(|event| event.event == "build_resolved")
            .collect::<Vec<_>>();
        assert_eq!(resolved.len(), 1);
        assert_eq!(
            resolved[0].details["continuation_action"],
            serde_json::json!(continuation)
        );
        assert_eq!(host.call_scopes("work"), vec!["D1", "D2", "D2"]);
        // A second start neither appends the event again nor dispatches again.
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        assert_eq!(journal_count(&store, &effort, "build_resolved"), 1);
        assert_eq!(host.call_count("work"), 3);
    }

    #[test]
    fn a_replayed_overlay_configuration_governs_its_recorded_first_action() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        assert_eq!(host.adapters("work"), vec!["codex", "codex"]);
        let stopped_state = read_state(&store, &effort);
        assert_eq!(
            stopped_state.sessions.worker.as_deref(),
            Some("worker-session")
        );
        assert_eq!(
            stopped_state.sessions.reviewer.as_deref(),
            Some("reviewer-session")
        );
        let overlay = temp("overlay");
        fs::write(
            overlay.join("config.toml"),
            "schema_version = 2\n[worker]\nadapter = \"cursor\"\n[reviewer]\nadapter = \"cursor\"\n",
        )
        .unwrap();

        // Only the resolution record exists: the overlay it recorded was never
        // published, and the transition never ran.
        let record = record_pending_resolution(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "the worker adapter moved to cursor",
            false,
            Some(overlay.join("config.toml")),
            Vec::new(),
        );
        let overlay_record = record.config_overlay.as_ref().unwrap();
        let continuation = record.transition.as_ref().unwrap().action.id.clone();
        assert_eq!(overlay_record.first_action_id, continuation);
        assert_eq!(overlay_record.version, 2);
        // The immutable overlay was published, but the state/session transition
        // was interrupted. Replay must compare against v1, not the now-effective
        // v2 config, and expire only the worker session whose adapter changed.
        write_immutable(
            &config_history_path(&build_dir, overlay_record),
            &orchestrate_contracts::encode(overlay_record).unwrap(),
        )
        .unwrap();
        assert!(build_dir.join(CONFIG_HISTORY_DIR).is_dir());
        assert_eq!(journal_count(&store, &effort, "build_resolved"), 0);

        let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(resumed, BuildResult::Completed(_)));
        // The replayed overlay governs exactly the action the record names, and
        // the earlier actions keep the settings they ran under.
        assert_eq!(host.adapters("work"), vec!["codex", "codex", "cursor"]);
        let continuation_packet: serde_json::Value = read_json(
            &build_dir
                .join("artifacts")
                .join(&continuation)
                .join("action.json"),
        )
        .unwrap();
        assert_eq!(continuation_packet["config_version"], 2);
        let continuation_invocation: serde_json::Value = read_json(
            &build_dir
                .join("artifacts")
                .join(&continuation)
                .join("invocation.json"),
        )
        .unwrap();
        assert_eq!(continuation_invocation["adapter"], "cursor");
        assert_eq!(
            continuation_invocation["session"]["requested"],
            serde_json::Value::Null
        );
        assert_eq!(continuation_invocation["session"]["resumed"], false);
        assert_eq!(continuation_invocation["mode"], "fresh");
        assert_eq!(continuation_invocation["build_action_continuation"], true);
        assert!(
            action_packets(&build_dir).iter().any(|(id, packet)| {
                packet["kind"] == "work"
                    && *id != continuation
                    && read_json::<serde_json::Value>(
                        &build_dir.join("artifacts").join(id).join("invocation.json"),
                    )
                    .is_ok_and(|record| record["session"]["observed"] == "worker-session")
            }),
            "the prior adapter's observed session was not retained in immutable invocation evidence"
        );
        assert!(
            action_packets(&build_dir).iter().any(|(id, packet)| {
                packet["kind"] == "review"
                    && read_json::<serde_json::Value>(
                        &build_dir.join("artifacts").join(id).join("invocation.json"),
                    )
                    .is_ok_and(|record| record["session"]["requested"] == "reviewer-session")
            }),
            "the unchanged reviewer role lost its saved session"
        );
        let applied_record: ResolutionRecord =
            read_json(&resolution_files(&store, &effort)[0]).unwrap();
        let mut applied_state = read_state(&store, &effort);
        let established_session = applied_state.sessions.worker.clone();
        assert!(established_session.is_some());
        apply_resolution(
            &store,
            &effort,
            &store.project_for(&effort).unwrap(),
            &build_dir,
            &build_dir.join("state.json"),
            &mut applied_state,
            &applied_record,
        )
        .unwrap();
        assert_eq!(applied_state.sessions.worker, established_session);
        assert_eq!(
            fs::read_dir(build_dir.join(CONFIG_HISTORY_DIR))
                .unwrap()
                .count(),
            1
        );
    }

    #[test]
    fn a_pending_resolution_is_rejected_when_the_repository_moved() {
        for change in ["head", "checkout"] {
            let (store, effort, repo, _reconciled, _adoption) = prepared();
            let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
            assert!(matches!(
                run_with_adapter(&store, request(&repo), &host).unwrap(),
                BuildResult::Blocked { .. }
            ));
            let record = record_pending_resolution(
                &store,
                &effort,
                ResolutionKind::ExistingAuthorityClarification,
                "the role already had authority for this phase",
                false,
                None,
                Vec::new(),
            );
            let before = fs::read(build_state_path(&store, &effort)).unwrap();
            match change {
                "head" => {
                    fs::write(repo.join("late-fix.txt"), "committed after resolution\n").unwrap();
                    git_ok(&repo, &["add", "."]);
                    git_ok(&repo, &["commit", "-m", "late fix"]);
                }
                _ => {
                    fs::write(repo.join("partial-work.txt"), "unrecorded partial work\n").unwrap();
                }
            }
            let calls = host.call_count("work");

            // Through the normal controller the stale record is rejected and
            // journaled: the stop stands and execution state is untouched.
            let blocked = run_with_adapter(&store, request(&repo), &host).unwrap();
            assert!(matches!(blocked, BuildResult::Blocked { .. }), "{change}");
            assert_eq!(
                fs::read(build_state_path(&store, &effort)).unwrap(),
                before,
                "{change}"
            );
            assert_eq!(host.call_count("work"), calls, "{change}");
            assert_eq!(
                journal_count(&store, &effort, "build_resolved"),
                0,
                "{change}"
            );
            assert_eq!(
                journal_count(&store, &effort, "build_resolution_replay_rejected"),
                1,
                "{change}"
            );
            // A second start neither applies the record nor journals it twice.
            assert!(matches!(
                run_with_adapter(&store, request(&repo), &host).unwrap(),
                BuildResult::Blocked { .. }
            ));
            assert_eq!(host.call_count("work"), calls, "{change}");
            assert_eq!(
                journal_count(&store, &effort, "build_resolution_replay_rejected"),
                1,
                "{change}"
            );

            // An identical retry reports the rejection instead of a transition
            // that never happened, and changes nothing.
            let error = resolve_action(
                &store,
                &effort,
                &record.binding.action.id,
                ResolutionKind::ExistingAuthorityClarification,
                "the role already had authority for this phase",
                false,
                None,
                Vec::new(),
            )
            .unwrap_err();
            assert!(
                error.to_string().contains("record a new resolution"),
                "{change}: {error:#}"
            );
            assert_eq!(
                fs::read(build_state_path(&store, &effort)).unwrap(),
                before,
                "{change}"
            );
            assert_eq!(resolution_files(&store, &effort).len(), 1, "{change}");

            // The recorded-but-rejected record does not close the stop: a fresh
            // submission against the current state applies and continues.
            let outcome = resolve_action(
                &store,
                &effort,
                &record.binding.action.id,
                ResolutionKind::ExistingAuthorityClarification,
                "the role already had authority for this phase, re-recorded",
                false,
                None,
                Vec::new(),
            )
            .unwrap();
            assert!(
                matches!(outcome, ResolutionOutcome::Resolved { .. }),
                "{change}"
            );
            assert_eq!(resolution_files(&store, &effort).len(), 2, "{change}");
            assert_eq!(
                journal_count(&store, &effort, "build_resolved"),
                1,
                "{change}"
            );
            let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
            assert!(matches!(resumed, BuildResult::Completed(_)), "{change}");
            assert_eq!(host.call_scopes("work"), vec!["D1", "D2", "D2"], "{change}");
            // The work the operator did is retained, never silently discarded.
            let kept = if change == "head" {
                "late-fix.txt"
            } else {
                "partial-work.txt"
            };
            assert!(
                repo.join(kept).exists(),
                "{change} discarded its partial work"
            );
        }
    }

    #[test]
    fn a_recorded_resolution_whose_binding_still_holds_applies() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        // Dirty work that was already there when the operator resolved is part
        // of the recorded binding, so it does not make the record stale.
        fs::write(repo.join("partial-work.txt"), "recorded partial work\n").unwrap();
        let record = record_pending_resolution(
            &store,
            &effort,
            ResolutionKind::ExistingAuthorityClarification,
            "the role already had authority for this phase",
            false,
            None,
            Vec::new(),
        );
        assert!(record.binding.checkout_status.contains("partial-work.txt"));
        assert_eq!(
            journal_count(&store, &effort, "build_resolution_replay_rejected"),
            0
        );

        let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(resumed, BuildResult::Completed(_)));
        assert_eq!(journal_count(&store, &effort, "build_resolved"), 1);
        assert_eq!(
            journal_count(&store, &effort, "build_resolution_replay_rejected"),
            0
        );
        assert_eq!(
            read_state(&store, &effort).applied_resolution_id.as_deref(),
            Some(record.resolution_id.as_str())
        );
        assert_eq!(host.call_scopes("work"), vec!["D1", "D2", "D2"]);
    }

    #[test]
    fn external_evidence_at_final_scope_resolves_with_new_evidence() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::BlockedAuditExternalEvidence);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        // One completed BLOCKED assessment and its one diagnosis, and no
        // automatic reassessment after the external-requirement stop.
        assert_eq!(host.call_count("final_audit"), 1);
        assert_eq!(host.call_count("unblock"), 1);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        assert_eq!(host.call_count("final_audit"), 1);
        assert_eq!(host.call_count("unblock"), 1);

        let stopped = stop_of(&store, &effort);
        assert_eq!(stopped.trigger, StopTrigger::ExternalRequirement);
        assert_eq!(stopped.unresolved_requirement_ids, vec!["R-1".to_owned()]);
        let attempt = stopped.interrupted.as_deref().unwrap();
        assert_eq!(action_kind_name(&attempt.kind), "final_audit");
        let attempt_id = attempt.id.clone();
        let blocked_audit = stopped.audit.clone().unwrap();
        assert!(stopped.assessment.is_some());
        let implementation = read_state(&store, &effort).implementation.clone().unwrap();

        let evidence = temp("external-evidence");
        fs::write(
            evidence.join("deployment.txt"),
            "live deployment verified\n",
        )
        .unwrap();
        let outcome = resolve_stop(
            &store,
            &effort,
            ResolutionKind::NewVerificationEvidence,
            "the missing live deployment evidence is now available",
            false,
            None,
            vec![evidence.join("deployment.txt")],
        )
        .unwrap();
        let ResolutionOutcome::Resolved {
            continuation_action,
            continuation_kind,
            ..
        } = outcome
        else {
            panic!("new evidence was refused for a genuine external-evidence stop")
        };
        assert_eq!(continuation_kind, "final_audit");
        assert_ne!(
            continuation_action, attempt_id,
            "the old attempt was reused"
        );

        let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
        let BuildResult::Completed(done) = resumed else {
            panic!("the new evidence did not produce a Build verdict")
        };
        assert_eq!(host.call_count("final_audit"), 2);
        assert_eq!(host.call_count("unblock"), 1);
        assert_eq!(artifact_count(&store, &effort, ArtifactKind::Audit), 2);
        // The new attempt is a distinct publication at the unchanged
        // implementation, and the BLOCKED attempt it answers survives.
        assert_eq!(done.implementation, implementation);
        assert_ne!(done.audit, blocked_audit);
        assert_eq!(
            store.load_envelope(&effort, &done.audit).unwrap().run_id,
            format!("build-audit-{continuation_action}")
        );
        assert_eq!(
            store.load_envelope(&effort, &blocked_audit).unwrap().run_id,
            format!("build-audit-{attempt_id}")
        );
        let (_, old_report): (_, orchestrate_contracts::AuditReport) = store
            .load_json(&effort, &blocked_audit, "audit.json")
            .unwrap();
        assert_eq!(old_report.verdict, Verdict::Blocked);
        assert_eq!(old_report.assessment.implementation, implementation);
        let (_, new_report): (_, orchestrate_contracts::AuditReport) =
            store.load_json(&effort, &done.audit, "audit.json").unwrap();
        assert_eq!(new_report.verdict, Verdict::Pass);
        assert_eq!(new_report.assessment.implementation, implementation);
    }

    #[test]
    fn successor_linkage_names_the_predecessor_and_the_new_contract() {
        let (store, effort, repo, reconciled_ref, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let state_path = build_state_path(&store, &effort);
        let before = fs::read(&state_path).unwrap();
        let head = git(&repo, ["rev-parse", "HEAD"]).unwrap();

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
            resolution_id,
            successor_guidance,
            ..
        } = outcome
        else {
            panic!("an authority amendment was accepted")
        };
        let record: ResolutionRecord = read_json(&resolution).unwrap();
        let linkage = record.refusal.as_ref().unwrap().successor.clone();
        // The refusal retains the predecessor contract, its commits and the
        // refused resolution, and it invents no successor contract.
        assert_eq!(linkage.resolution_id, resolution_id);
        assert_eq!(linkage.predecessor_effort_id, effort.id);
        assert_eq!(linkage.predecessor_reconciled, reconciled_ref);
        assert_eq!(
            linkage.predecessor_commits.discovery_baseline_commit,
            effort.baseline_commit
        );
        assert_eq!(
            linkage.predecessor_commits.build_start_commit,
            read_state(&store, &effort).frozen.build_start_commit
        );
        assert_eq!(linkage.predecessor_commits.head_commit, head);
        assert_eq!(linkage.successor_reconciled, None);
        assert!(successor_guidance.contains(&resolution_id));
        assert_eq!(fs::read(&state_path).unwrap(), before);

        // The predecessor stopped with real, committed delivery work in the
        // product repository; that work is what the successor must retain.
        let predecessor_work = git(&repo, ["show", "--name-only", "--format=%H", "HEAD"]).unwrap();
        assert!(
            predecessor_work.contains("work-1.txt"),
            "the predecessor made no committed progress to retain: {predecessor_work}"
        );

        // The successor effort declares the linkage in its own plan and records
        // the exact contract its own Discovery produced.  It runs in the same
        // product checkout, so the predecessor's commits and files are the
        // history it continues from rather than an unrelated repository.
        let successor = store
            .init_effort(
                &repo,
                "successor",
                RequestKind::Freeform,
                "work".into(),
                vec![],
            )
            .unwrap();
        let successor_reconciled = publish_reconciled(&store, &successor, "successor");
        prepare_build_files(
            &store,
            &successor,
            &successor_reconciled,
            Some(PlanPredecessor {
                effort_id: effort.id.clone(),
                reconciled: reconciled_ref.clone(),
                resolution_id: resolution_id.clone(),
            }),
        );
        // The successor's own decomposition, chosen by its planner, is neither
        // the predecessor's nor prescribed by the linkage.
        let successor_build = store.phase_dir(&successor, "build").unwrap();
        fs::write(
            successor_build.join("implementation-plan.md"),
            "## Delivery phase S1 — Successor phase\n\n**Requirements:** R-1\n\n**Completion evidence:** S1 is complete.\n\n**Deliberate later-phase exclusions:** none.\n\nTasks:\n- T1 Continue the implementation.\n",
        )
        .unwrap();
        write_bytes_sync(
            &successor_build.join("plan.json"),
            &orchestrate_contracts::encode(&BuildPlan {
                schema_version: 2,
                reconciled: successor_reconciled.clone(),
                detailed_plan: "implementation-plan.md".into(),
                predecessor: Some(PlanPredecessor {
                    effort_id: effort.id.clone(),
                    reconciled: reconciled_ref.clone(),
                    resolution_id: resolution_id.clone(),
                }),
                delivery_phases: vec![DeliveryPhase {
                    id: "S1".into(),
                    tasks: vec!["T1".into()],
                }],
            })
            .unwrap(),
        )
        .unwrap();
        let plan = load_plan(&store, &successor, &successor_build).unwrap();
        let successor_state =
            initialize_state(&store, &successor, &successor_build, &plan).unwrap();
        let recorded = successor_state.successor_of.clone().unwrap();
        assert_eq!(recorded.resolution_id, resolution_id);
        assert_eq!(recorded.predecessor_effort_id, effort.id);
        assert_eq!(recorded.predecessor_reconciled, reconciled_ref);
        assert_eq!(recorded.predecessor_commits.head_commit, head);
        assert_eq!(
            recorded.successor_reconciled,
            Some(successor_reconciled.clone())
        );
        assert_ne!(recorded.predecessor_reconciled, successor_reconciled);
        // The successor's own Build starts from the predecessor's retained work:
        // its frozen start commit descends from the predecessor's commits.
        assert_eq!(
            successor_state.frozen.build_start_commit,
            predecessor_work.lines().next().unwrap()
        );

        // The linkage is durable, the successor Build runs normally from it, and
        // the predecessor was not amended by any of this.
        let successor_host = ScenarioHost::new(Scenario::Basic);
        assert!(matches!(
            run_with_adapter(
                &store,
                BuildRequest {
                    effort: Some(successor.id.clone()),
                    project: repo.clone(),
                },
                &successor_host,
            )
            .unwrap(),
            BuildResult::Completed(_)
        ));
        assert_eq!(
            read_state(&store, &successor).successor_of,
            successor_state.successor_of
        );
        // The predecessor's exact work is still in the checkout the successor
        // delivered from, and its own record of it is unchanged.
        assert!(
            repo.join("work-1.txt").is_file(),
            "the successor lost the predecessor's committed work"
        );
        assert!(
            git_output_ok(&repo, &["merge-base", "--is-ancestor", &head, "HEAD"]),
            "the predecessor's head is not an ancestor of the successor's history"
        );
        assert_eq!(fs::read(&state_path).unwrap(), before);
        assert_eq!(
            read_state(&store, &effort).frozen.reconciled,
            reconciled_ref
        );
        assert!(read_state(&store, &effort).stop.is_some());
        // No fabricated lineage: the successor's frozen contract is its own, and
        // the predecessor's frozen contract and stop were never rewritten.
        assert_eq!(
            read_state(&store, &successor).frozen.reconciled,
            successor_reconciled
        );
        assert_ne!(successor_reconciled, reconciled_ref);

        // A linkage that names no recorded authority-change refusal, or another
        // contract, is rejected before anything is created for the successor.
        for predecessor in [
            PlanPredecessor {
                effort_id: effort.id.clone(),
                reconciled: reconciled_ref.clone(),
                resolution_id: "res-unknown".into(),
            },
            PlanPredecessor {
                effort_id: effort.id.clone(),
                reconciled: successor_reconciled.clone(),
                resolution_id: resolution_id.clone(),
            },
        ] {
            let other = store
                .init_effort(
                    &repo,
                    "rejected-successor",
                    RequestKind::Freeform,
                    "work".into(),
                    vec![],
                )
                .unwrap();
            let other_reconciled = publish_reconciled(&store, &other, "rejected");
            prepare_build_files(&store, &other, &other_reconciled, Some(predecessor.clone()));
            let other_build = store.phase_dir(&other, "build").unwrap();
            let other_plan = load_plan(&store, &other, &other_build).unwrap();
            let error = initialize_state(&store, &other, &other_build, &other_plan).unwrap_err();
            assert!(error.to_string().contains("successor linkage"), "{error:#}");
            assert!(!other_build.join("state.json").exists());
            assert_eq!(artifact_count(&store, &other, ArtifactKind::Adoption), 0);
        }
    }

    #[test]
    fn authority_amendment_is_additive_and_can_anchor_a_successor() {
        let (store, predecessor, repo, predecessor_reconciled, _) = prepared();
        let predecessor_build = store.phase_dir(&predecessor, "build").unwrap();
        let plan = load_plan(&store, &predecessor, &predecessor_build).unwrap();
        let mut state = initialize_state(&store, &predecessor, &predecessor_build, &plan).unwrap();
        state.action.dispatch = DispatchState::Running;
        let state_path = predecessor_build.join("state.json");
        save_state(&state_path, &state).unwrap();
        let action_dir = predecessor_build.join("artifacts").join(&state.action.id);
        fs::create_dir_all(&action_dir).unwrap();
        write_bytes_sync(
            &action_dir.join("action.json"),
            &serde_json::to_vec_pretty(&json!({"action_id": state.action.id})).unwrap(),
        )
        .unwrap();
        let state_before = fs::read(&state_path).unwrap();
        let config_before = fs::read(predecessor_build.join("config.toml")).unwrap();
        let plan_before = fs::read(predecessor_build.join("plan.json")).unwrap();
        let supplied = temp("authority-amendment-evidence");
        let supplied_file = supplied.join("authorization.txt");
        fs::write(&supplied_file, b"authority reviewed\n").unwrap();

        let outcome = amend_authority(
            &store,
            AuthorityAmendmentRequest {
                effort: predecessor.id.clone(),
                action: state.action.id.clone(),
                note: "Record the changed authority without resuming the interrupted action".into(),
                evidence: vec![supplied_file.clone()],
                confirm_not_running: true,
            },
        )
        .unwrap();
        assert_eq!(fs::read(&state_path).unwrap(), state_before);
        assert_eq!(
            fs::read(predecessor_build.join("config.toml")).unwrap(),
            config_before
        );
        assert_eq!(
            fs::read(predecessor_build.join("plan.json")).unwrap(),
            plan_before
        );
        let record: AuthorityAmendmentRecord = read_json(&outcome.record).unwrap();
        assert_eq!(record.amendment_id, outcome.amendment_id);
        assert_eq!(record.prior_action.id, state.action.id);
        assert_eq!(record.predecessor_reconciled, predecessor_reconciled);
        assert!(record.in_flight_confirmation);
        assert_eq!(record.successor.successor_reconciled, None);
        assert_eq!(record.evidence.len(), 1);
        assert_eq!(
            record.evidence[0].path,
            fs::canonicalize(&supplied_file).unwrap().to_string_lossy()
        );
        let archive_path = temp("authority-amendment-export-out").join("amendment.zip");
        let exported = export::export(&store, &predecessor, &archive_path).unwrap();
        assert!(exported.complete, "{exported:#?}");
        let archived = archive::members_by_name(&archive_path).unwrap();
        let evidence_name = format!(
            "authority-amendments/evidence/{}-0-authorization.txt",
            outcome.amendment_id
        );
        let evidence_member = archived
            .get(&evidence_name)
            .expect("authority-amendment operator evidence was not selected");
        let (bytes, digest) = archive::read_member(&archive_path, evidence_member).unwrap();
        assert_eq!(bytes, b"authority reviewed\n");
        assert_eq!(digest, record.evidence[0].sha256);
        assert_eq!(
            journal_count(&store, &predecessor, "build_authority_amendment_refused"),
            1
        );

        let successor = store
            .init_effort(
                &repo,
                "authority-successor",
                RequestKind::Freeform,
                "work".into(),
                vec![],
            )
            .unwrap();
        let successor_reconciled = publish_reconciled(&store, &successor, "authority successor");
        prepare_build_files(
            &store,
            &successor,
            &successor_reconciled,
            Some(PlanPredecessor {
                effort_id: predecessor.id.clone(),
                reconciled: predecessor_reconciled,
                resolution_id: outcome.amendment_id.clone(),
            }),
        );
        let successor_build = store.phase_dir(&successor, "build").unwrap();
        let successor_plan = load_plan(&store, &successor, &successor_build).unwrap();
        let successor_state =
            initialize_state(&store, &successor, &successor_build, &successor_plan).unwrap();
        let link = successor_state.successor_of.unwrap();
        assert_eq!(link.resolution_id, outcome.amendment_id);
        assert_eq!(link.predecessor_effort_id, predecessor.id);
        assert_eq!(link.successor_reconciled, Some(successor_reconciled));
    }

    #[test]
    fn blocked_results_keep_their_fields_and_add_trigger_data() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::ExternalRequirement);
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        let value = serde_json::to_value(&result).unwrap();
        assert_eq!(value["status"], "blocked");
        // The pre-existing fields keep their meaning and stay parseable...
        assert!(
            value["detail"]
                .as_str()
                .is_some_and(|detail| !detail.is_empty())
        );
        assert_eq!(
            value["state"],
            serde_json::json!(build_state_path(&store, &effort))
        );
        // ...and the typed trigger data is additive.
        let stop = stop_of(&store, &effort);
        assert_eq!(value["trigger"], "external_requirement");
        assert_eq!(value["stopped_action"], serde_json::json!(stop.action.id));
        assert_eq!(value["stop_record"], serde_json::to_value(&stop).unwrap());
        assert_eq!(
            value["stop_record"]["process_completion"],
            stop.process_completion
        );
    }

    #[test]
    fn a_tampered_requirement_projection_stops_before_dispatch() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let plan = load_plan(&store, &effort, &build_dir).unwrap();
        let state = initialize_state(&store, &effort, &build_dir, &plan).unwrap();
        let action_dir = build_dir.join("artifacts").join(&state.action.id);
        // A projection altered after the controller wrote it is detected before
        // any role receives it, and the durable stop names the controller.
        let projection_path =
            ensure_requirement_projection(&store, &effort, &action_dir, &state.frozen.reconciled)
                .unwrap();
        let mut projection: serde_json::Value = read_json(&projection_path).unwrap();
        projection["requirements"][0]["requirement"]["text"] =
            serde_json::json!("weakened by a later writer");
        write_bytes_sync(&projection_path, &serde_json::to_vec(&projection).unwrap()).unwrap();

        let host = ScenarioHost::new(Scenario::Basic);
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        let BuildResult::Blocked {
            trigger,
            stopped_action,
            stop_record,
            ..
        } = result
        else {
            panic!("a tampered projection did not stop the Build")
        };
        assert_eq!(trigger.as_deref(), Some("controller_failure"));
        assert_eq!(stopped_action.as_deref(), Some(state.action.id.as_str()));
        assert_eq!(
            host.call_count("work"),
            0,
            "a role received altered obligations"
        );
        assert!(!action_dir.join("transport.jsonl").exists());
        let record = stop_record.unwrap();
        assert_eq!(
            record["process_completion"],
            "the provider action was not accepted; it never started or its start is unknown"
        );
        assert_eq!(
            record["receipt_validation"],
            "receipt missing; the action produced no result"
        );

        // The action was never accepted, so nothing is resent, and the durable
        // record plus the frozen identities are the evidence of the refusal.
        let after = read_state(&store, &effort);
        let stop = after.stop.clone().unwrap();
        assert_eq!(stop.trigger, StopTrigger::ControllerFailure);
        assert_eq!(stop.action.dispatch, DispatchState::Prepared);
        assert!(stop.action_failure.is_none());
        assert!(
            stop.detail
                .contains("binding requirement projection differs"),
            "{}",
            stop.detail
        );
        assert_eq!(after.action.id, state.action.id);
        assert_eq!(after.phase_index, state.phase_index);
        assert_eq!(after.frozen.reconciled, state.frozen.reconciled);
        assert_eq!(after.stop_history.len(), 1);
        assert_eq!(journal_count(&store, &effort, "build_stopped"), 1);
    }

    #[test]
    fn malformed_or_missing_frozen_inputs_still_record_a_typed_stop() {
        for case in [
            "malformed-plan",
            "malformed-config",
            "missing-config",
            "missing-detailed-plan",
        ] {
            let (store, effort, repo, _reconciled, _adoption) = prepared();
            let build_dir = store.phase_dir(&effort, "build").unwrap();
            let plan = load_plan(&store, &effort, &build_dir).unwrap();
            let state = initialize_state(&store, &effort, &build_dir, &plan).unwrap();
            let before = read_state(&store, &effort);
            match case {
                "malformed-plan" => {
                    fs::write(build_dir.join("plan.json"), b"{\"schema_version\": 2,").unwrap()
                }
                "malformed-config" => {
                    fs::write(build_dir.join("config.toml"), "[worker\nadapter = ").unwrap()
                }
                "missing-config" => fs::remove_file(build_dir.join("config.toml")).unwrap(),
                _ => fs::remove_file(build_dir.join("implementation-plan.md")).unwrap(),
            }

            let host = ScenarioHost::new(Scenario::Basic);
            let result = run_with_adapter(&store, request(&repo), &host).unwrap();
            let BuildResult::Blocked {
                trigger,
                stopped_action,
                ..
            } = result
            else {
                panic!("{case} did not stop the Build")
            };
            assert_eq!(trigger.as_deref(), Some("frozen_input_mismatch"), "{case}");
            assert_eq!(
                stopped_action.as_deref(),
                Some(state.action.id.as_str()),
                "{case}"
            );
            assert_eq!(host.call_count("work"), 0, "{case} dispatched a role");

            // The typed stop is durable and the frozen identities and history
            // are exactly what they were.
            let after = read_state(&store, &effort);
            assert_eq!(after.action.id, before.action.id, "{case}");
            assert_eq!(after.phase_index, before.phase_index, "{case}");
            assert_eq!(after.frozen.reconciled, before.frozen.reconciled, "{case}");
            assert_eq!(after.frozen.adoption, before.frozen.adoption, "{case}");
            assert_eq!(
                after.frozen.plan_digest, before.frozen.plan_digest,
                "{case}"
            );
            assert_eq!(
                after.frozen.config_digest, before.frozen.config_digest,
                "{case}"
            );
            assert_eq!(
                after.frozen.detailed_plan_digest, before.frozen.detailed_plan_digest,
                "{case}"
            );
            assert_eq!(
                after.frozen.build_start_commit, before.frozen.build_start_commit,
                "{case}"
            );
            let stop = after.stop.clone().unwrap();
            assert_eq!(stop.trigger, StopTrigger::FrozenInputMismatch, "{case}");
            assert_eq!(stop.action.id, before.action.id, "{case}");
            assert_eq!(after.stop_history.len(), 1, "{case}");
            assert_eq!(journal_count(&store, &effort, "build_stopped"), 1, "{case}");

            // A changed frozen input is still not a resolution path.
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
            assert!(
                error.to_string().contains("cannot resolve"),
                "{case}: {error:#}"
            );
            assert!(resolution_files(&store, &effort).is_empty(), "{case}");
        }
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
        assert!(
            error.to_string().contains("valid Build config")
                || error.to_string().contains("unsupported"),
            "{error:#}"
        );
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
        let ResolutionOutcome::Resolved {
            resolution_id,
            continuation_action,
            config_version,
            ..
        } = &outcome
        else {
            panic!("overlay repair was refused")
        };
        assert_eq!(*config_version, Some(2));

        // The original config bytes and frozen digest are untouched.
        assert_eq!(
            fs::read(build_dir.join("config.toml")).unwrap(),
            original_config
        );
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
            &build_dir
                .join("artifacts")
                .join(&stopped_action)
                .join("action.json"),
        )
        .unwrap();
        assert_eq!(prior_packet["config_version"], 1);

        let resumed = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(matches!(resumed, BuildResult::Completed(_)));
        assert_eq!(host.adapters("work"), vec!["codex", "codex", "cursor"]);
        let continuation_packet: serde_json::Value = read_json(
            &build_dir
                .join("artifacts")
                .join(continuation_action)
                .join("action.json"),
        )
        .unwrap();
        assert_eq!(continuation_packet["config_version"], 2);
    }

    #[test]
    fn changing_the_reviewer_adapter_clears_only_its_session() {
        let (store, effort, repo, _, _) = prepared();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let before = read_state(&store, &effort);
        assert_eq!(before.sessions.worker.as_deref(), Some("worker-session"));
        assert_eq!(
            before.sessions.reviewer.as_deref(),
            Some("reviewer-session")
        );
        let overlay = temp("reviewer-adapter-overlay");
        fs::write(
            overlay.join("config.toml"),
            "schema_version = 2\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"codex\"\n",
        )
        .unwrap();
        resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "repair reviewer adapter",
            false,
            Some(overlay.join("config.toml")),
            Vec::new(),
        )
        .unwrap();
        let applied = read_state(&store, &effort);
        assert_eq!(applied.sessions.worker.as_deref(), Some("worker-session"));
        assert!(applied.sessions.reviewer.is_none());
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let (_, review) = action_packets(&build_dir)
            .into_iter()
            .find(|(_, packet)| packet["kind"] == "review" && packet["scope"] == "D2")
            .expect("the repaired reviewer action did not run");
        let record: serde_json::Value = read_json(
            &build_dir
                .join("artifacts")
                .join(review["action_id"].as_str().unwrap())
                .join("invocation.json"),
        )
        .unwrap();
        assert_eq!(record["adapter"], "codex");
        assert_eq!(record["session"]["requested"], serde_json::Value::Null);
        assert_eq!(record["session"]["resumed"], false);
    }

    #[test]
    fn same_adapter_overlay_preserves_persistent_sessions() {
        let (store, effort, repo, _, _) = prepared();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let before = read_state(&store, &effort);
        let overlay = temp("same-adapter-overlay");
        fs::write(
            overlay.join("config.toml"),
            "schema_version = 2\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"cursor\"\n",
        )
        .unwrap();
        resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "same adapter, repaired local environment",
            false,
            Some(overlay.join("config.toml")),
            Vec::new(),
        )
        .unwrap();
        let after = read_state(&store, &effort);
        assert_eq!(after.sessions.worker, before.sessions.worker);
        assert_eq!(after.sessions.reviewer, before.sessions.reviewer);
    }

    #[test]
    fn config_overlay_versions_are_immutable_and_govern_their_own_actions() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let original_config = fs::read(build_dir.join("config.toml")).unwrap();
        let host = ScenarioHost::new(Scenario::AlwaysBlockedAudit);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        assert_eq!(host.adapters("final_audit"), vec!["cursor", "cursor"]);
        let first_stopped = stop_of(&store, &effort).action.id.clone();

        // First repair: the reviewer adapter was misconfigured.
        let overlay = temp("overlay");
        fs::write(
            overlay.join("config.toml"),
            "schema_version = 2\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"codex\"\n",
        )
        .unwrap();
        let first = resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "the reviewer adapter was misconfigured",
            false,
            Some(overlay.join("config.toml")),
            Vec::new(),
        )
        .unwrap();
        let ResolutionOutcome::Resolved {
            resolution_id: first_id,
            continuation_action: first_continuation,
            config_version: first_version,
            ..
        } = &first
        else {
            panic!("the first repair was refused")
        };
        assert_eq!(*first_version, Some(2));
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        assert_eq!(
            host.adapters("final_audit"),
            vec!["cursor", "cursor", "codex", "codex"]
        );

        // Second repair at the next stop: a distinct third version.
        fs::write(
            overlay.join("config.toml"),
            "schema_version = 2\n[worker]\nadapter = \"cursor\"\n[reviewer]\nadapter = \"cursor\"\n",
        )
        .unwrap();
        let second = resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "the reviewer adapter was repaired again",
            false,
            Some(overlay.join("config.toml")),
            Vec::new(),
        )
        .unwrap();
        let ResolutionOutcome::Resolved {
            resolution_id: second_id,
            continuation_action: second_continuation,
            config_version: second_version,
            ..
        } = &second
        else {
            panic!("the second repair was refused")
        };
        assert_eq!(*second_version, Some(3));
        assert_ne!(first_id, second_id);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        // Both attempts under the second version, and neither under the first.
        assert_eq!(
            host.adapters("final_audit"),
            vec!["cursor", "cursor", "codex", "codex", "cursor", "cursor"]
        );

        // Every version is immutable, and each says which action it governs.
        let overlays = config_overlays(&build_dir).unwrap();
        assert_eq!(
            overlays
                .iter()
                .map(|overlay| overlay.version)
                .collect::<Vec<_>>(),
            vec![2, 3]
        );
        assert_eq!(overlays[0].first_action_id, *first_continuation);
        assert_eq!(overlays[1].first_action_id, *second_continuation);
        assert_eq!(overlays[0].resolution_id, *first_id);
        assert_eq!(overlays[1].resolution_id, *second_id);
        assert_eq!(
            fs::read_dir(build_dir.join(CONFIG_HISTORY_DIR))
                .unwrap()
                .count(),
            2
        );
        let effective = effective_config(&build_dir).unwrap();
        assert_eq!(effective.version, 3);
        assert_eq!(effective.config.worker.adapter, "cursor");

        // The original config bytes and frozen digest are untouched, and prior
        // action records keep the settings they ran under.
        assert_eq!(
            fs::read(build_dir.join("config.toml")).unwrap(),
            original_config
        );
        assert_eq!(
            read_state(&store, &effort).frozen.config_digest,
            digest_bytes(&original_config)
        );
        for (action_id, expected) in [
            (first_stopped, 1),
            (first_continuation.clone(), 2),
            (second_continuation.clone(), 3),
        ] {
            let packet: serde_json::Value = read_json(
                &build_dir
                    .join("artifacts")
                    .join(&action_id)
                    .join("action.json"),
            )
            .unwrap();
            assert_eq!(packet["config_version"], expected, "{action_id}");
        }
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
        for field in [
            "migration_version",
            "stop",
            "stop_history",
            "current_audit",
            "applied_resolution_id",
        ] {
            legacy.as_object_mut().unwrap().remove(field);
        }
        legacy["action"]
            .as_object_mut()
            .unwrap()
            .remove("resolution_id");
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
        legacy["action"]
            .as_object_mut()
            .unwrap()
            .remove("resolution_id");
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
        fs::write(
            evidence.join("deployment.txt"),
            "live deployment verified\n",
        )
        .unwrap();
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
        let BuildResult::Completed(done) = run_with_adapter(&store, request(&repo), &host).unwrap()
        else {
            panic!("Build did not complete")
        };
        let implementation = done.implementation.clone();
        let (_, implementation_record): (_, orchestrate_contracts::Implementation) = store
            .load_json(&effort, &implementation, "implementation.json")
            .unwrap();
        assert_eq!(
            implementation_record.target_commit,
            git(&repo, ["rev-parse", "HEAD"]).unwrap()
        );

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

    /// The advisory once-over is configured here with its own attributes, which
    /// must differ from both the worker's and the reviewer's.
    const ONCE_OVER_CONFIG: &str = "schema_version = 2\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"cursor\"\n[once_over]\nadapter = \"claude\"\nmodel = \"once-over-model\"\n";

    fn prepared_with_config(config_toml: &str) -> (Store, Effort, PathBuf) {
        let root = temp("store");
        let repo = fresh_repo("repo");
        let store = Store::open(&root).unwrap();
        let effort = store
            .init_effort(&repo, "build", RequestKind::Freeform, "work".into(), vec![])
            .unwrap();
        let reconciled_ref = publish_reconciled(&store, &effort, "test");
        prepare_build_files_with_config(&store, &effort, &reconciled_ref, None, config_toml);
        (store, effort, repo)
    }

    /// Every `action.json` of one Build, by action id.
    fn action_packets(build_dir: &Path) -> Vec<(String, serde_json::Value)> {
        let artifacts = build_dir.join("artifacts");
        let mut packets = fs::read_dir(artifacts)
            .unwrap()
            .map(|entry| {
                let path = entry.unwrap().path();
                let value: serde_json::Value = read_json(&path.join("action.json")).unwrap();
                (value["action_id"].as_str().unwrap().to_owned(), value)
            })
            .collect::<Vec<_>>();
        packets.sort_by(|left, right| left.0.cmp(&right.0));
        packets
    }

    /// R-035: absent configuration yields zero invocations.
    #[test]
    fn an_absent_once_over_never_runs() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::Basic);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        assert_eq!(host.call_count("once_over"), 0);
        assert!(read_state(&store, &effort).once_over.is_empty());
    }

    /// R-035: one advisory invocation per new commit submitted to Audit, after
    /// the final delivery Review and before that commit's first formal Audit.
    #[test]
    fn a_configured_once_over_runs_once_per_new_audit_commit() {
        let (store, effort, repo) = prepared_with_config(ONCE_OVER_CONFIG);
        let host = ScenarioHost::new(Scenario::Corrections);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        // Two commits reached the formal Audit: the reviewed delivery and the
        // correction at final scope.  Each got exactly one advisory invocation.
        assert_eq!(host.call_count("final_audit"), 2);
        assert_eq!(host.call_count("once_over"), 2);
        let calls = host.calls.lock().unwrap().clone();
        let audits = calls
            .iter()
            .enumerate()
            .filter(|(_, call)| call.starts_with("final_audit:"))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let advisories = calls
            .iter()
            .enumerate()
            .filter(|(_, call)| call.starts_with("once_over:"))
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        assert!(
            advisories[0] < audits[0] && audits[0] < advisories[1] && advisories[1] < audits[1]
        );

        let state = read_state(&store, &effort);
        assert_eq!(state.once_over.len(), 2);
        let audit_commits = state
            .once_over
            .iter()
            .map(|run| run.commit.clone())
            .collect::<HashSet<_>>();
        assert_eq!(audit_commits.len(), 2, "both advisories used one commit");
        assert!(
            state
                .once_over
                .iter()
                .all(|run| run.outcome == "advisory_complete")
        );

        // The advisory role used its own configured adapter and model, and its
        // report never entered the worker's or the formal Audit's packet.
        assert_eq!(
            host.adapters("once_over"),
            vec!["claude".to_owned(), "claude".to_owned()]
        );
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let advisory_ids = state
            .once_over
            .iter()
            .map(|run| run.action_id.clone())
            .collect::<Vec<_>>();
        for (action_id, packet) in action_packets(&build_dir) {
            let kind = packet["kind"].as_str().unwrap();
            if kind == "once_over" {
                continue;
            }
            let text = packet.to_string();
            for advisory in &advisory_ids {
                assert!(
                    !text.contains(advisory.as_str()),
                    "{kind} packet {action_id} referenced advisory action {advisory}"
                );
            }
            assert!(
                !text.contains("advisory"),
                "{kind} packet {action_id} received advisory material"
            );
        }
        // The advisory run is visible to the operator in status and on disk.
        for run in &state.once_over {
            assert!(Path::new(&run.report).is_file(), "missing {}", run.report);
        }
    }

    /// R-036: advisory blocked, invalid-receipt and failed outcomes leave the
    /// formal acceptance outcome exactly as it is with the role absent.
    #[test]
    fn advisory_once_over_outcomes_never_gate_the_formal_audit() {
        for scenario in [
            Scenario::OnceOverBlocked,
            Scenario::OnceOverInvalidReceipt,
            Scenario::OnceOverMissingReceipt,
            Scenario::OnceOverStaleReceipt,
        ] {
            let (store, effort, repo) = prepared_with_config(ONCE_OVER_CONFIG);
            let host = ScenarioHost::new(scenario);
            let result = run_with_adapter(&store, request(&repo), &host).unwrap();
            assert!(
                matches!(result, BuildResult::Completed(_)),
                "{scenario:?} gated the Build"
            );
            assert_eq!(host.call_count("once_over"), 1, "{scenario:?}");
            assert_eq!(host.call_count("final_audit"), 1, "{scenario:?}");
            assert_eq!(host.call_count("unblock"), 0, "{scenario:?}");
            let state = read_state(&store, &effort);
            let expected = match scenario {
                Scenario::OnceOverBlocked => "blocked",
                _ => "invalid_receipt",
            };
            assert_eq!(state.once_over[0].outcome, expected, "{scenario:?}");
            assert!(
                state.stop.is_none() && state.terminal.is_some(),
                "{scenario:?} changed the stop or acceptance state"
            );
        }
        // A transport-level advisory failure is recorded and equally non-gating.
        let (store, effort, repo) = prepared_with_config(ONCE_OVER_CONFIG);
        let host = ScenarioHost::new(Scenario::OnceOverSpawnFailure);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        assert_eq!(
            read_state(&store, &effort).once_over[0].outcome,
            "spawn_failed"
        );
    }

    /// R-015/R-016: the remedy keeps the original review correction as the
    /// action's feedback, adds the diagnosis separately, and names the exact
    /// predecessor with its report/result/transport or explicit absence.
    #[test]
    fn handoffs_keep_the_original_correction_and_name_the_predecessor() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::CorrectionBlockedThenRemedy);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        assert_eq!(host.call_count("unblock"), 1);
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let packets = action_packets(&build_dir);
        let by_kind = |kind: &str| {
            packets
                .iter()
                .filter(|(_, packet)| packet["kind"] == kind)
                .cloned()
                .collect::<Vec<_>>()
        };
        let corrections = by_kind("review")
            .into_iter()
            .filter(|(_, packet)| packet["scope"] == "D1")
            .collect::<Vec<_>>();
        let correction_report = corrections
            .first()
            .map(|(id, _)| {
                build_dir
                    .join("artifacts")
                    .join(id)
                    .join("report.md")
                    .to_string_lossy()
                    .into_owned()
            })
            .expect("the correction review ran");
        let resumed = by_kind("work")
            .into_iter()
            .find(|(_, packet)| packet["predecessor"]["handoff"] == "remedy")
            .expect("no action was resumed by a remedy");
        let packet = &resumed.1;
        // The original correction source is unchanged and still the feedback.
        assert_eq!(
            packet["feedback"].as_str(),
            Some(correction_report.as_str()),
            "the remedy replaced the original correction"
        );
        assert!(Path::new(&correction_report).is_file());
        let unblock_ids = by_kind("unblock")
            .iter()
            .map(|(id, _)| id.clone())
            .collect::<Vec<_>>();
        assert_eq!(unblock_ids.len(), 1);
        let remedy = packet["remedy"]["report"].as_str().unwrap();
        assert_eq!(
            remedy,
            build_dir
                .join("artifacts")
                .join(&unblock_ids[0])
                .join("report.md")
                .to_string_lossy(),
            "the diagnosis is not separately referenced"
        );
        assert!(Path::new(remedy).is_file());
        let predecessor = &packet["predecessor"];
        assert_eq!(predecessor["handoff"], "remedy");
        assert_eq!(predecessor["kind"], "work");
        assert_eq!(predecessor["scope"], "D1");
        assert_eq!(predecessor["result_present"], true);
        assert_eq!(predecessor["report_present"], false);
        assert!(predecessor["ending_commit"].as_str().is_some());
        assert!(
            predecessor["claims"]
                .as_str()
                .unwrap()
                .contains("unverified")
        );
        assert!(
            predecessor["environment_note"]
                .as_str()
                .unwrap()
                .contains("re-checkable")
        );
    }

    /// R-016: a continued or replaced action carries the same exact predecessor
    /// facts as a remedied one.
    #[test]
    fn continuations_and_replacements_carry_their_predecessor_facts() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::InterruptedTwice);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let handoffs = action_packets(&build_dir)
            .into_iter()
            .filter(|(_, packet)| packet["kind"] == "work")
            .filter_map(|(_, packet)| {
                packet["predecessor"]["handoff"]
                    .as_str()
                    .map(|kind| kind.to_owned())
                    .map(|kind| (kind, packet))
            })
            .collect::<Vec<_>>();
        let kinds = handoffs
            .iter()
            .map(|(kind, _)| kind.clone())
            .collect::<HashSet<_>>();
        assert!(
            kinds.contains("continuation") && kinds.contains("replacement"),
            "the ladder did not record its handoffs: {kinds:?}"
        );
        for (kind, packet) in &handoffs {
            let predecessor = &packet["predecessor"];
            assert_ne!(
                predecessor["action_id"], packet["action_id"],
                "{kind} named itself as its predecessor"
            );
            assert!(
                predecessor["transport"]
                    .as_str()
                    .unwrap()
                    .ends_with("transport.jsonl"),
                "{kind} lost the transport reference"
            );
            // Prior progress and unverified claims are not completion.
            assert!(
                predecessor["claims"]
                    .as_str()
                    .unwrap()
                    .contains("not completion")
            );
        }
    }

    /// Configured worker attributes that the invocation record must show.
    const ATTRIBUTED_CONFIG: &str = "schema_version = 2\n[worker]\nadapter = \"codex\"\nmodel = \"worker-model\"\nreasoning_effort = \"high\"\n[reviewer]\nadapter = \"cursor\"\n";

    /// R-032/R-033: every dispatch — fresh, resumed, replaced and failed-launch —
    /// keeps a passive invocation record, the actual prompt-elided command and
    /// the exact instruction copy.
    #[test]
    fn invocation_records_are_passive_prompt_elided_and_attributed() {
        let (store, effort, repo) = prepared_with_config(ATTRIBUTED_CONFIG);
        let host = ScenarioHost::new(Scenario::InterruptedTwice);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let mut modes = Vec::new();
        for (action_id, packet) in action_packets(&build_dir) {
            if packet["kind"] != "work" {
                continue;
            }
            let action_dir = build_dir.join("artifacts").join(&action_id);
            let record: serde_json::Value = read_json(&action_dir.join("invocation.json")).unwrap();
            let keys = record
                .as_object()
                .unwrap()
                .keys()
                .cloned()
                .collect::<HashSet<_>>();
            assert_eq!(
                keys,
                [
                    "schema_version",
                    "action_id",
                    "action",
                    "role",
                    "adapter",
                    "mode",
                    "build_action_continuation",
                    "config_version",
                    "session",
                    "requested",
                    "mapped",
                    "observed",
                    "executable",
                    "command",
                    "instruction",
                    "outputs",
                    "privacy",
                    "timing",
                    "completion",
                    "completion_detail",
                    "exit",
                ]
                .iter()
                .map(|key| (*key).to_owned())
                .collect::<HashSet<_>>(),
                "the invocation record carries unexpected fields"
            );
            assert_eq!(record["role"], "worker");
            assert_eq!(record["adapter"], "codex");
            // R-032 wants the action, kind and scope with the dispatch facts.
            assert_eq!(record["action"]["id"].as_str().unwrap(), action_id);
            assert_eq!(record["action"]["kind"], "work");
            assert_eq!(record["action"]["scope"], packet["scope"]);
            assert_eq!(
                record["requested"]["legacy_model"]["configured"], "worker-model",
                "the requested model was not recorded"
            );
            assert_eq!(record["requested"]["legacy_model"]["source"], "configured");
            assert_eq!(
                record["mapped"]["arguments"],
                serde_json::json!([
                    "--model",
                    "worker-model",
                    "-c",
                    "model_reasoning_effort=high"
                ])
            );
            assert_eq!(
                record["requested"]["permission"]["configured"],
                serde_json::Value::Null
            );
            assert!(
                record["requested"]["permission"]["source"]
                    .as_str()
                    .unwrap()
                    .contains("provider default")
            );
            // The command is the argv the dispatch actually launched: its
            // subcommand, working directory and directory grants are present,
            // and only the prompt is elided.
            let form = record["command"]["form"].as_array().unwrap();
            let form = form
                .iter()
                .map(|part| part.as_str().unwrap().to_owned())
                .collect::<Vec<_>>();
            let repo_text = fs::canonicalize(&repo)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            let build_text = fs::canonicalize(&build_dir)
                .unwrap()
                .to_string_lossy()
                .into_owned();
            assert_eq!(form[0], "codex");
            assert_eq!(form[1], "exec");
            assert_eq!(form[2], "--json");
            assert_eq!(form[3], "-C");
            assert_eq!(form[4], repo_text);
            assert_eq!(form[5], "--add-dir");
            assert_eq!(form[6], build_text);
            assert_eq!(form.last().map(String::as_str), Some("<prompt elided>"));
            assert!(form.contains(&"worker-model".to_owned()));
            assert!(form.contains(&"model_reasoning_effort=high".to_owned()));
            assert_eq!(record["command"]["working_directory"], repo_text.as_str());
            assert_eq!(
                record["command"]["directory_grants"],
                serde_json::json!([build_text])
            );
            assert!(
                !form.iter().any(|part| part.contains("Read ")),
                "the command embedded the prompt"
            );
            assert_eq!(
                record["command"]["prompt_sha256"].as_str().unwrap().len(),
                64
            );
            // The exact instruction copy is retained and digest-bound.
            let instruction = record["instruction"]["path"].as_str().unwrap();
            let bytes = fs::read(instruction).unwrap();
            assert_eq!(
                record["instruction"]["sha256"].as_str().unwrap(),
                digest_bytes(&bytes)
            );
            assert_eq!(
                bytes,
                canonical_role_guide(&ActionKind::Work).as_bytes(),
                "the retained instruction differs from the supplied one"
            );
            // Timing is dispatch-to-completion from a monotonic clock, in a
            // preparation → dispatch → end order.
            assert!(record["timing"]["elapsed_ms"].is_u64());
            assert!(record["timing"]["elapsed_observed"].as_bool().unwrap());
            let prepared = record["timing"]["prepared_at_ms"].as_u64().unwrap();
            let dispatched = record["timing"]["dispatched_at_ms"].as_u64().unwrap();
            let ended = record["timing"]["ended_at_ms"].as_u64().unwrap();
            assert!(prepared <= dispatched && dispatched <= ended, "{record:#?}");
            assert!(record["observed"]["model"].is_null());
            assert!(
                record["observed"]["note"]
                    .as_str()
                    .unwrap()
                    .contains("not observation")
            );
            assert!(record["privacy"].as_str().unwrap().contains("credentials"));
            assert!(record["completion"].as_str().is_some());
            modes.push(record["mode"].as_str().unwrap().to_owned());
        }
        assert!(modes.contains(&"fresh".to_owned()));
        assert!(
            modes.contains(&"resumed".to_owned()) || modes.contains(&"replacement".to_owned()),
            "the ladder did not record a resumed or replaced dispatch: {modes:?}"
        );

        // R-033: published implementation and Audit provenance name the exact
        // producing configuration, and an unset attribute stays unknown.
        let state = read_state(&store, &effort);
        let implementation = state.implementation.clone().unwrap();
        let envelope = store.load_envelope(&effort, &implementation).unwrap();
        assert_eq!(envelope.provenance.provider.as_deref(), Some("codex"));
        assert_eq!(envelope.provenance.model.as_deref(), Some("worker-model"));
        assert_eq!(envelope.provenance.model_effort.as_deref(), Some("high"));
        let audit = state.terminal.as_ref().unwrap().audit.clone();
        let envelope = store.load_envelope(&effort, &audit).unwrap();
        assert_eq!(envelope.provenance.provider.as_deref(), Some("cursor"));
        assert_eq!(envelope.provenance.model, None);
        assert_eq!(envelope.provenance.model_effort, None);
    }

    /// R-034: the raw provider transport is retained verbatim, with its original
    /// fields and scope, and no controller-side normalization.
    #[test]
    fn raw_provider_transport_is_retained_verbatim() {
        let (store, effort, repo) = prepared_with_config(ATTRIBUTED_CONFIG);
        let transport = "{\"type\":\"turn.completed\",\"usage\":{\"input_tokens\":120,\"cached_input_tokens\":40,\"output_tokens\":9,\"reasoning_output_tokens\":3}}\n";
        let host = TransportHost {
            lines: transport.to_owned(),
        };
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let work = action_packets(&build_dir)
            .into_iter()
            .find(|(_, packet)| packet["kind"] == "work")
            .unwrap();
        let log = fs::read_to_string(
            build_dir
                .join("artifacts")
                .join(&work.0)
                .join("transport.jsonl"),
        )
        .unwrap();
        assert_eq!(
            log, transport,
            "the raw transport was normalized or reordered"
        );
    }

    struct TransportHost {
        lines: String,
    }

    impl HostAdapter for TransportHost {
        fn invoke(
            &self,
            invocation: &Invocation,
            observer: &mut dyn InvocationObserver,
        ) -> Result<InvocationResult> {
            observer.accepted()?;
            fs::write(invocation.action.join("transport.jsonl"), &self.lines)?;
            let action: serde_json::Value = read_json(&invocation.action.join("action.json"))?;
            let kind = action["kind"].as_str().unwrap();
            let scope = action["scope"].as_str().unwrap();
            let receipt = match kind {
                "work" => {
                    fs::write(invocation.cwd.join(format!("{scope}.txt")), scope)?;
                    git_ok(&invocation.cwd, &["add", "."]);
                    git_ok(&invocation.cwd, &["commit", "-m", scope]);
                    json!({"action_id": action["action_id"], "scope": scope, "outcome": "complete", "commit": git(&invocation.cwd, ["rev-parse", "HEAD"])?})
                }
                "review" => {
                    json!({"action_id": action["action_id"], "scope": scope, "outcome": "pass", "commit": action["target_commit"]})
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
                    json!({"action_id": action["action_id"], "scope": scope, "outcome": "complete"})
                }
                other => bail!("unexpected transport-host action {other}"),
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

    /// Read-only roles: the worker writes and commits normally, while the
    /// reviewer and the final Audit write no file at all and state their
    /// evidence in the provider's own final response instead.
    struct ReadOnlyEvidenceHost {
        /// Replace the audit's response with prose that carries no block.
        audit_without_evidence: bool,
        /// How many of the next reviews write their own `result.json`
        /// (`changes_required`) beside a passing response block.
        conflicting_reviews: std::sync::atomic::AtomicUsize,
        /// How many of the next audits report coverage they cannot vouch for,
        /// which derives BLOCKED and reaches the Unblock diagnosis.
        audit_blocks: std::sync::atomic::AtomicUsize,
    }

    impl ReadOnlyEvidenceHost {
        fn new() -> Self {
            Self {
                audit_without_evidence: false,
                conflicting_reviews: std::sync::atomic::AtomicUsize::new(0),
                audit_blocks: std::sync::atomic::AtomicUsize::new(0),
            }
        }
    }

    /// One provider event per message, in the shape the adapters' streams use.
    fn transport_log(events: &[(&str, String)]) -> String {
        events
            .iter()
            .map(|(kind, text)| {
                let event = match *kind {
                    "agent" => json!({"type": "item.completed", "item": {"type": "agent_message", "text": text}}),
                    _ => json!({"type": "result", "subtype": "success", "is_error": false, "result": text}),
                };
                format!("{}\n", serde_json::to_string(&event).unwrap())
            })
            .collect()
    }

    impl HostAdapter for ReadOnlyEvidenceHost {
        fn invoke(
            &self,
            invocation: &Invocation,
            observer: &mut dyn InvocationObserver,
        ) -> Result<InvocationResult> {
            observer.accepted()?;
            observer.session_id(&format!("{}-session", invocation.role))?;
            let action: serde_json::Value = read_json(&invocation.action.join("action.json"))?;
            let kind = action["kind"].as_str().unwrap();
            let scope = action["scope"].as_str().unwrap();
            match kind {
                "work" => {
                    fs::write(
                        invocation.cwd.join(format!("{scope}.txt")),
                        format!("{scope} at {}\n", now_ms()),
                    )?;
                    git_ok(&invocation.cwd, &["add", "."]);
                    git_ok(&invocation.cwd, &["commit", "-m", scope]);
                    let receipt = json!({
                        "action_id": action["action_id"],
                        "scope": scope,
                        "outcome": "complete",
                        "commit": git(&invocation.cwd, ["rev-parse", "HEAD"])?,
                    });
                    write_bytes_sync(
                        &invocation.action.join("result.json"),
                        &serde_json::to_vec_pretty(&receipt)?,
                    )?;
                    write_bytes_sync(&invocation.action.join("report.md"), scope.as_bytes())?;
                    write_bytes_sync(
                        &invocation.action.join("transport.jsonl"),
                        transport_log(&[("agent", "worker finished".into())]).as_bytes(),
                    )?;
                }
                "review" => {
                    let receipt = json!({
                        "action_id": action["action_id"],
                        "scope": scope,
                        "outcome": "pass",
                        "commit": action["target_commit"],
                    });
                    if self
                        .conflicting_reviews
                        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                            remaining.checked_sub(1)
                        })
                        .is_ok()
                    {
                        let conflicting = json!({
                            "action_id": action["action_id"],
                            "scope": scope,
                            "outcome": "changes_required",
                            "commit": action["target_commit"],
                        });
                        write_bytes_sync(
                            &invocation.action.join("result.json"),
                            &serde_json::to_vec_pretty(&conflicting)?,
                        )?;
                        write_bytes_sync(
                            &invocation.action.join("report.md"),
                            b"the written correction set",
                        )?;
                    }
                    let response = format!(
                        "Review complete.\n\n```orchestrate-receipt\n{receipt}\n```\n\n````orchestrate-report\n# Review\n\nInspected the exact commit.\n\n```text\nan ordinary fence inside the report\n```\n\nThe phase is accepted.\n````\n"
                    );
                    write_bytes_sync(
                        &invocation.action.join("transport.jsonl"),
                        transport_log(&[("agent", response)]).as_bytes(),
                    )?;
                }
                "final_audit" => {
                    let response = if self.audit_without_evidence {
                        "The implementation looks good to me; I would pass it.\n".to_owned()
                    } else {
                        let receipt = json!({
                            "action_id": action["action_id"],
                            "scope": scope,
                            "outcome": "complete",
                        });
                        // A blocked attempt reports coverage it cannot vouch
                        // for; the remedy then authorizes a fresh assessment.
                        let blocked = self
                            .audit_blocks
                            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |remaining| {
                                remaining.checked_sub(1)
                            })
                            .is_ok();
                        let assessment = json!({
                            "reconciled": action["reconciled"],
                            "adoption": action["adoption"],
                            "implementation": action["implementation"],
                            "coverage": [{
                                "requirement_id": "R-1",
                                "state": if blocked { "unknown" } else { "pass" },
                                "rationale": if blocked {
                                    "the requirement could not be established from the supplied evidence"
                                } else {
                                    "the registered implementation satisfies the requirement"
                                },
                                "evidence": ["the verification checkout"],
                                "correction": "",
                            }],
                            "assessor_context": "read-only transport exercise",
                        });
                        format!(
                            "Audit complete.\n\n```orchestrate-receipt\n{receipt}\n```\n\n````orchestrate-report\n# Final Audit\n\nAssessed the registered implementation.\n````\n\n````orchestrate-assessment\n{assessment}\n````\n"
                        )
                    };
                    write_bytes_sync(
                        &invocation.action.join("transport.jsonl"),
                        transport_log(&[("result", response)]).as_bytes(),
                    )?;
                }
                "once_over" => {
                    let receipt = json!({
                        "action_id": action["action_id"],
                        "scope": scope,
                        "outcome": "advisory_complete",
                    });
                    let response = format!(
                        "Once-over complete.\n\n```orchestrate-receipt\n{receipt}\n```\n\n````orchestrate-report\n# Once-over\n\nAdvisory reading of the whole implementation.\n````\n"
                    );
                    write_bytes_sync(
                        &invocation.action.join("transport.jsonl"),
                        transport_log(&[("agent", response)]).as_bytes(),
                    )?;
                }
                "unblock" => {
                    let receipt = json!({
                        "action_id": action["action_id"],
                        "scope": scope,
                        "outcome": "remedy_available",
                    });
                    let response = format!(
                        "Diagnosis complete.\n\n```orchestrate-receipt\n{receipt}\n```\n\n````orchestrate-report\n# Unblock\n\nThe retry changes on one recorded condition: the missing evidence is supplied by reading the registered snapshot.\n````\n"
                    );
                    write_bytes_sync(
                        &invocation.action.join("transport.jsonl"),
                        transport_log(&[("agent", response)]).as_bytes(),
                    )?;
                }
                other => bail!("unexpected read-only host action {other}"),
            }
            Ok(InvocationResult {
                completion: InvocationCompletion::Completed,
            })
        }
    }

    /// The advisory once-over and the Unblock diagnosis are read-only roles too,
    /// so their evidence travels the same way: the once-over is recorded with
    /// its transported outcome and the remedy is the one the controller acts on.
    #[test]
    fn read_only_once_over_and_unblock_complete_from_their_own_final_response() {
        let (store, effort, repo) = prepared_with_config(
            "schema_version = 3\n[worker]\nadapter = \"cursor\"\nmodel_strength = \"standard\"\npermission = \"workspace_write\"\n[reviewer]\nadapter = \"codex\"\nmodel_strength = \"standard\"\npermission = \"read_only\"\n[unblocker]\nadapter = \"claude\"\nmodel_strength = \"standard\"\npermission = \"read_only\"\n[once_over]\nadapter = \"codex\"\nmodel_strength = \"standard\"\npermission = \"read_only\"\n",
        );
        let host = ReadOnlyEvidenceHost::new();
        host.audit_blocks.store(1, Ordering::SeqCst);
        let BuildResult::Completed(done) = run_with_adapter(&store, request(&repo), &host).unwrap()
        else {
            panic!("the read-only diagnostics did not complete the Build");
        };
        let (_, report): (_, orchestrate_contracts::AuditReport) =
            store.load_json(&effort, &done.audit, "audit.json").unwrap();
        assert_eq!(report.verdict, Verdict::Pass);

        let state = read_state(&store, &effort);
        assert_eq!(
            state
                .once_over
                .iter()
                .map(|run| run.outcome.as_str())
                .collect::<Vec<_>>(),
            vec!["advisory_complete"],
            "the advisory once-over was not recorded from its own response"
        );
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let kinds = action_kinds_by_id(&build_dir);
        for (id, kind) in &kinds {
            let action_dir = build_dir.join("artifacts").join(id);
            match kind.as_str() {
                "once_over" | "unblock" => {
                    assert!(
                        action_dir.join("transport-evidence.json").is_file(),
                        "{kind} did not take its evidence from its own response"
                    );
                    let receipt: Receipt = read_json(&action_dir.join("result.json")).unwrap();
                    assert_eq!(
                        receipt.outcome,
                        if kind == "once_over" {
                            "advisory_complete"
                        } else {
                            "remedy_available"
                        }
                    );
                    assert!(
                        !fs::read_to_string(action_dir.join("report.md"))
                            .unwrap()
                            .trim()
                            .is_empty()
                    );
                }
                _ => {}
            }
        }
        assert!(
            kinds.values().any(|kind| kind == "once_over"),
            "the configured once-over never ran"
        );
        assert!(
            kinds.values().any(|kind| kind == "unblock"),
            "the blocked Audit never reached its diagnosis"
        );
        assert_eq!(
            kinds.values().filter(|kind| *kind == "final_audit").count(),
            2,
            "the remedy did not authorize exactly one fresh reassessment"
        );
        assert_no_build_files_leaked(&repo);
    }

    /// The roles the read-only permission mapping selects still complete real
    /// Build actions: the controller persists the evidence their permission
    /// prevented them from writing and validates it exactly as a written file.
    #[test]
    fn read_only_roles_complete_a_build_from_their_own_final_response() {
        let (store, effort, repo) = prepared_with_config(
            "schema_version = 3\n[worker]\nadapter = \"cursor\"\nmodel_strength = \"standard\"\npermission = \"workspace_write\"\n[reviewer]\nadapter = \"codex\"\nmodel_strength = \"standard\"\nreasoning_effort = \"low\"\npermission = \"read_only\"\n",
        );
        let host = ReadOnlyEvidenceHost::new();
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        let BuildResult::Completed(done) = result else {
            panic!("the read-only roles did not complete the Build: {result:?}");
        };
        let (_, report): (_, orchestrate_contracts::AuditReport) =
            store.load_json(&effort, &done.audit, "audit.json").unwrap();
        assert_eq!(report.verdict, Verdict::Pass);
        assert_eq!(report.assessment.coverage[0].state, CoverageState::Pass);

        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let phases = action_kinds_by_id(&build_dir);
        for (id, kind) in &phases {
            let action_dir = build_dir.join("artifacts").join(id);
            match kind.as_str() {
                "work" => {
                    // A worker's own receipt is written, never transported.
                    assert!(action_dir.join("result.json").is_file());
                    assert!(
                        !action_dir.join("transport-evidence.json").exists(),
                        "a work action took evidence from the transport"
                    );
                }
                "review" | "final_audit" => {
                    // The role wrote nothing; the controller persisted what it
                    // said, and recorded where it came from.
                    let receipt: Receipt = read_json(&action_dir.join("result.json")).unwrap();
                    assert_eq!(receipt.action_id, *id);
                    assert_eq!(
                        receipt.outcome,
                        if kind == "review" { "pass" } else { "complete" }
                    );
                    let report = fs::read_to_string(action_dir.join("report.md")).unwrap();
                    assert!(report.starts_with('#'), "{report}");
                    let provenance: serde_json::Value =
                        read_json(&action_dir.join("transport-evidence.json")).unwrap();
                    assert_eq!(provenance["schema_version"], 1);
                    let persisted = provenance["persisted"].as_array().unwrap();
                    let expected = if kind == "final_audit" { 3 } else { 2 };
                    assert_eq!(persisted.len(), expected, "{kind} persisted {persisted:#?}");
                    assert!(
                        persisted
                            .iter()
                            .any(|entry| entry["artifact"] == "result.json")
                    );
                    assert!(
                        persisted
                            .iter()
                            .any(|entry| entry["artifact"] == "report.md")
                    );
                    if kind == "final_audit" {
                        assert!(
                            action_dir.join("assessment.json").is_file(),
                            "the audit assessment was not persisted"
                        );
                    }
                }
                other => panic!("unexpected action kind {other}"),
            }
        }
        assert!(
            phases.values().any(|kind| kind == "final_audit"),
            "the Build never reached its formal Audit"
        );
        let out = temp("export-transported-role-evidence");
        let archive_path = out.join("transported.zip");
        let exported = export::export(&store, &effort, &archive_path).unwrap();
        assert!(exported.complete, "{exported:#?}");
        let members = archive::members_by_name(&archive_path).unwrap();
        for (id, kind) in &phases {
            if matches!(kind.as_str(), "review" | "final_audit") {
                for leaf in ["output-contract.md", "transport-evidence.json"] {
                    assert!(
                        members.contains_key(&format!("build/artifacts/{id}/{leaf}")),
                        "the transported {kind} evidence omitted {leaf}"
                    );
                }
            }
        }
        assert_no_build_files_leaked(&repo);
    }

    /// A response that carries no valid artifact is a receipt failure, not an
    /// outcome inferred from what the role said about itself.
    #[test]
    fn a_read_only_response_without_a_valid_receipt_stops_instead_of_being_inferred() {
        let (store, effort, repo) = prepared_with_config(
            "schema_version = 3\n[worker]\nadapter = \"cursor\"\nmodel_strength = \"standard\"\npermission = \"workspace_write\"\n[reviewer]\nadapter = \"codex\"\nmodel_strength = \"standard\"\npermission = \"read_only\"\n",
        );
        let host = ReadOnlyEvidenceHost {
            audit_without_evidence: true,
            ..ReadOnlyEvidenceHost::new()
        };
        let BuildResult::Blocked {
            detail, trigger, ..
        } = run_with_adapter(&store, request(&repo), &host).unwrap()
        else {
            panic!("a prose-only audit response was accepted as completion");
        };
        assert_eq!(trigger.as_deref(), Some("invalid_receipt"));
        assert!(detail.contains("valid Build result receipt"), "{detail}");
        let state = read_state(&store, &effort);
        assert!(state.terminal.is_none(), "a verdict was published anyway");
        assert_eq!(artifact_count(&store, &effort, ArtifactKind::Audit), 0);
    }

    /// A file the role did write stays the authority: a response block never
    /// overrides it.
    #[test]
    fn a_written_receipt_outranks_a_transported_block() {
        let (store, effort, repo) = prepared_with_config(
            "schema_version = 3\n[worker]\nadapter = \"cursor\"\nmodel_strength = \"standard\"\npermission = \"workspace_write\"\n[reviewer]\nadapter = \"codex\"\nmodel_strength = \"standard\"\npermission = \"read_only\"\n",
        );
        let host = ReadOnlyEvidenceHost {
            conflicting_reviews: std::sync::atomic::AtomicUsize::new(1),
            ..ReadOnlyEvidenceHost::new()
        };
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        // The first review wrote `changes_required` and said `pass` in a block.
        // Its written file was consumed and never overwritten, so the Build ran
        // another review: the later one is the transported pass.
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let mut reviews = Vec::new();
        for (id, kind) in action_kinds_by_id(&build_dir) {
            if kind != "review" {
                continue;
            }
            let dir = build_dir.join("artifacts").join(&id);
            let receipt: Receipt = read_json(&dir.join("result.json")).unwrap();
            reviews.push((
                receipt.outcome,
                dir.join("transport-evidence.json").exists(),
            ));
        }
        assert!(
            reviews
                .iter()
                .any(|(outcome, transported)| outcome == "changes_required" && !transported),
            "the written correction was not the one consumed: {reviews:?}"
        );
        assert!(
            reviews.len() > 1,
            "the written correction did not drive a corrected action: {reviews:?}"
        );
    }

    /// The rule that finds a block: an exact info string, a closer at least as
    /// long as its opener, and the last complete block winning.
    #[test]
    fn marked_blocks_are_found_by_fence_rules() {
        let response = "intro\n\n```orchestrate-receipt\n{\"a\":1}\n```\n\n````orchestrate-report\nfirst\n```\nstill the report\n````\n";
        assert_eq!(
            fenced_block(response, RECEIPT_BLOCK).as_deref(),
            Some("{\"a\":1}")
        );
        assert_eq!(
            fenced_block(response, REPORT_BLOCK).as_deref(),
            Some("first\n```\nstill the report")
        );
        // A shorter fence never closes a longer one, and a different info
        // string is not this block.
        assert!(fenced_block("```orchestrate-receipt-x\n{}\n```\n", RECEIPT_BLOCK).is_none());
        assert!(fenced_block("```orchestrate-receipt\n{}\n", RECEIPT_BLOCK).is_none());
        let twice =
            "```orchestrate-receipt\n{\"a\":1}\n```\n```orchestrate-receipt\n{\"a\":2}\n```\n";
        assert_eq!(
            fenced_block(twice, RECEIPT_BLOCK).as_deref(),
            Some("{\"a\":2}")
        );
    }

    fn action_kinds_by_id(build_dir: &Path) -> BTreeMap<String, String> {
        action_packets(build_dir)
            .into_iter()
            .map(|(id, packet)| (id, packet["kind"].as_str().unwrap().to_owned()))
            .collect()
    }

    /// The controller adds no provider environment of its own, so a Claude role
    /// runs against the backend the launching environment and that host's own
    /// Claude Code configuration select.
    #[test]
    fn provider_commands_inherit_the_launch_environment() {
        let root = temp("provider-environment");
        for (adapter, model) in [("claude", "deepseek-flash"), ("codex", "gpt-5.5")] {
            let request = Invocation {
                adapter: adapter.into(),
                role: "reviewer".into(),
                config: RoleConfig {
                    adapter: adapter.into(),
                    model_strength: Some("standard".into()),
                    config_schema_version: 3,
                    model: None,
                    reasoning_effort: None,
                    permission: Some("read_only".into()),
                },
                cwd: root.clone(),
                build_dir: root.clone(),
                action: root.clone(),
                session_id: None,
                prompt: "review".into(),
            };
            let command = provider_command(&request).unwrap();
            assert_eq!(
                command.get_envs().count(),
                0,
                "{adapter} dispatch overrides a provider environment variable"
            );
            assert!(
                !command
                    .get_envs()
                    .any(|(name, _)| name == std::ffi::OsStr::new("ANTHROPIC_BASE_URL")),
                "{adapter} dispatch selects a provider backend for the role"
            );
            // The frozen v2 path is unchanged: a native model name still maps.
            let legacy = Invocation {
                config: RoleConfig {
                    adapter: adapter.into(),
                    model_strength: None,
                    config_schema_version: 2,
                    model: Some(model.into()),
                    reasoning_effort: None,
                    permission: None,
                },
                ..request
            };
            let command = provider_command(&legacy).unwrap();
            assert!(
                command.get_args().any(|argument| argument == model),
                "the legacy {adapter} model no longer reaches the provider"
            );
            assert_eq!(command.get_envs().count(), 0);
        }
    }

    /// `full_access` is a provider-neutral permission an operator must name,
    /// mapped at each adapter's edge and never widened into automatically.
    #[test]
    fn full_access_is_explicit_and_maps_at_every_adapter_edge() {
        let mapped = |adapter: &str| {
            let config = RoleConfig {
                adapter: adapter.into(),
                model_strength: None,
                config_schema_version: 3,
                model: None,
                reasoning_effort: None,
                permission: Some("full_access".into()),
            };
            validate_role("worker", &config, 3).unwrap();
            attribute_arguments_for("worker", &config).unwrap()
        };
        assert_eq!(
            mapped("codex"),
            vec!["--dangerously-bypass-approvals-and-sandbox".to_string()]
        );
        assert_eq!(
            mapped("claude"),
            vec!["--dangerously-skip-permissions".to_string()]
        );
        assert_eq!(mapped("cursor"), vec!["--force".to_string()]);
        // A narrow permission that cannot meet the commit contract is still
        // refused rather than reinterpreted as unrestricted access.
        let narrow = RoleConfig {
            adapter: "codex".into(),
            model_strength: Some("standard".into()),
            config_schema_version: 3,
            model: None,
            reasoning_effort: None,
            permission: Some("workspace_write".into()),
        };
        let refusal = validate_role("worker", &narrow, 3).unwrap_err().to_string();
        for fact in ["workspace_write", "commit contract", "full_access"] {
            assert!(refusal.contains(fact), "{refusal} does not name {fact}");
        }
        assert!(parse_config(
            "schema_version = 3\n[worker]\nadapter = \"codex\"\npermission = \"everything\"\n[reviewer]\nadapter = \"codex\"\n"
        )
        .is_err());
        // The same mapping reaches a fresh and a resumed dispatch, and it is
        // what static preflight reports.
        let config = RoleConfig {
            adapter: "codex".into(),
            model_strength: Some("standard".into()),
            config_schema_version: 3,
            model: None,
            reasoning_effort: None,
            permission: Some("full_access".into()),
        };
        for session in [None, Some("same-session")] {
            let request = Invocation {
                adapter: "codex".into(),
                role: "worker".into(),
                config: config.clone(),
                cwd: PathBuf::from("/tmp"),
                build_dir: PathBuf::from("/tmp"),
                action: PathBuf::from("/tmp"),
                session_id: session.map(str::to_owned),
                prompt: "work".into(),
            };
            assert!(
                provider_arguments(&request)
                    .unwrap()
                    .contains(&"--dangerously-bypass-approvals-and-sandbox".to_owned())
            );
        }
        let provenance = attribute_provenance(&config);
        assert_eq!(provenance["permission"]["configured"], "full_access");
    }

    /// Run one simple Build to completion and return its state.
    fn completed_build(name: &str) -> (Store, Effort, PathBuf) {
        let root = temp(name);
        let repo = fresh_repo("repo");
        let store = Store::open(&root).unwrap();
        let effort = store
            .init_effort(&repo, "build", RequestKind::Freeform, "work".into(), vec![])
            .unwrap();
        let reconciled_ref = publish_reconciled(&store, &effort, "test");
        prepare_build_files(&store, &effort, &reconciled_ref, None);
        let host = ScenarioHost::new(Scenario::Basic);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        (store, effort, repo)
    }

    /// The historical shape: a schema-3 state written before `migration_version`
    /// and the additive fields existed, whose action was accepted and never
    /// completed, with no durable stop to resolve.  The supported sequence
    /// migrates it, records the uncertain boundary, continues the interrupted
    /// scope once after confirmation, and repeats nothing when recovery is run
    /// again.
    #[test]
    fn a_pre_migration_interrupted_state_migrates_resolves_and_continues_exactly_once() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let plan = load_plan(&store, &effort, &build_dir).unwrap();
        let initial = initialize_state(&store, &effort, &build_dir, &plan).unwrap();
        let interrupted_id = initial.action.id.clone();
        // The interrupted action's own partial records, exactly as history keeps
        // them: a transport that began and a result that never arrived.
        let action_dir = build_dir.join("artifacts").join(&interrupted_id);
        fs::create_dir_all(&action_dir).unwrap();
        let partial = b"{\"type\":\"item.started\",\"item\":{\"type\":\"command_execution\"}}\n";
        fs::write(action_dir.join("transport.jsonl"), partial).unwrap();

        let state_path = build_state_path(&store, &effort);
        rewrite_state(&state_path, |state| {
            let object = state.as_object_mut().unwrap();
            for field in [
                "migration_version",
                "stop",
                "stop_history",
                "current_audit",
                "applied_resolution_id",
                "successor_of",
                "checkouts",
                "once_over",
                "phase_reviews",
                "cleanup",
            ] {
                object.remove(field);
            }
            // Phase one is accepted; the second phase was in flight.
            state["phase_index"] = json!(1);
            state["action"]["scope"] = json!("D2");
            state["action"]["dispatch"] = json!("running");
            state["action"]["transport_session_id"] = json!("historical-worker-session");
            state["sessions"] = json!({"worker": "historical-worker-session"});
        });
        let historical = fs::read(&state_path).unwrap();
        assert!(
            serde_json::from_slice::<serde_json::Value>(&historical).unwrap()["migration_version"]
                .is_null(),
            "the fixture is not in the pre-migration shape"
        );

        // One start records the uncertain boundary; it dispatches nothing.
        let host = ScenarioHost::new(Scenario::Basic);
        let BuildResult::Blocked { trigger, .. } =
            run_with_adapter(&store, request(&repo), &host).unwrap()
        else {
            panic!("an accepted-but-uncertain action did not stop the Build");
        };
        assert_eq!(trigger.as_deref(), Some("uncertain_acceptance"));
        assert_eq!(
            host.call_count("work"),
            0,
            "the uncertain action was resent"
        );
        assert!(!action_dir.join("transport.completed").exists());
        // Migration preserved the original bytes exactly once and kept the
        // frozen identities and the interrupted action's partial records.
        assert_eq!(
            fs::read(build_dir.join("evidence").join("state-v3-original.json")).unwrap(),
            historical
        );
        assert_eq!(journal_count(&store, &effort, "build_state_migrated"), 1);
        assert_eq!(
            fs::read(action_dir.join("transport.jsonl")).unwrap(),
            partial
        );
        let migrated = read_state(&store, &effort);
        assert_eq!(migrated.migration_version, BUILD_STATE_MIGRATION);
        assert_eq!(migrated.frozen.plan_digest, initial.frozen.plan_digest);
        assert_eq!(migrated.frozen.config_digest, initial.frozen.config_digest);
        assert_eq!(migrated.frozen.reconciled, initial.frozen.reconciled);
        assert_eq!(migrated.phase_index, 1, "an accepted phase was replayed");
        assert!(
            migrated.stop.is_some(),
            "read_state lost the durable stop; on disk: {}",
            fs::read_to_string(&state_path).unwrap()
        );
        let stop = stop_of(&store, &effort);
        assert_eq!(stop.action.id, interrupted_id);
        assert_eq!(
            stop.process_completion,
            "accepted; provider completion is uncertain"
        );

        // The refused-then-accepted sequence: confirmation is what authorizes a
        // continuation, and repeating an identical resolution is idempotent.
        assert!(
            resolve_stop(
                &store,
                &effort,
                ResolutionKind::ExistingAuthorityClarification,
                "the role already had this authority",
                false,
                None,
                Vec::new(),
            )
            .unwrap_err()
            .to_string()
            .contains("confirm the provider")
        );
        let first = resolve_stop(
            &store,
            &effort,
            ResolutionKind::ExistingAuthorityClarification,
            "the role already had this authority",
            true,
            None,
            Vec::new(),
        )
        .unwrap();
        let ResolutionOutcome::Resolved {
            resolution_id,
            continuation_action,
            continuation_kind,
            stopped_action,
            ..
        } = first.clone()
        else {
            panic!("the resolution did not continue the Build");
        };
        assert_eq!(stopped_action, interrupted_id);
        assert_ne!(continuation_action, interrupted_id);
        assert_eq!(continuation_kind, "work");
        // Repeating recovery names the same stopped action, not the stop the
        // applied transition already cleared, and changes nothing.
        let repeated = resolve_action(
            &store,
            &effort,
            &interrupted_id,
            ResolutionKind::ExistingAuthorityClarification,
            "the role already had this authority",
            true,
            None,
            Vec::new(),
        )
        .unwrap();
        let ResolutionOutcome::Resolved {
            resolution_id: repeated_id,
            continuation_action: repeated_action,
            ..
        } = repeated
        else {
            panic!("repeating an identical resolution changed its outcome");
        };
        assert_eq!(repeated_id, resolution_id);
        assert_eq!(repeated_action, continuation_action);
        assert_eq!(resolution_files(&store, &effort).len(), 1);
        assert_eq!(journal_count(&store, &effort, "build_resolved"), 1);
        let continued = read_state(&store, &effort);
        assert_eq!(continued.action.id, continuation_action);
        assert_eq!(continued.action.scope, "D2");
        assert_eq!(continued.action.dispatch, DispatchState::Prepared);

        // The continuation is a distinct action carrying the interrupted
        // predecessor's exact facts, and the Build reaches its acceptance
        // boundary without repeating the accepted phase.
        let completed = run_with_adapter(&store, request(&repo), &host).unwrap();
        assert!(
            matches!(completed, BuildResult::Completed(_)),
            "the continuation did not reach acceptance: {completed:?}"
        );
        assert_eq!(host.call_scopes("work"), vec!["D2"]);
        assert_eq!(
            git(&repo, ["rev-list", "--count", "HEAD"]).unwrap(),
            "2",
            "the fixture should hold the baseline and the continued work only"
        );
        assert_eq!(journal_count(&store, &effort, "build_resolved"), 1);
        assert_eq!(journal_count(&store, &effort, "build_state_migrated"), 1);
        // A completed Build dispatches nothing more and adds no resolution.
        let before = fs::read(&state_path).unwrap();
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        assert_eq!(fs::read(&state_path).unwrap(), before);
        assert_eq!(resolution_files(&store, &effort).len(), 1);
        assert_no_build_files_leaked(&repo);
    }

    /// R-026/R-027: cleanup removes only owned generated products from inactive
    /// owned checkouts, is dry-runnable and idempotent, and preserves source,
    /// unknown files and everything it does not own.
    #[test]
    fn cleanup_is_owned_scoped_dry_runnable_and_idempotent() {
        let (store, effort, repo) = completed_build("cleanup");
        let state = read_state(&store, &effort);
        assert!(
            !state.checkouts.is_empty(),
            "the controller recorded no checkout ownership"
        );
        let mut planted = Vec::new();
        for record in &state.checkouts {
            let checkout = PathBuf::from(&record.path);
            let target = checkout.join("target");
            fs::create_dir_all(target.join("debug/deps")).unwrap();
            fs::write(
                target.join("debug/deps/libfixture-0123456789abcdef.rlib"),
                vec![7u8; 4096],
            )
            .unwrap();
            fs::write(checkout.join("user-note.txt"), "user-owned\n").unwrap();
            planted.push(checkout);
        }
        // A directory merely named `target` outside controller ownership — here
        // inside the product checkout itself — is never eligible.
        let product_target = repo.join("target");
        fs::create_dir_all(&product_target).unwrap();
        fs::write(product_target.join("product.bin"), b"keep").unwrap();

        let dry = cleanup::cleanup(&store, &effort, true).unwrap();
        assert!(dry.dry_run && dry.complete);
        assert!(dry.reclaimed_bytes > 0, "dry run reclaimed nothing");
        assert!(
            dry.removed
                .iter()
                .all(|removal| removal.status == "would_remove")
        );
        for checkout in &planted {
            assert!(
                checkout.join("target").exists(),
                "dry run removed something"
            );
        }

        let outcome = cleanup::cleanup(&store, &effort, false).unwrap();
        assert!(outcome.complete, "{outcome:#?}");
        assert!(!outcome.removed.is_empty());
        for checkout in &planted {
            assert!(
                !checkout.join("target").exists(),
                "generated products remain"
            );
            assert!(
                checkout.join("user-note.txt").is_file(),
                "unknown file was removed"
            );
            assert!(
                git(checkout, ["rev-parse", "HEAD"]).is_ok(),
                "the checkout's history was disturbed"
            );
            assert!(
                checkout.join(".git").exists(),
                "the checkout lost its Git metadata"
            );
        }
        assert!(
            product_target.join("product.bin").is_file(),
            "the product checkout was touched"
        );

        // Repeated cleanup is harmless.
        let again = cleanup::cleanup(&store, &effort, false).unwrap();
        assert!(again.removed.is_empty());
        assert!(again.complete);

        // A recorded checkout whose HEAD moved is skipped, not cleaned.
        let record = state.checkouts[0].clone();
        let checkout = PathBuf::from(&record.path);
        fs::create_dir_all(checkout.join("target")).unwrap();
        fs::write(checkout.join("target/marker.bin"), b"x").unwrap();
        rewrite_state(&build_state_path(&store, &effort), |state| {
            state["checkouts"][0]["commit"] = serde_json::json!("0".repeat(40));
        });
        let moved = cleanup::cleanup(&store, &effort, true).unwrap();
        assert!(
            moved
                .skipped
                .iter()
                .any(|skip| skip.reason.contains("moved")),
            "a moved checkout was not skipped: {moved:#?}"
        );
        assert!(checkout.join("target/marker.bin").exists());
        // The recorded attempt is visible in state and the journal.
        let state = read_state(&store, &effort);
        assert!(state.cleanup.is_some());
        assert!(journal_count(&store, &effort, "build_cleanup") >= 3);
    }

    /// R-026/R-027: a repository that tracks files under a product directory
    /// name keeps them; nothing tracked is ever removed.
    #[test]
    fn cleanup_never_removes_tracked_files_under_a_product_directory() {
        let root = temp("cleanup-tracked-store");
        let repo = temp("cleanup-tracked-repo");
        git_ok(&repo, &["init"]);
        git_ok(&repo, &["config", "user.email", "test@example.com"]);
        git_ok(&repo, &["config", "user.name", "Test"]);
        fs::create_dir_all(repo.join("target")).unwrap();
        fs::write(repo.join("target/schema.sql"), "create table listings;\n").unwrap();
        fs::write(repo.join("source.txt"), "baseline\n").unwrap();
        git_ok(&repo, &["add", "."]);
        git_ok(&repo, &["commit", "-m", "tracked product directory"]);
        let store = Store::open(&root).unwrap();
        let effort = store
            .init_effort(&repo, "build", RequestKind::Freeform, "work".into(), vec![])
            .unwrap();
        let reconciled_ref = publish_reconciled(&store, &effort, "test");
        prepare_build_files(&store, &effort, &reconciled_ref, None);
        let host = ScenarioHost::new(Scenario::Basic);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let state = read_state(&store, &effort);
        assert!(!state.checkouts.is_empty());
        for record in &state.checkouts {
            let checkout = PathBuf::from(&record.path);
            // Generated output planted beside the tracked file.
            fs::create_dir_all(checkout.join("target/debug")).unwrap();
            fs::write(checkout.join("target/debug/generated.bin"), b"product").unwrap();
        }
        let outcome = cleanup::cleanup(&store, &effort, false).unwrap();
        assert!(
            outcome
                .skipped
                .iter()
                .any(|skip| skip.reason.contains("tracks files under target")),
            "a checkout with tracked product files was not skipped: {outcome:#?}"
        );
        for record in &state.checkouts {
            let checkout = PathBuf::from(&record.path);
            assert!(
                checkout.join("target/schema.sql").is_file(),
                "tracked source was removed"
            );
            assert!(
                checkout.join("target/debug/generated.bin").is_file(),
                "the ambiguous product directory was partially removed"
            );
        }
    }

    /// R-026/R-027: a symlinked target root is rejected before traversal in
    /// both dry-run and destructive cleanup, even when its external target has
    /// a familiar Cargo profile directory and ordinary debug files.
    #[test]
    fn cleanup_skips_symlinked_product_roots_without_touching_external_bytes() {
        let (store, effort, _repo) = completed_build("cleanup-symlink-root");
        let state = read_state(&store, &effort);
        let checkout = PathBuf::from(&state.checkouts[0].path);
        let external = temp("cleanup-symlink-external");
        fs::create_dir_all(external.join("debug")).unwrap();
        fs::write(
            external.join("debug/user-notes.txt"),
            b"private external notes\0\xff",
        )
        .unwrap();
        fs::write(external.join("sentinel.bin"), [0, 1, 2, 255]).unwrap();
        let before_notes = fs::read(external.join("debug/user-notes.txt")).unwrap();
        let before_sentinel = fs::read(external.join("sentinel.bin")).unwrap();
        symlink::symlink_dir(&external, checkout.join("target")).unwrap();

        let dry = cleanup::cleanup(&store, &effort, true).unwrap();
        assert!(
            dry.skipped.iter().any(|skip| {
                skip.reason.contains("product root") && skip.reason.contains("symlink")
            }),
            "dry-run did not explain the skipped symlink root: {dry:#?}"
        );
        assert_eq!(
            fs::read(external.join("debug/user-notes.txt")).unwrap(),
            before_notes
        );
        assert_eq!(
            fs::read(external.join("sentinel.bin")).unwrap(),
            before_sentinel
        );

        let actual = cleanup::cleanup(&store, &effort, false).unwrap();
        assert!(
            actual.skipped.iter().any(|skip| {
                skip.reason.contains("product root") && skip.reason.contains("symlink")
            }),
            "cleanup did not explain the skipped symlink root: {actual:#?}"
        );
        assert_eq!(
            fs::read(external.join("debug/user-notes.txt")).unwrap(),
            before_notes
        );
        assert_eq!(
            fs::read(external.join("sentinel.bin")).unwrap(),
            before_sentinel
        );
        assert!(
            fs::symlink_metadata(checkout.join("target"))
                .unwrap()
                .file_type()
                .is_symlink()
        );
    }

    /// R-026/R-027: a product directory is cleaned entry by entry, so a fixture
    /// the action's own evidence cites, and any file the controller cannot
    /// identify as generated Cargo output, survive a cleanup that still
    /// reclaims the output beside them.
    #[test]
    fn cleanup_preserves_cited_evidence_and_unknown_files_inside_product_directories() {
        let (store, effort, _repo) = completed_build("cleanup-evidence");
        let state = read_state(&store, &effort);
        let record = state.checkouts[0].clone();
        let checkout = PathBuf::from(&record.path);
        let target = checkout.join("target");
        // Genuine generated output, a generated directory that also holds a
        // cited fixture, and an unknown user file.
        fs::create_dir_all(target.join("debug/deps")).unwrap();
        fs::write(
            target.join("debug/deps/libfixture-0123456789abcdef.rlib"),
            vec![5u8; 3072],
        )
        .unwrap();
        fs::create_dir_all(target.join("debug/fixtures")).unwrap();
        fs::write(target.join("debug/fixtures/case.json"), br#"{"kept":true}"#).unwrap();
        fs::write(
            target.join("CACHEDIR.TAG"),
            b"Signature: 8a477f597d28d172789f06886806bc55\n",
        )
        .unwrap();
        fs::write(target.join("user-notes.txt"), b"not generated output\n").unwrap();
        fs::write(
            target.join("debug/user-notes.txt"),
            b"nested unknown bytes\0\xff",
        )
        .unwrap();
        // The action's own durable report cites the fixture inside the tree.
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        fs::write(
            build_dir
                .join("artifacts")
                .join(&record.action_id)
                .join("report.md"),
            "the regression fixture for this phase lives in target/debug/fixtures/case.json\n",
        )
        .unwrap();

        let dry = cleanup::cleanup(&store, &effort, true).unwrap();
        assert!(
            dry.preserved
                .iter()
                .any(|kept| kept.path.ends_with("user-notes.txt")
                    && kept.reason.contains("refuses to guess")),
            "the unknown file was not reported as preserved: {dry:#?}"
        );
        assert!(
            !dry.complete,
            "an ambiguous product directory was reported as a clean checkout"
        );
        assert!(
            dry.removed
                .iter()
                .any(|removal| removal.path.ends_with("CACHEDIR.TAG"))
        );
        assert!(
            target.join("CACHEDIR.TAG").exists(),
            "the dry run removed something"
        );

        let outcome = cleanup::cleanup(&store, &effort, false).unwrap();
        assert!(!outcome.complete, "{outcome:#?}");
        // Cited evidence stays readable, with its bytes intact.
        assert_eq!(
            fs::read(target.join("debug/fixtures/case.json")).unwrap(),
            br#"{"kept":true}"#
        );
        assert!(
            outcome
                .preserved
                .iter()
                .any(|kept| kept.path.ends_with("debug/fixtures")
                    && kept
                        .reason
                        .contains("cited by this action's durable records")),
            "the cited fixture was removed: {outcome:#?}"
        );
        // The unknown file, its parent and the un-cited generated entry behave
        // exactly as reported.
        assert!(
            target.join("user-notes.txt").is_file(),
            "an unknown file was removed"
        );
        assert_eq!(
            fs::read(target.join("debug/user-notes.txt")).unwrap(),
            b"nested unknown bytes\0\xff",
            "an unknown nested file was removed or changed"
        );
        assert!(
            outcome.preserved.iter().any(|kept| {
                kept.path.ends_with("debug/user-notes.txt")
                    && kept.reason.contains("refuses to guess")
            }),
            "the uncited nested file was not explicitly preserved: {outcome:#?}"
        );
        assert!(
            !target.join("CACHEDIR.TAG").exists(),
            "a known generated entry survived"
        );
        // Repeated cleanup is harmless and reports the same preserved content.
        let again = cleanup::cleanup(&store, &effort, false).unwrap();
        assert!(again.removed.is_empty());
        assert_eq!(again.preserved.len(), outcome.preserved.len());
    }

    /// R-027: legacy or unrecognized contents are skipped and reported rather
    /// than classified as disposable because they sit under a product name.
    #[test]
    fn cleanup_skips_ambiguous_legacy_content_instead_of_deleting_it() {
        let (store, effort, _repo) = completed_build("cleanup-legacy");
        let state = read_state(&store, &effort);
        let record = state.checkouts[0].clone();
        let checkout = PathBuf::from(&record.path);
        let target = checkout.join("target");
        // Only contents the controller cannot identify: an unknown profile and a
        // retained research directory.
        fs::create_dir_all(target.join("custom-profile/deps")).unwrap();
        fs::write(target.join("custom-profile/deps/lib.rlib"), b"unknown").unwrap();
        fs::write(target.join("old-results.json"), b"{\"legacy\":true}").unwrap();
        let outcome = cleanup::cleanup(&store, &effort, false).unwrap();
        assert!(!outcome.complete);
        assert!(
            outcome.removed.is_empty(),
            "ambiguous content was removed: {outcome:#?}"
        );
        assert!(target.join("custom-profile/deps/lib.rlib").is_file());
        assert!(target.join("old-results.json").is_file());
        assert_eq!(outcome.preserved.len(), 2, "{outcome:#?}");
        assert!(
            outcome
                .preserved
                .iter()
                .all(|kept| kept.reason.contains("refuses to guess"))
        );
    }

    /// R-027: a cleanup failure is recorded separately and leaves the Build's
    /// stop and acceptance untouched.
    /// R-028/R-029/R-030/R-031: export selects members from source state before
    /// traversal, never stages excluded classes, verifies the archive by reading
    /// it back, and reports collection independently of the Build outcome.
    #[test]
    fn export_selects_first_and_verifies_before_promotion() {
        let (store, effort, _repo) = completed_build("export");
        let state = read_state(&store, &effort);
        // A bulky unreadable excluded tree must never be staged or read.
        let mut huge = None;
        if let Some(record) = state.checkouts.first() {
            let path = PathBuf::from(&record.path).join("target/unreadable");
            fs::create_dir_all(&path).unwrap();
            fs::write(path.join("huge.bin"), vec![0u8; 1024]).unwrap();
            huge = Some(path);
        }
        let out = temp("export-out");
        let archive = out.join("effort.zip");
        let outcome = export::export(&store, &effort, &archive).unwrap();
        assert!(
            outcome.complete,
            "export was not complete: {:#?}",
            outcome
                .members
                .iter()
                .filter(|member| member.class != export::MemberClass::Copied)
                .collect::<Vec<_>>()
        );
        let promoted = outcome.archive.clone().expect("no archive was promoted");
        assert_eq!(promoted, archive);
        assert!(archive.is_file());
        let members = archive::members_by_name(&archive).unwrap();
        for expected in [
            "effort/effort.json",
            "effort/journal.jsonl",
            "build/state.json",
            "build/plan.json",
            "build/config.toml",
            "build/instructions/work.md",
            "git-evidence/checkout.json",
        ] {
            assert!(
                members.contains_key(expected),
                "the archive omits {expected}"
            );
        }
        assert!(
            members
                .keys()
                .any(|name| name.starts_with("build/artifacts/") && name.ends_with("action.json")),
            "no action packet reached the archive"
        );
        assert!(
            members.keys().any(|name| name.starts_with("artifacts/")),
            "no published bundle reached the archive"
        );
        // No excluded class was followed into the archive.
        for name in members.keys() {
            for excluded in [
                "/target/",
                "/scratch/",
                "/source/",
                "/verification/",
                "/once-over/",
            ] {
                assert!(
                    !name.contains(excluded),
                    "an excluded tree was staged: {name}"
                );
            }
        }
        // Hashes are verified by reading members back, not by CRC alone.
        let verification = &outcome.verification;
        assert_eq!(verification["missing"], serde_json::json!([]));
        assert_eq!(verification["hash_mismatches"], serde_json::json!([]));
        let state_member = members.get("build/state.json").unwrap();
        let (bytes, digest) = archive::read_member(&archive, state_member).unwrap();
        assert_eq!(
            digest,
            digest_bytes(&fs::read(build_state_path(&store, &effort)).unwrap())
        );
        assert!(!bytes.is_empty());
        if let Some(huge) = huge {
            assert!(huge.exists(), "an excluded tree was read or removed");
        }
        // The export wrote nothing into the Build's own state.
        assert!(read_state(&store, &effort).stop.is_none());
    }

    /// R-030: a member that changes while it is collected is classified as
    /// changed and prevented from producing a complete result.
    #[test]
    fn a_member_changing_during_collection_is_classified_not_completed() {
        let file = temporary_file("export-changed");
        fs::write(&file, b"before").unwrap();
        let metadata = fs::metadata(&file).unwrap();
        let bytes = fs::read(&file).unwrap();
        assert!(!export::changed_during_collection(
            metadata.modified().ok(),
            Some(&metadata),
            bytes.len() as u64
        ));
        // The same member with different bytes or a different timestamp is a change.
        assert!(export::changed_during_collection(
            metadata.modified().ok(),
            Some(&metadata),
            bytes.len() as u64 + 1
        ));
        fs::write(&file, b"after the collection started").unwrap();
        let later = fs::metadata(&file).unwrap();
        assert!(export::changed_during_collection(
            metadata.modified().ok(),
            Some(&later),
            bytes.len() as u64
        ));
        // A member that disappeared after being read is a change too.
        assert!(export::changed_during_collection(
            metadata.modified().ok(),
            None,
            bytes.len() as u64
        ));
        fs::remove_file(&file).unwrap();
    }

    fn temporary_file(name: &str) -> PathBuf {
        temp(name).join("member.bin")
    }

    /// R-030: an unreadable required member, a changed source file and a symlink
    /// each produce explicit incomplete status, a nonzero-class result and no
    /// complete archive.
    #[test]
    fn export_reports_incomplete_collection_without_promoting() {
        // A vanished required member.
        let (store, effort, _repo) = completed_build("export-missing");
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        fs::remove_file(build_dir.join("plan.json")).unwrap();
        let out = temp("export-out");
        let archive = out.join("missing.zip");
        let outcome = export::export(&store, &effort, &archive).unwrap();
        assert!(!outcome.complete);
        assert_eq!(outcome.export_status, "failed");
        assert!(outcome.archive.is_none());
        assert!(!archive.exists(), "an incomplete export was promoted");
        assert!(outcome.partial.exists(), "the partial archive was not kept");
        assert!(
            outcome
                .members
                .iter()
                .any(|member| member.name == "build/plan.json"
                    && member.class == export::MemberClass::Failed)
        );

        // A legitimately absent member is disclosed without failing collection.
        let (store, effort, _repo) = completed_build("export-absent");
        let out = temp("export-out");
        let archive = out.join("absent.zip");
        let outcome = export::export(&store, &effort, &archive).unwrap();
        let absent = outcome
            .members
            .iter()
            .filter(|member| member.class == export::MemberClass::Absent)
            .collect::<Vec<_>>();
        assert!(
            !absent.is_empty(),
            "a completed run should still disclose optional members it never wrote"
        );
        assert!(absent.iter().all(|member| {
            member
                .detail
                .as_deref()
                .is_some_and(|detail| detail.contains("absent"))
        }));
        assert!(outcome.complete);

        // A plan that exists but does not parse cannot name its detailed plan;
        // that is disclosed instead of silently exporting a partial inventory.
        let (store, effort, _repo) = completed_build("export-corrupt-plan");
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        fs::write(build_dir.join("plan.json"), b"{ this is not a plan").unwrap();
        let out = temp("export-out");
        let archive = out.join("corrupt-plan.zip");
        let outcome = export::export(&store, &effort, &archive).unwrap();
        assert!(
            !outcome.complete,
            "a corrupt plan produced a complete export"
        );
        assert!(outcome.archive.is_none());
        assert!(
            outcome
                .omissions
                .iter()
                .any(|omission| omission.contains("detailed implementation plan")),
            "the unreadable plan was not disclosed: {:#?}",
            outcome.omissions
        );

        // A symlinked member is refused rather than followed.
        let (store, effort, _repo) = completed_build("export-symlink");
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let outside = temp("export-outside");
        fs::write(outside.join("secret.txt"), "unrelated\n").unwrap();
        let link = build_dir.join("evidence");
        fs::create_dir_all(&link).unwrap();
        symlink::symlink_file(outside.join("secret.txt"), link.join("linked.json")).unwrap();
        let out = temp("export-out");
        let archive = out.join("symlink.zip");
        let outcome = export::export(&store, &effort, &archive).unwrap();
        assert!(!outcome.complete);
        assert!(outcome.archive.is_none());
        assert!(
            outcome
                .members
                .iter()
                .any(|member| member.name.ends_with("linked.json")
                    && member.class == export::MemberClass::Failed)
        );
        let mut archived = vec![];
        if outcome.partial.exists() {
            archived = archive::central_directory(&outcome.partial).unwrap();
        }
        assert!(
            archived
                .iter()
                .all(|member| !member.name.ends_with("linked.json")),
            "a symlink was followed into the archive"
        );
    }

    /// R-031: a symlinked directory component is refused before it is traversed
    /// or read, so unrelated files outside the effort can never reach the
    /// archive — not merely a symlinked leaf file.
    #[test]
    fn export_refuses_symlinked_directories_that_escape_the_effort() {
        let (store, effort, _repo) = completed_build("export-dir-escape");
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let outside = temp("export-dir-outside");
        fs::create_dir_all(outside.join("nested")).unwrap();
        fs::write(outside.join("secret.txt"), "unrelated\n").unwrap();
        fs::write(outside.join("nested/other.json"), "{}\n").unwrap();
        // An unreadable file proves the outside tree is not even walked.
        fs::write(outside.join("unreadable.bin"), vec![0u8; 512]).unwrap();
        // (a) a controller record directory that is a link out of the effort,
        let evidence = build_dir.join("evidence");
        if evidence.exists() {
            fs::remove_dir_all(&evidence).unwrap();
        }
        symlink::symlink_dir(&outside, &evidence).unwrap();
        // (b) an action directory that is a link out of the effort,
        let escaped_action = "act-1000000000000-1-1";
        symlink::symlink_dir(&outside, build_dir.join("artifacts").join(escaped_action)).unwrap();
        // (c) the generated instruction directory that is a link out of the effort.
        let instructions = build_dir.join("instructions");
        if instructions.is_dir() {
            fs::remove_dir_all(&instructions).unwrap();
        }
        symlink::symlink_dir(&outside, &instructions).unwrap();

        let out = temp("export-dir-escape-out");
        let archive = out.join("escaped.zip");
        let outcome = export::export(&store, &effort, &archive).unwrap();
        assert!(!outcome.complete, "an escape produced a complete export");
        assert!(outcome.archive.is_none());
        assert!(!archive.exists());
        for (name, source) in [
            ("build/evidence", "the evidence directory"),
            (
                &format!("build/artifacts/{escaped_action}/result.json"),
                "result.json",
            ),
            ("build/instructions/work.md", "work.md"),
        ] {
            let member = outcome
                .members
                .iter()
                .find(|member| member.name == name)
                .unwrap_or_else(|| panic!("no member {name} was selected"));
            assert_eq!(member.class, export::MemberClass::Failed, "{member:#?}");
            assert!(
                member
                    .detail
                    .as_deref()
                    .unwrap_or_default()
                    .contains("outside"),
                "{source} escaped without disclosure: {member:#?}"
            );
        }
        // Nothing from the outside tree reached even the partial archive.
        let archived = archive::central_directory(&outcome.partial).unwrap();
        for member in &archived {
            assert!(
                !member.name.contains("secret.txt")
                    && !member.name.contains("other.json")
                    && !member.name.contains("unreadable.bin"),
                "an escaped file was archived: {}",
                member.name
            );
        }
        assert!(outside.join("secret.txt").is_file());
        assert_eq!(
            fs::read(outside.join("nested/other.json")).unwrap(),
            b"{}\n",
            "the escaped tree was modified"
        );

        // The contrast: a link that resolves inside the effort is legitimate and
        // is still collected by its canonical content.
        let (store, effort, _repo) = completed_build("export-contained-link");
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let real = build_dir.join("contained-records");
        fs::create_dir_all(&real).unwrap();
        fs::write(real.join("record.json"), "{\"contained\":true}").unwrap();
        let link = build_dir.join("evidence");
        if link.exists() {
            fs::remove_dir_all(&link).unwrap();
        }
        symlink::symlink_dir(&real, &link).unwrap();
        let out = temp("export-contained-out");
        let archive = out.join("contained.zip");
        let outcome = export::export(&store, &effort, &archive).unwrap();
        assert!(outcome.complete, "{outcome:#?}");
        let archived = archive::members_by_name(&archive).unwrap();
        let member = archived
            .get("build/evidence/record.json")
            .expect("a link that stays inside the effort was refused");
        let (bytes, _) = archive::read_member(&archive, member).unwrap();
        assert_eq!(bytes, "{\"contained\":true}".as_bytes());
    }

    /// R-031/R-042: an export owns its scratch paths exclusively.  Files under
    /// similar names that already existed belong to someone else and are
    /// neither removed nor overwritten, and a failed export keeps only its own
    /// partial archive.
    #[test]
    fn export_owns_its_scratch_paths_and_never_removes_unrelated_files() {
        let (store, effort, _repo) = completed_build("export-owned-scratch");
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let out = temp("export-owned-out");
        let archive = out.join("evidence.zip");
        let foreign_partial = out.join("evidence.partial");
        let foreign_staging = out.join("evidence.evidence-staging");
        fs::write(&foreign_partial, "someone else's bytes\n").unwrap();
        fs::create_dir_all(foreign_staging.join("nested")).unwrap();
        fs::write(foreign_staging.join("nested/keep.txt"), "not mine\n").unwrap();

        let outcome = export::export(&store, &effort, &archive).unwrap();
        assert!(outcome.complete, "{outcome:#?}");
        assert_eq!(
            fs::read_to_string(&foreign_partial).unwrap(),
            "someone else's bytes\n",
            "an unrelated partial file was removed or overwritten"
        );
        assert!(
            foreign_staging.join("nested/keep.txt").is_file(),
            "an unrelated staging directory was removed"
        );
        assert!(
            outcome
                .leftovers
                .iter()
                .any(|leftover| leftover.contains("evidence.partial")),
            "the untouched foreign scratch paths were not disclosed: {outcome:#?}"
        );
        // This operation's own scratch directory is gone; only the promoted
        // archive remains of its outputs.
        assert!(
            !outcome.partial.exists(),
            "the promoted partial was left behind"
        );
        assert!(archive.is_file());

        // A second export to the same output owns a different partial path and
        // also completes, so two exports cannot delete each other's scratch.
        let second = export::export(&store, &effort, &archive).unwrap();
        assert!(second.complete);
        assert_ne!(second.partial, outcome.partial);

        // Concurrent exports to one output each own their paths; neither removes
        // the other's files, and both promote a verifiable archive.
        std::thread::scope(|scope| {
            let threads = (0..2)
                .map(|_| scope.spawn(|| export::export(&store, &effort, &archive)))
                .collect::<Vec<_>>();
            for thread in threads {
                let outcome = thread.join().unwrap().unwrap();
                assert!(outcome.complete, "a concurrent export failed: {outcome:#?}");
            }
        });
        assert_eq!(
            fs::read_to_string(&foreign_partial).unwrap(),
            "someone else's bytes\n"
        );

        // A failed export keeps its own partial evidence, removes only its own
        // staging directory, and still leaves the foreign files alone.
        let failed_archive = out.join("failed.zip");
        fs::remove_file(build_dir.join("plan.json")).unwrap();
        let failed = export::export(&store, &effort, &failed_archive).unwrap();
        assert!(!failed.complete);
        assert!(
            failed.partial.is_file(),
            "the failed attempt lost its own partial"
        );
        assert!(!failed_archive.exists());
        assert!(
            !failed.partial.with_extension("").exists() || !failed.partial.is_dir(),
            "the failed attempt left its staging directory behind"
        );
        assert!(foreign_staging.join("nested/keep.txt").is_file());
        assert_eq!(
            fs::read_to_string(&foreign_partial).unwrap(),
            "someone else's bytes\n"
        );
    }

    /// R-029/R-030: required members come from the controller's own writer
    /// obligations.  A completed action that lost its packet or its receipt is
    /// lost evidence; an interrupted or older action that never wrote a record
    /// is disclosed as that absence instead.
    #[test]
    fn export_classifies_lost_and_legitimately_absent_action_records() {
        // A completed action that lost its receipt is lost evidence.
        let (store, effort, _repo) = completed_build("export-lost-receipt");
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let damaged = fs::read_dir(build_dir.join("artifacts"))
            .unwrap()
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.file_name().to_string_lossy().into_owned())
            .find(|name| {
                build_dir
                    .join("artifacts")
                    .join(name)
                    .join("transport.completed")
                    .is_file()
            })
            .expect("no completed action exists");
        fs::remove_file(
            build_dir
                .join("artifacts")
                .join(&damaged)
                .join("result.json"),
        )
        .unwrap();
        let out = temp("export-lost-out");
        let outcome = export::export(&store, &effort, &out.join("lost.zip")).unwrap();
        assert!(
            !outcome.complete,
            "a lost receipt produced a complete export"
        );
        let lost = outcome
            .members
            .iter()
            .find(|member| member.name == format!("build/artifacts/{damaged}/result.json"))
            .expect("the lost receipt was not selected as required evidence");
        assert_eq!(lost.class, export::MemberClass::Failed);
        assert!(
            lost.detail
                .as_deref()
                .unwrap()
                .contains("required receipt evidence is unavailable")
        );
        assert!(outcome.archive.is_none());

        // A disappeared action the journal and state still reference is a
        // detectable omission, not something the copied subset defines.
        let (store, effort, _repo) = completed_build("export-lost-action");
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let state = read_state(&store, &effort);
        let vanished = state.action.id.clone();
        fs::remove_dir_all(build_dir.join("artifacts").join(&vanished)).unwrap();
        let out = temp("export-lost-action-out");
        let outcome = export::export(&store, &effort, &out.join("vanished.zip")).unwrap();
        assert!(!outcome.complete);
        assert!(
            outcome
                .omissions
                .iter()
                .any(|omission| omission.contains(&vanished)
                    && omission.contains("no artifact directory")),
            "the disappeared action was not disclosed: {:#?}",
            outcome.omissions
        );
        assert!(outcome.members.iter().any(|member| member.name
            == format!("build/artifacts/{vanished}/action.json")
            && member.class == export::MemberClass::Failed
            && member.required));

        // A journal-only reference is found even when no state field names it.
        let (store, effort, _repo) = completed_build("export-journal-ref");
        let ghost = "act-1234567890123-999-9";
        store
            .append_journal(
                &effort,
                "build_action_transition",
                Some("fixture"),
                serde_json::json!({"from_action": ghost, "detail": "a record only the journal kept"}),
            )
            .unwrap();
        let out = temp("export-journal-out");
        let outcome = export::export(&store, &effort, &out.join("journal.zip")).unwrap();
        assert!(!outcome.complete);
        assert!(
            outcome
                .omissions
                .iter()
                .any(|omission| omission.contains(ghost)),
            "a journal-referenced action was silently omitted: {:#?}",
            outcome.omissions
        );

        // An older or interrupted action that never wrote this version's records
        // is disclosed as their legitimate absence and still exports completely.
        let (store, effort, _repo) = completed_build("export-legacy-action");
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let legacy = build_dir.join("artifacts").join(
            fs::read_dir(build_dir.join("artifacts"))
                .unwrap()
                .filter_map(|entry| entry.ok())
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .next()
                .expect("no action exists"),
        );
        for name in [
            "invocation.json",
            "instruction.md",
            "binding-requirements.json",
        ] {
            fs::remove_file(legacy.join(name)).unwrap();
        }
        // Without the dispatch record this version's input members are not
        // required: the action looks like one an older controller wrote.
        let out = temp("export-legacy-out");
        let outcome = export::export(&store, &effort, &out.join("legacy.zip")).unwrap();
        for name in ["instruction.md", "binding-requirements.json"] {
            let member = outcome
                .members
                .iter()
                .find(|member| {
                    member.name
                        == format!(
                            "build/artifacts/{}/{}",
                            legacy.file_name().unwrap().to_string_lossy(),
                            name
                        )
                })
                .unwrap();
            assert_eq!(member.class, export::MemberClass::Absent, "{member:#?}");
            assert!(!member.required);
        }
        assert!(
            outcome.complete,
            "a legacy action's unavailable records failed the collection: {:#?}",
            outcome
                .members
                .iter()
                .filter(|member| member.class != export::MemberClass::Copied)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn export_detects_lost_accepted_correction_and_diagnosis_reports() {
        let (store, effort, repo, _, _) = prepared();
        let host = ScenarioHost::new(Scenario::Corrections);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let correction = fs::read_dir(build_dir.join("artifacts"))
            .unwrap()
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .find(|directory| {
                read_json::<serde_json::Value>(&directory.join("action.json"))
                    .is_ok_and(|action| action["kind"] == "review")
                    && read_json::<serde_json::Value>(&directory.join("result.json"))
                        .is_ok_and(|receipt| receipt["outcome"] == "changes_required")
            })
            .expect("the correction review did not run");
        let correction_report = correction.join("report.md");
        fs::remove_file(&correction_report).unwrap();
        let out = temp("export-lost-correction-report");
        let correction_export =
            export::export(&store, &effort, &out.join("correction.zip")).unwrap();
        assert!(!correction_export.complete && correction_export.archive.is_none());
        assert!(correction_export.members.iter().any(|member| {
            member.name.ends_with("/report.md")
                && member.source == correction_report.to_string_lossy()
                && member.required
                && member.class == export::MemberClass::Failed
        }));

        let (store, effort, repo, _, _) = prepared();
        let host = ScenarioHost::new(Scenario::ExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let stopped = read_state(&store, &effort).stop.unwrap();
        assert!(matches!(stopped.action.kind, ActionKind::Unblock));
        let diagnosis_report = store
            .phase_dir(&effort, "build")
            .unwrap()
            .join("artifacts")
            .join(&stopped.action.id)
            .join("report.md");
        fs::remove_file(&diagnosis_report).unwrap();
        let out = temp("export-lost-diagnosis-report");
        let diagnosis_export = export::export(&store, &effort, &out.join("diagnosis.zip")).unwrap();
        assert!(!diagnosis_export.complete && diagnosis_export.archive.is_none());
        assert!(diagnosis_export.members.iter().any(|member| {
            member.name.ends_with("/report.md")
                && member.source == diagnosis_report.to_string_lossy()
                && member.required
                && member.class == export::MemberClass::Failed
        }));
    }

    /// R-029/R-030: action ids created by resolution are opaque digest ids.
    /// The exporter must retain the reference from controller records and
    /// refuse complete promotion if that executed continuation later vanishes.
    #[test]
    fn export_detects_a_lost_executed_resolution_continuation() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::SecondPhaseExternalRequirement);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let resolution = resolve_stop(
            &store,
            &effort,
            ResolutionKind::ExistingAuthorityClarification,
            "the current adopted requirements already resolve the historical question",
            false,
            None,
            Vec::new(),
        )
        .unwrap();
        let ResolutionOutcome::Resolved {
            continuation_action,
            ..
        } = resolution
        else {
            panic!("the isolated fixture did not create a resolution continuation")
        };
        assert!(continuation_action.starts_with("act-") && continuation_action.len() == 28);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let continuation_dir = build_dir.join("artifacts").join(&continuation_action);
        assert!(
            continuation_dir.join("transport.completed").is_file(),
            "the continuation did not execute"
        );
        fs::remove_dir_all(&continuation_dir).unwrap();

        let out = temp("export-lost-resolution-continuation");
        let archive = out.join("lost-continuation.zip");
        let outcome = export::export(&store, &effort, &archive).unwrap();
        assert!(
            !outcome.complete,
            "lost resolution action was promoted as complete: {outcome:#?}"
        );
        assert!(outcome.archive.is_none() && !archive.exists());
        assert!(
            outcome.omissions.iter().any(|omission| {
                omission.contains(&continuation_action)
                    && omission.contains("no artifact directory")
            }),
            "the missing continuation was not explicitly reported: {outcome:#?}"
        );
    }

    /// R-030: an Audit blocked receipt does not imply an assessment was
    /// produced, but an assessment tied to a completed published Audit is
    /// required if its action-side copy disappears.
    #[test]
    fn export_distinguishes_blocked_audit_receipt_from_lost_published_assessment() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::BlockedAuditReceiptOnly);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let blocked = fs::read_dir(build_dir.join("artifacts"))
            .unwrap()
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .find(|directory| {
                read_json::<serde_json::Value>(&directory.join("action.json"))
                    .is_ok_and(|action| action["kind"] == "final_audit")
            })
            .expect("no final Audit action exists");
        let receipt: serde_json::Value = read_json(&blocked.join("result.json")).unwrap();
        assert_eq!(receipt["outcome"], "blocked");
        assert!(!blocked.join("assessment.json").exists());
        let out = temp("export-blocked-audit-receipt");
        let outcome = export::export(&store, &effort, &out.join("blocked.zip")).unwrap();
        assert!(
            outcome.complete,
            "blocked Audit's valid missing assessment prevented stopped-state export: {outcome:#?}"
        );
        let member_name = format!(
            "build/artifacts/{}/assessment.json",
            blocked.file_name().unwrap().to_string_lossy()
        );
        let member = outcome
            .members
            .iter()
            .find(|member| member.name == member_name)
            .unwrap();
        assert_eq!(member.class, export::MemberClass::Absent);
        assert!(!member.required);
        assert!(
            member
                .detail
                .as_deref()
                .unwrap()
                .contains("records blocked")
        );

        let (store, effort, _repo) = completed_build("export-lost-published-assessment");
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let audit_action = fs::read_dir(build_dir.join("artifacts"))
            .unwrap()
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .find(|directory| {
                read_json::<serde_json::Value>(&directory.join("action.json"))
                    .is_ok_and(|action| action["kind"] == "final_audit")
            })
            .expect("no completed final Audit exists");
        let action: serde_json::Value = read_json(&audit_action.join("action.json")).unwrap();
        let attempt = action["action_id"].as_str().unwrap();
        let published = store
            .list_artifacts(&effort)
            .unwrap()
            .into_iter()
            .filter(|reference| reference.kind == ArtifactKind::Audit)
            .any(|reference| {
                store.load_envelope(&effort, &reference).unwrap().run_id
                    == format!("build-audit-{attempt}")
            });
        assert!(
            published,
            "the final Audit action was not published under its exact attempt id"
        );
        fs::remove_file(audit_action.join("assessment.json")).unwrap();
        let out = temp("export-lost-published-assessment-out");
        let outcome = export::export(&store, &effort, &out.join("lost-assessment.zip")).unwrap();
        assert!(!outcome.complete && outcome.archive.is_none());
        let member_name = format!(
            "build/artifacts/{}/assessment.json",
            audit_action.file_name().unwrap().to_string_lossy()
        );
        let member = outcome
            .members
            .iter()
            .find(|member| member.name == member_name)
            .unwrap();
        assert_eq!(member.class, export::MemberClass::Failed);
        assert!(member.required);
        assert!(member.detail.as_deref().unwrap().contains("publication"));
    }

    struct FaultingRecordDirectoryReader {
        fail_directory: PathBuf,
        fail_entry_directory: PathBuf,
    }

    impl export::RecordDirectoryReader for FaultingRecordDirectoryReader {
        fn read_directory(
            &self,
            directory: &Path,
        ) -> std::result::Result<Vec<std::result::Result<PathBuf, String>>, String> {
            if directory == self.fail_directory {
                return Err("injected read_dir permission failure".into());
            }
            let entries = fs::read_dir(directory).map_err(|error| error.to_string())?;
            let mut paths = entries
                .map(|entry| {
                    entry
                        .map(|entry| entry.path())
                        .map_err(|error| error.to_string())
                })
                .collect::<Vec<_>>();
            if directory == self.fail_entry_directory {
                paths.push(Err("injected directory-entry error".into()));
            }
            Ok(paths)
        }
    }

    #[test]
    fn export_uses_finalized_discovery_manifest_for_nested_payloads_and_integrity() {
        let (store, effort, _repo) = completed_build("export-discovery-manifest");
        let reference = publish_finalized_discovery_with_nested_graph(&store, &effort);
        let (envelope, files) = store.load_bundle(&effort, &reference).unwrap();
        assert!(files.contains_key("graph/F-1.md"));
        let out = temp("export-discovery-manifest-out");
        let archive_path = out.join("discovery.zip");
        let outcome = export::export(&store, &effort, &archive_path).unwrap();
        assert!(outcome.complete, "{outcome:#?}");
        let archive_members = archive::members_by_name(&archive_path).unwrap();
        for payload in &envelope.payloads {
            let name = format!("artifacts/{}/{}", reference.artifact_id, payload.path);
            let archived = archive_members
                .get(&name)
                .unwrap_or_else(|| panic!("manifest payload {name} is missing"));
            let (bytes, digest) = archive::read_member(&archive_path, archived).unwrap();
            assert_eq!(bytes, files[&payload.path], "payload bytes changed: {name}");
            assert_eq!(digest, payload.sha256, "payload digest changed: {name}");
            assert_eq!(
                bytes.len() as u64,
                payload.bytes,
                "payload size changed: {name}"
            );
        }

        let discovery_dir = store.artifact_dir(&effort, &reference.artifact_id).unwrap();
        let nested = discovery_dir.join("graph/F-1.md");
        let original = fs::read(&nested).unwrap();
        fs::remove_file(&nested).unwrap();
        let missing_path = out.join("missing-payload.zip");
        let missing = export::export(&store, &effort, &missing_path).unwrap();
        let member_name = format!("artifacts/{}/graph/F-1.md", reference.artifact_id);
        assert!(!missing.complete && missing.archive.is_none() && !missing_path.exists());
        assert!(missing.members.iter().any(|member| {
            member.name == member_name
                && member.required
                && member.class == export::MemberClass::Failed
        }));
        assert!(discovery_dir.join("manifest.json").is_file());
        assert!(discovery_dir.join("technical-spec.md").is_file());

        let altered = b"altered graph payload\n";
        fs::write(&nested, altered).unwrap();
        let altered_path = out.join("altered-payload.zip");
        let changed = export::export(&store, &effort, &altered_path).unwrap();
        assert!(!changed.complete && changed.archive.is_none() && !altered_path.exists());
        assert!(changed.members.iter().any(|member| {
            member.name == member_name
                && member.required
                && member.class == export::MemberClass::Changed
        }));
        assert_eq!(
            fs::read(&nested).unwrap(),
            altered,
            "export changed the source file"
        );
        assert_ne!(original, altered);
    }

    /// R-029/R-030: deterministic failures at the actual directory inventory
    /// boundary remain visible and block complete archive promotion.
    #[test]
    fn export_reports_directory_inventory_and_entry_failures() {
        let (store, effort, _repo) = completed_build("export-inventory-failure");
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        fs::create_dir_all(build_dir.join("evidence/nested")).unwrap();
        fs::write(build_dir.join("evidence/nested/evidence.json"), b"evidence").unwrap();
        fs::create_dir_all(build_dir.join("preflight/live")).unwrap();
        fs::write(build_dir.join("preflight/live/report.json"), b"report").unwrap();
        let reader = FaultingRecordDirectoryReader {
            fail_directory: build_dir.join("evidence"),
            fail_entry_directory: build_dir.join("preflight"),
        };
        let out = temp("export-inventory-failure-out");
        let archive = out.join("cannot-promote.zip");
        let outcome =
            export::export_with_record_reader(&store, &effort, &archive, &reader).unwrap();
        assert!(!outcome.complete && outcome.archive.is_none() && !archive.exists());
        let read_failure = outcome
            .members
            .iter()
            .find(|member| member.name == "build/evidence/.inventory-error")
            .expect("read_dir failure was not recorded");
        assert_eq!(read_failure.class, export::MemberClass::Failed);
        assert!(
            read_failure
                .detail
                .as_deref()
                .unwrap()
                .contains("injected read_dir permission failure")
        );
        assert!(read_failure.source.ends_with("/build/evidence"));
        let entry_failure = outcome
            .members
            .iter()
            .find(|member| member.name.starts_with("build/preflight/.inventory-error-"))
            .expect("directory-entry failure was not recorded");
        assert_eq!(entry_failure.class, export::MemberClass::Failed);
        assert!(
            entry_failure
                .detail
                .as_deref()
                .unwrap()
                .contains("injected directory-entry error")
        );
        assert!(entry_failure.source.ends_with("/build/preflight"));
    }

    /// R-028/R-029: nested durable evidence, live-probe evidence and the
    /// operator evidence a resolution references are all selected, not skipped
    /// because they are not top-level files.
    #[test]
    fn export_collects_nested_live_probe_and_resolution_evidence() {
        let (store, effort, repo) = prepared_with_config(ATTRIBUTED_CONFIG);
        let host = ScenarioHost::new(Scenario::BlockedAuditExternalEvidence);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        // Nested durable evidence and a live-probe directory, as the preflight
        // command writes them.
        fs::create_dir_all(build_dir.join("evidence/nested")).unwrap();
        fs::write(
            build_dir.join("evidence/nested/record.json"),
            "{\"nested\":true}",
        )
        .unwrap();
        let probe = build_dir.join("preflight/live-123/probe-worker");
        fs::create_dir_all(&probe).unwrap();
        fs::write(probe.join("report.md"), "probe report\n").unwrap();
        fs::write(probe.join("invocation.json"), "{}\n").unwrap();
        // Operator evidence referenced by a durable resolution record.
        let supplied = temp("resolution-evidence");
        fs::write(
            supplied.join("live-verification.txt"),
            "live verification\n",
        )
        .unwrap();
        resolve_stop(
            &store,
            &effort,
            ResolutionKind::NewVerificationEvidence,
            "live verification evidence supplied",
            false,
            None,
            vec![supplied.join("live-verification.txt")],
        )
        .unwrap();

        let out = temp("export-nested-out");
        let archive = out.join("nested.zip");
        let outcome = export::export(&store, &effort, &archive).unwrap();
        assert!(outcome.complete, "{outcome:#?}");
        let members = archive::members_by_name(&archive).unwrap();
        assert!(members.contains_key("build/evidence/nested/record.json"));
        assert!(members.contains_key("build/preflight/live-123/probe-worker/report.md"));
        let referenced = members
            .keys()
            .find(|name| {
                name.starts_with("resolutions/evidence/") && name.ends_with("live-verification.txt")
            })
            .expect("the resolution's evidence was not collected");
        let (bytes, _) = archive::read_member(&archive, &members[referenced]).unwrap();
        assert_eq!(bytes, "live verification\n".as_bytes());

        // Evidence that changed since the record was written is a change, and
        // evidence that disappeared is failed: neither is silently omitted.
        fs::write(supplied.join("live-verification.txt"), "different now\n").unwrap();
        let changed = export::export(&store, &effort, &out.join("changed.zip")).unwrap();
        assert!(!changed.complete);
        let member = changed
            .members
            .iter()
            .find(|member| member.name.starts_with("resolutions/evidence/"))
            .unwrap();
        assert_eq!(member.class, export::MemberClass::Changed, "{member:#?}");
        assert!(member.detail.as_deref().unwrap().contains("published"));
        fs::remove_file(supplied.join("live-verification.txt")).unwrap();
        let missing = export::export(&store, &effort, &out.join("missing.zip")).unwrap();
        assert!(!missing.complete);
        assert!(missing.members.iter().any(
            |member| member.name.starts_with("resolutions/evidence/")
                && member.class == export::MemberClass::Failed
                && member.required
        ));
    }

    /// R-029/R-031: the exported Git evidence restores the implementation into
    /// an empty repository — including an unchanged-source re-verification whose
    /// commit range would be empty — and carries the partial work a stopped run
    /// left uncommitted.
    #[test]
    fn exported_git_evidence_restores_the_implementation_without_a_baseline() {
        let (store, effort, repo) = completed_build("export-git");
        let out = temp("export-git-out");
        let archive = out.join("effort.zip");
        let outcome = export::export(&store, &effort, &archive).unwrap();
        assert!(outcome.complete, "{outcome:#?}");
        let state = read_state(&store, &effort);
        let expected_head = git(&repo, ["rev-parse", "HEAD"]).unwrap();
        assert_eq!(expected_head, state.action.target_commit.clone().unwrap());

        let members = archive::members_by_name(&archive).unwrap();
        let bundle = members
            .get("git-evidence/history.bundle")
            .expect("no history bundle was exported");
        let (bytes, _) = archive::read_member(&archive, bundle).unwrap();
        let restore = temp("export-git-restore");
        fs::write(restore.join("history.bundle"), &bytes).unwrap();
        // The bundle clones into an empty repository: it needs no baseline.
        git_ok(&restore, &["clone", "--quiet", "history.bundle", "clone"]);
        let clone = restore.join("clone");
        assert_eq!(git(&clone, ["rev-parse", "HEAD"]).unwrap(), expected_head);
        assert_eq!(
            git(&clone, ["rev-list", "--count", "HEAD"]).unwrap(),
            git(&repo, ["rev-list", "--count", "HEAD"]).unwrap(),
            "the exported history is not the repository's own history"
        );
        assert!(clone.join("work-1.txt").is_file() && clone.join("work-2.txt").is_file());
        assert!(
            git_output_ok(
                &clone,
                &[
                    "merge-base",
                    "--is-ancestor",
                    &state.frozen.build_start_commit,
                    "HEAD"
                ]
            ),
            "the recorded build start is not an ancestor of the exported history"
        );

        // A stopped run with uncommitted tracked work carries that partial work;
        // untracked files are disclosed by name only.
        fs::write(repo.join("source.txt"), "uncommitted tracked progress\n").unwrap();
        fs::write(
            repo.join("partial-work.txt"),
            "uncommitted untracked progress\n",
        )
        .unwrap();
        let second = export::export(&store, &effort, &out.join("second.zip")).unwrap();
        assert!(second.complete, "{second:#?}");
        let members = archive::members_by_name(&out.join("second.zip")).unwrap();
        let patch = members
            .get("git-evidence/partial-work.patch")
            .expect("no partial-work patch was exported");
        let (bytes, _) = archive::read_member(&out.join("second.zip"), patch).unwrap();
        let patch = String::from_utf8_lossy(&bytes).into_owned();
        assert!(patch.contains("uncommitted tracked progress"), "{patch}");
        let untracked = members
            .get("git-evidence/untracked-files.txt")
            .expect("no untracked list was exported");
        let (bytes, _) = archive::read_member(&out.join("second.zip"), untracked).unwrap();
        let untracked = String::from_utf8_lossy(&bytes).into_owned();
        assert!(untracked.contains("partial-work.txt"), "{untracked}");
        assert!(
            !untracked.contains("uncommitted untracked progress"),
            "untracked file contents were collected"
        );

        // An unchanged-source re-verification has an empty commit range; its
        // implementation history is still exported and restorable.
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let plan = load_plan(&store, &effort, &build_dir).unwrap();
        initialize_state(&store, &effort, &build_dir, &plan).unwrap();
        let state = read_state(&store, &effort);
        assert_eq!(
            state.frozen.build_start_commit,
            git(&repo, ["rev-parse", "HEAD"]).unwrap(),
            "the fixture must have an unchanged source for this case"
        );
        let out = temp("export-unchanged-out");
        let archive = out.join("unchanged.zip");
        let outcome = export::export(&store, &effort, &archive).unwrap();
        assert!(outcome.complete, "{outcome:#?}");
        assert!(
            outcome
                .omissions
                .iter()
                .all(|omission| !omission.contains("could not be bundled"))
        );
        let members = archive::members_by_name(&archive).unwrap();
        let bundle = members
            .get("git-evidence/history.bundle")
            .expect("an unchanged-source run exported no history bundle");
        assert!(bundle.bytes > 0, "the history bundle is empty");
        let (bytes, _) = archive::read_member(&archive, bundle).unwrap();
        let restore = temp("export-unchanged-restore");
        fs::write(restore.join("history.bundle"), &bytes).unwrap();
        git_ok(&restore, &["clone", "--quiet", "history.bundle", "clone"]);
        assert_eq!(
            git(&restore.join("clone"), ["rev-parse", "HEAD"]).unwrap(),
            git(&repo, ["rev-parse", "HEAD"]).unwrap()
        );
    }

    fn git_output_ok(repo: &Path, args: &[&str]) -> bool {
        Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .is_ok_and(|output| output.status.success())
    }

    /// R-030: a change at the real collection boundary — after the member was
    /// stat'd and before its bytes are archived — is detected, and the archive
    /// is never promoted.
    #[test]
    fn a_change_at_the_archived_read_is_detected_and_never_promoted() {
        let (store, effort, _repo) = completed_build("export-mutation");
        let out = temp("export-mutation-out");
        let archive = out.join("mutation.zip");
        export::collection_hook::set(Some(Box::new(|path: &Path| {
            if path.ends_with("state.json") {
                fs::write(path, b"{\"mutated\": true}").unwrap();
            }
        })));
        let outcome = export::export(&store, &effort, &archive);
        export::collection_hook::set(None);
        let outcome = outcome.unwrap();
        assert!(
            !outcome.complete,
            "a change during collection was not detected"
        );
        assert!(outcome.archive.is_none());
        assert!(
            !archive.exists(),
            "an archive with unverified bytes was promoted"
        );
        let member = outcome
            .members
            .iter()
            .find(|member| member.name == "build/state.json")
            .unwrap();
        assert_eq!(member.class, export::MemberClass::Changed, "{member:#?}");
        assert!(member.detail.as_deref().unwrap().contains("changed"));
        // The mutated bytes are not in the archive, and the verification did not
        // accept them as a match for what was checked.
        let archived = archive::central_directory(&outcome.partial).unwrap();
        assert!(
            archived
                .iter()
                .all(|member| member.name != "build/state.json"),
            "the changed member was archived anyway"
        );
        // The state on disk is the mutated fixture's, proving the hook ran at the
        // collection boundary rather than before it.
        assert_eq!(
            fs::read_to_string(build_dir_state(&store, &effort)).unwrap(),
            "{\"mutated\": true}"
        );
    }

    fn build_dir_state(store: &Store, effort: &Effort) -> PathBuf {
        store.phase_dir(effort, "build").unwrap().join("state.json")
    }

    /// R-021/R-022: static preflight reports the effective configuration and
    /// writes nothing; a live probe is refused without explicit authorization.
    #[test]
    fn preflight_is_static_by_default_and_live_only_when_authorized() {
        let (store, effort, _repo) = prepared_with_config(ATTRIBUTED_CONFIG);
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let before = fs::read_dir(&build_dir).unwrap().count();
        let report = preflight::static_preflight(&store, &effort).unwrap();
        assert_eq!(report.mode, "static");
        assert!(report.live.is_none());
        assert_eq!(fs::read_dir(&build_dir).unwrap().count(), before);
        assert!(!build_dir.join("preflight").exists());
        let worker = report
            .roles
            .iter()
            .find(|role| role.role == "worker")
            .unwrap();
        assert_eq!(worker.adapter, "codex");
        assert_eq!(
            worker.attribute_arguments,
            vec![
                "--model".to_string(),
                "worker-model".into(),
                "-c".into(),
                "model_reasoning_effort=high".into(),
            ]
        );
        assert!(
            worker
                .command_form
                .iter()
                .any(|part| part == "<prompt elided>")
        );
        assert!(!worker.explicit_full_access);
        let unblocker = report
            .roles
            .iter()
            .find(|role| role.role == "unblocker")
            .unwrap();
        assert!(
            unblocker.notes.iter().any(|note| note.contains("fallback")),
            "the unblocker fallback is not documented: {unblocker:#?}"
        );
        let once_over = report
            .roles
            .iter()
            .find(|role| role.role == "once_over")
            .unwrap();
        assert!(
            once_over
                .notes
                .iter()
                .any(|note| note.contains("never runs"))
        );
        // Capabilities that static checks cannot establish stay unknown.
        assert!(
            report
                .unknown_capabilities
                .iter()
                .any(|capability| capability.contains("commit"))
        );

        // A live probe without authorization is refused before any inference.
        let host = ProbeHost::new(true);
        let error = preflight::live_preflight(&store, &effort, &host, false)
            .unwrap_err()
            .to_string();
        assert!(error.contains("explicit operator authorization"), "{error}");
        assert!(
            error.contains("quota") || error.contains("tokens"),
            "{error}"
        );
        assert_eq!(host.calls(), 0);

        // With authorization the probes run in isolated owned fixtures.
        let report = preflight::live_preflight(&store, &effort, &host, true).unwrap();
        assert!(report.authorized);
        let worker = report
            .probes
            .iter()
            .find(|probe| probe.role == "worker")
            .unwrap();
        assert_eq!(
            worker.outcome, "fresh_and_resumed_commits_verified",
            "{worker:#?}"
        );
        assert!(worker.resumed_session_verified);
        let first = worker.fresh_commit.as_deref().unwrap();
        let second = worker.resumed_commit.as_deref().unwrap();
        assert_ne!(first, second);
        assert_eq!(worker.resumed_parent.as_deref(), Some(first));
        let reviewer = report
            .probes
            .iter()
            .find(|probe| probe.role == "reviewer")
            .unwrap();
        assert_eq!(reviewer.outcome, "read_and_command_verified");
        assert!(Path::new(&report.evidence_dir).starts_with(&build_dir));
        for probe in &report.probes {
            let fixture = Path::new(&probe.fixture);
            assert!(
                fixture.starts_with(&build_dir),
                "the probe escaped the Build directory"
            );
            assert_ne!(
                fixture,
                store.project_for(&effort).unwrap().canonical_locator
            );
        }

        // R-022: read-only roles may be unable to write a sidecar report under
        // their provider sandbox. Their retained transport response is still
        // controller-owned evidence and must be checked against the unique
        // fixture token and actual HEAD, then copied into the evidence folder.
        let report =
            preflight::live_preflight(&store, &effort, &TransportOnlyProbeHost, true).unwrap();
        for role in ["reviewer"] {
            let probe = report
                .probes
                .iter()
                .find(|probe| probe.role == role)
                .unwrap();
            assert_eq!(probe.outcome, "read_and_command_verified", "{probe:#?}");
            let fixture = Path::new(&probe.fixture);
            let token = fs::read_to_string(fixture.join("README.md"))
                .unwrap()
                .lines()
                .find_map(|line| line.strip_prefix("probe token: "))
                .unwrap()
                .to_owned();
            let head = git(fixture, ["rev-parse", "HEAD"]).unwrap();
            let report_text = fs::read_to_string(&probe.evidence[0]).unwrap();
            assert!(report_text.contains(&token));
            assert!(report_text.contains(&head));
            assert!(Path::new(&probe.evidence[1]).is_file());
            assert!(
                git(
                    fixture,
                    ["status", "--porcelain=v1", "--untracked-files=all"]
                )
                .unwrap()
                .is_empty()
            );
        }

        // Edits without a commit fail the worker capability, not the Build.
        let failing = ProbeHost::new(false);
        let report = preflight::live_preflight(&store, &effort, &failing, true).unwrap();
        let worker = report
            .probes
            .iter()
            .find(|probe| probe.role == "worker")
            .unwrap();
        assert_eq!(worker.outcome, "cannot_commit", "{worker:#?}");

        // R-022: a completed process that produced no evidence at all proves no
        // capability, and neither does a plausible generic answer.
        let report = preflight::live_preflight(&store, &effort, &NoOpProbeHost, true).unwrap();
        for probe in &report.probes {
            assert_ne!(
                probe.outcome, "read_and_command_verified",
                "a no-op provider was certified: {probe:#?}"
            );
            assert_eq!(probe.outcome, "no_probe_evidence", "{probe:#?}");
            assert!(probe.detail.contains("cannot demonstrate"), "{probe:#?}");
        }
        let report =
            preflight::live_preflight(&store, &effort, &GenericAnswerProbeHost, true).unwrap();
        let reviewer = report
            .probes
            .iter()
            .find(|probe| probe.role == "reviewer")
            .unwrap();
        assert_eq!(
            reviewer.outcome, "no_verifiable_probe_output",
            "a generic answer was accepted as proof: {reviewer:#?}"
        );
        assert!(
            reviewer.detail.contains("probe token"),
            "the refusal does not name what was missing: {reviewer:#?}"
        );
        assert!(
            reviewer
                .detail
                .contains("isolated controller-owned fixture"),
            "the probe limits are not disclosed: {reviewer:#?}"
        );
    }

    struct ProbeHost {
        commit_worker: bool,
        calls: Mutex<usize>,
    }

    impl ProbeHost {
        fn new(commit_worker: bool) -> Self {
            Self {
                commit_worker,
                calls: Mutex::new(0),
            }
        }

        fn calls(&self) -> usize {
            *self.calls.lock().unwrap()
        }
    }

    impl HostAdapter for ProbeHost {
        fn invoke(
            &self,
            invocation: &Invocation,
            observer: &mut dyn InvocationObserver,
        ) -> Result<InvocationResult> {
            *self.calls.lock().unwrap() += 1;
            if invocation.role == "worker" {
                if let Some(session_id) = invocation.session_id.as_deref() {
                    assert_eq!(session_id, "probe-session");
                    fs::write(
                        invocation.cwd.join("preflight-worker-resumed.txt"),
                        "orchestrate resumed preflight\n",
                    )?;
                    git_ok(&invocation.cwd, &["add", "preflight-worker-resumed.txt"]);
                    if self.commit_worker {
                        git_ok(
                            &invocation.cwd,
                            &["commit", "-m", "preflight worker resumed commit"],
                        );
                    }
                    observer.session_id(session_id)?;
                } else {
                    fs::write(
                        invocation.cwd.join("preflight-worker.txt"),
                        "orchestrate preflight\n",
                    )?;
                    git_ok(&invocation.cwd, &["add", "preflight-worker.txt"]);
                    if self.commit_worker {
                        git_ok(
                            &invocation.cwd,
                            &["commit", "-m", "preflight worker commit"],
                        );
                    }
                    observer.session_id("probe-session")?;
                }
            } else {
                // A real role reads the fixture and runs the requested command;
                // its report carries the proof of both.
                let readme = fs::read_to_string(invocation.cwd.join("README.md"))?;
                let token = readme
                    .lines()
                    .find_map(|line| line.strip_prefix("probe token: "))
                    .expect("the fixture carries a probe token")
                    .to_owned();
                let head = git(&invocation.cwd, ["rev-parse", "HEAD"])?;
                fs::write(
                    invocation.action.join("report.md"),
                    format!("read the fixture token {token}\nran git rev-parse HEAD -> {head}\n"),
                )?;
            }
            if invocation.role == "worker" {
                fs::write(invocation.action.join("report.md"), "probe report\n")?;
            }
            Ok(InvocationResult {
                completion: InvocationCompletion::Completed,
            })
        }
    }

    /// A provider adapter that cannot write its sidecar report but returns the
    /// required fixture evidence in its captured transport response.
    struct TransportOnlyProbeHost;

    impl HostAdapter for TransportOnlyProbeHost {
        fn invoke(
            &self,
            invocation: &Invocation,
            _observer: &mut dyn InvocationObserver,
        ) -> Result<InvocationResult> {
            if invocation.role == "worker" {
                fs::write(
                    invocation.cwd.join("preflight-worker.txt"),
                    "orchestrate preflight\n",
                )?;
                git_ok(&invocation.cwd, &["add", "."]);
                git_ok(
                    &invocation.cwd,
                    &["commit", "-m", "preflight worker commit"],
                );
                let head = git(&invocation.cwd, ["rev-parse", "HEAD"])?;
                let event = serde_json::json!({
                    "type": "item.completed",
                    "item": {
                        "type": "agent_message",
                        "text": format!("created and committed the probe file at {head}")
                    }
                });
                fs::write(
                    invocation.action.join("transport.jsonl"),
                    format!("{event}\n"),
                )?;
            } else {
                let readme = fs::read_to_string(invocation.cwd.join("README.md"))?;
                let token = readme
                    .lines()
                    .find_map(|line| line.strip_prefix("probe token: "))
                    .expect("the fixture carries a probe token");
                let head = git(&invocation.cwd, ["rev-parse", "HEAD"])?;
                let event = serde_json::json!({
                    "type": "item.completed",
                    "item": {
                        "type": "agent_message",
                        "text": format!("read token {token}\nran git rev-parse HEAD -> {head}")
                    }
                });
                fs::write(
                    invocation.action.join("transport.jsonl"),
                    format!("{event}\n"),
                )?;
            }
            Ok(InvocationResult {
                completion: InvocationCompletion::Completed,
            })
        }
    }

    /// A provider that completes while doing nothing: a no-op must never be
    /// reported as read/command capability.
    struct NoOpProbeHost;

    impl HostAdapter for NoOpProbeHost {
        fn invoke(
            &self,
            _invocation: &Invocation,
            _observer: &mut dyn InvocationObserver,
        ) -> Result<InvocationResult> {
            Ok(InvocationResult {
                completion: InvocationCompletion::Completed,
            })
        }
    }

    /// A provider that answers plausibly — a generic report with no fixture
    /// evidence — but did not read the fixture or run the command.
    struct GenericAnswerProbeHost;

    impl HostAdapter for GenericAnswerProbeHost {
        fn invoke(
            &self,
            invocation: &Invocation,
            _observer: &mut dyn InvocationObserver,
        ) -> Result<InvocationResult> {
            fs::write(
                invocation.action.join("report.md"),
                "I read README.md and ran the requested command successfully.\n",
            )?;
            Ok(InvocationResult {
                completion: InvocationCompletion::Completed,
            })
        }
    }

    /// Every file under a directory with its digest and size, so a read-only
    /// projection can be proven not to have written anything.
    fn tree_snapshot(dir: &Path) -> Vec<(String, String, u64)> {
        let mut entries = Vec::new();
        let mut stack = vec![dir.to_path_buf()];
        while let Some(path) = stack.pop() {
            let Ok(read) = fs::read_dir(&path) else {
                continue;
            };
            for entry in read.flatten() {
                let child = entry.path();
                if child.is_dir() {
                    stack.push(child);
                } else if let Ok(bytes) = fs::read(&child) {
                    entries.push((
                        child.to_string_lossy().into_owned(),
                        digest_bytes(&bytes),
                        bytes.len() as u64,
                    ));
                }
            }
        }
        entries.sort();
        entries
    }

    /// The archive writer's local headers carry the real CRC, so strict readers
    /// (Info-ZIP `unzip -t`) accept an exported archive instead of flagging it
    /// corrupt, and the readback agrees with what was written.
    #[test]
    fn written_archives_carry_a_real_local_crc_and_round_trip() {
        let dir = temp("zip-crc");
        let member = dir.join("member.bin");
        fs::write(&member, b"orchestrate archive member\n").unwrap();
        let archive = dir.join("archive.zip");
        let mut writer = archive::ZipWriter::create(&archive).unwrap();
        let written = writer.add_file("member.bin", &member).unwrap().clone();
        writer.finish().unwrap();
        let bytes = fs::read(&archive).unwrap();
        assert_eq!(&bytes[..4], b"PK\x03\x04");
        let local_crc = u32::from_le_bytes([bytes[14], bytes[15], bytes[16], bytes[17]]);
        assert_eq!(
            local_crc, written.crc32,
            "the local header still holds a placeholder CRC"
        );
        let members = archive::members_by_name(&archive).unwrap();
        let archived = members.get("member.bin").expect("member is missing");
        assert_eq!(archived.crc32, written.crc32);
        let (read_back, digest) = archive::read_member(&archive, archived).unwrap();
        assert_eq!(read_back, fs::read(&member).unwrap());
        assert_eq!(digest, written.sha256);
        // A crafted archive whose central directory runs past the file is
        // rejected rather than panicking.
        let truncated = dir.join("truncated.zip");
        fs::write(&truncated, &bytes[..bytes.len() - 20]).unwrap();
        assert!(archive::members_by_name(&truncated).is_err());
        // An exclusively owned archive refuses a path that already exists
        // instead of truncating whatever is there.
        let taken = dir.join("taken.zip");
        fs::write(&taken, b"someone else's archive\n").unwrap();
        assert!(archive::ZipWriter::create_new(&taken).is_err());
        assert_eq!(
            fs::read(&taken).unwrap(),
            b"someone else's archive\n",
            "an existing file was truncated"
        );
    }

    /// R-024: the heartbeat guard reports an in-flight action, survives a missing
    /// activity file, and stops the moment it is dropped.
    #[test]
    fn heartbeats_report_activity_and_stop_with_the_guard() {
        let dir = temp("heartbeat");
        let heartbeat = progress::Heartbeat::start(
            false,
            dir.clone(),
            "act-1 work".into(),
            std::time::Duration::from_millis(10),
        );
        fs::write(dir.join("transport.jsonl"), b"{}\n").unwrap();
        std::thread::sleep(std::time::Duration::from_millis(60));
        // Dropping the guard joins the thread, so no heartbeat can outlive its
        // action's dispatch and a quiet provider never looks like a live one.
        drop(heartbeat);
        // A second guard with no activity file still terminates cleanly.
        let mut idle = dir.clone();
        idle.push("missing");
        drop(progress::Heartbeat::start(
            true,
            idle,
            "act-2 review".into(),
            std::time::Duration::from_millis(10),
        ));
    }

    /// R-023: status is read-only, reports the facts the controller knows, and
    /// keeps controller liveness separate from provider uncertainty.
    #[test]
    fn build_status_is_read_only_and_separates_liveness_from_uncertainty() {
        let (store, effort, _repo) = completed_build("status");
        let effort_dir = store.effort_dir(&effort);
        let before = tree_snapshot(&effort_dir);
        let status = status::build_status(&store, &effort).unwrap();
        assert_eq!(status["delivery"]["phases_accepted"], 2);
        assert_eq!(status["delivery"]["phases_total"], 2);
        assert!(
            status["delivery"]["tasks_in_accepted_phases"]
                .as_u64()
                .unwrap()
                >= 2
        );
        assert_eq!(status["acceptance"]["last_audit_verdict"], "PASS");
        assert_eq!(status["acceptance"]["state"], "terminal");
        assert_eq!(status["current"]["kind"], "final_audit");
        assert!(status["current"]["configured_role"]["adapter"].is_string());
        assert_eq!(status["controller"]["state"], "not_held");
        assert!(
            status["controller"]["detail"]
                .as_str()
                .unwrap()
                .contains("not proof"),
            "status claimed liveness facts it cannot have"
        );
        assert!(
            status["current"]["provider_activity"]["note"]
                .as_str()
                .unwrap()
                .contains("not engineering progress")
        );
        let lines = status::status_lines(&store, &effort).unwrap();
        assert!(lines.iter().any(|line| line.contains("phases accepted")));
        assert!(lines.iter().any(|line| line.contains("acceptance: PASS")));
        assert_eq!(
            before,
            tree_snapshot(&effort_dir),
            "status mutated the effort"
        );

        // A held lock, a stale lock and no lock are three different facts.
        let project = store.project_for(&effort).unwrap();
        let lock = store.project_dir(&project).join(".build-controller.lock");
        fs::write(&lock, format!("{}\n", std::process::id())).unwrap();
        assert_eq!(
            status::build_status(&store, &effort).unwrap()["controller"]["state"],
            "held"
        );
        fs::write(&lock, "999999999\n").unwrap();
        let stale = status::build_status(&store, &effort).unwrap();
        assert_eq!(stale["controller"]["state"], "stale");
        assert!(
            stale["controller"]["detail"]
                .as_str()
                .unwrap()
                .contains("may still be running")
        );
        fs::remove_file(&lock).unwrap();
    }

    /// R-023: a durable stop, its exact Audit and its unresolved requirement IDs
    /// stay visible after all phases were accepted.
    #[test]
    fn build_status_reports_the_stop_and_exact_unresolved_requirements() {
        let (store, effort, repo, _reconciled, _adoption) = prepared();
        let host = ScenarioHost::new(Scenario::AlwaysBlockedAudit);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        let status = status::build_status(&store, &effort).unwrap();
        assert_eq!(status["acceptance"]["state"], "stopped");
        assert_eq!(status["acceptance"]["last_audit_verdict"], "BLOCKED");
        assert_eq!(
            status["acceptance"]["unresolved_requirement_ids"],
            serde_json::json!(["R-1"])
        );
        assert_eq!(status["stop"]["trigger"], "audit_blocked");
        // The second completed BLOCKED assessment stops on the Audit attempt
        // itself: no continuation, replacement or further diagnosis was created.
        assert_eq!(status["current"]["kind"], "final_audit");
        assert_eq!(status["current"]["role"], "reviewer");
        assert_eq!(status["recovery"]["continuation_used"], false);
        // The audit named by the stop is the one the assessment published.
        assert_eq!(
            status["stop"]["audit"]["artifact_id"],
            status["acceptance"]["current_audit"]["artifact_id"]
        );
        let lines = status::status_lines(&store, &effort).unwrap();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("stopped: audit_blocked"))
        );
        assert!(lines.iter().any(|line| line.contains("R-1")));
    }

    #[test]
    fn build_status_labels_requested_model_strength_and_keeps_default_and_legacy_cases() {
        let v3 = "schema_version = 3\n[worker]\nadapter = \"codex\"\nmodel_strength = \"strong\"\n[reviewer]\nadapter = \"cursor\"\nmodel_strength = \"strong\"\n";
        let (store, effort, repo) = prepared_with_config(v3);
        let host = ScenarioHost::new(Scenario::Basic);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let status = status::build_status(&store, &effort).unwrap();
        assert_eq!(
            status["current"]["configured_role"]["model_strength"],
            "strong"
        );
        assert_eq!(
            status["current"]["configured_role"]["model"],
            serde_json::Value::Null
        );
        assert!(status["current"]["configured_role"]["observed_model"].is_null());
        let lines = status::status_lines(&store, &effort).unwrap();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("requested strong model strength"))
        );
        assert!(lines.iter().all(|line| !line.contains("provider default")));

        let unset_v3 =
            "schema_version = 3\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"cursor\"\n";
        let (store, effort, repo) = prepared_with_config(unset_v3);
        let host = ScenarioHost::new(Scenario::Basic);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let lines = status::status_lines(&store, &effort).unwrap();
        assert!(lines.iter().any(|line| line.contains("provider default")));

        let legacy_v2 = "schema_version = 2\n[worker]\nadapter = \"codex\"\nmodel = \"worker-model\"\n[reviewer]\nadapter = \"cursor\"\nmodel = \"reviewer-model\"\n";
        let (store, effort, repo) = prepared_with_config(legacy_v2);
        let host = ScenarioHost::new(Scenario::Basic);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let status = status::build_status(&store, &effort).unwrap();
        assert_eq!(
            status["current"]["configured_role"]["model"],
            "reviewer-model"
        );
        assert!(status["current"]["configured_role"]["model_strength"].is_null());
        let lines = status::status_lines(&store, &effort).unwrap();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("requested native model reviewer-model"))
        );
    }

    #[test]
    fn build_status_reports_binding_requirements_missing_from_the_current_audit() {
        let (store, effort, _repo, original, _) = prepared();
        let (_, mut reconciled): (_, ReconciledDiscovery) = store
            .load_json(&effort, &original, "reconciled-discovery.json")
            .unwrap();
        let mut second = reconciled.requirements[0].clone();
        second.requirement.id = "R-2".into();
        second.requirement.text = "second bound requirement".into();
        second.requirement.acceptance = "second check".into();
        reconciled.requirements.push(second);
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
                "status-two-requirements".into(),
                "IMPLEMENTATION_READY".into(),
                Vec::new(),
                build_provenance("test", orchestrate_guides::BUILD),
                files,
            )
            .unwrap();
        prepare_build_files(&store, &effort, &reconciled_ref, None);
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        let plan = load_plan(&store, &effort, &build_dir).unwrap();
        let mut state = initialize_state(&store, &effort, &build_dir, &plan).unwrap();
        let assessment = AuditAssessment {
            reconciled: reconciled_ref.clone(),
            adoption: state.frozen.adoption.clone(),
            implementation: state.frozen.adoption.clone(),
            coverage: vec![Coverage {
                requirement_id: "R-1".into(),
                state: CoverageState::Pass,
                rationale: "the first requirement passed".into(),
                evidence: vec!["test".into()],
                correction: String::new(),
            }],
            assessor_context: "status projection fixture".into(),
        };
        let report = orchestrate_contracts::AuditReport {
            assessment,
            verdict: Verdict::Blocked,
        };
        let mut audit_files = BTreeMap::new();
        audit_files.insert(
            "audit.json".into(),
            orchestrate_contracts::encode(&report).unwrap(),
        );
        let audit = store
            .publish_bundle(
                &effort,
                "audit",
                ArtifactKind::Audit,
                "status-missing-coverage".into(),
                "BLOCKED".into(),
                vec![reconciled_ref],
                build_provenance("test", orchestrate_guides::BUILD),
                audit_files,
            )
            .unwrap();
        state.current_audit = Some(audit);
        save_state(&build_dir.join("state.json"), &state).unwrap();

        let before = tree_snapshot(&store.effort_dir(&effort));
        let status = status::build_status(&store, &effort).unwrap();
        assert_eq!(status["acceptance"]["last_audit_verdict"], "BLOCKED");
        assert_eq!(
            status["acceptance"]["unresolved_requirement_ids"],
            json!(["R-2"])
        );
        let lines = status::status_lines(&store, &effort).unwrap();
        assert!(
            lines
                .iter()
                .any(|line| line.contains("acceptance: BLOCKED") && line.contains("R-2"))
        );
        assert_eq!(
            before,
            tree_snapshot(&store.effort_dir(&effort)),
            "status wrote to the effort"
        );
    }

    /// A host whose configured provider executables cannot be launched, so the
    /// controller must find that out before any product-changing work.
    struct MissingExecutableHost {
        missing: &'static str,
        invoked: Mutex<usize>,
    }

    impl MissingExecutableHost {
        fn new(missing: &'static str) -> Self {
            Self {
                missing,
                invoked: Mutex::new(0),
            }
        }
    }

    impl HostAdapter for MissingExecutableHost {
        fn launches_configured_executable(&self) -> bool {
            true
        }

        fn provider_executable_available(&self, program: &str) -> bool {
            program != self.missing
        }

        fn invoke(
            &self,
            _request: &Invocation,
            _observer: &mut dyn InvocationObserver,
        ) -> Result<InvocationResult> {
            *self.invoked.lock().unwrap() += 1;
            bail!("no dispatch may occur when a required role cannot be launched")
        }
    }

    #[cfg(unix)]
    #[test]
    fn executable_readiness_child_entry() {
        let Ok(mode) = std::env::var("ORCHESTRATE_B4_CHILD_MODE") else {
            return;
        };
        match mode.as_str() {
            "readiness" => {
                let store = Store::open(Path::new(
                    &std::env::var_os("ORCHESTRATE_B4_STORE").unwrap(),
                ))
                .unwrap();
                let request = BuildRequest {
                    effort: Some(std::env::var("ORCHESTRATE_B4_EFFORT").unwrap()),
                    project: PathBuf::from(std::env::var_os("ORCHESTRATE_B4_PROJECT").unwrap()),
                };
                let result = run(&store, request).unwrap();
                let BuildResult::Blocked { detail, .. } = result else {
                    panic!("a non-executable reviewer passed required-role readiness: {result:?}");
                };
                assert!(
                    detail.contains("reviewer") && detail.contains("claude"),
                    "{detail}"
                );
            }
            "path-search" => {
                let expected = PathBuf::from(std::env::var_os("ORCHESTRATE_B4_EXPECTED").unwrap());
                assert_eq!(
                    preflight::resolve_executable("claude").as_deref(),
                    Some(expected.as_path())
                );
                let output = Command::new("claude").arg("--version").output().unwrap();
                assert!(output.status.success());
                assert_eq!(
                    String::from_utf8_lossy(&output.stdout).trim(),
                    "later executable"
                );
            }
            other => panic!("unexpected child mode {other}"),
        }
    }

    /// R-021/R-006: an unlaunchable required role stops the Build before any
    /// product-changing dispatch, and the stop names the role, adapter and
    /// executable.  Work, not the failing reviewer, would otherwise run first.
    #[test]
    fn a_missing_required_executable_stops_before_any_product_work() {
        let (store, effort, repo) = prepared_with_config(ATTRIBUTED_CONFIG);
        let before = git(&repo, ["rev-parse", "HEAD"]).unwrap();
        let host = MissingExecutableHost::new("cursor-agent");
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        let BuildResult::Blocked {
            detail,
            trigger,
            stop_record,
            ..
        } = result
        else {
            panic!("an unlaunchable reviewer did not stop the Build");
        };
        assert_eq!(trigger.as_deref(), Some("spawn_failure"));
        assert!(detail.contains("reviewer"), "{detail}");
        assert!(detail.contains("cursor-agent"), "{detail}");
        assert_eq!(
            *host.invoked.lock().unwrap(),
            0,
            "the controller dispatched something before checking the required roles"
        );
        // The product, its checkout and the Build's authority are untouched.
        assert_eq!(git(&repo, ["rev-parse", "HEAD"]).unwrap(), before);
        assert!(
            git(&repo, ["status", "--porcelain", "--untracked-files=no"])
                .unwrap()
                .is_empty(),
            "product-changing work happened before the readiness check"
        );
        let state = read_state(&store, &effort);
        let stop = state.stop.expect("no durable stop was recorded");
        assert_eq!(trigger_name(&stop.trigger), "spawn_failure");
        let record = stop_record.unwrap();
        assert_eq!(record["trigger"], "spawn_failure");
        assert_eq!(record["action"]["kind"], "work");
        assert!(journal_count(&store, &effort, "build_stopped") >= 1);
    }

    #[cfg(unix)]
    #[test]
    fn local_readiness_skips_non_executable_role_files_and_matches_path_launch_search() {
        use std::os::unix::fs::PermissionsExt;

        let first = temp("readiness-path-first");
        let later = temp("readiness-path-later");
        let marker = first.join("worker-was-launched");
        let mut worker = fs::File::create(first.join("codex")).unwrap();
        use std::io::Write as _;
        writeln!(worker, "#!/bin/sh\nprintf x > '{}'", marker.display()).unwrap();
        fs::set_permissions(first.join("codex"), fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(first.join("claude"), b"#!/bin/sh\necho wrong\n").unwrap();
        fs::set_permissions(first.join("claude"), fs::Permissions::from_mode(0o644)).unwrap();
        let (store, effort, repo) = prepared_with_config(
            "schema_version = 2\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"claude\"\n",
        );
        let head_before = git(&repo, ["rev-parse", "HEAD"]).unwrap();
        let executable = std::env::current_exe().unwrap();
        let launch_path = preflight::resolve_executable("git")
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let make_path = |paths: &[&Path]| std::env::join_paths(paths.iter().copied()).unwrap();
        let first_path = make_path(&[
            &first,
            &launch_path,
            Path::new("/usr/bin"),
            Path::new("/bin"),
        ]);
        let readiness = Command::new(&executable)
            .args([
                "--exact",
                "tests::executable_readiness_child_entry",
                "--nocapture",
            ])
            .env("ORCHESTRATE_B4_CHILD_MODE", "readiness")
            .env("ORCHESTRATE_B4_STORE", store.root())
            .env("ORCHESTRATE_B4_EFFORT", &effort.id)
            .env("ORCHESTRATE_B4_PROJECT", &repo)
            .env("PATH", first_path)
            .output()
            .unwrap();
        assert!(
            readiness.status.success(),
            "readiness child failed: {}",
            String::from_utf8_lossy(&readiness.stderr)
        );
        assert!(
            !marker.exists(),
            "the worker launched before reviewer readiness passed"
        );
        assert_eq!(git(&repo, ["rev-parse", "HEAD"]).unwrap(), head_before);
        let state = store
            .phase_dir(&effort, "build")
            .unwrap()
            .join("state.json");
        let state: BuildState = read_json(&state).unwrap();
        let stop = state.stop.unwrap();
        assert!(stop.detail.contains("reviewer") && stop.detail.contains("claude"));

        let mut later_claude = fs::File::create(later.join("claude")).unwrap();
        writeln!(later_claude, "#!/bin/sh\necho 'later executable'").unwrap();
        fs::set_permissions(later.join("claude"), fs::Permissions::from_mode(0o755)).unwrap();
        let search_path = make_path(&[
            &first,
            &later,
            &launch_path,
            Path::new("/usr/bin"),
            Path::new("/bin"),
        ]);
        let search = Command::new(&executable)
            .args([
                "--exact",
                "tests::executable_readiness_child_entry",
                "--nocapture",
            ])
            .env("ORCHESTRATE_B4_CHILD_MODE", "path-search")
            .env("ORCHESTRATE_B4_EXPECTED", later.join("claude"))
            .env("PATH", search_path)
            .output()
            .unwrap();
        assert!(
            search.status.success(),
            "PATH-search child failed: {}",
            String::from_utf8_lossy(&search.stderr)
        );
    }

    /// R-021/R-013: the readiness check follows an applied overlay, so a Build
    /// resolved into a different role configuration is checked against the
    /// configuration its next action would actually use.
    #[test]
    fn readiness_rechecks_the_configuration_an_overlay_makes_effective() {
        let (store, effort, repo) = prepared_with_config(ATTRIBUTED_CONFIG);
        let first = MissingExecutableHost::new("cursor-agent");
        let result = run_with_adapter(&store, request(&repo), &first).unwrap();
        let BuildResult::Blocked { detail, .. } = result else {
            panic!("an unlaunchable reviewer did not stop the Build");
        };
        assert!(detail.contains("cursor-agent"), "{detail}");

        // The operator repairs the environment and repoints the reviewer at a
        // different adapter; the overlay governs the next invocation.
        let replacement = temp("readiness-config");
        fs::write(
            replacement.join("config.toml"),
            "schema_version = 2\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"claude\"\n",
        )
        .unwrap();
        resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "the reviewer now runs through a different adapter",
            false,
            Some(replacement.join("config.toml")),
            vec![],
        )
        .unwrap();

        let second = MissingExecutableHost::new("claude");
        let result = run_with_adapter(&store, request(&repo), &second).unwrap();
        let BuildResult::Blocked { detail, .. } = result else {
            panic!("the overlaid configuration was not checked");
        };
        assert!(
            detail.contains("claude"),
            "the readiness check used a superseded configuration: {detail}"
        );
        assert!(
            !detail.contains("cursor-agent"),
            "the readiness check reported the replaced adapter: {detail}"
        );
        assert_eq!(
            *second.invoked.lock().unwrap(),
            0,
            "the controller dispatched under the overlaid configuration anyway"
        );
        let stop = read_state(&store, &effort).stop.expect("no durable stop");
        assert_eq!(trigger_name(&stop.trigger), "spawn_failure");
    }

    /// R-018/R-021: only the required worker and reviewer roles are checked
    /// before product work.  An absent advisory once-over or optional unblocker
    /// executable never gates a Build.
    #[test]
    fn required_role_readiness_never_gates_optional_roles() {
        let config = parse_config(ONCE_OVER_CONFIG).unwrap();
        let programs = required_role_programs(&config).unwrap();
        let roles = programs
            .iter()
            .map(|(role, _, _)| *role)
            .collect::<Vec<_>>();
        assert_eq!(roles, vec!["worker", "reviewer"]);
        assert!(
            programs
                .iter()
                .any(|(_, adapter, program)| adapter == "codex" && *program == "codex")
        );
        assert!(
            programs
                .iter()
                .any(|(_, adapter, program)| adapter == "cursor" && *program == "cursor-agent")
        );
        // The same config gates nothing when every required program is present,
        // even though the advisory role's own program is not.
        assert!(static_readiness(&config, &|program| program != "claude").is_ok());
        let error = static_readiness(&config, &|program| program != "cursor-agent").unwrap_err();
        assert!(error.to_string().contains("reviewer"), "{error:#}");
    }

    /// R-013/R-036: an operator overlay that removes the optional once-over while
    /// its action is pending must not panic the controller.  The absence is
    /// recorded as attributable advisory history and the formal Audit proceeds.
    #[test]
    fn a_resolved_overlay_that_removes_the_once_over_records_its_absence() {
        let (store, effort, repo) = prepared_with_config(ONCE_OVER_CONFIG);
        let host = ScenarioHost::new(Scenario::OnceOverControllerInterrupted);
        let result = run_with_adapter(&store, request(&repo), &host).unwrap();
        let BuildResult::Blocked { stopped_action, .. } = result else {
            panic!("the interrupted once-over did not leave a durable stop");
        };
        let stopped = read_state(&store, &effort);
        assert_eq!(action_kind_name(&stopped.action.kind), "once_over");
        assert!(matches!(stopped.action.dispatch, DispatchState::Prepared));

        // The operator repairs the environment and supplies a configuration
        // overlay that no longer configures the advisory role.
        let replacement = temp("once-over-config");
        fs::write(
            replacement.join("config.toml"),
            "schema_version = 2\n[worker]\nadapter = \"codex\"\n[reviewer]\nadapter = \"cursor\"\n",
        )
        .unwrap();
        let outcome = resolve_stop(
            &store,
            &effort,
            ResolutionKind::EnvironmentRepair,
            "the advisory once-over was withdrawn while its action was pending",
            false,
            Some(replacement.join("config.toml")),
            vec![],
        )
        .unwrap();
        let stopped_action = stopped_action.expect("the stop names no action");
        let ResolutionOutcome::Resolved {
            continuation_kind,
            continuation_action,
            config_version,
            stopped_action: resolved,
            ..
        } = outcome
        else {
            panic!("the overlay resolution was refused");
        };
        assert_eq!(resolved, stopped_action);
        assert_eq!(continuation_kind, "once_over");
        assert_eq!(config_version, Some(2));

        // The Build continues through the controller loop: the now-unconfigured
        // role is recorded as absent and the formal Audit still decides.
        let host = ScenarioHost::new(Scenario::Basic);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        assert_eq!(
            host.call_count("once_over"),
            0,
            "the removed role ran anyway"
        );
        assert_eq!(host.call_count("final_audit"), 1);
        let state = read_state(&store, &effort);
        let advisory = state
            .once_over
            .iter()
            .find(|run| run.action_id == continuation_action)
            .expect("the advisory absence is not in the durable history");
        assert_eq!(advisory.outcome, "not_configured");
        // The absence is attributed to the exact commit the formal Audit then
        // assessed, and the run is journaled rather than silently skipped.
        assert_eq!(
            state.action.target_commit.as_deref(),
            Some(advisory.commit.as_str())
        );
        assert!(state.terminal.is_some(), "the formal Audit did not decide");
        assert!(journal_count(&store, &effort, "build_resolved") >= 1);
    }

    /// R-032/R-033: the session this dispatch asked for is recorded separately
    /// from the session the provider reported, so a fresh dispatch is never
    /// recorded as a resumed one and a continuation keeps its requested identity.
    #[test]
    fn invocation_records_keep_requested_and_observed_sessions_apart() {
        let (store, effort, repo) = prepared_with_config(ATTRIBUTED_CONFIG);
        let host = ScenarioHost::reporting_distinct_sessions(Scenario::InterruptedTwice);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        // Every dispatch, in the order the actions were recorded, with the two
        // session facts kept apart.
        let mut dispatches = Vec::new();
        for (action_id, packet) in action_packets(&build_dir) {
            if packet["kind"] != "work" {
                continue;
            }
            let record: serde_json::Value = read_json(
                &build_dir
                    .join("artifacts")
                    .join(&action_id)
                    .join("invocation.json"),
            )
            .unwrap();
            dispatches.push((
                record["mode"].as_str().unwrap().to_owned(),
                record["session"]["requested"].clone(),
                record["session"]["observed"].clone(),
                record["session"]["resumed"].clone(),
            ));
        }
        let fresh = dispatches
            .iter()
            .find(|(mode, ..)| mode == "fresh")
            .expect("no fresh dispatch was recorded");
        // The decisive regression: a fresh dispatch is fresh even though the
        // provider then reported a session of its own.
        assert_eq!(fresh.1, serde_json::Value::Null);
        assert_eq!(fresh.3, false);
        let observed = fresh
            .2
            .as_str()
            .expect("the provider's own session was not recorded")
            .to_owned();
        assert!(
            observed.contains("worker-provider-session"),
            "the observed session is not the provider's: {observed}"
        );
        // A continuation asked for a session that an earlier dispatch observed,
        // and the session it reported back is its own, not the predecessor's.
        let continued = dispatches
            .iter()
            .find(|(mode, ..)| mode == "resumed")
            .expect("no continued dispatch was recorded");
        let requested = continued
            .1
            .as_str()
            .expect("the continuation lost the session it was asked to resume");
        assert!(
            dispatches
                .iter()
                .any(|(_, _, seen, _)| seen.as_str() == Some(requested)),
            "the continuation resumed a session no earlier dispatch observed: {requested}"
        );
        assert_eq!(continued.3, true);
        assert_ne!(
            continued.2.as_str(),
            Some(requested),
            "the continuation reported the predecessor's session as its own"
        );
        // A replacement starts a new session: its mode comes from the handoff,
        // not from whether a session happened to be requested.
        if let Some(replacement) = dispatches.iter().find(|(mode, ..)| mode == "replacement") {
            assert_eq!(replacement.1, serde_json::Value::Null);
            assert_eq!(replacement.3, false);
        }
    }

    /// R-032: a launch that never started still keeps its passive record, with
    /// the failure represented explicitly instead of omitted.
    #[test]
    fn a_failed_launch_still_keeps_its_invocation_record() {
        let (store, effort, repo) = prepared_with_config(ONCE_OVER_CONFIG);
        let host = ScenarioHost::new(Scenario::OnceOverSpawnFailure);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        let state = read_state(&store, &effort);
        let advisory = state.once_over.first().expect("no advisory record");
        assert_eq!(advisory.outcome, "spawn_failed");
        let record: serde_json::Value = read_json(
            &Path::new(&advisory.report)
                .parent()
                .unwrap()
                .join("invocation.json"),
        )
        .unwrap();
        assert_eq!(record["completion"], "spawn_failed");
        assert!(record["exit"].is_null());
        assert!(
            record["completion_detail"]
                .as_str()
                .unwrap()
                .contains("cannot launch")
        );
        assert!(record["timing"]["elapsed_ms"].is_u64());
        assert_eq!(record["action"]["kind"], "once_over");
        assert_eq!(record["role"], "once_over");
    }

    /// R-035: a same-commit re-verification does not run the once-over again.
    #[test]
    fn same_commit_reverification_does_not_rerun_the_once_over() {
        let (store, effort, repo) = prepared_with_config(ONCE_OVER_CONFIG);
        let host = ScenarioHost::new(Scenario::BlockedAuditExternalEvidence);
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Blocked { .. }
        ));
        assert_eq!(host.call_count("once_over"), 1);
        let evidence = temp("live-evidence");
        fs::write(evidence.join("verification.txt"), "live verification\n").unwrap();
        resolve_stop(
            &store,
            &effort,
            ResolutionKind::NewVerificationEvidence,
            "live verification evidence supplied",
            false,
            None,
            vec![evidence.join("verification.txt")],
        )
        .unwrap();
        assert!(matches!(
            run_with_adapter(&store, request(&repo), &host).unwrap(),
            BuildResult::Completed(_)
        ));
        assert_eq!(
            host.call_count("once_over"),
            1,
            "the same commit was advised twice"
        );
        assert_eq!(host.call_count("final_audit"), 2);
    }
}
