//! Independent review contracts; these do not publish canonical Audit artifacts.
use crate::{ArtifactRef, AuditAssessment, PayloadDigest};
use serde::{Deserialize, Serialize};

pub const INVESTIGATION_VERSION: u32 = 1;
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Consensus,
    Wide,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum QuestionKind {
    Binary,
    OpenEnded,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Position {
    Go,
    NoGo,
    Unknown,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ObservationKind {
    Inspection,
    Execution,
    Testimony,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceLocation {
    pub source_id: String,
    pub path: String,
    #[serde(default)]
    pub line: Option<usize>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Observation {
    pub id: String,
    pub kind: ObservationKind,
    #[serde(default)]
    pub source: Option<SourceLocation>,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub receipt: Option<String>,
    pub observed: String,
    pub environment: String,
    #[serde(default)]
    pub depends_on: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    pub id: String,
    pub claim: String,
    pub scope: String,
    pub applicability: String,
    pub impact: String,
    pub material: bool,
    pub negative: bool,
    pub uncertainty: String,
    pub depends_on: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Challenge {
    pub id: String,
    pub targets: Vec<String>,
    pub reason: String,
    pub material: bool,
    pub limitations: String,
    pub depends_on: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Conclusion {
    pub id: String,
    pub answer: String,
    #[serde(default)]
    pub position: Option<Position>,
    pub depends_on: Vec<String>,
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaneGraph {
    pub schema_version: u32,
    pub lane_id: String,
    pub input_digest: String,
    pub observations: Vec<Observation>,
    pub findings: Vec<Finding>,
    pub challenges: Vec<Challenge>,
    pub conclusion: Conclusion,
    #[serde(default)]
    pub assessment: Option<AuditAssessment>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CommandReceipt {
    pub id: String,
    pub transport_line: usize,
    pub command: String,
    pub cwd: Option<String>,
    pub exit_code: Option<i32>,
    pub output: String,
    pub completed: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Supported,
    Contested,
    InsufficientEvidence,
    Rejected,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CanonicalFinding {
    pub id: String,
    pub proposition: String,
    pub scope: String,
    pub origins: Vec<String>,
    pub supporting_observations: Vec<String>,
    pub contradicting_observations: Vec<String>,
    pub disposition: Disposition,
    pub rationale: String,
    pub material: bool,
    pub negative: bool,
    pub demonstrated: bool,
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ChallengeDisposition {
    pub challenge: String,
    pub disposition: Disposition,
    pub observations: Vec<String>,
    pub rationale: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Reconciliation {
    pub schema_version: u32,
    pub input_digest: String,
    pub findings: Vec<CanonicalFinding>,
    pub challenges: Vec<ChallengeDisposition>,
    pub essential_findings: Vec<String>,
    pub answer: String,
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LaneState {
    Completed,
    Invalid,
    Failed,
    Unavailable,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LaneRecord {
    pub lane_id: String,
    pub state: LaneState,
    pub detail: String,
    pub position: Option<Position>,
    pub manifest_digest: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct VoteResult {
    pub go: usize,
    pub no_go: usize,
    pub unknown: usize,
    pub min_go_votes: usize,
    pub threshold_met: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Recommendation {
    Go,
    NoGo,
    ChangesRequired,
    ReviewRequired,
    Inconclusive,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Completion {
    Complete,
    Incomplete,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InvestigationResult {
    pub schema_version: u32,
    pub run_id: String,
    pub input_digest: String,
    pub mode: Mode,
    pub completion: Completion,
    pub requested: usize,
    pub completed: usize,
    pub invalid: usize,
    pub unavailable: usize,
    pub failed: usize,
    pub min_completed: usize,
    pub lanes: Vec<LaneRecord>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub votes: Option<VoteResult>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation: Option<Recommendation>,
    pub reconciliation: Option<Reconciliation>,
    pub limitations: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct InvestigationManifest {
    pub schema_version: u32,
    pub identity: String,
    pub input_digest: String,
    pub payloads: Vec<PayloadDigest>,
    pub audit_parents: Vec<ArtifactRef>,
}
