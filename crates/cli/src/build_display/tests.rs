use super::*;
use orchestrate_build::{FinishedAction, Stop};
use orchestrate_contracts::{ArtifactKind, ArtifactRef};

fn fixture(total: usize) -> (BuildState, BuildPlan) {
    let reference = ArtifactRef {
        kind: ArtifactKind::ReconciledDiscovery,
        artifact_id: "fixture".into(),
        digest: "digest".into(),
    };
    let plan = BuildPlan {
        schema_version: 4,
        reconciled: reference.clone(),
        phases: (0..total)
            .map(|index| format!("phase_{:02}_delivery {index}", index + 1))
            .collect(),
    };
    let state = BuildState {
        schema_version: 5,
        reconciled: reference.clone(),
        adoption: reference,
        build_start_commit: "baseline".into(),
        plan_digest: "digest".into(),
        scope: Scope::Phase { index: 0 },
        gate: Gate::Work,
        status: Status::Ready,
        checkpoint_commit: "abc0123456789def".into(),
        feedback: Vec::new(),
        unblock: None,
        current_action_id: None,
        implementation: None,
        completion: None,
        stop: None,
    };
    (state, plan)
}

fn observe(
    display: &mut Display<Vec<u8>>,
    state: &BuildState,
    plan: Option<&BuildPlan>,
    event: BuildEvent,
) {
    display.observe(BuildObservation {
        state,
        plan,
        event,
        effort_id: "effort-fixture",
    });
}

fn text(display: &Display<Vec<u8>>) -> String {
    String::from_utf8(display.writer.clone()).unwrap()
}
fn route(id: &str, gate: Gate, scope: Scope, outcome: ActionOutcome) -> BuildEvent {
    BuildEvent::ActionRouted(FinishedAction {
        action_id: id.into(),
        gate,
        scope,
        outcome,
    })
}

#[test]
fn frames_show_reviewed_progress_and_actual_scope_gate_status() {
    let (mut state, plan) = fixture(6);
    state.scope = Scope::Phase { index: 2 };
    state.status = Status::Running;
    state.current_action_id = Some("a-full-action-id-1234567890123456789".into());
    let display = Display::new(Vec::new(), Mode::Plain);
    let expected = format!(
        "{RULE}\n\n[██████░░░░░░░░░░░░░░]  2 / 6 phases reviewed\n\nStatus:       Running\nGate:         WORK\nScope:        Phase 3 / 6 — delivery 2\nAction:       a-full-action-id-1234567890123456789\nCheckpoint:   abc012345678\nStop:         —\n\n{RULE}\n"
    );
    assert_eq!(display.frame(&state, Some(&plan)), expected);
    state.gate = Gate::Review;
    assert_eq!(
        display.frame(&state, Some(&plan)),
        expected.replace("WORK", "REVIEW")
    );
    for gate in [Gate::Audit, Gate::Work, Gate::Unblock] {
        state.scope = Scope::Final;
        state.gate = gate;
        let frame = display.frame(&state, Some(&plan));
        assert!(frame.contains("[████████████████████]  6 / 6 phases reviewed"));
        assert!(frame.contains("Scope:        Final"));
        assert!(frame.contains(&format!("Gate:         {}", super::gate(&state.gate))));
        assert!(!frame.contains("Phase 7") && !frame.contains("Build complete"));
    }
    let (mut single, plan) = fixture(1);
    single.checkpoint_commit = "short".into();
    let frame = display.frame(&single, Some(&plan));
    assert!(frame.contains("[░░░░░░░░░░░░░░░░░░░░]  0 / 1 phases reviewed"));
    assert!(frame.contains("Action:       —\nCheckpoint:   short"));
    for status in [
        Status::Ready,
        Status::Running,
        Status::Stopped,
        Status::Complete,
    ] {
        single.status = status;
        assert!(
            display
                .frame(&single, Some(&plan))
                .contains(&format!("Status:       {}", super::status(&single.status)))
        );
    }
    // Neither full IDs nor checkpoints are modified in durable input facts.
    assert_eq!(state.checkpoint_commit, "abc0123456789def");
    assert_eq!(
        state.current_action_id.as_deref(),
        Some("a-full-action-id-1234567890123456789")
    );
}

