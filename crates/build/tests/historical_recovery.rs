//! Opt-in validation of the supported recovery sequence against an isolated
//! copy of real historical Build records.
//!
//! The environment this runs against is prepared outside the repository and is
//! never the original effort: `ORCHESTRATE_HISTORICAL_RECOVERY_ROOT` names a
//! directory holding
//!
//! ```text
//! <root>/store     an isolated copy of the store, whose project record points
//!                  at <root>/product
//! <root>/product   a disposable checkout of the corresponding project
//! ```
//!
//! The historical records are copied byte for byte and their project/store
//! paths are rewritten to that isolated directory before this test starts; the
//! original effort is never opened, resumed, edited or migrated.
//!
//! A deterministic adapter performs the continuation, so no historical product
//! work is dispatched to a provider.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use orchestrate_build::{
    BuildRequest, BuildResult, HostAdapter, Invocation, InvocationCompletion, InvocationObserver,
    InvocationResult, ResolutionKind, ResolutionOutcome, ResolutionRequest, resolve,
    run_with_adapter,
};
use orchestrate_core::{Effort, Store};

/// One dispatch, as the adapter observed it.
#[derive(Debug)]
struct Dispatch {
    role: String,
    kind: String,
    scope: String,
    action_dir: PathBuf,
}

/// Performs one role turn deterministically, writing the evidence files a
/// write-capable role would write.  It reads only what the action packet names.
struct DeterministicHost {
    dispatched: std::sync::Mutex<Vec<Dispatch>>,
}

impl DeterministicHost {
    fn new() -> Self {
        Self {
            dispatched: std::sync::Mutex::new(Vec::new()),
        }
    }

    fn dispatched(&self) -> Vec<Dispatch> {
        self.dispatched
            .lock()
            .unwrap()
            .iter()
            .map(describe)
            .collect()
    }
}

fn describe(dispatch: &Dispatch) -> Dispatch {
    Dispatch {
        role: dispatch.role.clone(),
        kind: dispatch.kind.clone(),
        scope: dispatch.scope.clone(),
        action_dir: dispatch.action_dir.clone(),
    }
}

impl HostAdapter for DeterministicHost {
    fn invoke(
        &self,
        invocation: &Invocation,
        observer: &mut dyn InvocationObserver,
    ) -> Result<InvocationResult, anyhow::Error> {
        observer.accepted()?;
        observer.session_id(&format!("deterministic-{}-session", invocation.role))?;
        let action: serde_json::Value = serde_json::from_slice(
            &fs::read(invocation.action.join("action.json"))
                .expect("the action packet is readable"),
        )
        .expect("the action packet is valid JSON");
        let kind = action["kind"].as_str().unwrap().to_owned();
        let scope = action["scope"].as_str().unwrap().to_owned();
        self.dispatched.lock().unwrap().push(Dispatch {
            role: invocation.role.clone(),
            kind: kind.clone(),
            scope: scope.clone(),
            action_dir: invocation.action.clone(),
        });
        let (kind, scope) = (kind.as_str(), scope.as_str());
        let receipt = match kind {
            "work" => {
                let note = invocation
                    .action
                    .join("action.json")
                    .to_string_lossy()
                    .into_owned();
                fs::write(
                    invocation.cwd.join(format!("{scope}-continuation.txt")),
                    format!("{scope} continuation\n{note}\n"),
                )?;
                git(&invocation.cwd, &["add", "."])?;
                git(&invocation.cwd, &["commit", "-m", &format!("{scope}")])?;
                fs::write(
                    invocation.action.join("report.md"),
                    format!("# {scope}\n\nThe deterministic adapter implemented this scope.\n"),
                )?;
                serde_json::json!({
                    "action_id": action["action_id"],
                    "scope": scope,
                    "outcome": "complete",
                    "commit": git(&invocation.cwd, &["rev-parse", "HEAD"])?,
                })
            }
            "review" => {
                fs::write(
                    invocation.action.join("report.md"),
                    format!("# {scope} review\n\nThe scope was inspected at its exact commit.\n"),
                )?;
                serde_json::json!({
                    "action_id": action["action_id"],
                    "scope": scope,
                    "outcome": "pass",
                    "commit": action["target_commit"],
                })
            }
            "final_audit" => {
                let requirements: serde_json::Value = serde_json::from_slice(&fs::read(
                    action["binding_requirements"].as_str().unwrap(),
                )?)?;
                let coverage = requirements["requirements"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|item| {
                        serde_json::json!({
                            "requirement_id": item["requirement"]["id"],
                            "state": "pass",
                            "rationale": "the deterministic continuation implemented and checked the requirement",
                            "evidence": [format!("the registered implementation at {}", action["target_commit"])],
                            "correction": "",
                        })
                    })
                    .collect::<Vec<_>>();
                let assessment = serde_json::json!({
                    "reconciled": action["reconciled"],
                    "adoption": action["adoption"],
                    "implementation": action["implementation"],
                    "coverage": coverage,
                    "assessor_context": "deterministic historical-recovery validation",
                });
                fs::write(
                    invocation.action.join("assessment.json"),
                    serde_json::to_vec_pretty(&assessment)?,
                )?;
                fs::write(
                    invocation.action.join("report.md"),
                    "# Final Audit\n\nEvery binding requirement was assessed.\n",
                )?;
                serde_json::json!({
                    "action_id": action["action_id"],
                    "scope": scope,
                    "outcome": "complete",
                })
            }
            other => panic!("the deterministic adapter cannot perform a {other} action"),
        };
        fs::write(
            invocation.action.join("result.json"),
            serde_json::to_vec_pretty(&receipt)?,
        )?;
        Ok(InvocationResult {
            completion: InvocationCompletion::Completed,
        })
    }
}

