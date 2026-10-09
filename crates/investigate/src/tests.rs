use super::*;
use orchestrate_contracts::{ImplementationStatus, Independence, Provenance, RequestKind};
use orchestrate_core::provider::{InvocationOutcome, InvocationPlan};
use serde_json::{Value, json};
use std::{
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

struct Fixture {
    base: PathBuf,
    root: PathBuf,
    config: PathBuf,
    repo: PathBuf,
    commit: String,
}
impl Fixture {
    fn new(mode: &str, question: &str, go_threshold: usize, min_completed: usize) -> Self {
        let base = std::env::temp_dir().join(format!(
            "orchestrate-investigate-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&base).unwrap();
        let root = base.join("store");
        let repo = base.join("repo");
        fs::create_dir(&repo).unwrap();
        storage::git(&repo, &["init", "-q"]).unwrap();
        storage::git(&repo, &["config", "user.email", "fixture@example.test"]).unwrap();
        storage::git(&repo, &["config", "user.name", "Fixture"]).unwrap();
        fs::write(repo.join("subject.txt"), "known pinned source\n").unwrap();
        storage::git(&repo, &["add", "subject.txt"]).unwrap();
        storage::git(&repo, &["commit", "-qm", "baseline"]).unwrap();
        let commit = storage::git(&repo, &["rev-parse", "HEAD"]).unwrap();
        fs::write(
            base.join("request.md"),
            "Assess the exact bounded fixture.\n",
        )
        .unwrap();
        let config = base.join("config.toml");
        let text = format!(
            "schema_version=1\nmode=\"{mode}\"\nquestion_kind=\"{question}\"\nrequest=\"request.md\"\ntarget=\"repo\"\nrevision=\"{commit}\"\nmax_parallel=3\n[[lanes]]\nadapter=\"codex\"\ncount=3\nmodel=\"fixture-codex\"\n[[lanes]]\nadapter=\"claude\"\ncount=2\nmodel=\"fixture-claude\"\n[reconciler]\nadapter=\"codex\"\n[completion]\nmin_completed={min_completed}\n{}",
            if mode == "consensus" {
                format!("[consensus]\nmin_go_votes={go_threshold}\n")
            } else {
                String::new()
            }
        );
        fs::write(&config, text).unwrap();
        Self {
            base,
            root,
            config,
            repo,
            commit,
        }
    }
    fn run(&self, fake: &Fake) -> InvestigationResult {
        run_with_invoker(&self.root, &self.config, fake).unwrap()
    }
    fn run_dir(&self, r: &InvestigationResult) -> PathBuf {
        self.root.join("investigations").join(&r.run_id)
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}
struct Fake {
    go_count: usize,
    material_defect: bool,
    unresolved: bool,
    unavailable: Option<usize>,
    malformed: Option<usize>,
    fabricated_execution: bool,
    drift: bool,
    omit_reconciliation_origin: bool,
    reconciler_tool: bool,
    missing_coverage: bool,
    wrong_refs: bool,
    reused_session: bool,
    synchronize: bool,
    arrived: AtomicUsize,
    active: AtomicUsize,
    maximum: AtomicUsize,
    calls: Mutex<Vec<(PathBuf, Value)>>,
}
impl Default for Fake {
    fn default() -> Self {
        Self {
            go_count: 5,
            material_defect: false,
            unresolved: false,
            unavailable: None,
            malformed: None,
            fabricated_execution: false,
            drift: false,
            omit_reconciliation_origin: false,
            reconciler_tool: false,
            missing_coverage: false,
            wrong_refs: false,
            reused_session: false,
            synchronize: false,
            arrived: AtomicUsize::new(0),
            active: AtomicUsize::new(0),
            maximum: AtomicUsize::new(0),
            calls: Mutex::new(vec![]),
        }
    }
}
impl InvocationApi for Fake {
    fn invoke(&self, plan: &InvocationPlan, cwd: &Path, _: &Path) -> Result<InvocationOutcome> {
        assert!(
            plan.record.session_in.is_none(),
            "every investigation role starts fresh"
        );
        if cwd.file_name().unwrap() == "reconciler" {
            assert_eq!(
                self.active.load(Ordering::SeqCst),
                0,
                "reconciliation waits for every lane"
            );
            let evidence: Value =
                serde_json::from_slice(&fs::read(cwd.join("evidence.json"))?).unwrap();
            let graphs: Vec<LaneGraph> = evidence["lanes"]
                .as_array()
                .unwrap()
                .iter()
                .map(|l| serde_json::from_value(l["graph"].clone()).unwrap())
                .collect();
            let mut groups: BTreeMap<bool, Vec<&LaneGraph>> = BTreeMap::new();
            for graph in &graphs {
                groups
                    .entry(graph.findings[0].negative)
                    .or_default()
                    .push(graph);
            }
            let mut findings = Vec::new();
            let mut challenges = Vec::new();
            let mut essentials = Vec::new();
            for (negative, group) in groups {
                let id = if negative { "defect" } else { "supported" };
                if !negative {
                    essentials.push(id.to_string());
                }
                findings.push(json!({"id":id,"proposition":group[0].findings[0].claim,"scope":"fixture","origins":group.iter().map(|g|format!("{}/F1",g.lane_id)).collect::<Vec<_>>(),"supporting_observations":group.iter().map(|g|format!("{}/O1",g.lane_id)).collect::<Vec<_>>(),"contradicting_observations":[],"disposition":if negative && !self.material_defect {"contested"}else{"supported"},"rationale":"bounded fixture evidence","material":group.iter().any(|g|g.findings[0].material),"negative":negative,"demonstrated":negative && self.material_defect,"limitations":[]}));
            }
            for graph in &graphs {
                for challenge in &graph.challenges {
                    challenges.push(json!({"challenge":format!("{}/{}",graph.lane_id,challenge.id),"disposition":"contested","observations":[format!("{}/O1",graph.lane_id)],"rationale":"material prerequisite remains unresolved"}));
                }
            }
            if self.omit_reconciliation_origin {
                findings[0]["origins"].as_array_mut().unwrap().pop();
            }
            let value = json!({"schema_version":1,"input_digest":evidence["input_digest"],"findings":findings,"challenges":challenges,"essential_findings":essentials,"answer":"The bounded fixture answer.","limitations":[]});
            let transport = if self.reconciler_tool {
                r#"{"type":"item.started","item":{"type":"web_search"}}"#.into()
            } else {
                String::new()
            };
            return Ok(outcome(value, transport));
        }
        let input: Value = serde_json::from_slice(&fs::read(cwd.join("lane-input.json"))?)?;
        assert!(
            input.get("mode").is_none()
                && input.get("consensus").is_none()
                && input.get("lanes").is_none()
        );
        self.calls
            .lock()
            .unwrap()
            .push((cwd.to_owned(), input.clone()));
        let number = input["lane_id"]
            .as_str()
            .unwrap()
            .trim_start_matches("lane-")
            .parse::<usize>()
            .unwrap();
        let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
        self.maximum.fetch_max(active, Ordering::SeqCst);
        if self.synchronize && number <= 3 {
            self.arrived.fetch_add(1, Ordering::SeqCst);
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while self.arrived.load(Ordering::SeqCst) < 3 {
                assert!(
                    std::time::Instant::now() < deadline,
                    "first cohort batch was not concurrent"
                );
                thread::sleep(Duration::from_millis(1));
            }
        }

        self.active.fetch_sub(1, Ordering::SeqCst);
        if self.unavailable == Some(number) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "fixture provider unavailable",
            )
            .into());
        }
        if self.malformed == Some(number) {
            return Ok(InvocationOutcome {
                success: true,
                exit_code: Some(0),
                final_response: Some("{malformed".into()),
                observed_session: None,
                stdout: String::new(),
                stderr: String::new(),
            });
        }
        fs::write(cwd.join("scratch/generated.txt"), "scratch is permitted")?;
        if self.drift && number == 1 {
            fs::write(cwd.join("source/subject.txt"), "modified source")?;
        }
        let negative = number > self.go_count;
        let binary = input["question_kind"] == "binary";
        let position = if binary {
            Some(if negative { "NO_GO" } else { "GO" })
        } else {
            None
        };
        let mut observation = json!({"id":"O1","kind":"inspection","source":{"source_id":"repository","path":"subject.txt","line":1},"observed":"known pinned source","environment":"frozen source static inspection","depends_on":[]});
        let mut transport = String::new();
        if (negative && self.material_defect) || self.fabricated_execution {
            observation = json!({"id":"O1","kind":"execution","command":"false","observed":"bounded fixture failed","environment":"scratch probe, exact pinned source","depends_on":[]});
            if !self.fabricated_execution {
                transport=r#"{"type":"item.completed","item":{"type":"command_execution","command":"false","status":"failed","exit_code":1,"aggregated_output":"reproduced fixture failure"}}"#.into();
            }
        }
        let challenges = if self.unresolved && number == 5 {
            vec![
                json!({"id":"X1","targets":["F1","C1"],"reason":"unresolved material prerequisite","material":true,"limitations":"no decisive observation","depends_on":["O1"]}),
            ]
        } else {
            vec![]
        };
        let mut value = json!({"schema_version":1,"lane_id":input["lane_id"],"input_digest":input["input_digest"],"observations":[observation],"findings":[{"id":"F1","claim":if negative{"fixture defect"}else{"fixture positive claim"},"scope":"fixture","applicability":"bounded request","impact":"fixture implication","material":negative && self.material_defect,"negative":negative,"uncertainty":"bounded evidence","depends_on":["O1"]}],"challenges":challenges,"conclusion":{"id":"C1","answer":"bounded fixture answer","position":position,"depends_on":if self.unresolved && number==5{vec!["F1","X1"]}else{vec!["F1"]},"limitations":[]}});
        if !input["audit"].is_null() {
            let contract: ReconciledDiscovery =
                decode(&fs::read(cwd.join("authority/reconciled-discovery.json"))?)?;
            let mut coverage:Vec<_>=contract.requirements.iter().map(|r|json!({"requirement_id":r.requirement.id,"state":"pass","rationale":"bounded fixture static proof","evidence":["F1"],"correction":""})).collect();
            if self.missing_coverage {
                coverage.remove(0);
            }
            value["assessment"] = json!({"reconciled":input["audit"]["reconciled"],"adoption":input["audit"]["adoption"],"implementation":input["audit"]["implementation"],"coverage":coverage,"assessor_context":"complete fixture scope"});
            if self.wrong_refs {
                value["assessment"]["implementation"]["digest"] = json!("wrong-digest");
            }
        }
        let mut result = outcome(value, transport);
        result.observed_session = Some(if self.reused_session {
            "reused-fixture-session".into()
        } else {
            format!("fresh-{}", input["lane_id"].as_str().unwrap())
        });
        Ok(result)
    }
}
fn outcome(value: Value, transport: String) -> InvocationOutcome {
    InvocationOutcome {
        success: true,
        exit_code: Some(0),
        final_response: Some(serde_json::to_string(&value).unwrap()),
        observed_session: Some("fresh-observed-session".into()),
        stdout: transport,
        stderr: String::new(),
    }
}

#[test]
fn concurrent_fresh_lanes_share_frozen_inputs_and_preserve_models() {
    let f = Fixture::new("consensus", "binary", 4, 5);
    let fake = Fake {
        synchronize: true,
        ..Default::default()
    };
    let r = f.run(&fake);
    assert_eq!(r.completion, Completion::Complete);
    assert_eq!(r.recommendation, Some(Recommendation::Go));
    assert!(fake.maximum.load(Ordering::SeqCst) > 1);
    assert!(fake.maximum.load(Ordering::SeqCst) <= 3);
    let calls = fake.calls.lock().unwrap();
    assert_eq!(calls.len(), 5);
    for (_, input) in &*calls {
        assert_eq!(input["input_digest"], r.input_digest);
        assert_eq!(input["repository"]["commit"], f.commit);
    }
    for (dir, _) in &*calls {
        let invocation: Value = decode(&fs::read(dir.join("invocation.json")).unwrap()).unwrap();
        assert!(matches!(
            invocation["model"].as_str(),
            Some("fixture-codex" | "fixture-claude")
        ));
    }
    assert_eq!(
        storage::git(&f.repo, &["status", "--porcelain"]).unwrap(),
        ""
    );
    assert_eq!(
        storage::git(&f.repo, &["rev-parse", "HEAD"]).unwrap(),
        f.commit
    );
    assert_eq!(inspect(&f.root, &r.run_id).unwrap().votes.unwrap().go, 5);
}
#[test]
fn consensus_counts_use_the_configured_cohort() {
    for (go, met) in [(4, true), (3, false)] {
        let f = Fixture::new("consensus", "binary", 4, 5);
        let r = f.run(&Fake {
            go_count: go,
            ..Default::default()
        });
        let v = r.votes.as_ref().unwrap();
        assert_eq!((v.go, v.no_go, v.threshold_met), (go, 5 - go, met));
        assert_eq!(r.requested, 5);
        if !met {
            assert_eq!(r.recommendation, Some(Recommendation::ReviewRequired));
        }
    }
}
#[test]
fn demonstrated_minority_defect_overrides_recommendation_not_votes() {
    let f = Fixture::new("consensus", "binary", 4, 5);
    let r = f.run(&Fake {
        go_count: 4,
        material_defect: true,
        ..Default::default()
    });
    assert!(r.votes.as_ref().unwrap().threshold_met);
    assert_eq!(r.recommendation, Some(Recommendation::NoGo));
    assert_eq!(r.completed, 5);
}
#[test]
fn unresolved_material_challenge_prevents_go() {
    let f = Fixture::new("consensus", "binary", 4, 5);
    let r = f.run(&Fake {
        unresolved: true,
        ..Default::default()
    });
    assert!(r.votes.as_ref().unwrap().threshold_met);
    assert_eq!(r.recommendation, Some(Recommendation::ReviewRequired));
}
#[test]
fn incomplete_and_unavailable_lanes_are_retained_and_never_vote() {
    let f = Fixture::new("consensus", "binary", 4, 5);
    let r = f.run(&Fake {
        unavailable: Some(5),
        malformed: Some(4),
        ..Default::default()
    });
    assert_eq!(r.completion, Completion::Incomplete);
    assert_eq!((r.completed, r.invalid, r.unavailable), (3, 1, 1));
    assert_eq!(r.votes.as_ref().unwrap().go, 3);
    assert_eq!(r.recommendation, Some(Recommendation::Inconclusive));
    assert!(f.run_dir(&r).join("lanes/lane-0004/response.txt").exists());
    assert!(r.reconciliation.is_some());
    let r2 = f.run(&Fake::default());
    assert_ne!(r.run_id, r2.run_id);
}
#[test]
fn minimum_completion_can_allow_missing_lanes_without_changing_denominator() {
    let f = Fixture::new("consensus", "binary", 4, 4);
    let r = f.run(&Fake {
        unavailable: Some(5),
        ..Default::default()
    });
    assert_eq!(r.completion, Completion::Complete);
    assert_eq!(r.requested, 5);
    assert_eq!(r.votes.as_ref().unwrap().go, 4);
}
#[test]
fn fabricated_execution_cannot_establish_go() {
    let f = Fixture::new("consensus", "binary", 4, 5);
    let r = f.run(&Fake {
        fabricated_execution: true,
        ..Default::default()
    });
    assert_eq!(r.invalid, 5);
    assert_eq!(r.votes.as_ref().unwrap().go, 0);
    assert_eq!(r.completion, Completion::Incomplete);
}
#[test]
fn source_drift_is_rejected_and_scratch_is_allowed() {
    let f = Fixture::new("consensus", "binary", 4, 5);
    let r = f.run(&Fake {
        drift: true,
        ..Default::default()
    });
    assert_eq!(r.invalid, 1);
    assert_eq!(r.completed, 4);
    assert!(
        f.run_dir(&r)
            .join("lanes/lane-0002/scratch/generated.txt")
            .exists()
    );
}
#[test]
fn wide_retains_singleton_and_duplicates_with_provenance_and_no_votes() {
    let f = Fixture::new("wide", "open_ended", 0, 5);
    let r = f.run(&Fake {
        go_count: 4,
        material_defect: true,
        ..Default::default()
    });
    assert_eq!(r.completion, Completion::Complete);
    assert!(r.votes.is_none() && r.recommendation.is_none());
    let findings = &r.reconciliation.unwrap().findings;
    assert_eq!(findings.len(), 2);
    assert!(findings.iter().any(|f| f.negative && f.origins.len() == 1));
    assert!(findings.iter().any(|f| !f.negative && f.origins.len() == 4));
    let result: Value =
        decode(&fs::read(f.run_dir(&f.run(&Fake::default())).join("result.json")).unwrap())
            .unwrap();
    assert!(result.get("votes").is_none());
}
#[test]
fn reconciliation_cannot_omit_findings_or_use_tools() {
    for fake in [
        Fake {
            omit_reconciliation_origin: true,
            ..Default::default()
        },
        Fake {
            reconciler_tool: true,
            ..Default::default()
        },
    ] {
        let f = Fixture::new("wide", "open_ended", 0, 5);
        let r = f.run(&fake);
        assert_eq!(r.completed, 5);
        assert_eq!(r.completion, Completion::Incomplete);
        assert!(r.reconciliation.is_none());
        assert!(f.run_dir(&r).join("reconciler/response.txt").exists());
    }
}
#[test]
fn inspection_detects_tampering_and_is_offline() {
    let f = Fixture::new("consensus", "binary", 4, 5);
    let r = f.run(&Fake::default());
    let file = f.run_dir(&r).join("lanes/lane-0001/graph.json");
    let original = fs::read(&file).unwrap();
    fs::write(&file, "{}").unwrap();
    assert!(inspect(&f.root, &r.run_id).is_err());
    fs::write(&file, original).unwrap();
    fs::write(f.run_dir(&r).join("report.md"), "forged report").unwrap();
    assert!(inspect(&f.root, &r.run_id).is_err());
    let absent = f.base.join("nonexistent");
    assert!(inspect(&absent, "missing").is_err());
    assert!(!absent.exists());
}
#[test]
fn stopped_run_is_incomplete_without_reaggregation_or_resume() {
    let f = Fixture::new("consensus", "binary", 4, 5);
    let r = f.run(&Fake::default());
    fs::remove_file(f.run_dir(&r).join("manifest.json")).unwrap();
    let inspected = inspect(&f.root, &r.run_id).unwrap();
    assert_eq!(inspected.completion, Completion::Incomplete);
    assert_eq!(inspected.completed, 5);
    assert!(inspected.reconciliation.is_none());
}
#[test]
fn configuration_rejects_wrong_modes_thresholds_and_native_options_before_launch() {
    let f = Fixture::new("consensus", "binary", 4, 5);
    let (valid, _) = config::load(&f.config).unwrap();
    for threshold in [3, 4, 5] {
        let mut c = valid.clone();
        c.consensus.as_mut().unwrap().min_go_votes = threshold;
        assert!(c.validate().is_ok());
    }
    let mut bad = valid.clone();
    bad.consensus.as_mut().unwrap().min_go_votes = 2;
    assert!(bad.validate().is_err());
    let mut bad = valid.clone();
    bad.mode = Mode::Wide;
    assert!(bad.validate().is_err());
    let mut bad = valid.clone();
    bad.question_kind = Some(QuestionKind::OpenEnded);
    assert!(bad.validate().is_err());
    let mut bad = valid.clone();
    bad.revision = Some("HEAD".into());
    assert!(bad.validate().is_err());
    let mut bad = valid.clone();
    bad.lanes[0].count = 0;
    assert!(bad.validate().is_err());
    let mut bad = valid.clone();
    bad.max_parallel = Some(0);
    assert!(bad.validate().is_err());
    let mut bad = valid.clone();
    bad.completion.min_completed = Some(6);
    assert!(bad.validate().is_err());
    let mut bad = valid.clone();
    bad.lanes[0].args = Some(vec!["resume".into()]);
    assert!(bad.validate().is_err());
    let mut bad = valid.clone();
    bad.lanes[0].args = Some(vec!["--model=other".into()]);
    assert!(bad.validate().is_err());
    let mut bad = valid.clone();
    bad.lanes[1].effort = Some("HIGH".into());
    assert!(bad.validate().is_err());
    fs::write(&f.config, "schema_version=1\nunknown=true").unwrap();
    let fake = Fake::default();
    assert!(run_with_invoker(&f.root, &f.config, &fake).is_err());
    assert!(fake.calls.lock().unwrap().is_empty());
    assert!(!f.root.exists());
}
#[test]
fn roots_inside_the_target_are_rejected_before_writing() {
    let f = Fixture::new("consensus", "binary", 4, 5);
    let root = f.repo.join("orchestration");
    assert!(run_with_invoker(&root, &f.config, &Fake::default()).is_err());
    assert!(!root.exists());
}

fn base_graph() -> LaneGraph {
    serde_json::from_value(json!({"schema_version":1,"lane_id":"lane","input_digest":"digest","observations":[{"id":"O1","kind":"testimony","observed":"reported assertion","environment":"reported testimony","depends_on":[]}],"findings":[{"id":"F1","claim":"scoped assertion","scope":"scope","applicability":"applies","impact":"impact","material":true,"negative":false,"uncertainty":"uncertain","depends_on":["O1"]}],"challenges":[],"conclusion":{"id":"C1","answer":"unknown","position":"UNKNOWN","depends_on":["F1"],"limitations":[]}})).unwrap()
}
#[test]
fn graph_validation_rejects_duplicate_missing_cyclic_and_forged_references() {
    let valid = base_graph();
    let check = |mut graph: LaneGraph| {
        evidence::validate_graph(
            &mut graph,
            "lane",
            "digest",
            &QuestionKind::Binary,
            &BTreeMap::new(),
            &[],
            None,
        )
    };
    assert!(check(valid.clone()).is_ok());
    let mut bad = valid.clone();
    bad.findings[0].id = "O1".into();
    assert!(check(bad).is_err());
    let mut bad = valid.clone();
    bad.findings[0].depends_on = vec!["missing".into()];
    assert!(check(bad).is_err());
    let mut bad = valid.clone();
    bad.observations[0].depends_on = vec!["C1".into()];
    assert!(check(bad).is_err());
    let mut bad = valid.clone();
    bad.conclusion.position = Some(Position::Go);
    assert!(check(bad).is_err());
    let mut bad = valid.clone();
    bad.input_digest = "wrong".into();
    assert!(check(bad).is_err());
    let mut bad = valid.clone();
    bad.observations[0].kind = ObservationKind::Execution;
    bad.observations[0].command = Some("false".into());
    bad.observations[0].receipt = Some("forged".into());
    assert!(check(bad).is_err());
    let mut bad = valid.clone();
    bad.observations[0].source = Some(SourceLocation {
        source_id: "missing".into(),
        path: "missing".into(),
        line: None,
    });
    assert!(check(bad).is_err());
}
#[test]
fn native_receipts_preserve_actual_exit_facts_and_missing_runtime_evidence() {
    let raw = "{\"type\":\"item.completed\",\"item\":{\"type\":\"command_execution\",\"command\":\"false\",\"exit_code\":1,\"status\":\"failed\",\"aggregated_output\":\"failed\"}}\n{\"type\":\"item.completed\",\"item\":{\"type\":\"command_execution\",\"command\":\"unobserved\",\"status\":\"completed\"}}";
    let receipts = evidence::receipts(raw);
    assert_eq!(receipts.len(), 2);
    assert_eq!(receipts[0].exit_code, Some(1));
    assert_eq!(receipts[1].exit_code, None);
    let mut graph = base_graph();
    graph.observations[0].kind = ObservationKind::Execution;
    graph.observations[0].command = Some("unobserved".into());
    let limits = evidence::validate_graph(
        &mut graph,
        "lane",
        "digest",
        &QuestionKind::Binary,
        &BTreeMap::new(),
        &receipts,
        None,
    )
    .unwrap();
    assert_eq!(graph.observations[0].kind, ObservationKind::Testimony);
    assert!(!limits.is_empty());
    let claude = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","id":"t1","name":"Bash","input":{"command":"false"}}]}}
{"type":"user","message":{"content":[{"type":"tool_result","tool_use_id":"t1","content":"failure"}]},"tool_use_result":{"stdout":"","stderr":"failure","exit_code":1}}"#;
    assert_eq!(evidence::receipts(claude)[0].exit_code, Some(1));
}

fn provenance() -> Provenance {
    Provenance {
        host: "fixture".into(),
        provider: None,
        model: None,
        model_effort: None,
        guide_digest: "fixture-guide".into(),
        independence: Independence::InputExcluded,
    }
}
fn add_conformance(f: &Fixture) -> AuditInputs {
    let store = Store::open(&f.root).unwrap();
    let effort = store
        .init_effort(
            &f.repo,
            "fixture",
            RequestKind::Freeform,
            "fixture request".into(),
            vec![],
        )
        .unwrap();
    let contract:ReconciledDiscovery=serde_json::from_value(json!({"reconciled_id":"fixture","context_id":effort.context.id,"baseline_commit":effort.baseline_commit,"baseline_tree":effort.baseline_tree,"goal":"fixture","core_result":"fixture","problem":"fixture","product_behavior_changed":[],"product_behavior_unchanged":[],"technical_behavior_changed":[],"technical_behavior_unchanged":[],"requirements":[{"requirement":{"id":"R1","text":"bounded obligation","acceptance":"bounded evidence","governing":false},"source_refs":[],"user_clarification":null,"frozen_user_constraint":false},{"requirement":{"id":"R2","text":"second bounded obligation","acceptance":"bounded evidence","governing":true},"source_refs":[],"user_clarification":null,"frozen_user_constraint":false}],"discovery_attribution":[],"evidence_synthesis":[],"disagreements":[],"rejected_alternatives":[],"implementation_risks":[],"compatibility_concerns":[],"caveats":[],"technical_suggestions":[],"blocking_issues":[]})).unwrap();
    let reconciled = store
        .publish_bundle(
            &effort,
            "reconcile",
            ArtifactKind::ReconciledDiscovery,
            "fixture".into(),
            "IMPLEMENTATION_READY".into(),
            vec![],
            provenance(),
            BTreeMap::from([(
                "reconciled-discovery.json".into(),
                encode(&contract).unwrap(),
            )]),
        )
        .unwrap();
    let adoption = orchestrate_audit::adopt(
        &store,
        &effort,
        reconciled.clone(),
        "fixture authorization".into(),
        provenance(),
    )
    .unwrap();
    let implementation = orchestrate_audit::register_implementation(
        &store,
        &effort,
        adoption.clone(),
        &f.commit,
        "fixture".into(),
        ImplementationStatus::Submitted,
        provenance(),
    )
    .unwrap();
    let audit = AuditInputs {
        effort: effort.id,
        reconciled,
        adoption,
        implementation,
    };
    let (mut config, _) = config::load(&f.config).unwrap();
    config.audit = Some(audit.clone());
    config.request = None;
    config.question_kind = None;
    config.target = None;
    config.revision = None;
    fs::write(&f.config, toml::to_string(&config).unwrap()).unwrap();
    audit
}
#[test]
fn conformance_covers_non_governing_requirements_and_never_publishes_audit() {
    let f = Fixture::new("consensus", "binary", 4, 5);
    let refs = add_conformance(&f);
    let r = f.run(&Fake::default());
    assert_eq!(r.completed, 5);
    assert_eq!(r.recommendation, Some(Recommendation::Go));
    let store = Store::open(&f.root).unwrap();
    let effort = store.load_effort(&refs.effort).unwrap();
    assert!(
        !store
            .list_artifacts(&effort)
            .unwrap()
            .iter()
            .any(|a| a.kind == ArtifactKind::Audit)
    );
    let r = f.run(&Fake {
        missing_coverage: true,
        ..Default::default()
    });
    assert_eq!(r.invalid, 5);
    let r = f.run(&Fake {
        wrong_refs: true,
        ..Default::default()
    });
    assert_eq!(r.invalid, 5);
}
#[test]
fn conformance_wrong_digest_fails_before_agents_launch() {
    let f = Fixture::new("consensus", "binary", 4, 5);
    add_conformance(&f);
    let (mut c, _) = config::load(&f.config).unwrap();
    c.audit.as_mut().unwrap().implementation.digest = "forged".into();
    fs::write(&f.config, toml::to_string(&c).unwrap()).unwrap();
    let fake = Fake::default();
    assert!(run_with_invoker(&f.root, &f.config, &fake).is_err());
    assert!(fake.calls.lock().unwrap().is_empty());
}

#[test]
fn reused_observed_sessions_are_ineligible_without_duplicate_votes() {
    let fixture = Fixture::new("consensus", "binary", 4, 5);
    let result = fixture.run(&Fake {
        reused_session: true,
        ..Default::default()
    });
    assert_eq!(result.invalid, 5);
    assert_eq!(result.votes.as_ref().unwrap().go, 0);
    assert_eq!(inspect(&fixture.root, &result.run_id).unwrap().invalid, 5);
}
#[test]
fn shell_wrapped_command_receipts_match_exact_native_tool_input() {
    let mut graph = base_graph();
    graph.observations[0].kind = ObservationKind::Execution;
    graph.observations[0].command = Some("printf '%s' \"actual output\"".into());
    let receipt = CommandReceipt {
        id: "receipt-1".into(),
        transport_line: 1,
        command: "/bin/zsh -lc 'printf '\\''%s'\\'' \"actual output\"'".into(),
        cwd: None,
        exit_code: Some(0),
        output: "actual output".into(),
        completed: true,
    };
    let limits = evidence::validate_graph(
        &mut graph,
        "lane",
        "digest",
        &QuestionKind::Binary,
        &BTreeMap::new(),
        &[receipt],
        None,
    )
    .unwrap();
    assert!(limits.is_empty());
    assert_eq!(graph.observations[0].kind, ObservationKind::Execution);
}

#[test]
fn cursor_native_receipts_require_terminal_numeric_status() {
    let transport = r#"{"type":"tool_call","subtype":"started","tool_call":{"shellToolCall":{"args":{"command":"false"}}}}
{"type":"tool_call","subtype":"completed","tool_call":{"shellToolCall":{"args":{"command":"false","workingDirectory":"/scratch"},"result":{"failure":{"exitCode":4,"stdout":"123\n","stderr":"","aborted":false},"isBackground":false}}}}
{"type":"tool_call","subtype":"completed","tool_call":{"shellToolCall":{"args":{"command":"true"},"result":{"success":{"exitCode":0,"interleavedOutput":"ok","aborted":false}}}}}
{"type":"tool_call","subtype":"completed","tool_call":{"shellToolCall":{"args":{"command":"unknown"},"result":{"success":{"stdout":"exit=0"}}}}}
{"type":"tool_call","subtype":"completed","tool_call":{"shellToolCall":{"args":{"command":"background"},"result":{"success":{"exitCode":0},"isBackground":true}}}}"#;
    let receipts = evidence::receipts(transport);
    assert_eq!(receipts.len(), 4);
    assert_eq!(receipts[0].exit_code, Some(4));
    assert_eq!(receipts[0].cwd.as_deref(), Some("/scratch"));
    assert!(receipts[0].completed);
    assert_eq!(receipts[1].exit_code, Some(0));
    assert!(receipts[1].completed);
    assert_eq!(receipts[2].exit_code, None);
    assert!(!receipts[2].completed);
    assert!(!receipts[3].completed);
}

#[test]
fn wide_keeps_scopes_contradictions_and_rejection_history_without_inventing_origins() {
    let fixture = Fixture::new("wide", "binary", 0, 5);
    let result = fixture.run(&Fake {
        go_count: 4,
        material_defect: true,
        ..Default::default()
    });
    assert!(result.votes.is_none());
    let mut graphs: Vec<LaneGraph> = (1..=5)
        .map(|index| {
            decode(
                &fs::read(
                    fixture
                        .run_dir(&result)
                        .join(format!("lanes/lane-{index:04}/graph.json")),
                )
                .unwrap(),
            )
            .unwrap()
        })
        .collect();
    let mut reconciliation = result.reconciliation.clone().unwrap();
    let positive = reconciliation
        .findings
        .iter()
        .position(|f| !f.negative)
        .unwrap();
    let negative = reconciliation
        .findings
        .iter()
        .position(|f| f.negative)
        .unwrap();
    reconciliation.findings[positive]
        .contradicting_observations
        .push("lane-0005/O1".into());
    reconciliation.findings[negative]
        .contradicting_observations
        .push("lane-0001/O1".into());
    let mut scoped = reconciliation.findings[positive].clone();
    scoped.id = "distinct-scope".into();
    scoped.scope = "second bounded context".into();
    scoped.origins = vec!["lane-0004/F1".into()];
    scoped.supporting_observations = vec!["lane-0004/O1".into()];
    graphs[3].findings[0].scope = scoped.scope.clone();
    reconciliation.findings[positive]
        .origins
        .retain(|r| r != "lane-0004/F1");
    reconciliation.findings[positive]
        .supporting_observations
        .retain(|r| r != "lane-0004/O1");
    reconciliation.findings.push(scoped);
    aggregation::validate(&reconciliation, &result.input_digest, &graphs).unwrap();
    assert_eq!(reconciliation.findings.len(), 3);
    reconciliation.findings[negative].disposition = Disposition::Rejected;
    reconciliation.findings[negative].demonstrated = false;
    reconciliation.findings[negative].rationale =
        "Attributable primary counterevidence rejects this scoped claim.".into();
    aggregation::validate(&reconciliation, &result.input_digest, &graphs).unwrap();
    assert_eq!(reconciliation.findings[negative].origins, ["lane-0005/F1"]);
    assert!(
        !reconciliation.findings[negative]
            .contradicting_observations
            .is_empty()
    );
    reconciliation.findings[negative].origins = vec!["lane-0005/invented-finding".into()];
    assert!(aggregation::validate(&reconciliation, &result.input_digest, &graphs).is_err());
}