#[test]
fn standalone_status_uses_recorded_facts_and_stop_specific_recovery() {
    let (mut state, plan) = fixture(2);
    let build_dir = std::env::temp_dir().join(format!("orchestrate-status-{}", unique_id()));
    let stderr = build_dir.join("actions/a-action-1/stderr.txt");
    std::fs::create_dir_all(stderr.parent().unwrap()).unwrap();
    std::fs::write(&stderr, "provider diagnostic\n").unwrap();

    for status in [Status::Ready, Status::Running, Status::Complete] {
        state.status = status.clone();
        let output = status_report(
            "effort-resolved",
            &build_dir,
            Some(&state),
            Some(&plan),
            None,
        );
        assert!(output.contains("Build status — effort-resolved"));
        assert!(output.contains(&format!("Status:       {}", super::status(&status))));
        assert!(output.contains("0 / 2 phases reviewed"));
    }

    state.status = Status::Stopped;
    state.current_action_id = Some("a-action-1".into());
    state.feedback = vec![orchestrate_build::state::FeedbackRef {
        path: "actions/a-action-1/report.md".into(),
        purpose: "Reviewer feedback".into(),
    }];
    state.stop = Some(Stop {
        kind: StopKind::ResetRequired,
        detail: "Provider failed.\nInspect actions/a-action-1/stderr.txt\u{1b}[2J".into(),
    });
    let root = std::path::Path::new("/tmp/user's orchestration store");
    let output = status_report(
        "effort-resolved",
        &build_dir,
        Some(&state),
        Some(&plan),
        Some(root),
    );
    assert!(output.contains(
        "Stop detail:\nProvider failed.\nInspect actions/a-action-1/stderr.txt\\u{1b}[2J"
    ));
    assert!(output.contains("actions/a-action-1/report.md — Reviewer feedback"));
    assert!(output.contains("Provider stderr:\n  actions/a-action-1/stderr.txt"));
    assert!(output.contains("git status --short"));
    assert!(output.contains("orchestrate --root '/tmp/user'\\''s orchestration store' build reset --effort effort-resolved"));
    assert!(output.contains("discards tracked edits"));
    assert!(!output.contains('\x1b'));

    state.stop.as_mut().unwrap().kind = StopKind::Blocked;
    state.stop.as_mut().unwrap().detail = "Address this report".into();
    let output = status_report("effort-resolved", &build_dir, Some(&state), None, None);
    assert!(output.contains("Phases reviewed: unavailable"));
    assert!(output.contains("Address this report"));
    assert!(output.contains("Address the blocker described in the feedback"));
    assert!(!output.contains("build reset"));

    let empty = status_report("effort-empty", &build_dir, None, None, None);
    assert!(empty.contains("Status: Uninitialized"));
    assert!(empty.contains("Build directory:"));
    assert!(!empty.contains("Action:") && !empty.contains("Checkpoint:"));
    std::fs::remove_dir_all(build_dir).unwrap();
}

fn unique_id() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

#[test]
fn bar_uses_floor_for_varied_totals_and_missing_facts_are_unavailable() {
    let display = Display::new(Vec::new(), Mode::Plain);
    for total in [1, 3, 7, 23] {
        let (mut state, plan) = fixture(total);
        for index in 0..total {
            state.scope = Scope::Phase { index };
            let frame = display.frame(&state, Some(&plan));
            assert_eq!(frame.matches('█').count(), index * 20 / total);
            assert_eq!(frame.matches('░').count(), 20 - index * 20 / total);
        }
    }
    let (mut state, plan) = fixture(2);
    state.scope = Scope::Phase { index: 2 };
    for facts in [Some(&plan), None] {
        let frame = display.frame(&state, facts);
        assert!(frame.contains("Phases reviewed: unavailable"));
        assert!(frame.contains("Scope:        Phase unavailable"));
        assert!(!frame.contains("delivery") && !frame.contains("Phase 3"));
    }
    let (_, empty) = fixture(0);
    state.scope = Scope::Final;
    assert!(
        display
            .frame(&state, Some(&empty))
            .contains("Phases reviewed: unavailable")
    );
    assert!(display.frame(&state, None).contains("Scope:        Final"));
    let mut invalid = plan;
    invalid.schema_version = 99;
    assert!(
        display
            .frame(&state, Some(&invalid))
            .contains("Phases reviewed: unavailable")
    );
}

#[test]
fn labels_strip_only_the_specified_ascii_prefix() {
    for (id, expected) in [
        ("phase_03_delivery", "delivery"),
        ("phase_123_a_B c", "a_B c"),
        ("phase_01_", ""),
        ("phase_3_delivery", "phase_3_delivery"),
        ("phase_03", "phase_03"),
        ("phase_aa_delivery", "phase_aa_delivery"),
        ("Phase_03_delivery", "Phase_03_delivery"),
        ("xphase_03_delivery", "xphase_03_delivery"),
        ("phase_０３_delivery", "phase_０３_delivery"),
        ("delivery", "delivery"),
    ] {
        assert_eq!(phase_label(id), expected);
    }
}

