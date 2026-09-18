//! Versioned public contracts for Orchestrate phase boundaries.
//!
//! This crate deliberately contains no store, source checkout, provider, or phase-runtime code.

use std::collections::HashSet;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Deserializer, Serialize, de::Visitor};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 1;
pub const PRODUCT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ArtifactRef {
    pub kind: ArtifactKind,
    pub artifact_id: String,
    pub digest: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    DiscoveryOpinion,
    ConsensusComparison,
    Agreement,
    Adoption,
    Implementation,
    Audit,
    Invalidation,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PayloadDigest {
    pub path: String,
    pub sha256: String,
    pub bytes: u64,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Provenance {
    pub host: String,
    pub execution_provider: Option<String>,
    pub requested_model: Option<String>,
    pub observed_model: Option<String>,
    pub requested_effort: Option<String>,
    pub observed_effort: Option<String>,
    pub guide_digest: String,
    pub independence: Independence,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Independence {
    InputExcludedCooperative,
    AccessEnforced,
    Compromised,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Envelope {
    pub schema_version: u32,
    pub kind: ArtifactKind,
    pub artifact_id: String,
    pub run_id: String,
    pub project_id: String,
    pub effort_id: String,
    pub cohort_id: Option<String>,
    pub outcome: String,
    pub created_at_ms: u128,
    pub finalized_at_ms: u128,
    pub producer_version: String,
    pub parents: Vec<ArtifactRef>,
    pub payloads: Vec<PayloadDigest>,
    pub provenance: Provenance,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Requirement {
    pub id: String,
    pub text: String,
    pub acceptance: String,
    #[serde(default)]
    pub condition: Option<String>,
    #[serde(default)]
    pub governing: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct CheckDisposition {
    pub category: String,
    pub disposition: Disposition,
    pub rationale: String,
    #[serde(default)]
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Completed,
    NotApplicable,
    Unavailable,
    Inaccessible,
    Pending,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Opinion {
    pub context_id: String,
    pub baseline_commit: String,
    pub slot: String,
    pub recommendation: String,
    pub observed_behavior: String,
    pub requirements: Vec<Requirement>,
    pub checks: Vec<CheckDisposition>,
    pub blockers: Vec<String>,
    pub limitations: Vec<String>,
    pub adversarial_review: String,
    pub review_current: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Position {
    Supports,
    Contradicts,
    Related,
    NotObserved,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct PositionRecord {
    pub requirement: Requirement,
    /// One position for each of the three original slots, keyed by `a`, `b`, and `c`.
    pub positions: std::collections::BTreeMap<String, Position>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ConsensusProposal {
    pub positions: Vec<PositionRecord>,
    pub adopted_requirement_ids: Vec<String>,
    pub selection_rationale: String,
    pub dissent: Vec<String>,
    pub counterexample_blocks: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Agreement {
    pub context_id: String,
    pub baseline_commit: String,
    pub goal: String,
    pub requirements: Vec<Requirement>,
    pub exclusions: Vec<String>,
    pub implementation_latitude: Vec<String>,
    pub verification_expectations: Vec<String>,
    pub common_supporters: Vec<String>,
    pub dissent: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Adoption {
    pub agreement: ArtifactRef,
    pub authorized_by: String,
    pub execution_scope: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Invalidation {
    pub target: ArtifactRef,
    pub reason: String,
    pub declared_by: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Implementation {
    pub adoption: ArtifactRef,
    pub agreement: ArtifactRef,
    pub starting_baseline: String,
    pub target_commit: String,
    pub target_tree: String,
    pub producer_declaration: String,
    pub status: String,
    pub declared_checks: Vec<String>,
    pub deviations: Vec<String>,
    pub unresolved_questions: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    Supported,
    Violated,
    Unresolved,
    NotApplicable,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Coverage {
    pub requirement_id: String,
    pub state: CoverageState,
    pub rationale: String,
    pub evidence: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FindingKind {
    ImplementationDefect,
    VerificationGap,
    AuthorityDefect,
    OptionalObservation,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Finding {
    pub kind: FindingKind,
    pub requirement_id: Option<String>,
    pub evidence: String,
    pub consequence: String,
    pub bounded_correction: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AuditAssessment {
    pub agreement: ArtifactRef,
    pub adoption: ArtifactRef,
    pub implementation: ArtifactRef,
    pub coverage: Vec<Coverage>,
    pub findings: Vec<Finding>,
    pub assessor_context: String,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AuditReport {
    #[serde(flatten)]
    pub assessment: AuditAssessment,
    pub verdict: Verdict,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Verdict {
    Pass,
    ChangesRequired,
    Blocked,
}

pub fn digest_bytes(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec_pretty(value)?
        .into_iter()
        .chain(*b"\n")
        .collect())
}

pub fn decode<T: for<'a> Deserialize<'a>>(bytes: &[u8]) -> Result<T> {
    reject_duplicate_json_keys(bytes)?;
    serde_json::from_slice(bytes).context("invalid JSON payload")
}

pub fn artifact_ref(envelope: &Envelope, manifest_bytes: &[u8]) -> ArtifactRef {
    ArtifactRef {
        kind: envelope.kind.clone(),
        artifact_id: envelope.artifact_id.clone(),
        digest: digest_bytes(manifest_bytes),
    }
}

pub fn validate_envelope(envelope: &Envelope) -> Result<()> {
    if envelope.schema_version != SCHEMA_VERSION {
        bail!(
            "unsupported artifact schema version {}",
            envelope.schema_version
        );
    }
    if envelope.artifact_id.is_empty() || envelope.run_id.is_empty() {
        bail!("artifact and run identity are required");
    }
    let mut paths = HashSet::new();
    for payload in &envelope.payloads {
        if payload.path.is_empty()
            || payload.path.starts_with('/')
            || payload
                .path
                .split('/')
                .any(|part| part == ".." || part.is_empty())
        {
            bail!("unsafe payload path: {}", payload.path);
        }
        if !paths.insert(&payload.path) {
            bail!("duplicate payload path: {}", payload.path);
        }
    }
    Ok(())
}

pub fn validate_opinion(opinion: &Opinion) -> Result<()> {
    if !matches!(opinion.slot.as_str(), "a" | "b" | "c") {
        bail!("opinion slot must be a, b, or c");
    }
    if opinion.recommendation.trim().is_empty() || opinion.observed_behavior.trim().is_empty() {
        bail!("opinion needs a recommendation and observed behavior");
    }
    if opinion
        .blockers
        .iter()
        .any(|blocker| !blocker.trim().is_empty())
    {
        bail!("opinion has unresolved blocking product decision");
    }
    if !opinion.review_current || opinion.adversarial_review.trim().is_empty() {
        bail!("opinion lacks a current adversarial review");
    }
    let required = [
        "intent",
        "current_behavior",
        "contracts",
        "edge_cases",
        "verification",
        "risks",
    ];
    for category in required {
        let check = opinion
            .checks
            .iter()
            .find(|check| check.category == category);
        match check {
            Some(CheckDisposition {
                disposition:
                    Disposition::Completed
                    | Disposition::NotApplicable
                    | Disposition::Unavailable
                    | Disposition::Inaccessible,
                rationale,
                ..
            }) if !rationale.trim().is_empty() => {}
            _ => bail!("opinion missing a disposition for required research category {category}"),
        }
    }
    Ok(())
}

pub fn validate_agreement(agreement: &Agreement) -> Result<()> {
    if agreement.goal.trim().is_empty() || agreement.requirements.is_empty() {
        bail!("agreement needs a goal and at least one required behavior");
    }
    let mut ids = HashSet::new();
    for requirement in &agreement.requirements {
        if requirement.id.trim().is_empty()
            || requirement.text.trim().is_empty()
            || requirement.acceptance.trim().is_empty()
        {
            bail!("agreement requirements need id, text, and acceptance criteria");
        }
        if !ids.insert(&requirement.id) {
            bail!("duplicate agreement requirement id {}", requirement.id);
        }
    }
    if agreement.common_supporters.len() < 2 {
        bail!("agreement lacks a strict common majority");
    }
    Ok(())
}

pub fn derive_verdict(agreement: &Agreement, assessment: &AuditAssessment) -> Result<Verdict> {
    let required: HashSet<_> = agreement
        .requirements
        .iter()
        .map(|r| r.id.as_str())
        .collect();
    let mut covered = HashSet::new();
    let mut blocked = false;
    for row in &assessment.coverage {
        if !required.contains(row.requirement_id.as_str())
            || !covered.insert(row.requirement_id.as_str())
        {
            bail!(
                "audit coverage has unknown or duplicate requirement {}",
                row.requirement_id
            );
        }
        if matches!(row.state, CoverageState::Unresolved) {
            blocked = true;
        }
        if matches!(row.state, CoverageState::NotApplicable) && row.rationale.trim().is_empty() {
            bail!("not-applicable coverage needs a justification");
        }
    }
    if covered.len() != required.len() {
        blocked = true;
    }
    let defect = assessment
        .findings
        .iter()
        .any(|finding| matches!(finding.kind, FindingKind::ImplementationDefect));
    if assessment.findings.iter().any(|finding| {
        matches!(
            finding.kind,
            FindingKind::AuthorityDefect | FindingKind::VerificationGap
        )
    }) {
        blocked = true;
    }
    if defect {
        Ok(Verdict::ChangesRequired)
    } else if blocked {
        Ok(Verdict::Blocked)
    } else {
        Ok(Verdict::Pass)
    }
}

fn reject_duplicate_json_keys(bytes: &[u8]) -> Result<()> {
    struct Check;
    impl<'de> Deserialize<'de> for Check {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            deserializer.deserialize_any(CheckVisitor)
        }
    }
    struct CheckVisitor;
    impl<'de> Visitor<'de> for CheckVisitor {
        type Value = Check;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("JSON without duplicate keys")
        }
        fn visit_bool<E: serde::de::Error>(self, _: bool) -> Result<Self::Value, E> {
            Ok(Check)
        }
        fn visit_i64<E: serde::de::Error>(self, _: i64) -> Result<Self::Value, E> {
            Ok(Check)
        }
        fn visit_u64<E: serde::de::Error>(self, _: u64) -> Result<Self::Value, E> {
            Ok(Check)
        }
        fn visit_f64<E: serde::de::Error>(self, _: f64) -> Result<Self::Value, E> {
            Ok(Check)
        }
        fn visit_str<E: serde::de::Error>(self, _: &str) -> Result<Self::Value, E> {
            Ok(Check)
        }
        fn visit_string<E: serde::de::Error>(self, _: String) -> Result<Self::Value, E> {
            Ok(Check)
        }
        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(Check)
        }
        fn visit_unit<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(Check)
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut seq: A,
        ) -> Result<Self::Value, A::Error> {
            while let Some::<Check>(_value) = seq.next_element()? {}
            Ok(Check)
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> Result<Self::Value, A::Error> {
            let mut keys = HashSet::new();
            while let Some(key) = map.next_key::<String>()? {
                if !keys.insert(key.clone()) {
                    return Err(serde::de::Error::custom(format!(
                        "duplicate JSON key: {key}"
                    )));
                }
                let _: Check = map.next_value()?;
            }
            Ok(Check)
        }
    }
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    Check::deserialize(&mut deserializer).context("invalid JSON or duplicate object key")?;
    deserializer.end().context("trailing JSON content")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sha256_fixed_vector() {
        assert_eq!(
            digest_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
    #[test]
    fn duplicate_keys_fail() {
        assert!(decode::<serde_json::Value>(br#"{\"x\":1,\"x\":2}"#).is_err());
    }
}
