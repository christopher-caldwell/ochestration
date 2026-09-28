//! Shared deterministic Build fixtures. Never invokes installed providers.
#![allow(dead_code)]
use anyhow::{Context, Result, bail, ensure};
use orchestrate_build::adapter::InvocationOutcome;
use orchestrate_build::{BuildConfig, BuildPlan, BuildRequest, RoleConfig, adapter};
use orchestrate_contracts::{
    ArtifactKind, ArtifactRef, DiscoverySourceRef, Independence, Provenance, ReconciledDiscovery,
    ReconciledRequirement, RequestKind, Requirement, digest_bytes, encode,
};
use orchestrate_core::{Effort, Store};
use serde_json::json;
use std::{
    collections::{BTreeMap, VecDeque},
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};
const PLAN_FILE: &str = "plan.json";
const CONFIG_FILE: &str = "config.toml";
pub struct Fixture {
    pub root: PathBuf,
    pub repo: PathBuf,
    pub effort: Effort,
    pub store: Store,
    pub build_dir: PathBuf,
    pub reconciled: ArtifactRef,
}

static FIXTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

pub fn make_fixture(phases: Vec<String>) -> Fixture {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let root = std::env::temp_dir().join(format!(
        "build-controller-{nonce}-{}-{}",
        std::process::id(),
        FIXTURE_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let repo = root.join("product");
    fs::create_dir_all(&repo).unwrap();
    git_ok(&repo, &["init", "-q"]);
    git_ok(&repo, &["config", "user.name", "Build Test"]);
    git_ok(
        &repo,
        &["config", "user.email", "build-test@example.invalid"],
    );
    fs::write(repo.join(".gitignore"), "keep.ignored\n").unwrap();
    fs::write(repo.join("product.txt"), "baseline\n").unwrap();
    git_ok(&repo, &["add", "."]);
    git_ok(&repo, &["commit", "-m", "baseline"]);
    let baseline_commit = git_text(&repo, &["rev-parse", "HEAD"]).unwrap();
    let baseline_tree = git_text(&repo, &["rev-parse", "HEAD^{tree}"]).unwrap();
    let store = Store::open(root.join("store")).unwrap();
    let effort = store
        .init_effort(
            &repo,
            "test-effort",
            RequestKind::Freeform,
            "Build test".into(),
            Vec::new(),
        )
        .unwrap();
    let requirement_ids = ["R-1".to_owned(), "R-2".to_owned()];
    let reconciled = ReconciledDiscovery {
        reconciled_id: "test-reconciled".into(),
        context_id: effort.context.id.clone(),
        baseline_commit,
        baseline_tree,
        goal: "Test Build routing".into(),
        core_result: "A checkpointed gate runner".into(),
        problem: "Exercise transitions deterministically".into(),
        product_behavior_changed: vec!["Build implementation".into()],
        product_behavior_unchanged: Vec::new(),
        technical_behavior_changed: Vec::new(),
        technical_behavior_unchanged: Vec::new(),
        requirements: requirement_ids
            .into_iter()
            .map(|id| ReconciledRequirement {
                requirement: Requirement {
                    id,
                    text: "Required behavior".into(),
                    acceptance: "Verified".into(),
                    condition: None,
                    governing: false,
                },
                source_refs: vec![DiscoverySourceRef {
                    discovery_artifact_id: "discovery-fixture".into(),
                    node_id: None,
                }],
                user_clarification: None,
                frozen_user_constraint: false,
            })
            .collect(),
        discovery_attribution: Vec::new(),
        evidence_synthesis: Vec::new(),
        disagreements: Vec::new(),
        rejected_alternatives: Vec::new(),
        implementation_risks: Vec::new(),
        compatibility_concerns: Vec::new(),
        caveats: Vec::new(),
        technical_suggestions: Vec::new(),
        blocking_issues: Vec::new(),
    };
    let mut files = BTreeMap::new();
    files.insert(
        "reconciled-discovery.json".into(),
        encode(&reconciled).unwrap(),
    );
    let reconciled_ref = store
        .publish_bundle(
            &effort,
            "reconcile",
            ArtifactKind::ReconciledDiscovery,
            "test-reconcile".into(),
            "IMPLEMENTATION_READY".into(),
            Vec::new(),
            Provenance {
                host: "orchestrate-build".into(),
                provider: Some("reconcile-test".into()),
                model: None,
                model_effort: None,
                guide_digest: digest_bytes(orchestrate_guides::FINAL_AUDIT.as_bytes()),
                independence: Independence::Unknown,
            },
            files,
        )
        .unwrap();
    let build_dir = store.phase_dir(&effort, "build").unwrap();
    for phase in &phases {
        let dir = build_dir.join(phase);
        fs::create_dir_all(dir.join("nested")).unwrap();
        fs::write(
            dir.join("phase.md"),
            format!("# {phase}\nComplete this entire phase."),
        )
        .unwrap();
        fs::write(
            dir.join("z-context.md"),
            "Second context: arbitrary prose, no task schema.",
        )
        .unwrap();
        fs::write(
            dir.join("a-notes.md"),
            "First context: decide task ordering yourself.",
        )
        .unwrap();
        fs::write(dir.join("ignored.txt"), "Do not include").unwrap();
        fs::write(dir.join("nested/task.md"), "Do not recurse").unwrap();
    }
    let plan = BuildPlan {
        schema_version: orchestrate_build::state::PLAN_VERSION,
        reconciled: reconciled_ref.clone(),
        phases,
    };
    fs::write(build_dir.join(PLAN_FILE), encode(&plan).unwrap()).unwrap();
    write_config(&build_dir.join(CONFIG_FILE), "codex", "codex");
    Fixture {
        root,
        repo,
        effort,
        store,
        build_dir,
        reconciled: reconciled_ref,
    }
}

pub fn one_phase() -> Vec<String> {
    vec!["phase_01".into()]
}

pub fn write_config(path: &Path, worker: &str, reviewer: &str) {
    let config = BuildConfig {
        schema_version: orchestrate_build::state::CONFIG_VERSION,
        worker: RoleConfig {
            adapter: worker.into(),
            model: Some("native-model".into()),
            args: Some(vec!["--search".into()]),
        },
        reviewer: RoleConfig {
            adapter: reviewer.into(),
            model: None,
            args: None,
        },
        unblocker: None,
    };
    fs::write(path, toml::to_string(&config).unwrap()).unwrap();
}

pub fn git_ok(repo: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn id_from_prompt(plan: &adapter::InvocationPlan) -> String {
    let prompt = plan.record.argv.last().unwrap();
    prompt
        .split("action_id: ")
        .nth(1)
        .unwrap()
        .split(|ch: char| ch.is_whitespace() || ch == '.')
        .next()
        .unwrap()
        .to_owned()
}

pub fn packet_from_record(record: &adapter::InvocationRecord) -> serde_json::Value {
    let prompt = record.argv.last().unwrap();
    let packet = prompt.split_once("ACTION PACKET:\n").unwrap().1.trim();
    serde_json::from_str(packet).unwrap()
}

#[derive(Clone, Copy, Debug)]
pub enum Step {
    WorkComplete,
    WorkBlocked,
    WorkInvalidCommit,
    WorkWrongHead,
    WorkDirtyAfterCommit,
    ReviewPass,
    ReviewChanges,
    ReviewBlocked,
    AuditPass,
    AuditFail,
    AuditUnknown,
    AuditBlocked,
    UnblockRetry,
    UnblockBlocked,
    Malformed,
    StaleAction,
    ProviderFailure,
    InvocationError,
    MissingResponse,
}

#[derive(Default)]
pub struct FakeInvoker {
    steps: Mutex<VecDeque<Step>>,
    records: Mutex<Vec<adapter::InvocationRecord>>,
    repo: Mutex<Option<PathBuf>>,
    observed_sessions: Mutex<Option<VecDeque<Option<String>>>>,
    pub no_sessions: bool,
}

impl FakeInvoker {
    pub fn new(steps: impl IntoIterator<Item = Step>, repo: &Path) -> Self {
        Self {
            steps: Mutex::new(steps.into_iter().collect()),
            records: Mutex::new(Vec::new()),
            repo: Mutex::new(Some(repo.to_path_buf())),
            observed_sessions: Mutex::new(None),
            no_sessions: false,
        }
    }
    pub fn set_observed_sessions(&mut self, sessions: impl IntoIterator<Item = Option<String>>) {
        *self.observed_sessions.get_mut().unwrap() = Some(sessions.into_iter().collect());
    }
    pub fn records(&self) -> Vec<adapter::InvocationRecord> {
        self.records.lock().unwrap().clone()
    }
    pub fn remaining(&self) -> usize {
        self.steps.lock().unwrap().len()
    }
}

impl adapter::InvocationApi for FakeInvoker {
    fn invoke(
        &self,
        plan: &adapter::InvocationPlan,
        cwd: &Path,
        _action_dir: &Path,
    ) -> Result<InvocationOutcome> {
        self.records.lock().unwrap().push(plan.record.clone());
        let step = self
            .steps
            .lock()
            .unwrap()
            .pop_front()
            .context("fake adapter has no queued response")?;
        if matches!(step, Step::InvocationError) {
            bail!("fake provider could not be spawned");
        }
        let packet = packet_from_record(&plan.record);
        assert_eq!(
            packet["binding_reconciled"]["requirements"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        let prompt = plan.record.argv.last().unwrap();
        let action_id = id_from_prompt(plan);
        let gate = if prompt.contains("Gate: Work;") {
            "Work"
        } else if prompt.contains("Gate: Review;") {
            "Review"
        } else if prompt.contains("Gate: Audit;") {
            "Audit"
        } else {
            "Unblock"
        };
        if matches!(gate, "Review" | "Audit" | "Unblock") {
            let packet: serde_json::Value =
                serde_json::from_slice(&fs::read(cwd.parent().unwrap().join("action.json"))?)?;
            assert_eq!(
                git_text(cwd, &["rev-parse", "HEAD"])?,
                packet["checkpoint_commit"].as_str().unwrap()
            );
            assert_eq!(git_text(cwd, &["status", "--porcelain"])?, "");
            assert_ne!(cwd, self.repo.lock().unwrap().as_ref().unwrap());
        }
        let response = match step {
            Step::WorkComplete => {
                let count = self.records.lock().unwrap().iter().filter(|record| record.argv.last().is_some_and(|prompt| prompt.contains("Gate: Work;"))).count();
                let filename = format!("work-step-{count}.txt");
                fs::write(cwd.join(filename), format!("step {count}: {action_id}\n"))?;
                git_ok(cwd, &["add", "-A"]);
                git_ok(cwd, &["commit", "-m", &format!("work step {count}")]);
                json!({"action_id": action_id, "outcome": "complete", "report": "Scoped work is committed and verified.", "commit": git_text(cwd, &["rev-parse", "HEAD"])?}).to_string()
            }
            Step::WorkBlocked => {
                fs::write(cwd.join("product.txt"), "partial committed work\n")?;
                git_ok(cwd, &["add", "product.txt"]);
                git_ok(cwd, &["commit", "-m", "partial phase"]);
                fs::write(cwd.join("product.txt"), "partial uncommitted work\n")?;
                fs::write(cwd.join("partial-untracked.txt"), "discard this partial work\n")?;
                json!({"action_id": action_id, "outcome": "blocked", "report": "Work needs a missing external requirement."}).to_string()
            }
            Step::WorkInvalidCommit => json!({"action_id": action_id, "outcome": "complete", "report": "Claimed commit is invalid.", "commit": "not-a-commit"}).to_string(),
            Step::WorkWrongHead | Step::WorkDirtyAfterCommit => {
                let count = self.records.lock().unwrap().iter().filter(|record| record.argv.last().is_some_and(|prompt| prompt.contains("Gate: Work;"))).count();
                fs::write(cwd.join(format!("wrong-head-{count}.txt")), "committed\n")?;
                git_ok(cwd, &["add", "-A"]);
                git_ok(cwd, &["commit", "-m", "work commit"]);
                let reported_commit = if matches!(step, Step::WorkWrongHead) { git_text(cwd, &["rev-parse", "HEAD^"])? } else { git_text(cwd, &["rev-parse", "HEAD"])? };
                if matches!(step, Step::WorkDirtyAfterCommit) { fs::write(cwd.join("left-untracked.txt"), "must be committed\n")?; }
                json!({"action_id": action_id, "outcome": "complete", "report": "Work commit claimed.", "commit": reported_commit}).to_string()
            }
            Step::ReviewPass => json!({"action_id": action_id, "outcome": "pass", "report": "Review passed the exact checkpoint.", "inspected_commit": git_text(cwd, &["rev-parse", "HEAD"])?}).to_string(),
            Step::ReviewChanges => json!({"action_id": action_id, "outcome": "changes_required", "report": "Correct the reported issue, then repeat review.", "inspected_commit": git_text(cwd, &["rev-parse", "HEAD"])?}).to_string(),
            Step::ReviewBlocked => json!({"action_id": action_id, "outcome": "blocked", "report": "Review lacks an external dependency.", "inspected_commit": git_text(cwd, &["rev-parse", "HEAD"])?}).to_string(),
            Step::AuditPass | Step::AuditFail | Step::AuditUnknown => {
                let packet: serde_json::Value = serde_json::from_slice(&fs::read(cwd.parent().unwrap().join("action.json"))?)?;
                let refs = [&packet["reconciled"], &packet["adoption"], &packet["implementation"]];
                let ids = packet["binding_reconciled"]["requirements"].as_array().unwrap();
                let mut coverage = ids.iter().enumerate().map(|(index, item)| {
                    let state = match step { Step::AuditFail if index == 0 => "fail", Step::AuditUnknown => "unknown", _ => "pass" };
                    json!({"requirement_id": item["requirement"]["id"], "state": state, "rationale": "Assessment from exact checkpoint", "evidence": ["inspection: exact checkpoint"], "correction": if state == "fail" { "Fix the failed requirement." } else { "" }})
                }).collect::<Vec<_>>();
                if matches!(step, Step::AuditUnknown) { coverage.clear(); }
                let assessment = json!({"reconciled": refs[0], "adoption": refs[1], "implementation": refs[2], "coverage": coverage, "assessor_context": "Inspected exact checkpoint."});
                json!({"action_id": action_id, "outcome": "complete", "report": "Audit assessment is complete.", "assessment": assessment}).to_string()
            }
            Step::AuditBlocked => json!({"action_id": action_id, "outcome": "blocked", "report": "Audit cannot assess without an external requirement."}).to_string(),
            Step::UnblockRetry => json!({"action_id": action_id, "outcome": "retry", "report": "Retry the same gate after the checkpoint reset."}).to_string(),
            Step::UnblockBlocked => json!({"action_id": action_id, "outcome": "blocked", "report": "An operator must supply the missing prerequisite."}).to_string(),
            Step::Malformed => "this is not JSON".into(),
            Step::StaleAction => json!({"action_id": "stale-action", "outcome": "blocked", "report": "Stale response."}).to_string(),
            Step::ProviderFailure | Step::InvocationError | Step::MissingResponse => String::new(),
        };
        let failure = matches!(step, Step::ProviderFailure);
        let missing = matches!(step, Step::MissingResponse);
        let response = if matches!(step, Step::StaleAction) {
            // Keep the stale action result structurally valid for the current gate.
            if gate == "Work" {
                json!({"action_id":"stale-action","outcome":"blocked","report":"Stale response."})
                    .to_string()
            } else {
                response
            }
        } else {
            response
        };
        let observed_session = if self.no_sessions {
            None
        } else if let Some(sessions) = self.observed_sessions.lock().unwrap().as_mut() {
            sessions.pop_front().flatten()
        } else {
            Some(format!("session-{gate}"))
        };
        Ok(InvocationOutcome {
            success: !failure,
            exit_code: Some(if failure { 7 } else { 0 }),
            final_response: if failure || missing {
                None
            } else {
                Some(response.clone())
            },
            observed_session,
            stdout: response,
            stderr: if failure {
                "provider failed".into()
            } else {
                String::new()
            },
        })
    }
}

pub fn git_text(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .with_context(|| format!("cannot run git {}", args.join(" ")))?;
    ensure!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

pub fn request(fixture: &Fixture) -> BuildRequest {
    BuildRequest {
        effort: Some(fixture.effort.id.clone()),
        project: fixture.repo.clone(),
    }
}
