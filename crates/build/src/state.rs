use anyhow::{Context, Result, bail, ensure};
use orchestrate_contracts::{ArtifactRef, decode, encode};
use orchestrate_core::write_bytes_sync;
use serde::{Deserialize, Serialize};
use std::{fs, path::Path};

pub const STATE_VERSION: u32 = 4;
pub const PLAN_VERSION: u32 = 3;
pub const CONFIG_VERSION: u32 = 4;

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BuildPlan {
    pub schema_version: u32,
    pub reconciled: ArtifactRef,
    pub detailed_plan: String,
    pub phases: Vec<PlanPhase>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlanPhase {
    pub id: String,
    pub tasks: Vec<String>,
    pub requirement_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BuildConfig {
    pub schema_version: u32,
    pub worker: RoleConfig,
    pub reviewer: RoleConfig,
    #[serde(default)]
    pub unblocker: Option<RoleConfig>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RoleConfig {
    pub adapter: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Gate {
    Work,
    Review,
    Audit,
    Unblock,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Scope {
    Phase { index: usize },
    Final,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    Ready,
    Running,
    Stopped,
    Complete,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Session {
    pub adapter: String,
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct FeedbackRef {
    pub path: String,
    pub purpose: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct UnblockContext {
    pub gate: Gate,
    pub scope: Scope,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StopKind {
    ResetRequired,
    ExternalRequirement,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Stop {
    pub kind: StopKind,
    pub detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct BuildCompletion {
    pub implementation: ArtifactRef,
    pub audit: ArtifactRef,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct BuildState {
    pub schema_version: u32,
    pub reconciled: ArtifactRef,
    pub adoption: ArtifactRef,
    pub build_start_commit: String,
    pub plan_digest: String,
    pub scope: Scope,
    pub gate: Gate,
    pub status: Status,
    pub checkpoint_commit: String,
    pub feedback: Vec<FeedbackRef>,
    pub unblock: Option<UnblockContext>,
    pub current_action_id: Option<String>,
    pub worker_session: Option<Session>,
    pub reviewer_session: Option<Session>,
    pub implementation: Option<ArtifactRef>,
    pub completion: Option<BuildCompletion>,
    pub stop: Option<Stop>,
}

pub fn load_state(path: &Path) -> Result<BuildState> {
    let bytes =
        fs::read(path).with_context(|| format!("cannot read Build state {}", path.display()))?;
    let version: serde_json::Value = decode(&bytes)?;
    let found = version
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    ensure!(
        found == STATE_VERSION as u64,
        "unsupported Build state schema version {found}; this Build requires schema {STATE_VERSION}. Historical state is intentionally unsupported; start a new Build from the current accepted artifacts."
    );
    let state: BuildState = decode(&bytes)?;
    ensure!(
        state.schema_version == STATE_VERSION,
        "unsupported Build state schema version {}",
        state.schema_version
    );
    Ok(state)
}

pub fn save_state(path: &Path, state: &BuildState) -> Result<()> {
    ensure!(
        state.schema_version == STATE_VERSION,
        "refusing to write unsupported Build state schema"
    );
    write_bytes_sync(path, &encode(state)?)
}

pub fn load_plan(path: &Path) -> Result<BuildPlan> {
    let bytes =
        fs::read(path).with_context(|| format!("cannot read Build plan {}", path.display()))?;
    let version: serde_json::Value = decode(&bytes)?;
    let found = version
        .get("schema_version")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    ensure!(
        found == PLAN_VERSION as u64,
        "unsupported Build plan schema version {found}; expected {PLAN_VERSION}. Regenerate the Build plan; migration is not supported."
    );
    let plan: BuildPlan = decode(&bytes)?;
    ensure!(
        plan.schema_version == PLAN_VERSION,
        "unsupported Build plan schema version {}",
        plan.schema_version
    );
    ensure!(
        !plan.phases.is_empty(),
        "Build plan must contain at least one phase"
    );
    Ok(plan)
}

pub fn load_config(path: &Path) -> Result<BuildConfig> {
    let text = fs::read_to_string(path)
        .with_context(|| format!("cannot read Build config {}", path.display()))?;
    let version: toml::Value = toml::from_str(&text).context("invalid Build config TOML")?;
    let found = version
        .get("schema_version")
        .and_then(toml::Value::as_integer)
        .unwrap_or(0);
    ensure!(
        found == CONFIG_VERSION as i64,
        "unsupported Build config schema version {found}; expected {CONFIG_VERSION}. Update config.toml; migration is not supported."
    );
    let config: BuildConfig = toml::from_str(&text).context("invalid Build config")?;
    ensure!(
        config.schema_version == CONFIG_VERSION,
        "unsupported Build config schema version {}",
        config.schema_version
    );
    for role in std::iter::once(&config.worker)
        .chain(std::iter::once(&config.reviewer))
        .chain(config.unblocker.iter())
    {
        ensure!(
            matches!(role.adapter.as_str(), "codex" | "claude" | "cursor"),
            "unsupported Build adapter {:?}; expected codex, claude, or cursor",
            role.adapter
        );
        ensure!(
            role.model
                .as_deref()
                .is_none_or(|model| !model.trim().is_empty()),
            "native model cannot be empty"
        );
        ensure!(
            role.args
                .as_ref()
                .is_none_or(|args| args.iter().all(|arg| !arg.contains('\0'))),
            "native arguments cannot contain NUL bytes"
        );
    }
    Ok(config)
}

pub fn current_action_dir(build_dir: &Path, id: &str) -> Result<std::path::PathBuf> {
    ensure!(
        !id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'),
        "invalid action id"
    );
    Ok(build_dir.join("actions").join(id))
}

pub fn validate_feedback_path(path: &str) -> Result<()> {
    ensure!(
        orchestrate_contracts::safe_relative_path(path),
        "unsafe feedback path {path}"
    );
    ensure!(
        !Path::new(path).is_absolute(),
        "feedback path must be relative"
    );
    if path.split('/').any(|part| part == ".") {
        bail!("feedback path must be normalized");
    }
    let parts = path.split('/').collect::<Vec<_>>();
    ensure!(
        parts.len() == 3
            && parts[0] == "actions"
            && !parts[1].is_empty()
            && parts[1]
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || character == '-')
            && matches!(parts[2], "report.md" | "assessment.json"),
        "feedback must reference one controller report or assessment under an action directory"
    );
    Ok(())
}

pub fn read_build_file(build_dir: &Path, relative: &str) -> Result<Vec<u8>> {
    ensure!(
        orchestrate_contracts::safe_relative_path(relative)
            && !relative.contains('\\')
            && !relative.contains(':'),
        "Build file path must be a normalized relative path"
    );
    let parts = relative.split('/').collect::<Vec<_>>();
    ensure!(
        parts.iter().all(|part| !part.is_empty() && *part != "."),
        "Build file path must be normalized"
    );
    let root = fs::canonicalize(build_dir)?;
    let mut path = root.clone();
    for (index, part) in parts.iter().enumerate() {
        path.push(part);
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("cannot read Build file {}", path.display()))?;
        ensure!(
            !metadata.file_type().is_symlink(),
            "Build file path contains a symlink: {}",
            path.display()
        );
        if index + 1 < parts.len() {
            ensure!(metadata.is_dir(), "Build path component is not a directory");
        } else {
            ensure!(metadata.is_file(), "Build path is not a regular file");
        }
    }
    let canonical = fs::canonicalize(&path)?;
    ensure!(
        canonical.starts_with(root),
        "Build file escapes the Build directory"
    );
    Ok(fs::read(canonical)?)
}