fn git(repo: &Path, args: &[&str]) -> Result<String, anyhow::Error> {
    let output = Command::new("git").args(args).current_dir(repo).output()?;
    anyhow::ensure!(
        output.status.success(),
        "git {args:?} failed in {}: {}",
        repo.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn read_json(path: &Path) -> serde_json::Value {
    serde_json::from_slice(&fs::read(path).unwrap_or_else(|error| {
        panic!("cannot read {}: {error}", path.display());
    }))
    .unwrap()
}

fn store_root() -> PathBuf {
    let root = std::env::var_os("ORCHESTRATE_HISTORICAL_RECOVERY_ROOT").unwrap_or_else(|| {
        panic!(
            "set ORCHESTRATE_HISTORICAL_RECOVERY_ROOT to an isolated directory holding store/ and product/ before running this test"
        )
    });
    PathBuf::from(root)
}

fn journal_events(store: &Store, effort: &Effort, event: &str) -> usize {
    store
        .read_journal(effort)
        .unwrap()
        .into_iter()
        .filter(|entry| entry.event == event)
        .count()
}

/// The full supported sequence on the historical boundary: migrate, record the
/// interrupted acceptance, resolve it after confirming nothing is running,
/// continue the interrupted scope, and repeat nothing.
#[test]
#[ignore = "operates on an isolated copy of real historical Build records; set ORCHESTRATE_HISTORICAL_RECOVERY_ROOT"]
fn historical_interrupted_build_migrates_resolves_and_continues() {
    let root = store_root();
    let product = fs::canonicalize(root.join("product")).expect("the isolated product checkout");
    let store = Store::open(&root.join("store")).expect("the isolated store opens");
    let project = store
        .project_by_locator(&product)
        .expect("the isolated project is registered")
        .expect("the isolated project record names this checkout");
    let effort = store
        .efforts_for_project(&project)
        .expect("the isolated project has an effort")
        .into_iter()
        .next()
        .expect("the isolated project has an effort");
    let build_dir = store.phase_dir(&effort, "build").unwrap();
    let state_path = build_dir.join("state.json");

    // The copied state is the historical one: schema 3, no migration marker and
    // no durable stop, with an action that was accepted and never completed.
    let historical = fs::read(&state_path).unwrap();
    let before = read_json(&state_path);
    assert_eq!(before["schema_version"], 3);
    assert!(before["migration_version"].is_null());
    assert!(before["stop"].is_null());
    assert_eq!(before["action"]["dispatch"], "running");
    let interrupted_action = before["action"]["id"].as_str().unwrap().to_owned();
    let interrupted_scope = before["action"]["scope"].as_str().unwrap().to_owned();
    let accepted_phases = before["phase_index"].as_u64().unwrap();
    let frozen_plan = before["frozen"]["plan_digest"].clone();
    let interrupted_dir = build_dir.join("artifacts").join(&interrupted_action);
    let partial_transport = fs::read(interrupted_dir.join("transport.jsonl")).ok();

    // One start migrates the records, records the uncertain boundary and
    // dispatches nothing.
    let host = DeterministicHost::new();
    let BuildResult::Blocked { trigger, .. } = run_with_adapter(
        &store,
        BuildRequest {
            effort: None,
            project: product.clone(),
        },
        &host,
    )
    .expect("the historical boundary is recorded rather than dispatched") else {
        panic!("the interrupted action did not stop the Build");
    };
    assert_eq!(trigger.as_deref(), Some("uncertain_acceptance"));
    assert!(
        host.dispatched().is_empty(),
        "the uncertain action was dispatched: {:?}",
        host.dispatched()
    );
    assert_eq!(
        fs::read(build_dir.join("evidence").join("state-v3-original.json")).unwrap(),
        historical,
        "the migration did not preserve the original state bytes"
    );
    assert_eq!(journal_events(&store, &effort, "build_state_migrated"), 1);
    let migrated = read_json(&state_path);
    assert_eq!(migrated["migration_version"], 1);
    assert_eq!(migrated["frozen"]["plan_digest"], frozen_plan);
    assert_eq!(migrated["phase_index"], accepted_phases);
    assert_eq!(migrated["action"]["id"], interrupted_action.as_str());
    assert_eq!(migrated["stop"]["trigger"], "uncertain_acceptance");
    assert_eq!(
        migrated["stop"]["process_completion"],
        "accepted; provider completion is uncertain"
    );
    assert_eq!(
        fs::read(interrupted_dir.join("transport.jsonl")).ok(),
        partial_transport,
        "the interrupted action's partial transport did not survive"
    );
    assert!(
        !interrupted_dir.join("transport.completed").exists(),
        "the uncertain action was marked complete"
    );

    // A resolution without the recorded confirmation is refused.
    let refusal = resolve(
        &store,
        ResolutionRequest {
            effort: effort.id.clone(),
            action: interrupted_action.clone(),
            kind: ResolutionKind::ExistingAuthorityClarification,
            note: "no provider process is running for this action".into(),
            evidence: Vec::new(),
            confirm_not_running: false,
            config: None,
        },
    )
    .unwrap_err()
    .to_string();
    assert!(refusal.contains("confirm the provider"), "{refusal}");

    let outcome = resolve(
        &store,
        ResolutionRequest {
            effort: effort.id.clone(),
            action: interrupted_action.clone(),
            kind: ResolutionKind::ExistingAuthorityClarification,
            note: "no provider process is running for this action".into(),
            evidence: Vec::new(),
            confirm_not_running: true,
            config: None,
        },
    )
    .expect("the confirmed resolution applies");
    let ResolutionOutcome::Resolved {
        resolution_id,
        continuation_action,
        continuation_kind,
        stopped_action,
        ..
    } = outcome
    else {
        panic!("the historical stop was refused rather than continued");
    };
    assert_eq!(stopped_action, interrupted_action);
    assert_ne!(continuation_action, interrupted_action);
    assert_eq!(continuation_kind, "work");

    // The continuation is a distinct action and the Build reaches its
    // acceptance boundary.
    let completed = run_with_adapter(
        &store,
        BuildRequest {
            effort: None,
            project: product.clone(),
        },
        &host,
    )
    .expect("the continuation runs");
    assert!(
        matches!(completed, BuildResult::Completed(_)),
        "the continuation did not reach acceptance: {completed:?}"
    );

    // The uncertain action was never resent, the phases already accepted were
    // never replayed, and the interrupted scope was continued.
    let dispatched = host.dispatched();
    assert!(
        !dispatched
            .iter()
            .any(|dispatch| dispatch.action_dir.ends_with(&interrupted_action)),
        "the uncertain action was resent"
    );
    let replayed = accepted_scopes(&build_dir, accepted_phases as usize, &dispatched);
    assert!(
        replayed.is_empty(),
        "already-accepted phases were replayed: {replayed:?}"
    );
    assert!(
        dispatched
            .iter()
            .any(|dispatch| dispatch.scope == interrupted_scope),
        "the interrupted scope {interrupted_scope} was never continued"
    );

    // Repeating recovery adds no second transition and no second record.
    let repeated = resolve(
        &store,
        ResolutionRequest {
            effort: effort.id.clone(),
            action: interrupted_action.clone(),
            kind: ResolutionKind::ExistingAuthorityClarification,
            note: "no provider process is running for this action".into(),
            evidence: Vec::new(),
            confirm_not_running: true,
            config: None,
        },
    )
    .expect("repeating the same resolution is idempotent");
    let ResolutionOutcome::Resolved {
        resolution_id: repeated_id,
        continuation_action: repeated_action,
        ..
    } = repeated
    else {
        panic!("repeating the resolution changed its outcome");
    };
    assert_eq!(repeated_id, resolution_id);
    assert_eq!(repeated_action, continuation_action);
    assert_eq!(journal_events(&store, &effort, "build_resolved"), 1);
    assert_eq!(journal_events(&store, &effort, "build_state_migrated"), 1);
    let resolutions = build_dir.join("resolutions");
    assert_eq!(
        fs::read_dir(&resolutions).unwrap().count(),
        1,
        "repeating recovery recorded a second resolution"
    );

    // A completed Build dispatches nothing more.
    let settled = fs::read(&state_path).unwrap();
    assert!(matches!(
        run_with_adapter(
            &store,
            BuildRequest {
                effort: None,
                project: product.clone()
            },
            &host
        )
        .unwrap(),
        BuildResult::Completed(_)
    ));
    assert_eq!(fs::read(&state_path).unwrap(), settled);
    assert_eq!(host.dispatched().len(), dispatched.len());
}

/// The delivery scopes that were already accepted before the interruption —
/// the plan's phases before the phase index the historical record carried —
/// that the adapter was nevertheless dispatched for.
fn accepted_scopes(build_dir: &Path, accepted: usize, dispatched: &[Dispatch]) -> Vec<String> {
    let plan = read_json(&build_dir.join("plan.json"));
    let mut replayed = plan["delivery_phases"]
        .as_array()
        .unwrap_or(&Vec::new())
        .iter()
        .take(accepted)
        .filter_map(|phase| phase["id"].as_str())
        .filter(|scope| dispatched.iter().any(|dispatch| dispatch.scope == *scope))
        .map(str::to_owned)
        .collect::<Vec<_>>();
    replayed.sort();
    replayed.dedup();
    replayed
}
