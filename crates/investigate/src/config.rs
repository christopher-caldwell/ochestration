use anyhow::{Context, Result, ensure};
use orchestrate_contracts::{
    ArtifactRef,
    investigation::{Mode, QuestionKind},
};
use orchestrate_core::provider::{ProviderConfig, prepare_invocation};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Config {
    pub schema_version: u32,
    pub mode: Mode,
    #[serde(default)]
    pub request: Option<PathBuf>,
    #[serde(default)]
    pub question_kind: Option<QuestionKind>,
    #[serde(default)]
    pub target: Option<PathBuf>,
    #[serde(default)]
    pub revision: Option<String>,
    #[serde(default)]
    pub inputs: Vec<PathBuf>,
    pub lanes: Vec<LaneConfig>,
    pub reconciler: ProviderConfig,
    #[serde(default)]
    pub completion: CompletionConfig,
    #[serde(default)]
    pub consensus: Option<ConsensusConfig>,
    #[serde(default)]
    pub max_parallel: Option<usize>,
    /// Bound each native lane/reconciler attempt; there is no automatic retry.
    // Omit the default so existing version-1 resolved config digests stay inspectable.
    #[serde(
        default = "default_provider_timeout_seconds",
        skip_serializing_if = "is_default_provider_timeout"
    )]
    pub provider_timeout_seconds: u64,
    #[serde(default)]
    pub audit: Option<AuditInputs>,
}
fn default_provider_timeout_seconds() -> u64 {
    30 * 60
}
fn is_default_provider_timeout(value: &u64) -> bool {
    *value == default_provider_timeout_seconds()
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaneConfig {
    pub count: usize,
    pub adapter: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
}
impl LaneConfig {
    pub fn provider(&self) -> ProviderConfig {
        ProviderConfig {
            adapter: self.adapter.clone(),
            model: self.model.clone(),
            effort: self.effort.clone(),
            args: self.args.clone(),
        }
    }
}
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CompletionConfig {
    pub min_completed: Option<usize>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ConsensusConfig {
    pub min_go_votes: usize,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AuditInputs {
    pub effort: String,
    #[serde(deserialize_with = "strict_reference")]
    pub reconciled: ArtifactRef,
    #[serde(deserialize_with = "strict_reference")]
    pub adoption: ArtifactRef,
    #[serde(deserialize_with = "strict_reference")]
    pub implementation: ArtifactRef,
}
impl Config {
    pub fn cohort_size(&self) -> usize {
        self.lanes.iter().map(|lane| lane.count).sum()
    }
    pub fn min_completed(&self) -> usize {
        self.completion
            .min_completed
            .unwrap_or_else(|| self.cohort_size())
    }
    pub fn question_kind(&self) -> QuestionKind {
        self.question_kind.clone().unwrap_or(QuestionKind::Binary)
    }
    pub fn validate(&self) -> Result<()> {
        ensure!(
            self.schema_version == 1,
            "unsupported investigation config version; expected 1"
        );
        let n = self.lanes.iter().try_fold(0usize, |n, lane| {
            ensure!(lane.count > 0, "lane count must be positive");
            n.checked_add(lane.count).context("lane count overflow")
        })?;
        ensure!(n > 0, "at least one lane is required");
        ensure!(
            (1..=n).contains(&self.min_completed()),
            "min_completed must be between 1 and the cohort size"
        );
        ensure!(
            self.max_parallel.is_none_or(|n| n > 0),
            "max_parallel must be positive"
        );
        ensure!(
            self.provider_timeout_seconds > 0
                && std::time::Instant::now()
                    .checked_add(std::time::Duration::from_secs(
                        self.provider_timeout_seconds
                    ))
                    .is_some(),
            "provider_timeout_seconds must be positive and within the supported clock range"
        );
        ensure!(
            self.audit.is_some() || self.request.is_some(),
            "a request file is required for a generic investigation"
        );
        ensure!(
            self.audit.is_some() || self.question_kind.is_some(),
            "generic requests require an explicit question_kind"
        );
        ensure!(
            self.audit.is_none() || self.question_kind() == QuestionKind::Binary,
            "conformance investigations require binary questions"
        );
        ensure!(
            self.target.is_some() || self.revision.is_none(),
            "revision requires target"
        );
        ensure!(
            self.audit.is_some() || self.target.is_none() || self.revision.is_some(),
            "repository inputs require an exact revision"
        );
        if let Some(revision) = &self.revision {
            ensure!(
                (revision.len() == 40 || revision.len() == 64)
                    && revision.bytes().all(|b| b.is_ascii_hexdigit()),
                "revision must be an exact full commit ID, not a moving reference"
            );
        }
        match self.mode {
            Mode::Consensus => {
                ensure!(
                    self.question_kind() == QuestionKind::Binary,
                    "consensus requires a binary question"
                );
                let threshold = self
                    .consensus
                    .as_ref()
                    .context("consensus requires min_go_votes")?
                    .min_go_votes;
                ensure!(
                    threshold > n / 2 && threshold <= n,
                    "min_go_votes must be greater than half the configured cohort and at most its size"
                );
            }
            Mode::Wide => ensure!(
                self.consensus.is_none(),
                "wide does not accept consensus configuration"
            ),
        }
        for provider in self
            .lanes
            .iter()
            .map(LaneConfig::provider)
            .chain(std::iter::once(self.reconciler.clone()))
        {
            ensure!(
                provider
                    .model
                    .as_deref()
                    .is_none_or(|s| !s.trim().is_empty()),
                "model cannot be empty"
            );
            ensure!(
                provider
                    .args
                    .as_deref()
                    .unwrap_or_default()
                    .iter()
                    .all(|s| !s.contains('\0')),
                "native arguments cannot contain NUL"
            );
            // Fresh investigation sessions cannot request provider continuity or add sibling directories.
            for arg in provider.args.as_deref().unwrap_or_default() {
                let flag = arg.split('=').next().unwrap_or(arg);
                let forbidden = matches!(
                    flag,
                    "--continue"
                        | "--session-id"
                        | "--fork-session"
                        | "--add-dir"
                        | "fork"
                        | "--worktree"
                        | "--worktree-base"
                ) || (provider.adapter == "claude" && matches!(flag, "-c" | "-r"))
                    || (provider.adapter == "codex" && flag == "-m")
                    || (provider.adapter == "cursor" && flag == "-w");
                ensure!(
                    !forbidden,
                    "investigation args cannot request session continuity or additional directories"
                );
            }
            prepare_invocation(&provider, "preflight", Path::new("/"), None, &[])?;
        }
        Ok(())
    }
}
pub fn load(path: &Path) -> Result<(Config, Vec<u8>)> {
    let bytes = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
    let mut config: Config = toml::from_str(std::str::from_utf8(&bytes)?)
        .context("invalid investigation config TOML")?;
    let parent = fs::canonicalize(path)?
        .parent()
        .context("config has no parent")?
        .to_owned();
    let resolve = |path: &mut PathBuf| {
        if path.is_relative() {
            *path = parent.join(&*path);
        }
    };
    if let Some(path) = &mut config.request {
        resolve(path);
    }
    if let Some(path) = &mut config.target {
        resolve(path);
    }
    for path in &mut config.inputs {
        resolve(path);
    }
    config.validate()?;
    Ok((config, bytes))
}

fn strict_reference<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<ArtifactRef, D::Error> {
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Reference {
        kind: orchestrate_contracts::ArtifactKind,
        artifact_id: String,
        digest: String,
    }
    let reference = Reference::deserialize(deserializer)?;
    Ok(ArtifactRef {
        kind: reference.kind,
        artifact_id: reference.artifact_id,
        digest: reference.digest,
    })
}