#[test]
fn mode_resolution_obeys_explicit_override_then_no_color_and_stderr() {
    for tty in [false, true] {
        for no_color in [None, Some(""), Some("1")] {
            assert_eq!(Mode::resolve(Some("always"), no_color, tty), Mode::Terminal);
            assert_eq!(Mode::resolve(Some("never"), no_color, tty), Mode::Plain);
            assert_eq!(
                Mode::resolve(Some("auto"), no_color, tty),
                if tty { Mode::Terminal } else { Mode::Plain }
            );
            for invalid in ["", "invalid", "ALWAYS"] {
                assert_eq!(Mode::resolve(Some(invalid), no_color, tty), Mode::Plain);
            }
            assert_eq!(
                Mode::resolve(None, no_color, tty),
                if tty && no_color != Some("1") {
                    Mode::Terminal
                } else {
                    Mode::Plain
                }
            );
        }
    }
}

#[test]
fn history_seeds_once_and_new_confirmed_phase_reviews_do_not_duplicate() {
    let (mut state, plan) = fixture(4);
    state.scope = Scope::Phase { index: 2 };
    let mut display = Display::new(Vec::new(), Mode::Plain);
    observe(&mut display, &state, Some(&plan), BuildEvent::Snapshot);
    observe(&mut display, &state, Some(&plan), BuildEvent::Snapshot);
    state.status = Status::Running;
    state.gate = Gate::Review;
    state.current_action_id = Some("a-review-third".into());
    observe(&mut display, &state, Some(&plan), BuildEvent::ActionStarted);
    observe(&mut display, &state, Some(&plan), BuildEvent::ActionStarted);
    state.status = Status::Ready;
    state.gate = Gate::Work;
    state.scope = Scope::Phase { index: 3 };
    state.current_action_id = None;
    let event = route(
        "a-review-third",
        Gate::Review,
        Scope::Phase { index: 2 },
        ActionOutcome::Pass,
    );
    observe(&mut display, &state, Some(&plan), event.clone());
    observe(&mut display, &state, Some(&plan), event);
    observe(&mut display, &state, Some(&plan), BuildEvent::Snapshot);
    let output = text(&display);
    for phase in [1, 2, 3] {
        assert_eq!(
            output.matches(&format!("✓ Phase {phase} reviewed")).count(),
            1
        );
    }
    assert_eq!(output.matches("Confirmed prior phase reviews").count(), 1);
    assert_eq!(output.matches("REVIEW started").count(), 1);
    assert!(output.find("Phase 2 reviewed").unwrap() < output.find("REVIEW started").unwrap());
    assert!(!output.contains("Phase 4 reviewed"));
    assert_eq!(output.matches("Status:       ").count(), 1); // no intermediate full snapshots
    assert!(!output.contains('\x1b'));

    let mut resumed = Display::new(Vec::new(), Mode::Plain);
    state.scope = Scope::Final;
    state.gate = Gate::Work;
    observe(&mut resumed, &state, Some(&plan), BuildEvent::Snapshot);
    assert_eq!(text(&resumed).matches(" reviewed — ").count(), 4);
    assert!(!text(&resumed).contains("Build complete"));
}

