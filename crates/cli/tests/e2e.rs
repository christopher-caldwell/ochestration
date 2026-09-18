use orchestrate_contracts::{
    Agreement, AgreementRequirement, ArtifactKind, ArtifactRef, AuditAssessment, ConsensusProposal,
    ConsensusRequirement, Coverage, CoverageState, EvidenceKind, EvidenceNode, EvidenceStatus,
    ImplementationStatus, Requirement,
};
use orchestrate_core::Store;
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};

fn temporary(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let p = std::env::temp_dir().join(format!("orchestrate-{name}-{nonce}"));
    fs::create_dir_all(&p).unwrap();
    p
}
fn git(repo: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .args(args)
            .current_dir(repo)
            .status()
            .unwrap()
            .success()
    )
}
fn command(root: &Path, args: &[&str]) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_orchestrate"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn reference(value: &serde_json::Value, key: &str) -> ArtifactRef {
    serde_json::from_value(value["details"][key].clone()).unwrap()
}
fn spec() -> &'static str {
    "# Technical specification\n\n## Interpreted request\nA request.\n\n## Current behavior\nA baseline.\n\n## Recommendation\nDo the work.\n\n## Required changes\nOne change.\n\n## Unchanged behavior\nEverything else.\n\n## Decisions and rationale\nA decision.\n\n## Requirements and acceptance criteria\nA requirement.\n\n## Conditions\nNone.\n\n## Alternatives or disagreement\nNone.\n\n## Limitations\nNone.\n\n## Verification\nA test.\n"
}
fn node(
    id: &str,
    kind: EvidenceKind,
    status: EvidenceStatus,
    depends: Vec<&str>,
    mandatory: bool,
) -> EvidenceNode {
    EvidenceNode {
        id: id.into(),
        kind,
        status,
        depends_on: depends.into_iter().map(str::to_owned).collect(),
        sources: vec!["src:fixture".into()],
        required: false,
        mandatory,
        title: id.into(),
        body: "evidence".into(),
    }
}
fn write_workspace(workspace: &Path) {
    fs::write(workspace.join("technical-spec.md"), spec()).unwrap();
    let f = node(
        "F-1",
        EvidenceKind::Finding,
        EvidenceStatus::Accepted,
        vec![],
        false,
    );
    let r = node(
        "R-1",
        EvidenceKind::Requirement,
        EvidenceStatus::Accepted,
        vec!["F-1"],
        true,
    );
    for n in [f, r] {
        let front=serde_yaml::to_string(&serde_json::json!({"id":n.id,"kind":n.kind,"status":n.status,"depends_on":n.depends_on,"sources":n.sources,"mandatory":n.mandatory})).unwrap();
        fs::write(
            workspace.join("graph").join(format!("{}.md", n.id)),
            format!("---\n{front}---\n\n# {}\n\n{}\n", n.title, n.body),
        )
        .unwrap();
    }
}

