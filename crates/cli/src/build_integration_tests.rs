//! Controller → display acceptance with the shared FakeInvoker; no providers.
use super::{
    build_display::{Display, Mode},
    build_output,
    build_test_support::*,
};
use orchestrate_build::{
    BuildObservation, BuildObserver, BuildState, Gate, Scope, Status, StopKind,
};
use orchestrate_build::{
    adapter::{InvocationApi, InvocationOutcome, InvocationPlan},
    state::{load_state, save_state},
};
use std::{
    fs,
    io::{self, Write},
    path::Path,
    sync::{Arc, Mutex, mpsc},
    time::Duration,
};

fn drive(fixture: &Fixture, fake: &dyn InvocationApi, mode: Mode) -> (String, serde_json::Value) {
    let mut bytes = Vec::new();
    let mut display = Display::new(&mut bytes, mode);
    let result = orchestrate_build::run_with_invoker_and_observer(
        &fixture.store,
        request(fixture),
        fake,
        &mut |observation: BuildObservation<'_>| {
            assert_eq!(
                *observation.state,
                load_state(&fixture.build_dir.join("state.json")).unwrap()
            );
            display.observe(observation);
        },
    )
    .unwrap();
    let stdout = format!("{}\n", build_output(result));
    let parsed: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(parsed.as_object().unwrap().len(), 3);
    assert!(!stdout.contains('\x1b') && !stdout.contains("Orchestrate Build"));
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.starts_with("Orchestrate Build\n"));
    assert!(
        !text.contains("INITIALIZED checkpoint=")
            && !text.contains("RUNNING gate=")
            && !text.contains("ROUTED gate=")
    );
    assert_eq!(text.contains('\x1b'), mode == Mode::Terminal);
    for prose in [
        "Scoped work is committed",
        "Review passed the exact checkpoint",
        "Assessment from exact checkpoint",
        "An operator must supply",
        "provider failed",
        "arbitrary prose, no task schema",
    ] {
        assert!(!text.contains(prose), "display leaked prose: {prose}");
    }
    (text, parsed)
}

fn state(fixture: &Fixture) -> BuildState {
    load_state(&fixture.build_dir.join("state.json")).unwrap()
}
fn plain(text: &str) -> String {
    let mut chars = text.chars();
    let mut clean = String::new();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            assert_eq!(chars.next(), Some('['));
            for code in chars.by_ref() {
                if code.is_ascii_alphabetic() {
                    break;
                }
            }
        } else if ch != '\r' {
            clean.push(ch);
        }
    }
    clean
}
fn ordered(text: &str, phrases: &[&str]) {
    let mut rest = text;
    for phrase in phrases {
        rest = rest
            .split_once(phrase)
            .unwrap_or_else(|| panic!("missing ordered event {phrase} in {rest}"))
            .1;
    }
}