#[test]
fn events_distinguish_corrections_unblock_stops_and_recorded_completion() {
    let (mut state, plan) = fixture(1);
    let mut display = Display::new(Vec::new(), Mode::Plain);
    observe(&mut display, &state, Some(&plan), BuildEvent::Initialized);
    observe(
        &mut display,
        &state,
        Some(&plan),
        route(
            "a-review",
            Gate::Review,
            state.scope.clone(),
            ActionOutcome::ChangesRequired,
        ),
    );
    assert!(text(&display).contains("REVIEW changes required"));
    assert!(!text(&display).contains("Phase 1 reviewed"));
    state.scope = Scope::Final;
    observe(
        &mut display,
        &state,
        Some(&plan),
        route(
            "a-audit",
            Gate::Audit,
            Scope::Final,
            ActionOutcome::ChangesRequired,
        ),
    );
    state.status = Status::Running;
    state.current_action_id = Some("a-correction".into());
    observe(&mut display, &state, Some(&plan), BuildEvent::ActionStarted);
    assert!(text(&display).contains("AUDIT changes required — Final"));
    assert!(text(&display).contains("WORK started — Final"));
    assert!(!text(&display).contains("Build complete"));
    state.gate = Gate::Unblock;
    state.status = Status::Ready;
    observe(
        &mut display,
        &state,
        Some(&plan),
        route(
            "a-work-block",
            Gate::Work,
            Scope::Final,
            ActionOutcome::Blocked,
        ),
    );
    assert!(text(&display).contains("Recorded: Ready / UNBLOCK — Final"));
    assert!(!text(&display).contains("Build stopped"));
    state.gate = Gate::Work;
    observe(
        &mut display,
        &state,
        Some(&plan),
        route(
            "a-unblock",
            Gate::Unblock,
            Scope::Final,
            ActionOutcome::Retry,
        ),
    );
    assert!(text(&display).contains("UNBLOCK retry confirmed"));
    state.gate = Gate::Audit;
    state.status = Status::Complete;
    observe(
        &mut display,
        &state,
        Some(&plan),
        route("a-pass", Gate::Audit, Scope::Final, ActionOutcome::Pass),
    );
    observe(&mut display, &state, Some(&plan), BuildEvent::Terminal);
    let output = text(&display);
    assert!(output.contains("AUDIT passed — Final\n  Recorded: Complete / AUDIT"));
    assert_eq!(output.matches("✓ Build complete").count(), 1);
    assert_eq!(output.matches("Status:       ").count(), 2);
    assert!(!output.contains('\x1b'));

    for kind in [StopKind::Blocked, StopKind::ResetRequired] {
        state.status = Status::Stopped;
        state.current_action_id = Some("a-retained-stopped-id".into());
        state.stop = Some(Stop {
            kind: kind.clone(),
            detail: "SECRET PROVIDER PROSE".into(),
        });
        let mut stopped = Display::new(Vec::new(), Mode::Plain);
        observe(&mut stopped, &state, Some(&plan), BuildEvent::Snapshot);
        observe(&mut stopped, &state, Some(&plan), BuildEvent::Terminal);
        let output = text(&stopped);
        assert!(output.contains(&format!("Build stopped — {}", stop_kind(&kind))));
        assert!(output.contains("orchestrate build status --effort effort-fixture"));
        assert!(output.contains("Next steps:"));
        assert!(output.contains("orchestrate build --effort effort-fixture"));
        if kind == StopKind::ResetRequired {
            assert!(output.contains("git status --short"));
            assert!(output.contains("preserves ignored files"));
        } else {
            assert!(output.contains("recorded blocker and feedback"));
            assert!(!output.contains("build reset"));
        }
        assert!(output.contains("Action:       a-retained-stopped-id"));
        assert!(!output.contains("SECRET") && !output.contains("Build complete"));
    }
}

#[derive(Default)]
struct Screen {
    rows: Vec<String>,
    row: usize,
}
impl Screen {
    fn replay(&mut self, bytes: &[u8]) {
        let stream = std::str::from_utf8(bytes).unwrap();
        let mut chars = stream.chars();
        while let Some(character) = chars.next() {
            match character {
                '\x1b' => {
                    assert_eq!(chars.next(), Some('['));
                    let mut parameters = String::new();
                    for ch in chars.by_ref() {
                        if ch.is_ascii_alphabetic() {
                            match ch {
                                'm' => {}
                                'A' => {
                                    assert_eq!(parameters, "1");
                                    self.row = self.row.checked_sub(1).unwrap();
                                }
                                'K' => {
                                    assert_eq!(parameters, "2");
                                    self.rows[self.row].clear();
                                }
                                _ => panic!("unsupported terminal operation {parameters}{ch}"),
                            }
                            break;
                        }
                        parameters.push(ch);
                    }
                }
                '\n' => self.row += 1,
                '\r' => {} // only occurs immediately before clearing a whole row
                ch => {
                    self.rows
                        .resize_with(self.rows.len().max(self.row + 1), String::new);
                    self.rows[self.row].push(ch);
                }
            }
        }
    }
}

