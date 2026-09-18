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
fn command_error(root: &Path, args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_orchestrate"))
        .arg("--root")
        .arg(root)
        .args(args)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "command unexpectedly succeeded: {}",
        String::from_utf8_lossy(&output.stdout)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}
fn git_output(repo: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
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
fn write_node(workspace: &Path, n: &EvidenceNode) {
    let front = serde_yaml::to_string(&serde_json::json!({
        "id":n.id,
        "kind":n.kind,
        "status":n.status,
        "depends_on":n.depends_on,
        "sources":n.sources,
        "required":n.required,
        "mandatory":n.mandatory
    }))
    .unwrap();
    fs::write(
        workspace.join("graph").join(format!("{}.md", n.id)),
        format!("---\n{front}---\n\n# {}\n\n{}\n", n.title, n.body),
    )
    .unwrap();
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
    for n in [&f, &r] {
        write_node(workspace, n);
    }
}

#[test]
fn typed_request_intake_changes_context_identity() {
    let root = temporary("typed-intake-store");
    let repo = temporary("typed-intake-repo");
    git(&repo, &["init"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("source.txt"), "committed\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "baseline"]);
    let ticket = temporary("typed-intake-request").join("AGE-377.md");
    fs::write(
        &ticket,
        "Investigate exact behavior.\n\nDo not summarize.\n",
    )
    .unwrap();

    let inline = command(
        &root,
        &[
            "init",
            "--project",
            repo.to_str().unwrap(),
            "--effort",
            "inline",
            "--request",
            "Investigate exact behavior.\n\nDo not summarize.\n",
        ],
    );
    let ticket_effort = command(
        &root,
        &[
            "init",
            "--project",
            repo.to_str().unwrap(),
            "--effort",
            "ticket",
            "--request-file",
            ticket.to_str().unwrap(),
            "--request-kind",
            "ticket",
        ],
    );
    let changed_request = command(
        &root,
        &[
            "init",
            "--project",
            repo.to_str().unwrap(),
            "--effort",
            "changed-request",
            "--request",
            "Different request",
        ],
    );
    let changed_constraint = command(
        &root,
        &[
            "init",
            "--project",
            repo.to_str().unwrap(),
            "--effort",
            "changed-constraint",
            "--request",
            "Investigate exact behavior.\n\nDo not summarize.\n",
            "--constraint",
            "Explicit clarification",
        ],
    );
    let store = Store::open(&root).unwrap();
    let inline_effort = store
        .load_effort(inline["details"]["effort"].as_str().unwrap())
        .unwrap();
    let ticket_effort = store
        .load_effort(ticket_effort["details"]["effort"].as_str().unwrap())
        .unwrap();
    let changed_request_effort = store
        .load_effort(changed_request["details"]["effort"].as_str().unwrap())
        .unwrap();
    let changed_constraint_effort = store
        .load_effort(changed_constraint["details"]["effort"].as_str().unwrap())
        .unwrap();

    assert_eq!(
        inline_effort.context.request_kind,
        orchestrate_contracts::RequestKind::Freeform
    );
    assert_eq!(
        ticket_effort.context.request_kind,
        orchestrate_contracts::RequestKind::Ticket
    );
    assert_eq!(
        fs::read_to_string(
            store
                .effort_dir(&ticket_effort.project_id, &ticket_effort.id)
                .join("request.md")
        )
        .unwrap(),
        "Investigate exact behavior.\n\nDo not summarize.\n"
    );
    assert_ne!(inline_effort.context.id, ticket_effort.context.id);
    assert_ne!(inline_effort.context.id, changed_request_effort.context.id);
    assert_ne!(
        inline_effort.context.id,
        changed_constraint_effort.context.id
    );
}

#[test]
fn request_file_requires_kind_and_valid_utf8() {
    let root = temporary("invalid-intake-store");
    let repo = temporary("invalid-intake-repo");
    git(&repo, &["init"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("source.txt"), "committed\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "baseline"]);
    let request = temporary("invalid-intake-request").join("request.md");
    fs::write(&request, "request\n").unwrap();
    let invalid = temporary("invalid-intake-request").join("invalid.md");
    fs::write(&invalid, [0xff, 0xfe]).unwrap();

    assert!(
        command_error(
            &root,
            &[
                "init",
                "--project",
                repo.to_str().unwrap(),
                "--effort",
                "missing-kind",
                "--request-file",
                request.to_str().unwrap(),
            ],
        )
        .contains("--request-file requires --request-kind")
    );
    assert!(
        command_error(
            &root,
            &[
                "init",
                "--project",
                repo.to_str().unwrap(),
                "--effort",
                "invalid-utf8",
                "--request-file",
                invalid.to_str().unwrap(),
                "--request-kind",
                "ticket",
            ],
        )
        .contains("not valid UTF-8")
    );
    assert!(
        command_error(
            &root,
            &[
                "init",
                "--project",
                repo.to_str().unwrap(),
                "--effort",
                "conflict",
                "--request",
                "inline",
                "--request-file",
                request.to_str().unwrap(),
            ],
        )
        .contains("cannot be used with")
    );
}

#[test]
fn discovery_workspace_is_self_describing_git_checkout_and_preserves_blockers() {
    let root = temporary("discovery-run-store");
    let repo = temporary("discovery-run-repo");
    git(&repo, &["init"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("history.txt"), "first\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "first"]);
    fs::write(repo.join("history.txt"), "second\n").unwrap();
    git(&repo, &["commit", "-am", "second"]);
    let baseline = git_output(&repo, &["rev-parse", "HEAD"]).trim().to_owned();
    fs::write(repo.join("history.txt"), "dirty\n").unwrap();
    fs::write(repo.join("untracked.txt"), "private\n").unwrap();
    let ticket = temporary("discovery-run-request").join("AGE-377.md");
    fs::write(&ticket, "Investigate the behavior exactly.\n").unwrap();
    let init = command(
        &root,
        &[
            "init",
            "--project",
            repo.to_str().unwrap(),
            "--effort",
            "age-377",
            "--request-file",
            ticket.to_str().unwrap(),
            "--request-kind",
            "ticket",
        ],
    );
    let effort_id = init["details"]["effort"].as_str().unwrap().to_owned();
    let prepared = command(
        &root,
        &[
            "discovery",
            "prepare",
            "--effort",
            &effort_id,
            "--slot",
            "a",
            "--host",
            "claude-code",
            "--provider",
            "anthropic",
            "--model",
            "opus-5",
            "--model-effort",
            "high",
        ],
    );
    let run = prepared["details"]["run"].as_str().unwrap();
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    let run_json: serde_json::Value =
        serde_json::from_slice(&fs::read(workspace.join("run.json")).unwrap()).unwrap();
    assert_eq!(run_json["run_id"], run);
    assert_eq!(run_json["phase"], "discovery");
    assert_eq!(run_json["slot"], "a");
    assert_eq!(run_json["effort_id"], effort_id);
    assert_eq!(run_json["request_kind"], "ticket");
    assert_eq!(run_json["baseline_commit"], baseline);
    assert_eq!(run_json["host"], "claude-code");
    assert_eq!(run_json["provider"], "anthropic");
    assert_eq!(run_json["model"], "opus-5");
    assert_eq!(run_json["model_effort"], "high");
    assert_eq!(
        fs::read_to_string(workspace.join("request.md")).unwrap(),
        "Investigate the behavior exactly.\n"
    );
    let source = workspace.join("source");
    assert_eq!(git_output(&source, &["rev-parse", "HEAD"]).trim(), baseline);
    assert_eq!(
        fs::read_to_string(source.join("history.txt")).unwrap(),
        "second\n"
    );
    assert!(!source.join("untracked.txt").exists());
    assert!(git_output(&source, &["log", "--oneline"]).lines().count() >= 2);
    assert_eq!(
        git_output(&source, &["show", "HEAD^:history.txt"]),
        "first\n"
    );
    assert!(!git_output(&source, &["blame", "--", "history.txt"]).is_empty());
    assert_eq!(
        fs::read_to_string(repo.join("history.txt")).unwrap(),
        "dirty\n"
    );
    assert_eq!(
        fs::read_to_string(repo.join("untracked.txt")).unwrap(),
        "private\n"
    );

    write_workspace(&workspace);
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
    let discovery_reference = reference(&finalized, "artifact");
    let store = Store::open(&root).unwrap();
    let effort = store.load_effort(&effort_id).unwrap();
    let (manifest, files) = store.load_bundle(&effort, &discovery_reference).unwrap();
    assert!(files.contains_key("run.json"));
    assert_eq!(manifest.provenance.host, "claude-code");
    assert_eq!(manifest.provenance.provider.as_deref(), Some("anthropic"));
    assert_eq!(manifest.provenance.model.as_deref(), Some("opus-5"));
    assert_eq!(manifest.provenance.model_effort.as_deref(), Some("high"));

    let blocked = command(
        &root,
        &[
            "discovery",
            "prepare",
            "--effort",
            &effort_id,
            "--slot",
            "b",
        ],
    );
    let blocked_run = blocked["details"]["run"].as_str().unwrap();
    let blocked_workspace = PathBuf::from(blocked["details"]["workspace"].as_str().unwrap());
    fs::write(blocked_workspace.join("technical-spec.md"), spec()).unwrap();
    let mut question = node(
        "Q-008",
        EvidenceKind::Question,
        EvidenceStatus::Blocked,
        vec![],
        false,
    );
    question.required = true;
    question.title = "What does the vendor do?".into();
    question.body = "The answer changes the local validation contract.".into();
    write_node(&blocked_workspace, &question);
    let validation = command(
        &root,
        &[
            "discovery",
            "validate",
            "--effort",
            &effort_id,
            "--run",
            blocked_run,
        ],
    );
    assert_eq!(validation["semantic_outcome"], "BLOCKED");
    assert_eq!(validation["details"]["run"], blocked_run);
    assert_eq!(validation["details"]["questions"][0]["id"], "Q-008");
    assert_eq!(
        validation["details"]["questions"][0]["body"],
        "The answer changes the local validation contract."
    );
    let blocked_finalized = command(
        &root,
        &[
            "discovery",
            "finalize",
            "--effort",
            &effort_id,
            "--run",
            blocked_run,
        ],
    );
    let blocked_reference = reference(&blocked_finalized, "artifact");
    assert_eq!(blocked_reference.kind, ArtifactKind::Discovery);
}

#[test]
fn optional_blocked_question_blocks_discovery_and_is_reported() {
    let root = temporary("optional-blocked-question-store");
    let repo = temporary("optional-blocked-question-repo");
    git(&repo, &["init"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("source.txt"), "committed\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "baseline"]);

    let init = command(
        &root,
        &[
            "init",
            "--project",
            repo.to_str().unwrap(),
            "--effort",
            "optional-blocked-question",
            "--request",
            "change fixture",
        ],
    );
    let effort_id = init["details"]["effort"].as_str().unwrap().to_owned();
    let prepared = command(
        &root,
        &[
            "discovery",
            "prepare",
            "--effort",
            &effort_id,
            "--slot",
            "a",
        ],
    );
    let run = prepared["details"]["run"].as_str().unwrap().to_owned();
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    fs::write(workspace.join("technical-spec.md"), spec()).unwrap();
    let question = node(
        "Q-optional-blocked",
        EvidenceKind::Question,
        EvidenceStatus::Blocked,
        vec![],
        false,
    );
    assert!(!question.required);
    write_node(&workspace, &question);

    let validation = command(
        &root,
        &[
            "discovery",
            "validate",
            "--effort",
            &effort_id,
            "--run",
            &run,
        ],
    );
    assert_eq!(validation["semantic_outcome"], "BLOCKED");
    assert_eq!(validation["details"]["questions"][0]["id"], question.id);

    assert!(
        command_error(
            &root,
            &[
                "discovery",
                "validate",
                "--effort",
                &effort_id,
                "--run",
                &run,
                "--outcome",
                "IMPLEMENTATION_READY",
            ],
        )
        .contains("implementation-ready Discovery cannot have a blocked question")
    );
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
