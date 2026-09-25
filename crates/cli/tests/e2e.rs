use orchestrate_contracts::{
    ArtifactKind, ArtifactRef, AuditAssessment, Coverage, CoverageState, DiscoverySourceRef,
    EvidenceKind, EvidenceNode, EvidenceStatus, EvidenceSynthesis, ReconcileProposal,
    ReconciledDiscovery, ReconciledRequirement, Requirement, TechnicalSuggestion, Verification,
};
use orchestrate_core::Store;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{Arc, Barrier},
    thread,
    time::{SystemTime, UNIX_EPOCH},
};

fn temporary(name: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("orchestrate-{name}-{nonce}"));
    fs::create_dir_all(&path).unwrap();
    path
}
fn git(repo: &Path, args: &[&str]) {
    assert!(
        Command::new("git")
            .args(args)
            .current_dir(repo)
            .status()
            .unwrap()
            .success()
    );
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
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}
fn reference(value: &serde_json::Value, key: &str) -> ArtifactRef {
    serde_json::from_value(value["details"][key].clone()).unwrap()
}
fn create_repo(name: &str) -> PathBuf {
    let repo = temporary(&format!("{name}-repo"));
    git(&repo, &["init"]);
    git(&repo, &["config", "user.email", "test@example.com"]);
    git(&repo, &["config", "user.name", "Test"]);
    fs::write(repo.join("source.txt"), "committed\n").unwrap();
    git(&repo, &["add", "."]);
    git(&repo, &["commit", "-m", "baseline"]);
    repo
}
fn new_effort(name: &str) -> (PathBuf, PathBuf, String) {
    new_effort_with_constraints(name, &[])
}
fn new_effort_with_constraints(name: &str, constraints: &[&str]) -> (PathBuf, PathBuf, String) {
    let root = temporary(&format!("{name}-store"));
    let repo = create_repo(name);
    let mut args = vec![
        "init",
        "--project",
        repo.to_str().unwrap(),
        "--effort",
        name,
        "--request",
        "change fixture",
    ];
    for constraint in constraints {
        args.extend(["--constraint", constraint]);
    }
    let result = command(&root, &args);
    (
        root,
        repo,
        result["details"]["effort"].as_str().unwrap().to_owned(),
    )
}
fn node(
    id: &str,
    kind: EvidenceKind,
    status: EvidenceStatus,
    depends_on: Vec<&str>,
    mandatory: bool,
) -> EvidenceNode {
    let verification = matches!(kind, EvidenceKind::Finding).then_some(Verification::Inspection);
    EvidenceNode {
        id: id.into(),
        kind,
        status,
        depends_on: depends_on.into_iter().map(str::to_owned).collect(),
        sources: vec!["fixture".into()],
        required: false,
        mandatory,
        title: id.into(),
        body: "evidence".into(),
        verification,
    }
}
fn write_node(workspace: &Path, node: &EvidenceNode) {
    let front = serde_yaml::to_string(&serde_json::json!({
        "id": node.id, "kind": node.kind, "status": node.status, "depends_on": node.depends_on,
        "sources": node.sources, "required": node.required, "mandatory": node.mandatory,
        "verification": node.verification
    }))
    .unwrap();
    fs::write(
        workspace.join("graph").join(format!("{}.md", node.id)),
        format!("---\n{front}---\n\n# {}\n\n{}\n", node.title, node.body),
    )
    .unwrap();
}
fn write_ready_workspace(workspace: &Path) {
    fs::write(
        workspace.join("technical-spec.md"),
        "# Technical specification\n\nEnough context for reconciliation.\n",
    )
    .unwrap();
    let finding = node(
        "F-1",
        EvidenceKind::Finding,
        EvidenceStatus::Accepted,
        vec![],
        false,
    );
    let requirement = node(
        "R-1",
        EvidenceKind::Requirement,
        EvidenceStatus::Accepted,
        vec!["F-1"],
        true,
    );
    write_node(workspace, &finding);
    write_node(workspace, &requirement);
}
fn write_ready_workspace_with_verification(workspace: &Path, verification: Verification) {
    fs::write(
        workspace.join("technical-spec.md"),
        "# Technical specification\n\nEnough context for reconciliation.\n",
    )
    .unwrap();
    let mut finding = node(
        "F-1",
        EvidenceKind::Finding,
        EvidenceStatus::Accepted,
        vec![],
        false,
    );
    finding.verification = Some(verification);
    let requirement = node(
        "R-1",
        EvidenceKind::Requirement,
        EvidenceStatus::Accepted,
        vec!["F-1"],
        true,
    );
    write_node(workspace, &finding);
    write_node(workspace, &requirement);
}
fn write_blocked_workspace(workspace: &Path) {
    fs::write(
        workspace.join("technical-spec.md"),
        "# Technical specification\n\nBlocked by a material question.\n",
    )
    .unwrap();
    write_node(
        workspace,
        &node(
            "Q-1",
            EvidenceKind::Question,
            EvidenceStatus::Blocked,
            vec![],
            false,
        ),
    );
}
fn finalize_discovery(root: &Path, effort: &str) -> ArtifactRef {
    let prepared = command(root, &["discovery", "prepare", "--effort", effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    write_ready_workspace(&workspace);
    let run = prepared["details"]["run"].as_str().unwrap();
    reference(
        &command(
            root,
            &["discovery", "finalize", "--effort", effort, "--run", run],
        ),
        "artifact",
    )
}
fn finalize_discovery_with_workspace(root: &Path, effort: &str) -> (ArtifactRef, PathBuf) {
    let prepared = command(root, &["discovery", "prepare", "--effort", effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    write_ready_workspace(&workspace);
    let run = prepared["details"]["run"].as_str().unwrap();
    let artifact = reference(
        &command(
            root,
            &["discovery", "finalize", "--effort", effort, "--run", run],
        ),
        "artifact",
    );
    (artifact, workspace)
}
fn finalize_discovery_with_verification(
    root: &Path,
    effort: &str,
    verification: Verification,
) -> ArtifactRef {
    let prepared = command(root, &["discovery", "prepare", "--effort", effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    write_ready_workspace_with_verification(&workspace, verification);
    let run = prepared["details"]["run"].as_str().unwrap();
    reference(
        &command(
            root,
            &["discovery", "finalize", "--effort", effort, "--run", run],
        ),
        "artifact",
    )
}
fn finalize_blocked_discovery(root: &Path, effort: &str) -> ArtifactRef {
    let prepared = command(root, &["discovery", "prepare", "--effort", effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    write_blocked_workspace(&workspace);
    let run = prepared["details"]["run"].as_str().unwrap();
    reference(
        &command(
            root,
            &["discovery", "finalize", "--effort", effort, "--run", run],
        ),
        "artifact",
    )
}
fn proposal(source: &ArtifactRef) -> ReconcileProposal {
    ReconcileProposal {
        core_result: "Deliver the requested behavior while preserving existing behavior.".into(),
        problem: "The requested behavior is not currently delivered.".into(),
        product_behavior_changed: vec!["The requested behavior is delivered.".into()],
        product_behavior_unchanged: vec!["Unrelated behavior remains unchanged.".into()],
        technical_behavior_changed: vec!["The focused implementation path changes.".into()],
        technical_behavior_unchanged: vec!["Unrelated internals remain unchanged.".into()],
        requirements: vec![ReconciledRequirement {
            requirement: Requirement {
                id: "R-1".into(),
                text: "The required behavior is delivered.".into(),
                acceptance: "A focused implementation check demonstrates it.".into(),
                condition: None,
                governing: false,
            },
            source_refs: vec![DiscoverySourceRef {
                discovery_artifact_id: source.artifact_id.clone(),
                node_id: Some("R-1".into()),
            }],
            user_clarification: None,
            frozen_user_constraint: false,
        }],
        evidence_synthesis: vec![EvidenceSynthesis {
            id: "E-1".into(),
            conclusion: "The requested behavior needs implementation.".into(),
            source_refs: vec![DiscoverySourceRef {
                discovery_artifact_id: source.artifact_id.clone(),
                node_id: Some("F-1".into()),
            }],
            verification_methods: vec![Verification::Inspection],
            evidence_summary: "The source was inspected.".into(),
            limitations: "No controlled reproduction was needed for this fixture.".into(),
        }],
        disagreements: vec![],
        rejected_alternatives: vec![],
        implementation_risks: vec![],
        compatibility_concerns: vec![],
        caveats: vec![],
        technical_suggestions: vec![TechnicalSuggestion {
            id: "TS-1".into(),
            text: "An optional implementation technique.".into(),
            source_refs: vec![DiscoverySourceRef {
                discovery_artifact_id: source.artifact_id.clone(),
                node_id: Some("F-1".into()),
            }],
        }],
        blocking_issues: vec![],
    }
}
fn write_proposal(root: &Path, proposal: &ReconcileProposal) -> PathBuf {
    let path = root.join("reconcile-proposal.json");
    fs::write(&path, orchestrate_contracts::encode(proposal).unwrap()).unwrap();
    path
}
fn reconcile(root: &Path, effort: &str, inputs: &[ArtifactRef]) -> ArtifactRef {
    let proposal_path = write_proposal(root, &proposal(&inputs[0]));
    let mut args = vec!["reconcile", "finalize", "--effort", effort];
    for input in inputs {
        args.push("--discovery");
        args.push(&input.artifact_id);
    }
    args.extend(["--bundle", proposal_path.to_str().unwrap()]);
    reference(&command(root, &args), "reconciled")
}

#[test]
fn prepared_request_has_no_slots_or_quorum_and_legacy_fields_are_rejected() {
    let (root, repo, _) = new_effort("prepared");
    let prepared = root.join("request.prepared.md");
    fs::write(&prepared, format!("---\nroot: {}\nproject: {}\neffort: from-file\nrequest_kind: freeform\nconstraints: []\n---\nrequest\n", root.display(), repo.display())).unwrap();
    let output = command(&root, &["init", "--from-file", prepared.to_str().unwrap()]);
    assert_eq!(output["semantic_outcome"], "EFFORT_READY");
    let legacy = root.join("legacy.prepared.md");
    fs::write(&legacy, format!("---\nroot: {}\nproject: {}\neffort: legacy\nrequest_kind: freeform\nslots: [a, b]\n---\nrequest\n", root.display(), repo.display())).unwrap();
    assert!(
        command_error(&root, &["init", "--from-file", legacy.to_str().unwrap()])
            .contains("unknown field `slots`")
    );
    assert!(
        command_error(
            &root,
            &[
                "init",
                "--project",
                repo.to_str().unwrap(),
                "--effort",
                "x",
                "--request",
                "x",
                "--slot",
                "a"
            ]
        )
        .contains("unexpected argument '--slot'")
    );
    assert!(
        command_error(
            &root,
            &[
                "init",
                "--project",
                repo.to_str().unwrap(),
                "--effort",
                "x",
                "--request",
                "x",
                "--quorum",
                "2"
            ]
        )
        .contains("unexpected argument '--quorum'")
    );
    assert!(
        command_error(
            &root,
            &[
                "init",
                "--project",
                repo.to_str().unwrap(),
                "--effort",
                "x",
                "--request",
                "x",
                "--providers",
                "providers.toml"
            ]
        )
        .contains("unexpected argument '--providers'")
    );
    assert!(
        command_error(&root, &["discovery", "prepare-all", "--effort", "x"])
            .contains("unrecognized subcommand 'prepare-all'")
    );
}

#[test]
fn concurrent_identical_initialization_converges_on_one_frozen_effort() {
    let root = temporary("concurrent-store");
    let repo = create_repo("concurrent");
    const INITIALIZERS: usize = 24;
    let barrier = Arc::new(Barrier::new(INITIALIZERS));
    let handles: Vec<_> = (0..INITIALIZERS)
        .map(|_| {
            let root = root.clone();
            let repo = repo.clone();
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                Command::new(env!("CARGO_BIN_EXE_orchestrate"))
                    .arg("--root")
                    .arg(root)
                    .args([
                        "init",
                        "--project",
                        repo.to_str().unwrap(),
                        "--effort",
                        "parallel",
                        "--request",
                        "same request",
                    ])
                    .output()
                    .unwrap()
            })
        })
        .collect();
    let mut efforts = Vec::new();
    for handle in handles {
        let output = handle.join().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        efforts.push(serde_json::from_slice::<serde_json::Value>(&output.stdout).unwrap()["details"]["effort"].as_str().unwrap().to_owned());
    }
    assert!(efforts.iter().all(|effort| effort == &efforts[0]));
    let projects: Vec<_> = fs::read_dir(root.join("projects"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.is_dir())
        .collect();
    assert_eq!(projects.len(), 1, "only one logical project is created");
    let project: orchestrate_core::Project =
        serde_json::from_slice(&fs::read(projects[0].join("project.json")).unwrap()).unwrap();
    assert_eq!(project.canonical_locator, fs::canonicalize(&repo).unwrap());
    let effort_dirs: Vec<_> = fs::read_dir(projects[0].join("efforts"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.join("effort.json").is_file())
        .collect();
    assert_eq!(effort_dirs.len(), 1, "only one logical effort is created");
    let effort = Store::open(&root)
        .unwrap()
        .load_effort(&efforts[0])
        .unwrap();
    assert!(!effort.baseline_commit.is_empty() && !effort.baseline_tree.is_empty());
}

#[test]
fn effort_slug_freezes_request_and_constraints_but_other_slugs_are_allowed() {
    let root = temporary("immutable-effort-store");
    let repo = create_repo("immutable-effort");
    let init = |slug: &str, request: &str, constraint: &str| {
        command(
            &root,
            &[
                "init",
                "--project",
                repo.to_str().unwrap(),
                "--effort",
                slug,
                "--request",
                request,
                "--constraint",
                constraint,
            ],
        )
    };
    let first = init("example-effort", "request A", "constraint A");
    let repeated = init("example-effort", "request A", "constraint A");
    assert_eq!(first["details"]["effort"], repeated["details"]["effort"]);
    assert!(
        command_error(
            &root,
            &[
                "init",
                "--project",
                repo.to_str().unwrap(),
                "--effort",
                "example-effort",
                "--request",
                "request B",
                "--constraint",
                "constraint A"
            ]
        )
        .contains("prepared request is immutable")
    );
    assert!(
        command_error(
            &root,
            &[
                "init",
                "--project",
                repo.to_str().unwrap(),
                "--effort",
                "example-effort",
                "--request",
                "request A",
                "--constraint",
                "constraint B"
            ]
        )
        .contains("prepared request is immutable")
    );
    assert_eq!(
        init("revised-effort", "request B", "constraint B")["semantic_outcome"],
        "EFFORT_READY"
    );
}

#[test]
fn storage_uses_human_names_while_preserving_internal_identity_and_file_provenance() {
    let root = temporary("human-storage-store");
    let repo = create_repo("DB_Financial_Tracker");
    let prepared = temporary("prepared-source").join("prepared.md");
    fs::write(&prepared, format!("---\nroot: {}\nproject: {}\neffort: add_new_endpoint_for_sales\nrequest_kind: freeform\nconstraints: []\n---\nrequest\n", root.display(), repo.display())).unwrap();
    let result = command(&root, &["init", "--from-file", prepared.to_str().unwrap()]);
    let effort_id = result["details"]["effort"].as_str().unwrap();
    let store = Store::open(&root).unwrap();
    let effort = store.load_effort(effort_id).unwrap();
    let project_name = repo.file_name().unwrap().to_string_lossy().to_lowercase();
    let directory = root
        .join("projects")
        .join(project_name)
        .join("efforts")
        .join("add_new_endpoint_for_sales");
    assert!(directory.join("effort.json").is_file());
    assert!(effort.id.starts_with("effort-") && effort.context.id.starts_with("ctx-"));
    let source = effort.prepared_source.unwrap();
    assert_eq!(source.absolute_path, fs::canonicalize(&prepared).unwrap());
    assert_eq!(source.sha256.len(), 64);
    assert!(source.initialized_at_ms > 0);
}

#[test]
fn colliding_human_project_names_from_different_repositories_are_rejected() {
    let root = temporary("project-collision-store");
    let first_parent = temporary("project-collision-a");
    let second_parent = temporary("project-collision-b");
    let first = first_parent.join("same_name");
    let second = second_parent.join("same_name");
    for repo in [&first, &second] {
        fs::create_dir_all(repo).unwrap();
        git(repo, &["init"]);
        git(repo, &["config", "user.email", "test@example.com"]);
        git(repo, &["config", "user.name", "Test"]);
        fs::write(repo.join("source.txt"), "baseline\n").unwrap();
        git(repo, &["add", "."]);
        git(repo, &["commit", "-m", "baseline"]);
    }
    command(
        &root,
        &[
            "init",
            "--project",
            first.to_str().unwrap(),
            "--effort",
            "one",
            "--request",
            "one",
        ],
    );
    assert!(
        command_error(
            &root,
            &[
                "init",
                "--project",
                second.to_str().unwrap(),
                "--effort",
                "two",
                "--request",
                "two"
            ]
        )
        .contains("already used by a different canonical repository")
    );
}

#[test]
fn discovery_is_arbitrarily_repeatable_without_slots() {
    let (root, _repo, effort) = new_effort("many-discoveries");
    let artifacts: Vec<_> = (0..5).map(|_| finalize_discovery(&root, &effort)).collect();
    assert_eq!(artifacts.len(), 5);
    assert_eq!(
        artifacts
            .iter()
            .map(|a| &a.artifact_id)
            .collect::<std::collections::HashSet<_>>()
            .len(),
        5
    );
    assert!(
        artifacts
            .iter()
            .all(|artifact| artifact.kind == ArtifactKind::Discovery)
    );
}

#[test]
fn discovery_verification_and_human_source_label_round_trip() {
    let (root, _repo, effort) = new_effort("discovery-provenance");
    let prepared = command(
        &root,
        &[
            "discovery",
            "prepare",
            "--effort",
            &effort,
            "--host",
            "Codex",
        ],
    );
    let label = prepared["details"]["source_label"].as_str().unwrap();
    assert!(label.starts_with("codex_"));
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    write_ready_workspace(&workspace);
    let run = prepared["details"]["run"].as_str().unwrap();
    let artifact = reference(
        &command(
            &root,
            &["discovery", "finalize", "--effort", &effort, "--run", run],
        ),
        "artifact",
    );
    let store = Store::open(&root).unwrap();
    let effort_state = store.load_effort(&effort).unwrap();
    let (_, _, nodes) =
        orchestrate_discovery::load_discovery(&store, &effort_state, &artifact).unwrap();
    assert_eq!(
        nodes
            .iter()
            .find(|node| node.id == "F-1")
            .unwrap()
            .verification,
        Some(Verification::Inspection)
    );
    let (_, files) = store.load_bundle(&effort_state, &artifact).unwrap();
    let run: orchestrate_contracts::DiscoveryRun =
        orchestrate_contracts::decode(&files["run.json"]).unwrap();
    assert_eq!(run.source_label, label);
}

#[test]
fn discovery_publication_rechecks_complete_frozen_context_and_source() {
    let (root, _repo, effort) = new_effort_with_constraints("frozen-inputs", &["preserve order"]);
    let prepared = command(&root, &["discovery", "prepare", "--effort", &effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    let run = prepared["details"]["run"].as_str().unwrap();
    write_ready_workspace(&workspace);

    let context_path = workspace.join("context.json");
    let mut context: serde_json::Value =
        serde_json::from_slice(&fs::read(&context_path).unwrap()).unwrap();
    context["request"] = serde_json::json!("mutated request");
    fs::write(&context_path, serde_json::to_vec(&context).unwrap()).unwrap();
    assert!(
        command_error(
            &root,
            &["discovery", "validate", "--effort", &effort, "--run", run]
        )
        .contains("context request differs")
    );
    assert!(
        command_error(
            &root,
            &["discovery", "finalize", "--effort", &effort, "--run", run]
        )
        .contains("context request differs")
    );
    assert!(
        fs::read_dir(workspace.parent().unwrap())
            .unwrap()
            .all(|entry| !entry.unwrap().path().join("manifest.json").exists())
    );

    let prepared = command(&root, &["discovery", "prepare", "--effort", &effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    let run = prepared["details"]["run"].as_str().unwrap();
    write_ready_workspace(&workspace);
    command(
        &root,
        &["discovery", "validate", "--effort", &effort, "--run", run],
    );
    fs::write(workspace.join("source").join("untracked.txt"), "drift\n").unwrap();
    assert!(
        command_error(
            &root,
            &["discovery", "finalize", "--effort", &effort, "--run", run]
        )
        .contains("source checkout is dirty")
    );
}

#[test]
fn discovery_context_requires_complete_values_but_allows_equivalent_json_and_scratch_files() {
    let (root, _repo, effort) =
        new_effort_with_constraints("context-values", &["first constraint", "second constraint"]);
    let prepared = command(&root, &["discovery", "prepare", "--effort", &effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    write_ready_workspace(&workspace);
    let context_path = workspace.join("context.json");
    let context: serde_json::Value =
        serde_json::from_slice(&fs::read(&context_path).unwrap()).unwrap();
    fs::write(
        &context_path,
        serde_json::to_string_pretty(&context).unwrap(),
    )
    .unwrap();
    fs::write(
        workspace.join("scratch").join("experiment.txt"),
        "allowed\n",
    )
    .unwrap();
    let run = prepared["details"]["run"].as_str().unwrap();
    reference(
        &command(
            &root,
            &["discovery", "finalize", "--effort", &effort, "--run", run],
        ),
        "artifact",
    );

    let prepared = command(&root, &["discovery", "prepare", "--effort", &effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    write_ready_workspace(&workspace);
    let context_path = workspace.join("context.json");
    let mut context: serde_json::Value =
        serde_json::from_slice(&fs::read(&context_path).unwrap()).unwrap();
    context.as_object_mut().unwrap().remove("request");
    fs::write(&context_path, serde_json::to_vec(&context).unwrap()).unwrap();
    let run = prepared["details"]["run"].as_str().unwrap();
    assert!(
        command_error(
            &root,
            &["discovery", "validate", "--effort", &effort, "--run", run]
        )
        .contains("missing field `request`")
    );

    let prepared = command(&root, &["discovery", "prepare", "--effort", &effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    write_ready_workspace(&workspace);
    let context_path = workspace.join("context.json");
    let mut context: serde_json::Value =
        serde_json::from_slice(&fs::read(&context_path).unwrap()).unwrap();
    context.as_object_mut().unwrap().remove("constraints");
    fs::write(&context_path, serde_json::to_vec(&context).unwrap()).unwrap();
    let run = prepared["details"]["run"].as_str().unwrap();
    assert!(
        command_error(
            &root,
            &["discovery", "validate", "--effort", &effort, "--run", run]
        )
        .contains("missing field `constraints`")
    );

    let prepared = command(&root, &["discovery", "prepare", "--effort", &effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    write_ready_workspace(&workspace);
    let context_path = workspace.join("context.json");
    let mut context: serde_json::Value =
        serde_json::from_slice(&fs::read(&context_path).unwrap()).unwrap();
    context["constraints"] = serde_json::json!(["second constraint", "first constraint"]);
    fs::write(&context_path, serde_json::to_vec(&context).unwrap()).unwrap();
    let run = prepared["details"]["run"].as_str().unwrap();
    assert!(
        command_error(
            &root,
            &["discovery", "validate", "--effort", &effort, "--run", run]
        )
        .contains("context constraints differ")
    );
}

#[test]
fn discovery_rejects_missing_or_enclosing_source_worktree_and_blocked_drift() {
    let (root, _repo, effort) = new_effort("source-worktree");
    let prepared = command(&root, &["discovery", "prepare", "--effort", &effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    write_ready_workspace(&workspace);
    fs::rename(workspace.join("source"), workspace.join("source-moved")).unwrap();
    let run = prepared["details"]["run"].as_str().unwrap();
    assert!(
        command_error(
            &root,
            &["discovery", "finalize", "--effort", &effort, "--run", run]
        )
        .contains("source checkout is missing")
    );

    let prepared = command(&root, &["discovery", "prepare", "--effort", &effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    write_ready_workspace(&workspace);
    fs::rename(workspace.join("source"), workspace.join("repository")).unwrap();
    fs::rename(
        workspace.join("repository").join(".git"),
        workspace.join(".git"),
    )
    .unwrap();
    fs::rename(workspace.join("repository"), workspace.join("source")).unwrap();
    let run = prepared["details"]["run"].as_str().unwrap();
    assert!(
        command_error(
            &root,
            &["discovery", "validate", "--effort", &effort, "--run", run]
        )
        .contains("rather than itself")
    );

    let prepared = command(&root, &["discovery", "prepare", "--effort", &effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    write_blocked_workspace(&workspace);
    fs::write(workspace.join("source").join("drift.txt"), "not allowed\n").unwrap();
    let run = prepared["details"]["run"].as_str().unwrap();
    assert!(
        command_error(
            &root,
            &["discovery", "finalize", "--effort", &effort, "--run", run]
        )
        .contains("source checkout is dirty")
    );
}

#[test]
fn discovery_rejects_every_git_state_drift_including_ignored_files() {
    let (root, _repo, effort) = new_effort("git-drift");
    for drift in [
        "staged",
        "unstaged",
        "ignored",
        "same-tree-commit",
        "assume-unchanged",
        "skip-worktree",
    ] {
        let prepared = command(&root, &["discovery", "prepare", "--effort", &effort]);
        let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
        let run = prepared["details"]["run"].as_str().unwrap();
        let source = workspace.join("source");
        write_ready_workspace(&workspace);
        match drift {
            "staged" => {
                fs::write(source.join("source.txt"), "staged drift\n").unwrap();
                git(&source, &["add", "source.txt"]);
            }
            "unstaged" => {
                fs::write(source.join("source.txt"), "unstaged drift\n").unwrap();
            }
            "ignored" => {
                fs::write(source.join(".git/info/exclude"), "generated.out\n").unwrap();
                fs::write(source.join("generated.out"), "ignored drift\n").unwrap();
            }
            "same-tree-commit" => {
                git(&source, &["config", "user.email", "test@example.com"]);
                git(&source, &["config", "user.name", "Test"]);
                git(
                    &source,
                    &["commit", "--allow-empty", "-m", "different commit"],
                );
            }
            "assume-unchanged" => {
                git(
                    &source,
                    &["update-index", "--assume-unchanged", "source.txt"],
                );
                fs::write(source.join("source.txt"), "hidden tracked drift\n").unwrap();
            }
            "skip-worktree" => {
                git(&source, &["update-index", "--skip-worktree", "source.txt"]);
                fs::write(source.join("source.txt"), "hidden tracked drift\n").unwrap();
            }
            _ => unreachable!(),
        }
        let error = command_error(
            &root,
            &["discovery", "finalize", "--effort", &effort, "--run", run],
        );
        assert!(
            error.contains("source") || error.contains("HEAD differs"),
            "{drift}: {error}"
        );
    }
}

#[test]
fn discovery_allows_unchanged_source_with_index_metadata_flags() {
    let (root, _repo, effort) = new_effort("git-index-metadata");
    for flag in ["--assume-unchanged", "--skip-worktree"] {
        let prepared = command(&root, &["discovery", "prepare", "--effort", &effort]);
        let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
        write_ready_workspace(&workspace);
        git(
            &workspace.join("source"),
            &["update-index", flag, "source.txt"],
        );
        let run = prepared["details"]["run"].as_str().unwrap();
        reference(
            &command(
                &root,
                &["discovery", "finalize", "--effort", &effort, "--run", run],
            ),
            "artifact",
        );
    }
}

#[test]
fn discovery_rejects_generated_technical_spec_even_with_a_valid_graph() {
    let (root, _repo, effort) = new_effort("technical-spec-body");
    let prepared = command(&root, &["discovery", "prepare", "--effort", &effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    let run = prepared["details"]["run"].as_str().unwrap();
    write_ready_workspace(&workspace);
    fs::write(
        workspace.join("technical-spec.md"),
        "\n# Technical specification\r\n\r\n",
    )
    .unwrap();
    assert!(
        command_error(
            &root,
            &["discovery", "finalize", "--effort", &effort, "--run", run]
        )
        .contains("no content beyond Markdown headings")
    );
}

#[test]
fn reconcile_binds_an_explicit_arbitrary_set_without_quorum() {
    let (root, _repo, effort) = new_effort("reconcile-inputs");
    let inputs: Vec<_> = (0..5).map(|_| finalize_discovery(&root, &effort)).collect();
    let mut args = vec!["reconcile", "inputs", "--effort", &effort];
    for input in &inputs {
        args.push("--discovery");
        args.push(&input.artifact_id);
    }
    let resolved = command(&root, &args);
    assert_eq!(
        resolved["details"]["discoveries"].as_array().unwrap().len(),
        5
    );
    assert!(
        command_error(
            &root,
            &[
                "reconcile",
                "inputs",
                "--effort",
                &effort,
                "--discovery",
                &inputs[0].artifact_id
            ]
        )
        .contains("at least two")
    );
    assert!(
        command_error(
            &root,
            &[
                "reconcile",
                "inputs",
                "--effort",
                &effort,
                "--discovery",
                &inputs[0].artifact_id,
                "--discovery",
                &inputs[0].artifact_id
            ]
        )
        .contains("duplicate Discovery artifact")
    );
    let blocked = finalize_blocked_discovery(&root, &effort);
    assert!(
        command_error(
            &root,
            &[
                "reconcile",
                "inputs",
                "--effort",
                &effort,
                "--discovery",
                &inputs[0].artifact_id,
                "--discovery",
                &blocked.artifact_id
            ]
        )
        .contains("not implementation-ready")
    );
}

#[test]
fn reconciliation_preserves_exact_parents_and_allows_a_minority_source() {
    let (root, _repo, effort) = new_effort("reconcile-finalize");
    let inputs: Vec<_> = (0..3).map(|_| finalize_discovery(&root, &effort)).collect();
    let reconciled = reconcile(&root, &effort, &inputs);
    let late = finalize_discovery(&root, &effort);
    let store = Store::open(&root).unwrap();
    let effort_state = store.load_effort(&effort).unwrap();
    let (manifest, reconciled_payload): (_, ReconciledDiscovery) = store
        .load_json(&effort_state, &reconciled, "reconciled-discovery.json")
        .unwrap();
    assert_eq!(manifest.parents, inputs);
    assert!(!manifest.parents.contains(&late));
    assert_eq!(
        reconciled_payload.requirements[0].source_refs.len(),
        1,
        "one strong Discovery source is structurally sufficient"
    );
    assert_eq!(reconciled_payload.technical_suggestions.len(), 1);
    assert_eq!(reconciled_payload.discovery_attribution.len(), 3);
    let files = store.load_bundle(&effort_state, &reconciled).unwrap().1;
    let markdown = String::from_utf8(files["reconciled-discovery.md"].clone()).unwrap();
    assert!(
        reconciled_payload
            .discovery_attribution
            .iter()
            .all(|source| markdown.contains(&source.label))
    );
}

#[test]
fn reconciliation_binds_verification_methods_to_directly_cited_findings() {
    let (root, _repo, effort) = new_effort("reconcile-verification");
    let inspection = finalize_discovery_with_verification(&root, &effort, Verification::Inspection);
    let experiment = finalize_discovery_with_verification(&root, &effort, Verification::Experiment);

    let mut mismatched = proposal(&inspection);
    mismatched.evidence_synthesis[0].source_refs = vec![DiscoverySourceRef {
        discovery_artifact_id: experiment.artifact_id.clone(),
        node_id: Some("F-1".into()),
    }];
    let path = write_proposal(&root, &mismatched);
    assert!(
        command_error(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &inspection.artifact_id,
                "--discovery",
                &experiment.artifact_id,
                "--bundle",
                path.to_str().unwrap(),
            ],
        )
        .contains("verification methods mismatch")
    );

    let mut mixed = proposal(&inspection);
    mixed.evidence_synthesis[0].source_refs = vec![
        DiscoverySourceRef {
            discovery_artifact_id: experiment.artifact_id.clone(),
            node_id: Some("F-1".into()),
        },
        DiscoverySourceRef {
            discovery_artifact_id: inspection.artifact_id.clone(),
            node_id: Some("F-1".into()),
        },
        DiscoverySourceRef {
            discovery_artifact_id: experiment.artifact_id.clone(),
            node_id: Some("F-1".into()),
        },
    ];
    mixed.evidence_synthesis[0].verification_methods = vec![
        Verification::Inspection,
        Verification::Experiment,
        Verification::Experiment,
    ];
    let path = write_proposal(&root, &mixed);
    let reconciled = reference(
        &command(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &inspection.artifact_id,
                "--discovery",
                &experiment.artifact_id,
                "--bundle",
                path.to_str().unwrap(),
            ],
        ),
        "reconciled",
    );
    assert_eq!(reconciled.kind, ArtifactKind::ReconciledDiscovery);

    let mut no_finding = proposal(&inspection);
    no_finding.evidence_synthesis[0].source_refs = vec![DiscoverySourceRef {
        discovery_artifact_id: inspection.artifact_id.clone(),
        node_id: None,
    }];
    no_finding.evidence_synthesis[0].verification_methods = vec![];
    let path = write_proposal(&root, &no_finding);
    let reconciled = reference(
        &command(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &inspection.artifact_id,
                "--discovery",
                &experiment.artifact_id,
                "--bundle",
                path.to_str().unwrap(),
            ],
        ),
        "reconciled",
    );
    let store = Store::open(&root).unwrap();
    let effort_state = store.load_effort(&effort).unwrap();
    let (_, files) = store.load_bundle(&effort_state, &reconciled).unwrap();
    assert!(
        String::from_utf8(files["reconciled-discovery.md"].clone())
            .unwrap()
            .contains("Not attributed to specific Finding nodes")
    );

    let mut unsupported = proposal(&inspection);
    unsupported.evidence_synthesis[0].source_refs[0].node_id = Some("R-1".into());
    let path = write_proposal(&root, &unsupported);
    assert!(
        command_error(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &inspection.artifact_id,
                "--discovery",
                &experiment.artifact_id,
                "--bundle",
                path.to_str().unwrap(),
            ],
        )
        .contains("verification methods mismatch")
    );
}

#[test]
fn reconciliation_preserves_verification_for_rejected_findings() {
    let (root, _repo, effort) = new_effort("rejected-finding-verification");
    let prepared = command(&root, &["discovery", "prepare", "--effort", &effort]);
    let workspace = PathBuf::from(prepared["details"]["workspace"].as_str().unwrap());
    write_ready_workspace(&workspace);
    let mut rejected = node(
        "F-2",
        EvidenceKind::Finding,
        EvidenceStatus::Rejected,
        vec![],
        false,
    );
    rejected.verification = Some(Verification::Experiment);
    write_node(&workspace, &rejected);
    let run = prepared["details"]["run"].as_str().unwrap();
    let rejected_source = reference(
        &command(
            &root,
            &["discovery", "finalize", "--effort", &effort, "--run", run],
        ),
        "artifact",
    );
    let companion = finalize_discovery(&root, &effort);

    let mut mismatched = proposal(&rejected_source);
    mismatched.evidence_synthesis[0].source_refs[0].node_id = Some("F-2".into());
    let path = write_proposal(&root, &mismatched);
    assert!(
        command_error(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &rejected_source.artifact_id,
                "--discovery",
                &companion.artifact_id,
                "--bundle",
                path.to_str().unwrap(),
            ],
        )
        .contains("verification methods mismatch")
    );
    let mut attributed = proposal(&rejected_source);
    attributed.evidence_synthesis[0].source_refs[0].node_id = Some("F-2".into());
    attributed.evidence_synthesis[0].verification_methods = vec![Verification::Experiment];
    let path = write_proposal(&root, &attributed);
    reference(
        &command(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &rejected_source.artifact_id,
                "--discovery",
                &companion.artifact_id,
                "--bundle",
                path.to_str().unwrap(),
            ],
        ),
        "reconciled",
    );
}

#[test]
fn reconciled_markdown_renders_source_node_ids_in_every_citation_section() {
    let (root, _repo, effort) = new_effort("reconcile-source-rendering");
    let inputs = [
        finalize_discovery(&root, &effort),
        finalize_discovery(&root, &effort),
    ];
    let mut proposed = proposal(&inputs[0]);
    proposed
        .rejected_alternatives
        .push(orchestrate_contracts::RejectedAlternative {
            direction: "Do nothing.".into(),
            reason: "The cited finding establishes a change is needed.".into(),
            source_refs: vec![DiscoverySourceRef {
                discovery_artifact_id: inputs[0].artifact_id.clone(),
                node_id: Some("F-1".into()),
            }],
        });
    let path = write_proposal(&root, &proposed);
    let reconciled = reference(
        &command(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &inputs[0].artifact_id,
                "--discovery",
                &inputs[1].artifact_id,
                "--bundle",
                path.to_str().unwrap(),
            ],
        ),
        "reconciled",
    );
    let store = Store::open(&root).unwrap();
    let effort_state = store.load_effort(&effort).unwrap();
    let (_, payload): (_, ReconciledDiscovery) = store
        .load_json(&effort_state, &reconciled, "reconciled-discovery.json")
        .unwrap();
    let (_, files) = store.load_bundle(&effort_state, &reconciled).unwrap();
    let markdown = String::from_utf8(files["reconciled-discovery.md"].clone()).unwrap();
    let source = payload
        .discovery_attribution
        .iter()
        .find(|source| source.discovery_artifact_id == inputs[0].artifact_id)
        .unwrap();
    assert!(markdown.contains(&format!(
        "{} (`{}`) / R-1",
        source.label, source.discovery_artifact_id
    )));
    assert_eq!(
        markdown
            .matches(&format!(
                "{} (`{}`) / F-1",
                source.label, source.discovery_artifact_id
            ))
            .count(),
        3,
        "synthesis, rejected alternative, and suggestion each retain F-1"
    );

    let mut artifact_only = proposal(&inputs[0]);
    artifact_only.requirements[0].source_refs[0].node_id = None;
    artifact_only.evidence_synthesis[0].source_refs[0].node_id = None;
    artifact_only.evidence_synthesis[0].verification_methods = vec![];
    artifact_only.technical_suggestions[0].source_refs[0].node_id = None;
    artifact_only
        .rejected_alternatives
        .push(orchestrate_contracts::RejectedAlternative {
            direction: "Keep the old behavior.".into(),
            reason: "Discovery did not support this direction.".into(),
            source_refs: vec![DiscoverySourceRef {
                discovery_artifact_id: inputs[0].artifact_id.clone(),
                node_id: None,
            }],
        });
    let path = write_proposal(&root, &artifact_only);
    let artifact_only = reference(
        &command(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &inputs[0].artifact_id,
                "--discovery",
                &inputs[1].artifact_id,
                "--bundle",
                path.to_str().unwrap(),
            ],
        ),
        "reconciled",
    );
    let (_, payload): (_, ReconciledDiscovery) = store
        .load_json(&effort_state, &artifact_only, "reconciled-discovery.json")
        .unwrap();
    let (_, files) = store.load_bundle(&effort_state, &artifact_only).unwrap();
    let markdown = String::from_utf8(files["reconciled-discovery.md"].clone()).unwrap();
    let source = payload
        .discovery_attribution
        .iter()
        .find(|source| source.discovery_artifact_id == inputs[0].artifact_id)
        .unwrap();
    assert_eq!(
        markdown
            .matches(&format!(
                "{} (`{}`)",
                source.label, source.discovery_artifact_id
            ))
            .count(),
        4,
        "requirements, synthesis, alternatives, and suggestions render artifact-only sources"
    );
    assert!(!markdown.contains(&format!(
        "{} (`{}`) /",
        source.label, source.discovery_artifact_id
    )));
}

#[test]
fn supported_version_six_bundles_reconcile_after_mutable_workspaces_are_removed() {
    assert_eq!(orchestrate_contracts::SCHEMA_VERSION, 6);
    assert_eq!(orchestrate_contracts::STORE_FORMAT_VERSION, 6);
    let (root, _repo, effort) = new_effort("historical-version-six");
    let (first, first_workspace) = finalize_discovery_with_workspace(&root, &effort);
    let (second, second_workspace) = finalize_discovery_with_workspace(&root, &effort);
    let existing = reconcile(&root, &effort, &[first.clone(), second.clone()]);
    let store = Store::open(&root).unwrap();
    let effort_state = store.load_effort(&effort).unwrap();
    let immutable = [first.clone(), second.clone(), existing.clone()];
    let before: Vec<_> = immutable
        .iter()
        .map(|reference| {
            let directory = store
                .artifact_dir(&effort_state, &reference.artifact_id)
                .unwrap();
            let manifest = fs::read(directory.join("manifest.json")).unwrap();
            let (_, files) = store.load_bundle(&effort_state, reference).unwrap();
            (reference.clone(), manifest, files)
        })
        .collect();

    fs::remove_dir_all(&first_workspace).unwrap();
    fs::remove_dir_all(&second_workspace).unwrap();
    assert!(!first_workspace.exists() && !second_workspace.exists());

    let new_reconciliation = reconcile(&root, &effort, &[first, second]);
    assert_eq!(new_reconciliation.kind, ArtifactKind::ReconciledDiscovery);
    for (reference, manifest, files) in before {
        let directory = store
            .artifact_dir(&effort_state, &reference.artifact_id)
            .unwrap();
        assert_eq!(fs::read(directory.join("manifest.json")).unwrap(), manifest);
        assert_eq!(
            store.load_bundle(&effort_state, &reference).unwrap().1,
            files
        );
    }
}

#[test]
fn reconciliation_allows_empty_changed_behavior_categories() {
    let (root, _repo, effort) = new_effort("reconcile-empty-behavior");
    let selected = [
        finalize_discovery(&root, &effort),
        finalize_discovery(&root, &effort),
    ];
    for (product_changed, technical_changed) in [
        (vec![], vec!["The implementation changes.".into()]),
        (vec!["The product behavior changes.".into()], vec![]),
    ] {
        let mut proposed = proposal(&selected[0]);
        proposed.product_behavior_changed = product_changed.clone();
        proposed.technical_behavior_changed = technical_changed.clone();
        let path = write_proposal(&root, &proposed);
        let reconciled = reference(
            &command(
                &root,
                &[
                    "reconcile",
                    "finalize",
                    "--effort",
                    &effort,
                    "--discovery",
                    &selected[0].artifact_id,
                    "--discovery",
                    &selected[1].artifact_id,
                    "--bundle",
                    path.to_str().unwrap(),
                ],
            ),
            "reconciled",
        );
        let store = Store::open(&root).unwrap();
        let effort_state = store.load_effort(&effort).unwrap();
        let (_, payload): (_, ReconciledDiscovery) = store
            .load_json(&effort_state, &reconciled, "reconciled-discovery.json")
            .unwrap();
        assert_eq!(payload.product_behavior_changed, product_changed);
        assert_eq!(payload.technical_behavior_changed, technical_changed);
    }
}

#[test]
fn reconciliation_rejects_unselected_or_missing_evidence_sources() {
    let (root, _repo, effort) = new_effort("reconcile-sources");
    let selected = [
        finalize_discovery(&root, &effort),
        finalize_discovery(&root, &effort),
    ];
    let unselected = finalize_discovery(&root, &effort);
    let mut invalid = proposal(&selected[0]);
    invalid.requirements[0].source_refs[0].discovery_artifact_id = unselected.artifact_id.clone();
    let path = write_proposal(&root, &invalid);
    assert!(
        command_error(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &selected[0].artifact_id,
                "--discovery",
                &selected[1].artifact_id,
                "--bundle",
                path.to_str().unwrap()
            ]
        )
        .contains("unselected Discovery")
    );
    let mut invalid = proposal(&selected[0]);
    invalid.requirements[0].source_refs[0].node_id = Some("F-missing".into());
    let path = write_proposal(&root, &invalid);
    assert!(
        command_error(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &selected[0].artifact_id,
                "--discovery",
                &selected[1].artifact_id,
                "--bundle",
                path.to_str().unwrap()
            ]
        )
        .contains("missing evidence node")
    );
    let mut invalid = proposal(&selected[0]);
    invalid.technical_suggestions[0].source_refs[0].discovery_artifact_id = unselected.artifact_id;
    let path = write_proposal(&root, &invalid);
    assert!(
        command_error(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &selected[0].artifact_id,
                "--discovery",
                &selected[1].artifact_id,
                "--bundle",
                path.to_str().unwrap()
            ]
        )
        .contains("unselected Discovery")
    );
}

#[test]
fn reconciliation_rejects_model_supplied_governing_requirements() {
    let (root, _repo, effort) = new_effort("governing-bypass");
    let selected = [
        finalize_discovery(&root, &effort),
        finalize_discovery(&root, &effort),
    ];
    let mut invalid = proposal(&selected[0]);
    invalid.requirements[0].requirement.governing = true;
    let path = write_proposal(&root, &invalid);
    assert!(
        command_error(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &selected[0].artifact_id,
                "--discovery",
                &selected[1].artifact_id,
                "--bundle",
                path.to_str().unwrap(),
            ],
        )
        .contains("cannot declare requirement R-1 governing")
    );
}

#[test]
fn reconciliation_requires_discovery_authority_for_ordinary_requirements() {
    let (root, _repo, effort) = new_effort("ordinary-requirement-source");
    let selected = [
        finalize_discovery(&root, &effort),
        finalize_discovery(&root, &effort),
    ];
    let mut invalid = proposal(&selected[0]);
    invalid.requirements[0].source_refs = vec![];
    let path = write_proposal(&root, &invalid);
    assert!(
        command_error(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &selected[0].artifact_id,
                "--discovery",
                &selected[1].artifact_id,
                "--bundle",
                path.to_str().unwrap(),
            ],
        )
        .contains("needs a selected Discovery source reference")
    );
}

#[test]
fn reconciliation_preserves_frozen_constraints_as_governing_authority() {
    let (root, _repo, effort) = new_effort_with_constraints(
        "frozen-governing",
        &["Staging behavior must remain unchanged."],
    );
    let selected = [
        finalize_discovery(&root, &effort),
        finalize_discovery(&root, &effort),
    ];
    let reconciled = reconcile(&root, &effort, &selected);
    let store = Store::open(&root).unwrap();
    let effort_state = store.load_effort(&effort).unwrap();
    let (_, payload): (_, ReconciledDiscovery) = store
        .load_json(&effort_state, &reconciled, "reconciled-discovery.json")
        .unwrap();
    let constraint = payload
        .requirements
        .iter()
        .find(|item| item.requirement.id == "GOV-1")
        .unwrap();
    assert!(constraint.requirement.governing);
    assert!(constraint.frozen_user_constraint);
}

#[test]
fn reconciliation_accepts_explicit_user_clarification_as_direct_authority() {
    let (root, _repo, effort) = new_effort("user-clarification");
    let selected = [
        finalize_discovery(&root, &effort),
        finalize_discovery(&root, &effort),
    ];
    let mut clarified = proposal(&selected[0]);
    clarified.requirements[0].source_refs = vec![];
    clarified.requirements[0].user_clarification =
        Some("The user chose UTC timestamps for exported reports.".into());
    let path = write_proposal(&root, &clarified);
    let reconciled = reference(
        &command(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &selected[0].artifact_id,
                "--discovery",
                &selected[1].artifact_id,
                "--bundle",
                path.to_str().unwrap(),
            ],
        ),
        "reconciled",
    );
    let store = Store::open(&root).unwrap();
    let effort_state = store.load_effort(&effort).unwrap();
    let (_, payload): (_, ReconciledDiscovery) = store
        .load_json(&effort_state, &reconciled, "reconciled-discovery.json")
        .unwrap();
    let requirement = &payload.requirements[0];
    assert!(requirement.requirement.governing);
    assert_eq!(
        requirement.user_clarification.as_deref(),
        Some("The user chose UTC timestamps for exported reports.")
    );
}

#[test]
fn reconciled_markdown_is_rendered_only_from_the_structured_contract() {
    let (root, _repo, effort) = new_effort("rendered-contract");
    let selected = [
        finalize_discovery(&root, &effort),
        finalize_discovery(&root, &effort),
    ];
    let reconciled = reconcile(&root, &effort, &selected);
    let store = Store::open(&root).unwrap();
    let effort_state = store.load_effort(&effort).unwrap();
    let (_, payload): (_, ReconciledDiscovery) = store
        .load_json(&effort_state, &reconciled, "reconciled-discovery.json")
        .unwrap();
    let (_, files) = store.load_bundle(&effort_state, &reconciled).unwrap();
    assert_eq!(
        String::from_utf8(files["reconciled-discovery.md"].clone()).unwrap(),
        orchestrate_reconcile::render_reconciled_discovery(&payload)
    );

    let mut invalid = serde_json::to_value(proposal(&selected[0])).unwrap();
    invalid["reconciliation_md"] = serde_json::json!("An unstructured binding obligation.");
    let path = root.join("invalid-reconcile-proposal.json");
    fs::write(&path, serde_json::to_vec(&invalid).unwrap()).unwrap();
    assert!(
        command_error(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &selected[0].artifact_id,
                "--discovery",
                &selected[1].artifact_id,
                "--bundle",
                path.to_str().unwrap(),
            ],
        )
        .contains("unknown field `reconciliation_md`")
    );
}

#[test]
fn adoption_implementation_and_audit_follow_the_reconciled_binding_contract() {
    let (root, repo, effort) = new_effort("audit-contract");
    let inputs = [
        finalize_discovery(&root, &effort),
        finalize_discovery(&root, &effort),
    ];
    let reconciled = reconcile(&root, &effort, &inputs);
    let adoption = reference(
        &command(
            &root,
            &[
                "reconcile",
                "adopt",
                "--effort",
                &effort,
                "--reconciled",
                &reconciled.artifact_id,
                "--authorization-label",
                "reviewer",
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
                &effort,
                "--adoption",
                &adoption.artifact_id,
            ],
        ),
        "implementation",
    );
    let assessment = AuditAssessment {
        reconciled: reconciled.clone(),
        adoption: adoption.clone(),
        implementation: implementation.clone(),
        coverage: vec![Coverage {
            requirement_id: "R-1".into(),
            state: CoverageState::Pass,
            rationale: "The binding behavior was verified.".into(),
            evidence: vec!["test: focused-check".into()],
            correction: String::new(),
        }],
        assessor_context:
            "Inspected the exact registered snapshot; TS-1 is advisory and has no coverage row."
                .into(),
    };
    let path = root.join("assessment.json");
    fs::write(&path, orchestrate_contracts::encode(&assessment).unwrap()).unwrap();
    let audit = command(
        &root,
        &[
            "audit",
            "finalize",
            "--effort",
            &effort,
            "--bundle",
            path.to_str().unwrap(),
        ],
    );
    assert_eq!(audit["details"]["verdict"], "PASS");
    assert!(repo.join("source.txt").exists());
}

#[test]
fn blocked_reconciled_discovery_cannot_be_adopted() {
    let (root, _repo, effort) = new_effort("blocked-reconcile");
    let inputs = [
        finalize_discovery(&root, &effort),
        finalize_discovery(&root, &effort),
    ];
    let mut blocked = proposal(&inputs[0]);
    blocked.blocking_issues =
        vec!["The selected artifacts cannot resolve a material choice.".into()];
    let path = write_proposal(&root, &blocked);
    let reconciled = reference(
        &command(
            &root,
            &[
                "reconcile",
                "finalize",
                "--effort",
                &effort,
                "--discovery",
                &inputs[0].artifact_id,
                "--discovery",
                &inputs[1].artifact_id,
                "--bundle",
                path.to_str().unwrap(),
            ],
        ),
        "reconciled",
    );
    assert!(
        command_error(
            &root,
            &[
                "reconcile",
                "adopt",
                "--effort",
                &effort,
                "--reconciled",
                &reconciled.artifact_id,
                "--authorization-label",
                "reviewer"
            ]
        )
        .contains("not implementation-ready")
    );
}
