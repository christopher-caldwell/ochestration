use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

use orchestrate_contracts::{
    ArtifactRef, AuditAssessment, CheckDisposition, ConsensusProposal, Coverage, CoverageState,
    Disposition, Finding, FindingKind, Opinion, Position, PositionRecord, Requirement,
};
use orchestrate_core::Store;

fn temporary(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    let path = std::env::temp_dir().join(format!("orchestrate-{name}-{nonce}"));
    fs::create_dir_all(&path).unwrap_or_else(|error| panic!("create temp root: {error}"));
    path
}

fn command(root: &Path, arguments: &[&str]) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_orchestrate"))
        .arg("--root")
        .arg(root)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("start orchestrate: {error}"));
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|error| panic!("parse CLI JSON: {error}"))
}

fn rejected_command(root: &Path, arguments: &[&str]) {
    let output = Command::new(env!("CARGO_BIN_EXE_orchestrate"))
        .arg("--root")
        .arg(root)
        .args(arguments)
        .output()
        .unwrap_or_else(|error| panic!("start orchestrate: {error}"));
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {:?}",
        arguments
    );
}

fn git(repo: &Path, arguments: &[&str]) {
    let status = Command::new("git")
        .args(arguments)
        .current_dir(repo)
        .status()
        .unwrap_or_else(|error| panic!("start git: {error}"));
    assert!(status.success(), "git {:?} failed", arguments);
}

fn reference(value: &serde_json::Value, key: &str) -> ArtifactRef {
    serde_json::from_value(value["details"][key].clone())
        .unwrap_or_else(|error| panic!("{key} reference: {error}"))
}

fn opinion(context_id: &str, baseline: &str, slot: &str) -> Opinion {
    let check = |category: &str| CheckDisposition {
        category: category.to_owned(),
        disposition: Disposition::Completed,
        rationale: "Inspected synthetic committed source.".to_owned(),
        evidence: vec!["source-manifest".to_owned()],
    };
    Opinion {
        context_id: context_id.to_owned(),
        baseline_commit: baseline.to_owned(),
        slot: slot.to_owned(),
        recommendation: "Preserve the explicit group-membership behavior.".to_owned(),
        observed_behavior: "Committed source exposes a stable fixture.".to_owned(),
        requirements: vec![Requirement {
            id: "R1".to_owned(),
            text: "Bookings can belong to multiple groups.".to_owned(),
            acceptance: "A booking may retain two memberships.".to_owned(),
            condition: None,
            governing: false,
        }],
        checks: [
            "intent",
            "current_behavior",
            "contracts",
            "edge_cases",
            "verification",
            "risks",
        ]
        .into_iter()
        .map(check)
        .collect(),
        blockers: Vec::new(),
        limitations: Vec::new(),
        adversarial_review: "No contradictory evidence in this synthetic fixture.".to_owned(),
        review_current: true,
    }
}

fn proposal() -> ConsensusProposal {
    let mut positions = std::collections::BTreeMap::new();
    positions.insert("a".to_owned(), Position::Supports);
    positions.insert("b".to_owned(), Position::Supports);
    positions.insert("c".to_owned(), Position::Supports);
    ConsensusProposal {
        positions: vec![PositionRecord {
            requirement: Requirement {
                id: "R1".to_owned(),
                text: "Bookings can belong to multiple groups.".to_owned(),
                acceptance: "A booking may retain two memberships.".to_owned(),
                condition: None,
                governing: false,
            },
            positions,
        }],
        adopted_requirement_ids: vec!["R1".to_owned()],
        selection_rationale: "All slots support the same scoped behavior.".to_owned(),
        dissent: Vec::new(),
        counterexample_blocks: false,
    }
}

