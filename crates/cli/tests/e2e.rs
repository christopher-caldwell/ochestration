use orchestrate_contracts::{
    Agreement, AgreementRequirement, ArtifactKind, ArtifactRef, AuditAssessment, ConsensusProposal,
    ConsensusRequirement, Coverage, CoverageState, EvidenceKind, EvidenceNode, EvidenceStatus,
    Implementation, ImplementationStatus, Requirement,
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
fn command_without_root(args: &[&str]) -> serde_json::Value {
    let output = Command::new(env!("CARGO_BIN_EXE_orchestrate"))
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
fn command_error_without_root(args: &[&str]) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_orchestrate"))
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

fn new_effort(name: &str, request: &str) -> (PathBuf, PathBuf, String) {
    let root = temporary(&format!("{name}-store"));
    let repo = temporary(&format!("{name}-repo"));
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
            name,
            "--request",
            request,
        ],
    );
    let effort = init["details"]["effort"].as_str().unwrap().to_owned();
    (root, repo, effort)
}
fn finalize_discovery(root: &Path, effort: &str, slot: &str) -> ArtifactRef {
    let prepared = command(
        root,
        &["discovery", "prepare", "--effort", effort, "--slot", slot],
    );
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    write_workspace(&workspace);
    let run = prepared["details"]["run"].as_str().unwrap();
    let finalized = command(
        root,
        &["discovery", "finalize", "--effort", effort, "--run", run],
    );
    reference(&finalized, "artifact")
}
fn write_proposal(root: &Path, name: &str) -> PathBuf {
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
    let path = root.join(format!("{name}.json"));
    fs::write(&path, orchestrate_contracts::encode(&proposal).unwrap()).unwrap();
    path
}
fn consensus_agreement(root: &Path, effort: &str, proposal: &Path) -> ArtifactRef {
    let consensus = command(
        root,
        &[
            "consensus",
            "finalize",
            "--effort",
            effort,
            "--bundle",
            proposal.to_str().unwrap(),
        ],
    );
    reference(&consensus, "agreement")
}
fn implementation_payload(root: &Path, effort: &str, reference: &ArtifactRef) -> Implementation {
    let store = Store::open(root).unwrap();
    let effort = store.load_effort(effort).unwrap();
    let (_, files) = store.load_bundle(&effort, reference).unwrap();
    orchestrate_contracts::decode(files.get("implementation.json").unwrap()).unwrap()
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

    let raw_yamlish = temporary("raw-yamlish-request").join("raw.md");
    let raw_yamlish_body = "---\nroot: not-a-prepared-file\n---\nThis remains raw.\n";
    fs::write(&raw_yamlish, raw_yamlish_body).unwrap();
    let result = command(
        &root,
        &[
            "init",
            "--project",
            repo.to_str().unwrap(),
            "--effort",
            "raw-yamlish",
            "--request-file",
            raw_yamlish.to_str().unwrap(),
            "--request-kind",
            "freeform",
        ],
    );
    let store = Store::open(&root).unwrap();
    let effort = store
        .load_effort(result["details"]["effort"].as_str().unwrap())
        .unwrap();
    assert_eq!(effort.context.request, raw_yamlish_body);
}

