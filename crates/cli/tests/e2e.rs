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
    let first = init("age-351", "request A", "constraint A");
    let repeated = init("age-351", "request A", "constraint A");
    assert_eq!(first["details"]["effort"], repeated["details"]["effort"]);
    assert!(
        command_error(
            &root,
            &[
                "init",
                "--project",
                repo.to_str().unwrap(),
                "--effort",
                "age-351",
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
                "age-351",
                "--request",
                "request A",
                "--constraint",
                "constraint B"
            ]
        )
        .contains("prepared request is immutable")
    );
    assert_eq!(
        init("age-351-revised", "request B", "constraint B")["semantic_outcome"],
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
        Some("The user chose patient-local time for patient-facing timestamps.".into());
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
        Some("The user chose patient-local time for patient-facing timestamps.")
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