#[test]
fn cli_full_chain_passes_and_keeps_dirty_source_untouched() {
    let root = temporary("pass-store");
    let repo = temporary("pass-repo");
    git(&repo, &["init"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test User"]);
    fs::write(repo.join("booking.txt"), "committed baseline\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "baseline"]);
    fs::write(repo.join("booking.txt"), "dirty local change\n").unwrap();
    fs::write(repo.join("private-repro.txt"), "must-not-copy\n").unwrap();

    let init = command(
        &root,
        &[
            "init",
            "--project",
            repo.to_str().unwrap(),
            "--effort",
            "groups",
            "--request",
            "Support memberships",
            "--constraint",
            "A booking may belong to multiple groups.",
        ],
    );
    let effort_id = init["details"]["effort"].as_str().unwrap().to_owned();
    let store = Store::open(&root).unwrap();
    let effort = store.load_effort(&effort_id).unwrap();
    assert_eq!(
        fs::read_to_string(repo.join("booking.txt")).unwrap(),
        "dirty local change\n"
    );
    assert!(repo.join("private-repro.txt").exists());

    let mut opinion_refs = Vec::new();
    for slot in ["a", "b", "c"] {
        let path = root.join(format!("{slot}.json"));
        fs::write(
            &path,
            orchestrate_contracts::encode(&opinion(
                &effort.context.id,
                &effort.cohort.baseline_commit,
                slot,
            ))
            .unwrap(),
        )
        .unwrap();
        let output = command(
            &root,
            &[
                "discovery",
                "finalize",
                "--effort",
                &effort_id,
                "--opinion",
                path.to_str().unwrap(),
            ],
        );
        opinion_refs.push(reference(&output, "artifact"));
    }
    let proposal_path = root.join("proposal.json");
    fs::write(
        &proposal_path,
        orchestrate_contracts::encode(&proposal()).unwrap(),
    )
    .unwrap();
    let consensus = command(
        &root,
        &[
            "consensus",
            "finalize",
            "--effort",
            &effort_id,
            "--opinion",
            &opinion_refs[0].artifact_id,
            "--opinion",
            &opinion_refs[1].artifact_id,
            "--opinion",
            &opinion_refs[2].artifact_id,
            "--proposal",
            proposal_path.to_str().unwrap(),
        ],
    );
    assert_eq!(consensus["semantic_outcome"], "ELIGIBLE_CANDIDATE");
    let agreement = reference(&consensus, "agreement");
    let adoption = command(
        &root,
        &[
            "agreement",
            "adopt",
            "--effort",
            &effort_id,
            "--agreement",
            &agreement.artifact_id,
            "--authorized-by",
            "test-user",
        ],
    );
    let adoption = reference(&adoption, "adoption");
    let implementation = command(
        &root,
        &[
            "implementation",
            "register",
            "--effort",
            &effort_id,
            "--adoption",
            &adoption.artifact_id,
            "--project",
            repo.to_str().unwrap(),
            "--declaration",
            "external test builder",
        ],
    );
    let implementation = reference(&implementation, "implementation");
    let assessment = AuditAssessment {
        agreement: agreement.clone(),
        adoption: adoption.clone(),
        implementation: implementation.clone(),
        coverage: vec![
            Coverage {
                requirement_id: "R1".to_owned(),
                state: CoverageState::Supported,
                rationale: "Focused observed test receipt.".to_owned(),
                evidence: vec!["receipt-r1".to_owned()],
            },
            Coverage {
                requirement_id: "GOV-1".to_owned(),
                state: CoverageState::Supported,
                rationale: "Focused observed test receipt.".to_owned(),
                evidence: vec!["receipt-gov".to_owned()],
            },
        ],
        findings: Vec::new(),
        assessor_context: "fresh independent assessor".to_owned(),
    };
    let assessment_path = root.join("assessment.json");
    fs::write(
        &assessment_path,
        orchestrate_contracts::encode(&assessment).unwrap(),
    )
    .unwrap();
    let audit = command(
        &root,
        &[
            "audit",
            "finalize",
            "--effort",
            &effort_id,
            "--assessment",
            assessment_path.to_str().unwrap(),
        ],
    );
    let audit_ref = reference(&audit, "audit");
    let inspect = command(
        &root,
        &[
            "inspect",
            "--effort",
            &effort_id,
            "--artifact",
            &audit_ref.artifact_id,
        ],
    );
    assert_eq!(inspect["details"]["outcome"], "PASS");
    let lineage = command(
        &root,
        &[
            "lineage",
            "--effort",
            &effort_id,
            "--artifact",
            &audit_ref.artifact_id,
        ],
    );
    let nodes = lineage["details"]["nodes"].as_array().unwrap();
    assert_eq!(nodes[0]["artifact"]["artifact_id"], audit_ref.artifact_id);
    assert_eq!(
        nodes.len(),
        8,
        "all authoritative audit parents are traversed"
    );

    let audit_path = store
        .effort_dir(&effort.project_id, &effort.id)
        .join("audit")
        .join(&audit_ref.artifact_id)
        .join("payload.json");
    fs::write(&audit_path, b"tampered\n").unwrap();
    let rejected = Command::new(env!("CARGO_BIN_EXE_orchestrate"))
        .args([
            "--root",
            root.to_str().unwrap(),
            "inspect",
            "--effort",
            &effort_id,
            "--artifact",
            &audit_ref.artifact_id,
        ])
        .output()
        .unwrap();
    assert!(
        !rejected.status.success(),
        "tampered public payload was inspected as valid"
    );
}

#[test]
fn audit_violation_cannot_pass() {
    let agreement = orchestrate_contracts::Agreement {
        context_id: "c".to_owned(),
        baseline_commit: "b".to_owned(),
        goal: "goal".to_owned(),
        requirements: vec![Requirement {
            id: "R1".to_owned(),
            text: "required".to_owned(),
            acceptance: "proof".to_owned(),
            condition: None,
            governing: false,
        }],
        exclusions: vec![],
        implementation_latitude: vec![],
        verification_expectations: vec![],
        common_supporters: vec!["a".to_owned(), "b".to_owned()],
        dissent: vec![],
    };
    let reference = |kind: orchestrate_contracts::ArtifactKind, id: &str| ArtifactRef {
        kind,
        artifact_id: id.to_owned(),
        digest: "d".repeat(64),
    };
    let assessment = AuditAssessment {
        agreement: reference(orchestrate_contracts::ArtifactKind::Agreement, "g"),
        adoption: reference(orchestrate_contracts::ArtifactKind::Adoption, "a"),
        implementation: reference(orchestrate_contracts::ArtifactKind::Implementation, "i"),
        coverage: vec![Coverage {
            requirement_id: "R1".to_owned(),
            state: CoverageState::Violated,
            rationale: "probe failed".to_owned(),
            evidence: vec!["receipt".to_owned()],
        }],
        findings: vec![Finding {
            kind: FindingKind::ImplementationDefect,
            requirement_id: Some("R1".to_owned()),
            evidence: "focused probe".to_owned(),
            consequence: "contract broken".to_owned(),
            bounded_correction: "repair R1".to_owned(),
        }],
        assessor_context: "fresh".to_owned(),
    };
    assert_eq!(
        orchestrate_contracts::derive_verdict(&agreement, &assessment).unwrap(),
        orchestrate_contracts::Verdict::ChangesRequired
    );
}

#[test]
fn invalidated_discovery_lineage_cannot_be_reused() {
    let root = temporary("invalidation-store");
    let repo = temporary("invalidation-repo");
    git(&repo, &["init"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test User"]);
    fs::write(repo.join("source.txt"), "baseline\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "baseline"]);
    let init = command(
        &root,
        &[
            "init",
            "--project",
            repo.to_str().unwrap(),
            "--effort",
            "invalidate-chain",
            "--request",
            "Preserve source",
            "--constraint",
            "No unsupported scope",
        ],
    );
    let effort_id = init["details"]["effort"].as_str().unwrap().to_owned();
    let effort = Store::open(&root).unwrap().load_effort(&effort_id).unwrap();
    let mut refs = Vec::new();
    for slot in ["a", "b", "c"] {
        let path = root.join(format!("invalidated-{slot}.json"));
        fs::write(
            &path,
            orchestrate_contracts::encode(&opinion(
                &effort.context.id,
                &effort.cohort.baseline_commit,
                slot,
            ))
            .unwrap(),
        )
        .unwrap();
        let finalized = command(
            &root,
            &[
                "discovery",
                "finalize",
                "--effort",
                &effort_id,
                "--opinion",
                path.to_str().unwrap(),
            ],
        );
        refs.push(reference(&finalized, "artifact"));
    }
    command(
        &root,
        &[
            "invalidate",
            "--effort",
            &effort_id,
            "--artifact",
            &refs[0].artifact_id,
            "--reason",
            "fresh evidence contradicts this opinion",
            "--declared-by",
            "test-reviewer",
        ],
    );
    let proposal_path = root.join("invalidation-proposal.json");
    fs::write(
        &proposal_path,
        orchestrate_contracts::encode(&proposal()).unwrap(),
    )
    .unwrap();
    rejected_command(
        &root,
        &[
            "consensus",
            "finalize",
            "--effort",
            &effort_id,
            "--opinion",
            &refs[0].artifact_id,
            "--opinion",
            &refs[1].artifact_id,
            "--opinion",
            &refs[2].artifact_id,
            "--proposal",
            proposal_path.to_str().unwrap(),
        ],
    );
}