#[test]
fn complete_flow_preserves_source_and_journal() {
    let root = temporary("store");
    let repo = temporary("repo");
    git(&repo, &["init"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("source.txt"), "committed\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "baseline"]);
    fs::write(repo.join("source.txt"), "dirty\n").unwrap();
    fs::write(repo.join("untracked.txt"), "private\n").unwrap();
    let init = command(
        &root,
        &[
            "init",
            "--project",
            repo.to_str().unwrap(),
            "--effort",
            "flow",
            "--request",
            "change fixture",
            "--constraint",
            "preserve boundary",
        ],
    );
    let effort_id = init["details"]["effort"].as_str().unwrap().to_owned();
    let store = Store::open(&root).unwrap();
    let effort = store.load_effort(&effort_id).unwrap();
    assert_eq!(
        fs::read_to_string(repo.join("source.txt")).unwrap(),
        "dirty\n"
    );
    assert!(
        !root
            .join("snapshots")
            .join(&effort.cohort.baseline_commit)
            .join("source")
            .join("source-manifest.json")
            .exists()
    );
    let mut refs = Vec::new();
    for slot in ["a", "b", "c"] {
        let prepared = command(
            &root,
            &[
                "discovery",
                "prepare",
                "--effort",
                &effort_id,
                "--slot",
                slot,
            ],
        );
        let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
        write_workspace(&workspace);
        let run = prepared["details"]["run"].as_str().unwrap();
        let finalized = command(
            &root,
            &[
                "discovery",
                "finalize",
                "--effort",
                &effort_id,
                "--run",
                run,
            ],
        );
        refs.push(reference(&finalized, "artifact"));
    }
    let proposal = ConsensusProposal {
        requirements: vec![ConsensusRequirement {
            requirement: Requirement {
                id: "R1".into(),
                text: "Required behavior".into(),
                acceptance: "Focused proof".into(),
                condition: None,
                governing: false,
            },
            supporters: vec!["a".into(), "b".into(), "c".into()],
            source_refs: BTreeMap::new(),
        }],
        comparison_md: "# Comparison\n\nAll slots agree.\n".into(),
        selection_rationale: "coherent".into(),
        dissent: vec![],
        counterexample_blocks: false,
    };
    let proposal_path = root.join("proposal.json");
    fs::write(
        &proposal_path,
        orchestrate_contracts::encode(&proposal).unwrap(),
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
            &refs[0].artifact_id,
            "--opinion",
            &refs[1].artifact_id,
            "--opinion",
            &refs[2].artifact_id,
            "--bundle",
            proposal_path.to_str().unwrap(),
        ],
    );
    let agreement = reference(&consensus, "agreement");
    let adoption = reference(
        &command(
            &root,
            &[
                "agreement",
                "adopt",
                "--effort",
                &effort_id,
                "--agreement",
                &agreement.artifact_id,
                "--authorization-label",
                "tester",
            ],
        ),
        "adoption",
    );
    let implementation = reference(
        &command(
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
                "external",
            ],
        ),
        "implementation",
    );
    let assessment = AuditAssessment {
        agreement: agreement.clone(),
        adoption: adoption.clone(),
        implementation: implementation.clone(),
        coverage: vec![
            Coverage {
                requirement_id: "R1".into(),
                state: CoverageState::Pass,
                rationale: "tested".into(),
                evidence: vec!["test".into()],
                correction: String::new(),
            },
            Coverage {
                requirement_id: "GOV-1".into(),
                state: CoverageState::NotApplicable,
                rationale: "already preserved by scope".into(),
                evidence: vec![],
                correction: String::new(),
            },
        ],
        assessor_context: "fresh".into(),
    };
    let assessment_path = root.join("assessment.json");
    fs::write(
        &assessment_path,
        orchestrate_contracts::encode(&assessment).unwrap(),
    )
    .unwrap();
    let audit = reference(
        &command(
            &root,
            &[
                "audit",
                "finalize",
                "--effort",
                &effort_id,
                "--bundle",
                assessment_path.to_str().unwrap(),
            ],
        ),
        "audit",
    );
    let inspected = command(
        &root,
        &[
            "inspect",
            "--effort",
            &effort_id,
            "--artifact",
            &audit.artifact_id,
        ],
    );
    assert_eq!(inspected["details"]["manifest"]["outcome"], "PASS");
    let journal = command(&root, &["journal", "--effort", &effort_id]);
    assert!(
        journal["details"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["event"] == "audit_finalized")
    );
}

#[test]
fn audit_coverage_controls_verdict() {
    let agreement = Agreement {
        agreement_id: "a".into(),
        context_id: "c".into(),
        cohort_id: "h".into(),
        baseline_commit: "b".into(),
        goal: "goal".into(),
        requirements: vec![AgreementRequirement {
            requirement: Requirement {
                id: "R1".into(),
                text: "required".into(),
                acceptance: "proof".into(),
                condition: None,
                governing: false,
            },
            supporters: vec!["a".into(), "b".into()],
            source_refs: BTreeMap::new(),
        }],
    };
    let reference = |kind: ArtifactKind, id: &str| ArtifactRef {
        kind,
        artifact_id: id.into(),
        digest: "d".repeat(64),
    };
    let assessment = AuditAssessment {
        agreement: reference(ArtifactKind::Agreement, "g"),
        adoption: reference(ArtifactKind::Adoption, "a"),
        implementation: reference(ArtifactKind::Implementation, "i"),
        coverage: vec![Coverage {
            requirement_id: "R1".into(),
            state: CoverageState::Fail,
            rationale: "broken".into(),
            evidence: vec!["src/lib.rs:1".into()],
            correction: "fix it".into(),
        }],
        assessor_context: "fresh".into(),
    };
    assert_eq!(
        orchestrate_contracts::derive_verdict(
            &agreement,
            &assessment,
            &ImplementationStatus::Submitted
        )
        .unwrap(),
        orchestrate_contracts::Verdict::ChangesRequired
    );
}