#[test]
fn integrated_multiphase_corrections_and_audit_completion_preserve_evidence() {
    for mode in [Mode::Plain, Mode::Terminal] {
        let fixture = make_fixture(vec![
            "phase_01_foundation".into(),
            "phase_02_delivery".into(),
        ]);
        let fake = FakeInvoker::new(
            [
                Step::WorkComplete,
                Step::ReviewChanges,
                Step::WorkComplete,
                Step::ReviewPass,
                Step::WorkComplete,
                Step::ReviewPass,
                Step::AuditFail,
                Step::WorkComplete,
                Step::AuditPass,
            ],
            &fixture.repo,
        );
        let (output, stdout) = drive(&fixture, &fake, mode);
        if mode == Mode::Terminal {
            assert!(output.contains("\x1b[33m! REVIEW changes required"));
            assert!(output.contains("\x1b[33m! AUDIT changes required"));
            assert!(output.contains("\x1b[32m✓ Phase 1 reviewed"));
            assert!(output.contains("\x1b[32m✓ Build complete"));
        }
        let output = plain(&output);
        ordered(
            &output,
            &[
                "0 / 2 phases reviewed",
                "WORK started — Phase 1 / 2 — foundation",
                "REVIEW changes required — Phase 1 / 2",
                "WORK started — Phase 1 / 2",
                "Phase 1 reviewed — foundation",
                "WORK started — Phase 2 / 2 — delivery",
                "Phase 2 reviewed — delivery",
                "AUDIT started — Final",
                "AUDIT changes required — Final",
                "Recorded: Ready / WORK — Final",
                "WORK started — Final",
                "AUDIT started — Final",
                "AUDIT passed — Final",
                "Build complete",
            ],
        );
        for phase in [1, 2] {
            assert_eq!(
                output.matches(&format!("✓ Phase {phase} reviewed")).count(),
                1
            );
        }
        assert_eq!(output.matches("✓ Build complete").count(), 1);
        assert!(!output.contains("Phase 3"));
        if mode == Mode::Terminal {
            let correction = output
                .split_once("AUDIT changes required")
                .unwrap()
                .1
                .split_once("AUDIT passed")
                .unwrap()
                .0;
            assert!(correction.contains("2 / 2 phases reviewed"));
            assert!(correction.contains("Gate:         WORK\nScope:        Final"));
            assert!(!correction.contains("Build complete"));
        }
        assert_eq!(stdout["operation_status"], "SUCCESS");
        assert_eq!(stdout["semantic_outcome"], "BUILD_COMPLETE");
        let saved = state(&fixture);
        assert_eq!(saved.schema_version, 5);
        assert_eq!(saved.status, Status::Complete);
        assert_eq!(
            stdout["details"]["audit"],
            serde_json::to_value(saved.completion.as_ref().unwrap().audit.clone()).unwrap()
        );
        let plan: serde_json::Value =
            serde_json::from_slice(&fs::read(fixture.build_dir.join("plan.json")).unwrap())
                .unwrap();
        assert_eq!(plan["schema_version"], 4);
        assert_eq!(plan["phases"][1], "phase_02_delivery");
        assert_eq!(fake.remaining(), 0);
        let records = fake.records();
        assert_eq!(records.len(), 9);
        assert_eq!(records[2].session_in.as_deref(), Some("session-Work"));
        assert!(records[4].session_in.is_none());
        assert!(records[6].session_in.is_none());
        assert!(records[8].session_in.is_none());
        for record in &records {
            let packet = packet_from_record(record);
            let id = packet["action_id"].as_str().unwrap();
            let dir = fixture.build_dir.join("actions").join(id);
            let mut names: Vec<_> = fs::read_dir(&dir)
                .unwrap()
                .map(|entry| entry.unwrap().file_name().into_string().unwrap())
                .collect();
            names.sort();
            let mut expected = vec![
                "action.json",
                "invocation.json",
                "prompt.md",
                "report.md",
                "result.json",
            ];
            if packet["gate"] == "audit" {
                expected.push("assessment.json");
            }
            expected.sort();
            assert_eq!(names, expected, "no display artifacts in action directory");
            let result: serde_json::Value =
                serde_json::from_slice(&fs::read(dir.join("result.json")).unwrap()).unwrap();
            assert_eq!(result["action_id"], packet["action_id"]);
            if packet["gate"] == "work" {
                assert!(record.argv.contains(&"--search".to_owned()));
                assert_eq!(record.model.as_deref(), Some("native-model"));
            }
        }
        let mut names: Vec<_> = fs::read_dir(&fixture.build_dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        let mut expected = vec![
            "actions".to_owned(),
            "config.toml".into(),
            "phase_01_foundation".into(),
            "phase_02_delivery".into(),
            "plan.json".into(),
            "state.json".into(),
        ];
        for record in &records {
            let packet = packet_from_record(record);
            if let Some(id) = packet["implementation"]["artifact_id"].as_str() {
                expected.push(id.to_owned());
            }
        }
        expected.sort();
        expected.dedup();
        assert_eq!(
            names, expected,
            "only existing controller evidence and registered implementation artifacts"
        );
    }
}

#[test]
fn integrated_blocked_retry_second_block_and_final_unblock_are_distinct() {
    for mode in [Mode::Plain, Mode::Terminal] {
        for steps in [
            vec![Step::WorkBlocked, Step::UnblockRetry, Step::WorkBlocked],
            vec![Step::WorkBlocked, Step::UnblockBlocked],
        ] {
            let fixture = make_fixture(one_phase());
            let fake = FakeInvoker::new(steps, &fixture.repo);
            let (output, stdout) = drive(&fixture, &fake, mode);
            let output = plain(&output);
            ordered(
                &output,
                &[
                    "WORK blocked — Phase 1 / 1",
                    "Recorded: Ready / UNBLOCK",
                    "UNBLOCK started",
                    "Build stopped — blocked",
                ],
            );
            assert_eq!(output.matches("UNBLOCK started").count(), 1);
            if fake.records().len() == 3 {
                assert!(output.contains("UNBLOCK retry confirmed"));
            } else {
                assert!(output.contains("UNBLOCK blocked"));
            }
            let saved = state(&fixture);
            assert_eq!(saved.stop.as_ref().unwrap().kind, StopKind::Blocked);
            assert!(output.contains(saved.current_action_id.as_ref().unwrap()));
            assert_eq!(stdout["operation_status"], "STOPPED");
            assert_eq!(stdout["semantic_outcome"], "BLOCKED");
            assert_eq!(
                stdout["details"]["detail"],
                saved.stop.as_ref().unwrap().detail
            );
            let next = FakeInvoker::new(
                [Step::WorkComplete, Step::ReviewPass, Step::AuditPass],
                &fixture.repo,
            );
            let (continued, stdout) = drive(&fixture, &next, mode);
            ordered(
                &plain(&continued),
                &[
                    "Stopped",
                    "Build continued",
                    "WORK started",
                    "Build complete",
                ],
            );
            assert_eq!(stdout["semantic_outcome"], "BUILD_COMPLETE");
        }
        let fixture = make_fixture(one_phase());
        let fake = FakeInvoker::new(
            [
                Step::WorkComplete,
                Step::ReviewBlocked,
                Step::UnblockRetry,
                Step::ReviewPass,
                Step::AuditUnknown,
                Step::UnblockRetry,
                Step::AuditPass,
            ],
            &fixture.repo,
        );
        let (output, _) = drive(&fixture, &fake, mode);
        ordered(
            &plain(&output),
            &[
                "REVIEW blocked",
                "UNBLOCK retry confirmed — Phase 1 / 1",
                "Phase 1 reviewed",
                "AUDIT blocked — Final",
                "UNBLOCK started — Final",
                "UNBLOCK retry confirmed — Final",
                "AUDIT passed",
                "Build complete",
            ],
        );
        assert!(
            fake.records()
                .iter()
                .filter(|record| packet_from_record(record)["gate"] == "unblock")
                .all(|record| record.session_in.is_none())
        );
    }
}

#[test]
fn integrated_errors_restarts_and_early_returns_preserve_known_facts() {
    for mode in [Mode::Plain, Mode::Terminal] {
        for step in [Step::ProviderFailure, Step::Malformed] {
            let fixture = make_fixture(one_phase());
            let (output, stdout) = drive(&fixture, &FakeInvoker::new([step], &fixture.repo), mode);
            let saved = state(&fixture);
            let output = plain(&output);
            ordered(
                &output,
                &[
                    "WORK started",
                    "Build stopped — reset_required",
                    "orchestrate build status",
                ],
            );
            assert!(!output.contains("WORK complete"));
            assert_eq!(saved.stop.as_ref().unwrap().kind, StopKind::ResetRequired);
            assert!(output.contains(saved.current_action_id.as_ref().unwrap()));
            assert_eq!(stdout["semantic_outcome"], "BLOCKED");
            // Already stopped requires neither an executable plan nor config.
            fs::remove_file(fixture.build_dir.join("plan.json")).unwrap();
            fs::remove_file(fixture.build_dir.join("config.toml")).unwrap();
            let empty = FakeInvoker::new([], &fixture.repo);
            let (output, _) = drive(&fixture, &empty, mode);
            assert!(plain(&output).contains("Phases reviewed: unavailable"));
            assert_eq!(state(&fixture), saved);
            assert!(empty.records().is_empty());
            // Reopening a recorded Running state is uncertainty, not completion.
            let mut interrupted = saved;
            interrupted.status = Status::Running;
            interrupted.stop = None;
            save_state(&fixture.build_dir.join("state.json"), &interrupted).unwrap();
            let (output, _) = drive(&fixture, &empty, mode);
            let output = plain(&output);
            ordered(&output, &["Running", "Build stopped — reset_required"]);
            assert!(!output.contains("started") && !output.contains("complete"));
            assert_eq!(
                state(&fixture).current_action_id,
                interrupted.current_action_id
            );
        }
        let fixture = make_fixture(one_phase());
        drive(
            &fixture,
            &FakeInvoker::new(
                [Step::WorkComplete, Step::ReviewPass, Step::AuditPass],
                &fixture.repo,
            ),
            mode,
        );
        let saved = state(&fixture);
        fs::remove_file(fixture.build_dir.join("plan.json")).unwrap();
        fs::remove_file(fixture.build_dir.join("config.toml")).unwrap();
        let (output, stdout) = drive(&fixture, &FakeInvoker::new([], &fixture.repo), mode);
        assert_eq!(state(&fixture), saved);
        assert_eq!(stdout["semantic_outcome"], "BUILD_COMPLETE");
        assert!(plain(&output).contains("Phases reviewed: unavailable"));
        assert!(!output.contains("started"));
    }
}

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<(Vec<u8>, usize)>>);
impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().0.extend(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        let mut state = self.0.lock().unwrap();
        state.1 = state.0.len();
        Ok(())
    }
}
struct Paused {
    fake: FakeInvoker,
    entered: mpsc::Sender<()>,
    release: Mutex<mpsc::Receiver<()>>,
}
impl InvocationApi for Paused {
    fn invoke(
        &self,
        plan: &InvocationPlan,
        cwd: &Path,
        dir: &Path,
    ) -> anyhow::Result<InvocationOutcome> {
        self.entered.send(()).unwrap();
        self.release
            .lock()
            .unwrap()
            .recv_timeout(Duration::from_secs(15))
            .unwrap();
        self.fake.invoke(plan, cwd, dir)
    }
}

