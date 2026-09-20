//! Versioned public contracts for Orchestrate phase boundaries.

use std::collections::HashSet;

use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Deserializer, Serialize, de::Visitor};
use sha2::{Digest, Sha256};

pub const SCHEMA_VERSION: u32 = 5;
pub const STORE_FORMAT_VERSION: u32 = 5;
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
    Discovery,
    ReconciledDiscovery,
    Adoption,
    Implementation,
    Audit,
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
    pub provider: Option<String>,
    pub model: Option<String>,
    pub model_effort: Option<String>,
    pub guide_digest: String,
    pub independence: Independence,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Independence {
    InputExcluded,
    Compromised,
    Unknown,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RequestKind {
    Ticket,
    Freeform,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DiscoveryRun {
    pub run_id: String,
    pub phase: String,
    pub effort_id: String,
    pub context_id: String,
    pub request_kind: RequestKind,
    pub baseline_commit: String,
    pub baseline_tree: String,
    pub host: String,
    pub provider: Option<String>,
    pub model: Option<String>,
    pub model_effort: Option<String>,
    pub independence: Independence,
    pub guide_digest: String,
}

impl DiscoveryRun {
    pub fn provenance(&self) -> Provenance {
        Provenance {
            host: self.host.clone(),
            provider: self.provider.clone(),
            model: self.model.clone(),
            model_effort: self.model_effort.clone(),
            guide_digest: self.guide_digest.clone(),
            independence: self.independence.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Envelope {
    pub schema_version: u32,
    pub kind: ArtifactKind,
    pub artifact_id: String,
    pub run_id: String,
    pub project_id: String,
    pub effort_id: String,
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
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    Question,
    Finding,
    Decision,
    Requirement,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStatus {
    Open,
    Answered,
    NoChange,
    Blocked,
    Accepted,
    Rejected,
    Invalidated,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct EvidenceNode {
    pub id: String,
    pub kind: EvidenceKind,
    pub status: EvidenceStatus,
    #[serde(default)]
    pub depends_on: Vec<String>,
    #[serde(default)]
    pub sources: Vec<String>,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub mandatory: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DiscoverySummary {
    pub context_id: String,
    pub baseline_commit: String,
    pub baseline_tree: String,
    pub outcome: String,
    pub node_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct DiscoverySourceRef {
    pub discovery_artifact_id: String,
    #[serde(default)]
    pub node_id: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReconciledRequirement {
    pub requirement: Requirement,
    #[serde(default)]
    pub source_refs: Vec<DiscoverySourceRef>,
    #[serde(default)]
    pub user_clarification: Option<String>,
    #[serde(default)]
    pub frozen_user_constraint: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct TechnicalSuggestion {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub source_refs: Vec<DiscoverySourceRef>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ReconcileProposal {
    pub core_result: String,
    pub requirements: Vec<ReconciledRequirement>,
    #[serde(default)]
    pub technical_suggestions: Vec<TechnicalSuggestion>,
    #[serde(default)]
    pub blocking_issues: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct ReconciledDiscovery {
    pub reconciled_id: String,
    pub context_id: String,
    pub baseline_commit: String,
    pub baseline_tree: String,
    pub goal: String,
    pub core_result: String,
    pub requirements: Vec<ReconciledRequirement>,
    #[serde(default)]
    pub technical_suggestions: Vec<TechnicalSuggestion>,
    #[serde(default)]
    pub blocking_issues: Vec<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Adoption {
    pub reconciled: ArtifactRef,
    pub authorization_label: String,
    pub adopted_at_ms: u128,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ImplementationStatus {
    Submitted,
    Partial,
    Blocked,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Implementation {
    pub adoption: ArtifactRef,
    pub reconciled: ArtifactRef,
    pub starting_baseline: String,
    pub target_commit: String,
    pub target_tree: String,
    pub producer_declaration: String,
    pub status: ImplementationStatus,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CoverageState {
    Pass,
    Fail,
    Unknown,
    NotApplicable,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct Coverage {
    pub requirement_id: String,
    pub state: CoverageState,
    pub rationale: String,
    pub evidence: Vec<String>,
    pub correction: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub struct AuditAssessment {
    pub reconciled: ArtifactRef,
    pub adoption: ArtifactRef,
    pub implementation: ArtifactRef,
    pub coverage: Vec<Coverage>,
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
pub fn safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.split('/').any(|part| part.is_empty() || part == "..")
}

pub fn validate_envelope(envelope: &Envelope) -> Result<()> {
    ensure!(
        envelope.schema_version == SCHEMA_VERSION,
        "unsupported artifact schema version {}",
        envelope.schema_version
    );
    ensure!(
        !envelope.artifact_id.is_empty() && !envelope.run_id.is_empty(),
        "artifact and run identity are required"
    );
    let mut paths = HashSet::new();
    for payload in &envelope.payloads {
        ensure!(
            safe_relative_path(&payload.path),
            "unsafe payload path: {}",
            payload.path
        );
        ensure!(
            paths.insert(&payload.path),
            "duplicate payload path: {}",
            payload.path
        );
    }
    Ok(())
}
pub fn validate_evidence_node(node: &EvidenceNode) -> Result<()> {
    let prefix = match node.kind {
        EvidenceKind::Question => "Q-",
        EvidenceKind::Finding => "F-",
        EvidenceKind::Decision => "D-",
        EvidenceKind::Requirement => "R-",
    };
    ensure!(
        node.id.starts_with(prefix)
            && node.id.len() > prefix.len()
            && node
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')),
        "invalid evidence node id {}",
        node.id
    );
    let valid = matches!(
        (&node.kind, &node.status),
        (
            EvidenceKind::Question,
            EvidenceStatus::Open
                | EvidenceStatus::Answered
                | EvidenceStatus::NoChange
                | EvidenceStatus::Blocked
        ) | (
            EvidenceKind::Finding,
            EvidenceStatus::Accepted | EvidenceStatus::Rejected | EvidenceStatus::Invalidated
        ) | (
            EvidenceKind::Decision | EvidenceKind::Requirement,
            EvidenceStatus::Accepted | EvidenceStatus::Rejected
        )
    );
    ensure!(valid, "invalid status for evidence node {}", node.id);
    ensure!(
        !node.required || matches!(node.kind, EvidenceKind::Question),
        "only questions may be required"
    );
    ensure!(
        !node.mandatory || matches!(node.kind, EvidenceKind::Requirement),
        "only requirements may be mandatory"
    );
    Ok(())
}
pub fn validate_requirement(requirement: &Requirement) -> Result<()> {
    ensure!(
        !requirement.id.trim().is_empty()
            && !requirement.text.trim().is_empty()
            && !requirement.acceptance.trim().is_empty(),
        "requirements need id, text, and acceptance criteria"
    );
    Ok(())
}
pub fn validate_reconciled_discovery(reconciled: &ReconciledDiscovery) -> Result<()> {
    ensure!(
        !reconciled.reconciled_id.is_empty()
            && !reconciled.goal.trim().is_empty()
            && !reconciled.core_result.trim().is_empty(),
        "reconciled Discovery needs identity, goal, and a core result"
    );
    let mut ids = HashSet::new();
    for item in &reconciled.requirements {
        validate_requirement(&item.requirement)?;
        ensure!(
            ids.insert(&item.requirement.id),
            "duplicate reconciled requirement id {}",
            item.requirement.id
        );
        match (&item.user_clarification, item.frozen_user_constraint) {
            (None, false) => ensure!(
                !item.requirement.governing && !item.source_refs.is_empty(),
                "Discovery-derived requirement {} needs Discovery authority and cannot be governing",
                item.requirement.id
            ),
            (Some(clarification), false) => ensure!(
                item.requirement.governing
                    && !clarification.trim().is_empty()
                    && item.source_refs.is_empty(),
                "user-clarification requirement {} needs explicit governing authority",
                item.requirement.id
            ),
            (None, true) => ensure!(
                item.requirement.governing && item.source_refs.is_empty(),
                "frozen user constraint {} needs governing authority",
                item.requirement.id
            ),
            (Some(_), true) => bail!(
                "requirement {} cannot be both a user clarification and frozen user constraint",
                item.requirement.id
            ),
        }
    }
    let mut suggestion_ids = HashSet::new();
    for suggestion in &reconciled.technical_suggestions {
        ensure!(
            !suggestion.id.trim().is_empty() && !suggestion.text.trim().is_empty(),
            "technical suggestions need an id and text"
        );
        ensure!(
            suggestion_ids.insert(&suggestion.id),
            "duplicate technical suggestion id {}",
            suggestion.id
        );
    }
    Ok(())
}
pub fn derive_verdict(
    reconciled: &ReconciledDiscovery,
    assessment: &AuditAssessment,
    implementation_status: &ImplementationStatus,
) -> Result<Verdict> {
    let required: HashSet<_> = reconciled
        .requirements
        .iter()
        .map(|item| item.requirement.id.as_str())
        .collect();
    let mut covered = HashSet::new();
    let mut blocked = false;
    let mut failed = false;
    for row in &assessment.coverage {
        ensure!(
            required.contains(row.requirement_id.as_str())
                && covered.insert(row.requirement_id.as_str()),
            "audit coverage has unknown or duplicate requirement {}",
            row.requirement_id
        );
        match row.state {
            CoverageState::Pass => ensure!(
                row.evidence
                    .iter()
                    .any(|reference| !reference.trim().is_empty()),
                "pass coverage needs at least one evidence reference"
            ),
            CoverageState::Fail => {
                ensure!(
                    row.evidence
                        .iter()
                        .any(|reference| !reference.trim().is_empty()),
                    "fail coverage needs at least one evidence reference"
                );
                ensure!(
                    !row.correction.trim().is_empty(),
                    "fail coverage needs a correction"
                );
                failed = true;
            }
            CoverageState::Unknown => blocked = true,
            CoverageState::NotApplicable if row.rationale.trim().is_empty() => {
                bail!("not-applicable coverage needs a justification")
            }
            _ => {}
        }
    }
    if covered.len() != required.len() {
        blocked = true;
    }
    if failed {
        Ok(Verdict::ChangesRequired)
    } else if blocked || !matches!(implementation_status, ImplementationStatus::Submitted) {
        Ok(Verdict::Blocked)
    } else {
        Ok(Verdict::Pass)
    }
}

fn reject_duplicate_json_keys(bytes: &[u8]) -> Result<()> {
    struct Check;
    impl<'de> Deserialize<'de> for Check {
        fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
            d.deserialize_any(CheckVisitor)
        }
    }
    struct CheckVisitor;
    impl<'de> Visitor<'de> for CheckVisitor {
        type Value = Check;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("JSON without duplicate keys")
        }
        fn visit_bool<E: serde::de::Error>(self, _: bool) -> std::result::Result<Check, E> {
            Ok(Check)
        }
        fn visit_i64<E: serde::de::Error>(self, _: i64) -> std::result::Result<Check, E> {
            Ok(Check)
        }
        fn visit_u64<E: serde::de::Error>(self, _: u64) -> std::result::Result<Check, E> {
            Ok(Check)
        }
        fn visit_f64<E: serde::de::Error>(self, _: f64) -> std::result::Result<Check, E> {
            Ok(Check)
        }
        fn visit_str<E: serde::de::Error>(self, _: &str) -> std::result::Result<Check, E> {
            Ok(Check)
        }
        fn visit_string<E: serde::de::Error>(self, _: String) -> std::result::Result<Check, E> {
            Ok(Check)
        }
        fn visit_none<E: serde::de::Error>(self) -> std::result::Result<Check, E> {
            Ok(Check)
        }
        fn visit_unit<E: serde::de::Error>(self) -> std::result::Result<Check, E> {
            Ok(Check)
        }
        fn visit_seq<A: serde::de::SeqAccess<'de>>(
            self,
            mut s: A,
        ) -> std::result::Result<Check, A::Error> {
            while s.next_element::<Check>()?.is_some() {}
            Ok(Check)
        }
        fn visit_map<A: serde::de::MapAccess<'de>>(
            self,
            mut map: A,
        ) -> std::result::Result<Check, A::Error> {
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
    let mut d = serde_json::Deserializer::from_slice(bytes);
    Check::deserialize(&mut d).context("invalid JSON or duplicate object key")?;
    d.end().context("trailing JSON content")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reconciled() -> ReconciledDiscovery {
        ReconciledDiscovery {
            reconciled_id: "reconciled".into(),
            context_id: "context".into(),
            baseline_commit: "baseline".into(),
            baseline_tree: "tree".into(),
            goal: "goal".into(),
            core_result: "result".into(),
            requirements: vec!["R-1", "R-2"]
                .into_iter()
                .map(|id| ReconciledRequirement {
                    requirement: Requirement {
                        id: id.into(),
                        text: "required behavior".into(),
                        acceptance: "proof".into(),
                        condition: None,
                        governing: false,
                    },
                    source_refs: vec![DiscoverySourceRef {
                        discovery_artifact_id: "discovery".into(),
                        node_id: None,
                    }],
                    user_clarification: None,
                    frozen_user_constraint: false,
                })
                .collect(),
            technical_suggestions: vec![],
            blocking_issues: vec![],
        }
    }

    fn assessment(coverage: Vec<Coverage>) -> AuditAssessment {
        let reference = |kind: ArtifactKind, id: &str| ArtifactRef {
            kind,
            artifact_id: id.into(),
            digest: "d".repeat(64),
        };
        AuditAssessment {
            reconciled: reference(ArtifactKind::ReconciledDiscovery, "reconciled"),
            adoption: reference(ArtifactKind::Adoption, "adoption"),
            implementation: reference(ArtifactKind::Implementation, "implementation"),
            coverage,
            assessor_context: "test".into(),
        }
    }

    fn row(id: &str, state: CoverageState) -> Coverage {
        Coverage {
            requirement_id: id.into(),
            state,
            rationale: "assessed".into(),
            evidence: vec!["test".into()],
            correction: String::new(),
        }
    }

    #[test]
    fn duplicate_keys_fail() {
        assert!(decode::<serde_json::Value>(br#"{\"x\":1,\"x\":2}"#).is_err());
    }

    #[test]
    fn legacy_resolved_question_status_is_rejected() {
        assert!(serde_json::from_str::<EvidenceStatus>("\"resolved\"").is_err());
        assert_eq!(
            serde_json::from_str::<EvidenceStatus>("\"answered\"").unwrap(),
            EvidenceStatus::Answered
        );
    }

    #[test]
    fn failed_requirement_cannot_pass() {
        let mut failed = row("R-1", CoverageState::Fail);
        failed.correction = "Fix the required behavior.".into();
        assert_eq!(
            derive_verdict(
                &reconciled(),
                &assessment(vec![failed, row("R-2", CoverageState::Pass)]),
                &ImplementationStatus::Submitted,
            )
            .unwrap(),
            Verdict::ChangesRequired
        );
    }

    #[test]
    fn passed_coverage_needs_evidence() {
        let mut passed = row("R-1", CoverageState::Pass);
        passed.evidence = vec![" ".into()];
        assert!(
            derive_verdict(
                &reconciled(),
                &assessment(vec![passed, row("R-2", CoverageState::Pass)]),
                &ImplementationStatus::Submitted,
            )
            .is_err()
        );
    }

    #[test]
    fn failed_coverage_needs_evidence() {
        let mut failed = row("R-1", CoverageState::Fail);
        failed.evidence = vec![];
        failed.correction = "Fix the requirement.".into();
        assert!(
            derive_verdict(
                &reconciled(),
                &assessment(vec![failed, row("R-2", CoverageState::Pass)]),
                &ImplementationStatus::Submitted,
            )
            .is_err()
        );
    }

    #[test]
    fn failed_coverage_needs_a_correction() {
        let failed = row("R-1", CoverageState::Fail);
        assert!(
            derive_verdict(
                &reconciled(),
                &assessment(vec![failed, row("R-2", CoverageState::Pass)]),
                &ImplementationStatus::Submitted,
            )
            .is_err()
        );
    }

    #[test]
    fn unknown_requirement_cannot_pass() {
        assert_eq!(
            derive_verdict(
                &reconciled(),
                &assessment(vec![
                    row("R-1", CoverageState::Unknown),
                    row("R-2", CoverageState::Pass)
                ]),
                &ImplementationStatus::Submitted,
            )
            .unwrap(),
            Verdict::Blocked
        );
    }

    #[test]
    fn missing_coverage_cannot_pass() {
        assert_eq!(
            derive_verdict(
                &reconciled(),
                &assessment(vec![row("R-1", CoverageState::Pass)]),
                &ImplementationStatus::Submitted,
            )
            .unwrap(),
            Verdict::Blocked
        );
    }

    #[test]
    fn partial_implementation_cannot_pass() {
        assert_eq!(
            derive_verdict(
                &reconciled(),
                &assessment(vec![
                    row("R-1", CoverageState::Pass),
                    row("R-2", CoverageState::Pass)
                ]),
                &ImplementationStatus::Partial,
            )
            .unwrap(),
            Verdict::Blocked
        );
    }

    #[test]
    fn passed_and_justified_not_applicable_requirements_can_pass() {
        let mut not_applicable = row("R-2", CoverageState::NotApplicable);
        not_applicable.rationale = "requirement does not apply to this target".into();
        assert_eq!(
            derive_verdict(
                &reconciled(),
                &assessment(vec![row("R-1", CoverageState::Pass), not_applicable]),
                &ImplementationStatus::Submitted,
            )
            .unwrap(),
            Verdict::Pass
        );
    }
}