#[test]
fn terminal_replaces_only_frame_and_retains_all_previous_history_rows() {
    let (mut state, plan) = fixture(3);
    state.scope = Scope::Phase { index: 1 };
    let mut display = Display::new(Vec::new(), Mode::Terminal);
    let mut screen = Screen::default();
    let mut consumed = 0;
    let mut immutable = Vec::<String>::new();
    let mut replay = |display: &Display<Vec<u8>>| {
        screen.replay(&display.writer[consumed..]);
        consumed = display.writer.len();
        assert_eq!(&screen.rows[..immutable.len()], immutable.as_slice());
        let frame_start = screen.row - display.frame_lines;
        immutable = screen.rows[..frame_start].to_vec();
    };
    observe(&mut display, &state, Some(&plan), BuildEvent::Snapshot);
    replay(&display);
    for step in 0..12 {
        state.current_action_id = Some(format!("a-{step}"));
        state.status = Status::Running;
        observe(&mut display, &state, Some(&plan), BuildEvent::ActionStarted);
        replay(&display);
        state.current_action_id = None;
        state.status = Status::Ready;
        observe(
            &mut display,
            &state,
            Some(&plan),
            route(
                &format!("a-{step}"),
                Gate::Review,
                state.scope.clone(),
                ActionOutcome::ChangesRequired,
            ),
        );
        replay(&display);
    }
    state.status = Status::Stopped;
    state.stop = Some(Stop {
        kind: StopKind::ResetRequired,
        detail: "hidden".into(),
    });
    observe(&mut display, &state, Some(&plan), BuildEvent::Terminal);
    replay(&display);
    let visible = screen.rows.join("\n");
    assert!(visible.contains("Phase 1 reviewed"));
    assert!(visible.contains("Build stopped — reset_required"));
    assert_eq!(visible.matches("Status:       ").count(), 1);
    assert!(visible.contains("Status:       Stopped"));
    assert_eq!(visible.matches("REVIEW changes required").count(), 12);
    assert_eq!(visible.matches("WORK started").count(), 12);
    assert!(text(&display).contains("\x1b[1A\r\x1b[2K"));
    assert!(!text(&display).contains("?1049") && !text(&display).contains("\x1b[2J"));
}

#[test]
fn sparse_colors_follow_facts_and_plain_text_has_the_same_meaning() {
    let (mut state, plan) = fixture(2);
    for (gate, status, tone) in [
        (Gate::Work, Status::Running, "36"),
        (Gate::Unblock, Status::Ready, "33"),
        (Gate::Audit, Status::Stopped, "31"),
        (Gate::Audit, Status::Complete, "32"),
    ] {
        state.gate = gate;
        state.status = status;
        let colored = Display::new(Vec::new(), Mode::Terminal).frame(&state, Some(&plan));
        assert!(colored.contains(&format!("Status:       \x1b[{tone}m")));
        assert!(colored.contains("Checkpoint:   \x1b[2m"));
        let plain = Display::new(Vec::new(), Mode::Plain).frame(&state, Some(&plan));
        let mut screen = Screen::default();
        screen.replay(colored.as_bytes());
        assert_eq!(screen.rows.join("\n") + "\n", plain);
    }
}

#[derive(Default)]
struct FlushWriter {
    bytes: Vec<u8>,
    flushed: usize,
    flushes: usize,
}
impl Write for FlushWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes.extend(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        self.flushed = self.bytes.len();
        self.flushes += 1;
        Ok(())
    }
}

#[test]
fn each_observation_flushes_before_returning_in_both_modes() {
    for mode in [Mode::Plain, Mode::Terminal] {
        let (mut state, plan) = fixture(1);
        let mut display = Display::new(FlushWriter::default(), mode);
        display.observe(BuildObservation {
            event: BuildEvent::Initialized,
            state: &state,
            plan: Some(&plan),
            effort_id: "effort-fixture",
        });
        state.status = Status::Running;
        state.current_action_id = Some("a-paused".into());
        display.observe(BuildObservation {
            event: BuildEvent::ActionStarted,
            state: &state,
            plan: Some(&plan),
            effort_id: "effort-fixture",
        });
        assert_eq!(display.writer.flushes, 2);
        assert_eq!(display.writer.flushed, display.writer.bytes.len());
        let output = String::from_utf8(display.writer.bytes).unwrap();
        assert!(output.contains("WORK started — Phase 1 / 1"));
        assert!(
            output.contains("Status: Running")
                && output.contains("a-paused")
                && output.contains("abc012345678")
        );
    }
}

#[test]
fn output_errors_are_local_and_identifiers_cannot_inject_terminal_controls() {
    struct Broken;
    impl Write for Broken {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> io::Result<()> {
            Err(io::ErrorKind::BrokenPipe.into())
        }
    }
    let (state, mut plan) = fixture(1);
    let mut display = Display::new(Broken, Mode::Terminal);
    display.observe(BuildObservation {
        event: BuildEvent::Initialized,
        state: &state,
        plan: Some(&plan),
        effort_id: "effort-fixture",
    });
    assert!(display.failed);
    display.observe(BuildObservation {
        event: BuildEvent::Terminal,
        state: &state,
        plan: None,
        effort_id: "effort-fixture",
    });
    plan.phases[0] = "phase_01_name\x1b[2J\nline".into();
    let frame = Display::new(Vec::new(), Mode::Plain).frame(&state, Some(&plan));
    assert!(!frame.contains('\x1b'));
    assert!(frame.contains("name\\u{1b}[2J\\nline"));
}