#[test]
fn integrated_paused_invocation_has_flushed_running_output_before_result() {
    for mode in [Mode::Plain, Mode::Terminal] {
        let fixture = make_fixture(one_phase());
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let fake = Paused {
            fake: FakeInvoker::new([Step::ProviderFailure], &fixture.repo),
            entered: entered_tx,
            release: Mutex::new(release_rx),
        };
        let capture = Capture::default();
        std::thread::scope(|scope| {
            let fixture = &fixture;
            let fake = &fake;
            let sink = capture.clone();
            let worker = scope.spawn(move || {
                orchestrate_build::run_with_invoker_and_observer(
                    &fixture.store,
                    request(fixture),
                    fake,
                    &mut Display::new(sink, mode),
                )
                .unwrap()
            });
            entered_rx.recv_timeout(Duration::from_secs(15)).unwrap();
            {
                let sink = capture.0.lock().unwrap();
                assert_eq!(sink.0.len(), sink.1, "running output must be flushed");
                let text = plain(std::str::from_utf8(&sink.0).unwrap());
                let saved = state(fixture);
                assert_eq!(saved.status, Status::Running);
                assert!(text.contains("WORK started — Phase 1 / 1"));
                assert!(text.contains("Status: Running"));
                assert!(text.contains(saved.current_action_id.as_ref().unwrap()));
                assert!(text.contains(&saved.checkpoint_commit[..12]));
                assert!(!text.contains("complete") && !text.contains("stopped"));
                assert!(fake.fake.records().is_empty());
            }
            release_tx.send(()).unwrap();
            assert_eq!(
                build_output(worker.join().unwrap())["semantic_outcome"],
                "BLOCKED"
            );
        });
        assert!(
            plain(std::str::from_utf8(&capture.0.lock().unwrap().0).unwrap())
                .contains("Build stopped — reset_required")
        );
    }
}