#[test]
fn prepared_file_init_uses_its_values_and_preserves_body_bytes() {
    let root = temporary("prepared-store");
    let repo = temporary("prepared-repo");
    git(&repo, &["init"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("source.txt"), "committed\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "baseline"]);
    let body = "# Discovery Request\r\n\r\nUnicode: 😀  \r\n```text\r\n---\r\n```\r\n";
    let prepared = temporary("prepared-file").join("request.prepared.md");
    fs::write(
        &prepared,
        format!(
            "---\r\nroot: {}\r\nproject: {}\r\neffort: age-377\r\nrequest_kind: freeform\r\nconstraints:\r\n  - Keep exact wording\r\n---\r\n{body}",
            root.display(),
            repo.display()
        ),
    )
    .unwrap();

    let result = command_without_root(&["init", "--from-file", prepared.to_str().unwrap()]);
    let store = Store::open(&root).unwrap();
    let effort = store
        .load_effort(result["details"]["effort"].as_str().unwrap())
        .unwrap();
    assert_eq!(effort.slug, "age-377");
    assert_eq!(
        effort.context.request_kind,
        orchestrate_contracts::RequestKind::Freeform
    );
    assert_eq!(effort.context.constraints, ["Keep exact wording"]);
    assert_eq!(
        fs::read(
            store
                .effort_dir(&effort.project_id, &effort.id)
                .join("request.md")
        )
        .unwrap(),
        body.as_bytes()
    );
}

#[test]
fn prepared_file_rejects_conflicts_and_invalid_input_before_opening_a_store() {
    let parent = temporary("prepared-invalid-parent");
    let root = parent.join("unopened-store");
    let valid = parent.join("valid.md");
    fs::write(
        &valid,
        format!(
            "---\nroot: {}\nproject: /absolute/project\neffort: effort\nrequest_kind: freeform\n---\nrequest\n",
            root.display()
        ),
    )
    .unwrap();
    let error = command_error_without_root(&[
        "--root",
        root.to_str().unwrap(),
        "init",
        "--from-file",
        valid.to_str().unwrap(),
    ]);
    assert!(error.contains("--from-file cannot be combined"));
    assert!(!root.exists());

    for (name, contents, expected) in [
        (
            "unknown",
            format!(
                "---\nroot: {}\nproject: /absolute/project\neffort: effort\nrequest_kind: freeform\nextra: no\n---\nrequest\n",
                root.display()
            ),
            "frontmatter is invalid",
        ),
        (
            "relative",
            "---\nroot: relative\nproject: /absolute/project\neffort: effort\nrequest_kind: freeform\n---\nrequest\n".into(),
            "root must be an absolute path",
        ),
        (
            "invalid-kind",
            format!(
                "---\nroot: {}\nproject: /absolute/project\neffort: effort\nrequest_kind: other\n---\nrequest\n",
                root.display()
            ),
            "frontmatter is invalid",
        ),
        (
            "duplicate",
            format!(
                "---\nroot: {}\nroot: /another/root\nproject: /absolute/project\neffort: effort\nrequest_kind: freeform\n---\nrequest\n",
                root.display()
            ),
            "frontmatter is invalid",
        ),
        (
            "unclosed",
            format!(
                "---\nroot: {}\nproject: /absolute/project\neffort: effort\nrequest_kind: freeform\n",
                root.display()
            ),
            "missing a closing frontmatter delimiter",
        ),
        (
            "placeholder",
            format!(
                "---\nroot: {}\nproject: /absolute/project\neffort: effort\nrequest_kind: ticket\n---\n<!-- PASTE ORIGINAL TICKET VERBATIM HERE -->\n",
                root.display()
            ),
            "replace the original ticket placeholder",
        ),
        (
            "empty-body",
            format!(
                "---\nroot: {}\nproject: /absolute/project\neffort: effort\nrequest_kind: freeform\n---\n \n",
                root.display()
            ),
            "body must not be whitespace only",
        ),
    ] {
        let path = parent.join(format!("{name}.md"));
        fs::write(&path, contents).unwrap();
        let error = command_error_without_root(&["init", "--from-file", path.to_str().unwrap()]);
        assert!(error.contains(expected), "{error}");
        assert!(!root.exists());
    }
    let invalid_utf8 = parent.join("invalid-utf8.md");
    fs::write(&invalid_utf8, [0xff, 0xfe]).unwrap();
    assert!(
        command_error_without_root(&["init", "--from-file", invalid_utf8.to_str().unwrap(),])
            .contains("not valid UTF-8")
    );
    assert!(!root.exists());
}

#[test]
fn prepared_ticket_requires_replacement_then_uses_default_constraints() {
    let root = temporary("prepared-ticket-store");
    let repo = temporary("prepared-ticket-repo");
    git(&repo, &["init"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("source.txt"), "committed\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "baseline"]);
    let prepared = temporary("prepared-ticket-file").join("request.prepared.md");
    let frontmatter = format!(
        "---\nroot: {}\nproject: {}\neffort: ticket-input\nrequest_kind: ticket\n---\n",
        root.display(),
        repo.display()
    );
    fs::write(
        &prepared,
        format!("{frontmatter}<!-- PASTE ORIGINAL TICKET VERBATIM HERE -->\n"),
    )
    .unwrap();
    assert!(
        command_error_without_root(&["init", "--from-file", prepared.to_str().unwrap()])
            .contains("replace the original ticket placeholder")
    );
    assert!(!root.join("store.json").exists());

    let ticket = "# Original ticket\n\nPreserve this text exactly.  \n";
    fs::write(&prepared, format!("{frontmatter}{ticket}")).unwrap();
    let result = command_without_root(&["init", "--from-file", prepared.to_str().unwrap()]);
    let store = Store::open(&root).unwrap();
    let effort = store
        .load_effort(result["details"]["effort"].as_str().unwrap())
        .unwrap();
    assert_eq!(
        effort.context.request_kind,
        orchestrate_contracts::RequestKind::Ticket
    );
    assert!(effort.context.constraints.is_empty());
    assert_eq!(
        fs::read(
            store
                .effort_dir(&effort.project_id, &effort.id)
                .join("request.md")
        )
        .unwrap(),
        ticket.as_bytes()
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
    assert_eq!(finalized["details"]["outcome"], "IMPLEMENTATION_READY");
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
    assert_eq!(blocked_finalized["details"]["outcome"], "BLOCKED");
    let inspected = command(
        &root,
        &[
            "inspect",
            "--effort",
            &effort_id,
            "--artifact",
            &blocked_reference.artifact_id,
        ],
    );
    assert_eq!(inspected["details"]["manifest"]["outcome"], "BLOCKED");
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
    let audit_result = command(
        &root,
        &[
            "audit",
            "finalize",
            "--effort",
            &effort_id,
            "--bundle",
            assessment_path.to_str().unwrap(),
        ],
    );
    assert_eq!(audit_result["details"]["verdict"], "PASS");
    let audit = reference(&audit_result, "audit");
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

#[test]
fn removed_provider_commands_and_manual_overrides_do_not_parse() {
    let root = temporary("removed-commands-store");
    for command in [["discovery", "run"], ["consensus", "run"], ["audit", "run"]] {
        let error = command_error(&root, &command);
        assert!(
            error.contains("unrecognized subcommand") || error.contains("unexpected argument"),
            "{error}"
        );
    }
    assert!(
        command_error(
            &root,
            &[
                "discovery",
                "validate",
                "--effort",
                "effort",
                "--run",
                "run",
                "--outcome",
                "BLOCKED",
            ],
        )
        .contains("unexpected argument")
    );
    assert!(
        command_error(
            &root,
            &[
                "implementation",
                "register",
                "--effort",
                "effort",
                "--project",
                "/tmp/project",
            ],
        )
        .contains("unexpected argument")
    );
}

#[test]
fn discovery_finalize_rejects_an_invalid_workspace_without_publishing() {
    let (root, _repo, effort) = new_effort("discovery-invalid", "change fixture");
    let prepared = command(
        &root,
        &["discovery", "prepare", "--effort", &effort, "--slot", "a"],
    );
    let run = prepared["details"]["run"].as_str().unwrap();

    let error = command_error(
        &root,
        &["discovery", "finalize", "--effort", &effort, "--run", run],
    );
    assert!(
        error.contains("implementation-ready Discovery needs at least one evidence node"),
        "{error}"
    );
    let status = command(&root, &["status", "--effort", &effort]);
    assert!(
        status["details"]["artifacts"]
            .as_array()
            .unwrap()
            .is_empty(),
        "an invalid workspace must not publish an artifact"
    );
    let journal = command(&root, &["journal", "--effort", &effort]);
    assert!(
        journal["details"]
            .as_array()
            .unwrap()
            .iter()
            .any(|event| event["event"] == "finalization_rejected")
    );
}

#[test]
fn consensus_infers_sole_eligible_discovery_opinions() {
    let (root, _repo, effort) = new_effort("consensus-inference", "change fixture");
    let expected: Vec<_> = ["a", "b", "c"]
        .iter()
        .map(|slot| finalize_discovery(&root, &effort, slot).artifact_id)
        .collect();
    let proposal = write_proposal(&root, "proposal-inference");
    let consensus = command(
        &root,
        &[
            "consensus",
            "finalize",
            "--effort",
            &effort,
            "--bundle",
            proposal.to_str().unwrap(),
        ],
    );
    let agreement = reference(&consensus, "agreement");
    assert_eq!(agreement.kind, ArtifactKind::Agreement);

    let store = Store::open(&root).unwrap();
    let effort_state = store.load_effort(&effort).unwrap();
    let (_, files) = store
        .load_bundle(&effort_state, &reference(&consensus, "comparison"))
        .unwrap();
    let comparison: serde_json::Value =
        serde_json::from_slice(files.get("comparison.json").unwrap()).unwrap();
    let opinions: Vec<String> = comparison["opinions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| value["artifact_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(opinions, expected);
}

#[test]
fn consensus_inference_requires_an_eligible_artifact_for_every_slot() {
    let (root, _repo, effort) = new_effort("consensus-missing-slot", "change fixture");
    for slot in ["a", "b"] {
        finalize_discovery(&root, &effort, slot);
    }
    let proposal = write_proposal(&root, "proposal-missing-slot");
    let error = command_error(
        &root,
        &[
            "consensus",
            "finalize",
            "--effort",
            &effort,
            "--bundle",
            proposal.to_str().unwrap(),
        ],
    );
    assert!(
        error.contains("no eligible finalized Discovery artifact exists for slot c"),
        "{error}"
    );
}

#[test]
fn consensus_inference_rejects_ambiguous_slot_candidates() {
    let (root, _repo, effort) = new_effort("consensus-ambiguous-slot", "change fixture");
    let first = finalize_discovery(&root, &effort, "a");
    let second = finalize_discovery(&root, &effort, "a");
    for slot in ["b", "c"] {
        finalize_discovery(&root, &effort, slot);
    }
    let proposal = write_proposal(&root, "proposal-ambiguous-slot");
    let error = command_error(
        &root,
        &[
            "consensus",
            "finalize",
            "--effort",
            &effort,
            "--bundle",
            proposal.to_str().unwrap(),
        ],
    );
    assert!(
        error.contains("multiple eligible Discovery artifacts exist for slot a"),
        "{error}"
    );
    assert!(error.contains(&first.artifact_id), "{error}");
    assert!(error.contains(&second.artifact_id), "{error}");
    assert!(error.contains("--opinion"), "{error}");
}

#[test]
fn agreement_adoption_infers_the_sole_eligible_agreement() {
    let (root, _repo, effort) = new_effort("adoption-inference", "change fixture");
    for slot in ["a", "b", "c"] {
        finalize_discovery(&root, &effort, slot);
    }
    let proposal = write_proposal(&root, "proposal-adoption");
    let agreement = consensus_agreement(&root, &effort, &proposal);
    let adoption = reference(
        &command(
            &root,
            &[
                "agreement",
                "adopt",
                "--effort",
                &effort,
                "--authorization-label",
                "tester",
            ],
        ),
        "adoption",
    );
    assert_eq!(adoption.kind, ArtifactKind::Adoption);
    let store = Store::open(&root).unwrap();
    let effort_state = store.load_effort(&effort).unwrap();
    let (_, adoption_payload): (_, orchestrate_contracts::Adoption) = store
        .load_json(&effort_state, &adoption, "adoption.json")
        .unwrap();
    assert_eq!(adoption_payload.agreement, agreement);
}

#[test]
fn agreement_adoption_requires_an_eligible_candidate() {
    let (root, _repo, effort) = new_effort("adoption-missing", "change fixture");
    let error = command_error(
        &root,
        &[
            "agreement",
            "adopt",
            "--effort",
            &effort,
            "--authorization-label",
            "tester",
        ],
    );
    assert!(
        error.contains("no eligible Agreement artifact exists"),
        "{error}"
    );
}

#[test]
fn agreement_adoption_rejects_ambiguous_candidates() {
    let (root, _repo, effort) = new_effort("adoption-ambiguous", "change fixture");
    for slot in ["a", "b", "c"] {
        finalize_discovery(&root, &effort, slot);
    }
    let proposal = write_proposal(&root, "proposal-ambiguous-agreement");
    let first = consensus_agreement(&root, &effort, &proposal);
    let second = consensus_agreement(&root, &effort, &proposal);
    let error = command_error(
        &root,
        &[
            "agreement",
            "adopt",
            "--effort",
            &effort,
            "--authorization-label",
            "tester",
        ],
    );
    assert!(
        error.contains("multiple eligible Agreement artifacts exist"),
        "{error}"
    );
    assert!(error.contains(&first.artifact_id), "{error}");
    assert!(error.contains(&second.artifact_id), "{error}");
    assert!(error.contains("--agreement"), "{error}");
}

#[test]
fn implementation_registration_uses_the_stored_project_and_defaults() {
    let (root, repo, effort) = new_effort("registration-defaults", "change fixture");
    for slot in ["a", "b", "c"] {
        finalize_discovery(&root, &effort, slot);
    }
    let proposal = write_proposal(&root, "proposal-registration");
    consensus_agreement(&root, &effort, &proposal);
    let adoption = reference(
        &command(
            &root,
            &[
                "agreement",
                "adopt",
                "--effort",
                &effort,
                "--authorization-label",
                "tester",
            ],
        ),
        "adoption",
    );

    let implementation = reference(
        &command(&root, &["implementation", "register", "--effort", &effort]),
        "implementation",
    );
    let head = git_output(&repo, &["rev-parse", "HEAD"]).trim().to_owned();
    let payload = implementation_payload(&root, &effort, &implementation);
    assert_eq!(payload.adoption, adoption);
    assert_eq!(payload.status, ImplementationStatus::Submitted);
    assert_eq!(payload.producer_declaration, "external implementation");
    assert_eq!(payload.target_commit, head);

    let overridden = reference(
        &command(
            &root,
            &[
                "implementation",
                "register",
                "--effort",
                &effort,
                "--adoption",
                &adoption.artifact_id,
                "--commit",
                &head,
                "--status",
                "partial",
                "--declaration",
                "Partial implementation; blocked by pending review",
            ],
        ),
        "implementation",
    );
    let payload = implementation_payload(&root, &effort, &overridden);
    assert_eq!(payload.status, ImplementationStatus::Partial);
    assert_eq!(
        payload.producer_declaration,
        "Partial implementation; blocked by pending review"
    );
}