#[test]
fn integrated_resumes_seed_phase_and_final_history_without_replaying_actions() {
    for mode in [Mode::Plain, Mode::Terminal] {
        for final_work in [None, Some(false), Some(true)] {
            let phases = if final_work.is_none() {
                vec![
                    "phase_01_a".into(),
                    "phase_02_b".into(),
                    "phase_03_c".into(),
                ]
            } else {
                one_phase()
            };
            let fixture = make_fixture(phases);
            let steps = match final_work {
                None => vec![
                    Step::WorkComplete,
                    Step::ReviewPass,
                    Step::WorkComplete,
                    Step::ReviewPass,
                    Step::ProviderFailure,
                ],
                Some(false) => vec![Step::WorkComplete, Step::ReviewPass, Step::ProviderFailure],
                Some(true) => vec![
                    Step::WorkComplete,
                    Step::ReviewPass,
                    Step::AuditFail,
                    Step::ProviderFailure,
                ],
            };
            drive(&fixture, &FakeInvoker::new(steps, &fixture.repo), mode);
            orchestrate_build::reset(&fixture.store, &fixture.effort.id).unwrap();
            let ready = state(&fixture);
            let next = if final_work == Some(false) {
                vec![Step::AuditPass]
            } else if final_work == Some(true) {
                vec![Step::WorkComplete, Step::AuditPass]
            } else {
                vec![Step::WorkComplete, Step::ReviewPass, Step::AuditPass]
            };
            let (output, _) = drive(&fixture, &FakeInvoker::new(next, &fixture.repo), mode);
            let output = plain(&output);
            assert!(output.contains("Confirmed prior phase reviews (from saved scope)"));
            assert!(!output.contains("Build initialized"));
            if final_work.is_none() {
                assert_eq!(ready.scope, Scope::Phase { index: 2 });
                assert!(output.contains("2 / 3 phases reviewed"));
                ordered(
                    &output,
                    &[
                        "Phase 1 reviewed — a",
                        "Phase 2 reviewed — b",
                        "WORK started — Phase 3 / 3 — c",
                        "Phase 3 reviewed — c",
                        "Build complete",
                    ],
                );
                assert_eq!(output.matches("Phase 1 reviewed").count(), 1);
                assert_eq!(output.matches("Phase 2 reviewed").count(), 1);
            } else {
                assert_eq!(ready.scope, Scope::Final);
                assert!(output.contains("1 / 1 phases reviewed"));
                assert_eq!(
                    ready.gate,
                    if final_work == Some(true) {
                        Gate::Work
                    } else {
                        Gate::Audit
                    }
                );
                ordered(
                    &output,
                    &[
                        "Phase 1 reviewed",
                        if final_work == Some(true) {
                            "WORK started — Final"
                        } else {
                            "AUDIT started — Final"
                        },
                        "Build complete",
                    ],
                );
                assert!(!output.contains("Phase 2"));
            }
            let (complete, _) = drive(&fixture, &FakeInvoker::new([], &fixture.repo), mode);
            assert!(!complete.contains("started"));
            let count = if final_work.is_none() { 3 } else { 1 };
            assert_eq!(plain(&complete).matches("✓ Phase ").count(), count);
            assert_eq!(plain(&complete).matches("Build complete").count(), 1);
        }
    }
}

#[test]
fn integrated_output_failure_cannot_change_controller_completion() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }
    let fixture = make_fixture(one_phase());
    let fake = FakeInvoker::new(
        [Step::WorkComplete, Step::ReviewPass, Step::AuditPass],
        &fixture.repo,
    );
    let result = orchestrate_build::run_with_invoker_and_observer(
        &fixture.store,
        request(&fixture),
        &fake,
        &mut Display::new(Broken, Mode::Terminal),
    )
    .unwrap();
    assert_eq!(build_output(result)["semantic_outcome"], "BUILD_COMPLETE");
    assert_eq!(state(&fixture).status, Status::Complete);
    assert_eq!(fake.records().len(), 3);
    assert_eq!(fake.remaining(), 0);
}
