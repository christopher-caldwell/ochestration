//! Public CLI and real native subprocess transport, with a scripted executable instead of a model.
use std::{
    fs,
    path::PathBuf,
    process::Command,
    time::{SystemTime, UNIX_EPOCH},
};
struct Fixture {
    path: PathBuf,
}
impl Fixture {
    fn new() -> Self {
        static SEQUENCE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        let path = std::env::temp_dir().join(format!(
            "orchestrate-investigate-cli-{}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos(),
            SEQUENCE.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }
}

#[cfg(unix)]
mod interruption {
    use super::*;
    use nix::{
        errno::Errno,
        sys::signal::{Signal, kill, killpg},
        unistd::Pid,
    };
    use std::{
        process::{Child, ExitStatus},
        time::{Duration, Instant},
    };

    fn config(f: &Fixture, count: usize, parallel: usize, minimum: usize) {
        fs::write(
            f.path.join("request.md"),
            "Controlled process lifecycle test.\n",
        )
        .unwrap();
        fs::write(f.path.join("config.toml"), format!("schema_version=1\nmode='wide'\nquestion_kind='binary'\nrequest='request.md'\nprovider_timeout_seconds=30\nmax_parallel={parallel}\n[[lanes]]\nadapter='codex'\ncount={count}\n[reconciler]\nadapter='codex'\n[completion]\nmin_completed={minimum}\n")).unwrap();
    }

    fn provider(f: &Fixture) -> PathBuf {
        let bin = scripted_provider(f);
        let path = bin.join("codex");
        let original = fs::read_to_string(&path).unwrap();
        let stalled = r#"
name=${PWD##*/}
stall=no
case "${ORCHESTRATE_TEST_INTERRUPT_STAGE:-}" in
  lanes) case "$name" in lane-*) stall=yes;; esac;;
  queued) [ "$name" != lane-0002 ] || stall=yes;;
  reconciler) [ "$name" != reconciler ] || stall=yes;;
esac
if [ "$stall" = yes ]; then
  if [ "${ORCHESTRATE_TEST_BLOCK_STDIN:-no}" != yes ]; then cat >/dev/null; fi
  printf '%s\n' "$$" > provider.pid
  printf '{"type":"thread.started","thread_id":"partial-interrupted-session"}\n'
  printf 'partial provider stderr\n' >&2
  sh -c 'echo "$$" > tool.pid; while :; do echo tool >> tool.work; sleep 0.05; done' &
  while :; do echo provider >> provider.work; sleep 0.05; done
fi
"#;
        fs::write(
            path,
            original.replacen("set -eu\n", &format!("set -eu\n{stalled}"), 1),
        )
        .unwrap();
        bin
    }

    struct Running<'a> {
        child: Child,
        fixture: &'a Fixture,
    }
    impl<'a> Running<'a> {
        fn start(fixture: &'a Fixture, bin: PathBuf, stage: &str, blocked_stdin: bool) -> Self {
            use std::os::unix::process::CommandExt;
            let mut paths = vec![bin];
            paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
            let child = cli()
                .args([
                    "--root",
                    fixture.path.join("store").to_str().unwrap(),
                    "investigate",
                    "run",
                    "--config",
                    fixture.path.join("config.toml").to_str().unwrap(),
                    "--json",
                ])
                .env("PATH", std::env::join_paths(paths).unwrap())
                .env("ORCHESTRATE_TEST_INTERRUPT_STAGE", stage)
                .env(
                    "ORCHESTRATE_TEST_BLOCK_STDIN",
                    if blocked_stdin { "yes" } else { "no" },
                )
                .current_dir(&fixture.path)
                .stdout(fs::File::create(fixture.path.join("cli.stdout")).unwrap())
                .stderr(fs::File::create(fixture.path.join("cli.stderr")).unwrap())
                .process_group(0)
                .spawn()
                .unwrap();
            Self { child, fixture }
        }
        fn run_dir(&self) -> Option<PathBuf> {
            fs::read_dir(self.fixture.path.join("store/investigations"))
                .ok()?
                .next()?
                .ok()
                .map(|e| e.path())
        }
        fn active_dirs(&self) -> Vec<PathBuf> {
            let Some(run) = self.run_dir() else {
                return vec![];
            };
            let mut dirs = fs::read_dir(run.join("lanes"))
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|e| e.path())
                .collect::<Vec<_>>();
            dirs.push(run.join("reconciler"));
            dirs.into_iter()
                .filter(|d| d.join("provider.pid").exists())
                .collect()
        }
        fn ready(&mut self, count: usize) -> Vec<PathBuf> {
            let deadline = Instant::now() + Duration::from_secs(10);
            loop {
                let dirs = self.active_dirs();
                if dirs.len() == count
                    && dirs.iter().all(|d| {
                        d.join("tool.pid").exists()
                            && fs::metadata(d.join("tool.work")).is_ok_and(|m| m.len() > 0)
                            && fs::metadata(d.join("provider.work")).is_ok_and(|m| m.len() > 0)
                    })
                {
                    return dirs;
                }
                assert!(
                    self.child.try_wait().unwrap().is_none(),
                    "controller exited before readiness: {}",
                    fs::read_to_string(self.fixture.path.join("cli.stderr")).unwrap()
                );
                assert!(Instant::now() < deadline, "provider readiness timed out");
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        fn interrupt(&self, signal: Signal, group: bool) {
            let pid = Pid::from_raw(self.child.id() as i32);
            if group {
                killpg(pid, signal).unwrap();
            } else {
                kill(pid, signal).unwrap();
            }
        }
        fn exit(&mut self, signal: Signal) {
            let deadline = Instant::now() + Duration::from_secs(5);
            let status: ExitStatus = loop {
                if let Some(status) = self.child.try_wait().unwrap() {
                    break status;
                }
                assert!(
                    Instant::now() < deadline,
                    "interruption failed to bound shutdown"
                );
                std::thread::sleep(Duration::from_millis(10));
            };
            assert_eq!(
                status.code(),
                Some(128 + signal as i32),
                "{}",
                fs::read_to_string(self.fixture.path.join("cli.stderr")).unwrap()
            );
            assert!(
                fs::read(self.fixture.path.join("cli.stdout"))
                    .unwrap()
                    .is_empty()
            );
            let run = self.run_dir().unwrap();
            assert!(
                !run.join("manifest.json").exists(),
                "interrupted run was finalized"
            );
        }
        fn inspect(&self) -> serde_json::Value {
            let run = self.run_dir().unwrap();
            let output = cli()
                .args([
                    "--root",
                    self.fixture.path.join("store").to_str().unwrap(),
                    "investigate",
                    "inspect",
                    "--run",
                    run.file_name().unwrap().to_str().unwrap(),
                    "--json",
                ])
                .env("PATH", "/no-provider-executables")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(value["completion"], "INCOMPLETE");
            assert_eq!(value["recommendation"], "INCONCLUSIVE");
            value
        }
    }
    impl Drop for Running<'_> {
        fn drop(&mut self) {
            // Clean every observed invocation group even when the test fails against a regression.
            for dir in self.active_dirs() {
                if let Ok(pid) = fs::read_to_string(dir.join("provider.pid"))
                    && let Ok(pid) = pid.trim().parse::<i32>()
                {
                    let _ = killpg(Pid::from_raw(pid), Signal::SIGKILL);
                }
            }
            if self.child.try_wait().ok().flatten().is_none() {
                let _ = killpg(Pid::from_raw(self.child.id() as i32), Signal::SIGKILL);
                let _ = self.child.wait();
            }
        }
    }

    fn assert_stopped(dirs: &[PathBuf]) {
        let sizes = |dir: &PathBuf| {
            ["provider.work", "tool.work"].map(|name| fs::metadata(dir.join(name)).unwrap().len())
        };
        let before: Vec<_> = dirs.iter().map(sizes).collect();
        std::thread::sleep(Duration::from_millis(250));
        let after: Vec<_> = dirs.iter().map(sizes).collect();
        assert_eq!(
            before, after,
            "owned processes continued working after interruption"
        );
        for dir in dirs {
            let pid = fs::read_to_string(dir.join("provider.pid"))
                .unwrap()
                .trim()
                .parse()
                .unwrap();
            assert_eq!(
                kill(Pid::from_raw(pid), None),
                Err(Errno::ESRCH),
                "provider was not reaped"
            );
            assert!(
                fs::read_to_string(dir.join("transport.jsonl"))
                    .unwrap()
                    .contains("partial-interrupted-session")
            );
            assert!(
                fs::read_to_string(dir.join("stderr.txt"))
                    .unwrap()
                    .contains("partial provider stderr")
            );
        }
    }

    #[test]
    fn controller_group_sigint_and_direct_signals_stop_concurrent_lanes() {
        for (signal, group) in [
            (Signal::SIGINT, true),
            (Signal::SIGINT, false),
            (Signal::SIGTERM, false),
        ] {
            let f = Fixture::new();
            config(&f, 2, 2, 2);
            let mut run = Running::start(&f, provider(&f), "lanes", false);
            let dirs = run.ready(2);
            run.interrupt(signal, group);
            run.exit(signal);
            assert_stopped(&dirs);
            let value = run.inspect();
            assert_eq!(value["completed"], 0);
            assert_eq!(value["failed"], 2);
        }
    }

    #[test]
    fn blocked_stdin_and_repeated_signals_still_clean_up() {
        let f = Fixture::new();
        config(&f, 1, 1, 1);
        fs::write(f.path.join("request.md"), "x".repeat(1024 * 1024)).unwrap();
        let mut run = Running::start(&f, provider(&f), "lanes", true);
        let dirs = run.ready(1);
        // Deliver both while the same handler remains installed; neither may bypass cleanup.
        run.interrupt(Signal::SIGINT, false);
        let _ = kill(Pid::from_raw(run.child.id() as i32), Signal::SIGINT);
        run.exit(Signal::SIGINT);
        assert_stopped(&dirs);
        run.inspect();
    }

    #[test]
    fn interrupted_reconciler_preserves_completed_lanes_without_completing_run() {
        let f = Fixture::new();
        config(&f, 2, 2, 1);
        let mut run = Running::start(&f, provider(&f), "reconciler", false);
        let dirs = run.ready(1);
        assert_eq!(dirs[0].file_name().unwrap(), "reconciler");
        run.interrupt(Signal::SIGTERM, false);
        run.exit(Signal::SIGTERM);
        assert_stopped(&dirs);
        assert_eq!(run.inspect()["completed"], 2);
    }

    #[test]
    fn interrupt_skips_queued_lanes_even_after_minimum_completed() {
        let f = Fixture::new();
        config(&f, 3, 1, 1);
        let mut run = Running::start(&f, provider(&f), "queued", false);
        let dirs = run.ready(1);
        let root = run.run_dir().unwrap();
        assert!(root.join("lanes/lane-0001/manifest.json").exists());
        run.interrupt(Signal::SIGINT, false);
        run.exit(Signal::SIGINT);
        assert_stopped(&dirs);
        assert!(!root.join("lanes/lane-0003").exists());
        assert!(!root.join("reconciler").exists());
        let value = run.inspect();
        assert_eq!(value["completed"], 1);
        assert_eq!(value["failed"], 2);
    }

    #[test]
    fn interrupt_cleanup_does_not_affect_another_investigation() {
        let first = Fixture::new();
        let second = Fixture::new();
        config(&first, 2, 2, 2);
        config(&second, 2, 2, 2);
        let mut a = Running::start(&first, provider(&first), "lanes", false);
        let mut b = Running::start(&second, provider(&second), "lanes", false);
        let a_dirs = a.ready(2);
        let b_dirs = b.ready(2);
        let before: Vec<_> = b_dirs
            .iter()
            .map(|d| fs::metadata(d.join("tool.work")).unwrap().len())
            .collect();
        a.interrupt(Signal::SIGINT, true);
        a.exit(Signal::SIGINT);
        assert_stopped(&a_dirs);
        assert!(b.child.try_wait().unwrap().is_none());
        for (dir, size) in b_dirs.iter().zip(before) {
            assert!(fs::metadata(dir.join("tool.work")).unwrap().len() > size);
        }
        b.interrupt(Signal::SIGTERM, false);
        b.exit(Signal::SIGTERM);
        assert_stopped(&b_dirs);
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
fn cli() -> Command {
    Command::new(env!("CARGO_BIN_EXE_orchestrate"))
}
#[test]
fn guide_and_inspect_are_bootstrap_safe_and_read_only() {
    let f = Fixture::new();
    let root = f.path.join("absent");
    let guide = cli()
        .args(["--root", root.to_str().unwrap(), "investigate", "guide"])
        .current_dir(&f.path)
        .output()
        .unwrap();
    assert!(guide.status.success());
    assert_eq!(
        String::from_utf8(guide.stdout).unwrap(),
        orchestrate_guides::INVESTIGATE
    );
    assert!(!root.exists());
    let inspect = cli()
        .args([
            "--root",
            root.to_str().unwrap(),
            "investigate",
            "inspect",
            "--run",
            "missing",
            "--json",
        ])
        .current_dir(&f.path)
        .output()
        .unwrap();
    assert_eq!(inspect.status.code(), Some(2));
    assert!(!root.exists());
}
#[test]
fn invalid_config_does_not_initialize_storage() {
    let f = Fixture::new();
    let root = f.path.join("absent");
    let config = f.path.join("invalid.toml");
    fs::write(&config,"schema_version=99\nmode='consensus'\nquestion_kind='binary'\nrequest='request.md'\nlanes=[]\n[reconciler]\nadapter='codex'\n").unwrap();
    let output = cli()
        .args([
            "--root",
            root.to_str().unwrap(),
            "investigate",
            "run",
            "--config",
            config.to_str().unwrap(),
            "--json",
        ])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!root.exists());
}
#[cfg(unix)]
fn scripted_provider(f: &Fixture) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let bin = f.path.join("bin");
    fs::create_dir(&bin).unwrap();
    let script = bin.join("codex");
    fs::write(&script,r##"#!/bin/sh
set -eu
cat >/dev/null
if [ -f lane-input.json ]; then
  lane=$(sed -n 's/ *"lane_id": "\([^"]*\)".*/\1/p' lane-input.json)
  if [ "${ORCHESTRATE_TEST_STALL_LANE:-}" = "$lane" ]; then
    printf '{"type":"thread.started","thread_id":"stalled-native-session"}\n'
    sleep 60
  fi
  digest=$(sed -n 's/ *"input_digest": "\([^"]*\)".*/\1/p' lane-input.json)
  result="{\"schema_version\":1,\"lane_id\":\"$lane\",\"input_digest\":\"$digest\",\"observations\":[{\"id\":\"O\",\"kind\":\"inspection\",\"source\":{\"source_id\":\"request\",\"path\":\"request.md\",\"line\":1},\"observed\":\"bounded question\",\"environment\":\"static\",\"depends_on\":[]}],\"findings\":[{\"id\":\"F\",\"claim\":\"request is present\",\"scope\":\"request\",\"applicability\":\"request identity\",\"impact\":\"bounded fixture\",\"material\":false,\"negative\":false,\"uncertainty\":\"none\",\"depends_on\":[\"O\"]}],\"challenges\":[],\"conclusion\":{\"id\":\"C\",\"answer\":\"request is present\",\"position\":\"GO\",\"depends_on\":[\"F\"],\"limitations\":[]}}"
else
  if [ "${ORCHESTRATE_TEST_STALL_RECONCILER:-}" = "yes" ]; then
    printf '{"type":"thread.started","thread_id":"stalled-reconciler"}\n'
    sleep 60
  fi
  digest=$(sed -n 's/ *"input_digest": "\([^"]*\)".*/\1/p' evidence.json | head -1)
  origins=$(sed -n 's/ *"lane_id": "\([^" ]*\)".*/"\1\/F"/p' evidence.json | paste -sd, -)
  observations=$(sed -n 's/ *"lane_id": "\([^" ]*\)".*/"\1\/O"/p' evidence.json | paste -sd, -)
  result="{\"schema_version\":1,\"input_digest\":\"$digest\",\"findings\":[{\"id\":\"K\",\"proposition\":\"request is present\",\"scope\":\"request\",\"origins\":[$origins],\"supporting_observations\":[$observations],\"contradicting_observations\":[],\"disposition\":\"supported\",\"rationale\":\"bounded request source\",\"material\":false,\"negative\":false,\"demonstrated\":false,\"limitations\":[]}],\"challenges\":[],\"essential_findings\":[\"K\"],\"answer\":\"request is present\",\"limitations\":[]}"
fi
escaped=$(printf '%s' "$result" | sed 's/\\/\\\\/g; s/"/\\"/g')
printf '{"type":"result","result":"%s","session_id":"fresh-fixture-%s"}\n' "$escaped" "${lane:-reconciler}"
"##).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    bin
}
#[cfg(unix)]
#[test]
fn scripted_native_run_and_offline_inspect_round_trip_json() {
    let f = Fixture::new();
    let bin = scripted_provider(&f);
    let root = f.path.join("store");
    fs::write(f.path.join("request.md"), "A bounded question.\n").unwrap();
    let config = f.path.join("config.toml");
    fs::write(&config,"schema_version=1\nmode='consensus'\nquestion_kind='binary'\nrequest='request.md'\n[[lanes]]\nadapter='codex'\ncount=1\n[reconciler]\nadapter='codex'\n[consensus]\nmin_go_votes=1\n").unwrap();
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let run = cli()
        .args([
            "--root",
            root.to_str().unwrap(),
            "investigate",
            "run",
            "--config",
            config.to_str().unwrap(),
            "--json",
        ])
        .env("PATH", std::env::join_paths(paths).unwrap())
        .current_dir(&f.path)
        .output()
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let value: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
    assert_eq!(value["completion"], "COMPLETE", "{value:#}");
    assert_eq!(value["recommendation"], "GO");
    assert_eq!(value["votes"]["go"], 1);
    let run_id = value["run_id"].as_str().unwrap();
    let inspect = cli()
        .args([
            "--root",
            root.to_str().unwrap(),
            "investigate",
            "inspect",
            "--run",
            run_id,
            "--json",
        ])
        .env("PATH", "/nonexistent-provider-path")
        .current_dir(&f.path)
        .output()
        .unwrap();
    assert!(
        inspect.status.success(),
        "{}",
        String::from_utf8_lossy(&inspect.stderr)
    );
    let inspected: serde_json::Value = serde_json::from_slice(&inspect.stdout).unwrap();
    assert_eq!(inspected, value);
}

#[cfg(unix)]
#[test]
fn stalled_native_lanes_and_reconciler_finish_with_honest_partial_results() {
    let f = Fixture::new();
    let bin = scripted_provider(&f);
    let mut paths = vec![bin];
    paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap()));
    let path = std::env::join_paths(paths).unwrap();
    fs::write(
        f.path.join("request.md"),
        "Inspect the bounded question only; no test execution.\n",
    )
    .unwrap();
    let config = f.path.join("config.toml");
    let root = f.path.join("store");
    for (minimum, stalled_reconciler) in [(1, false), (3, false), (1, true)] {
        fs::write(&config, format!("schema_version=1\nmode='wide'\nquestion_kind='binary'\nrequest='request.md'\nprovider_timeout_seconds=1\n[[lanes]]\nadapter='codex'\ncount=3\n[reconciler]\nadapter='codex'\n[completion]\nmin_completed={minimum}\n")).unwrap();
        let start = std::time::Instant::now();
        let run = cli()
            .args([
                "--root",
                root.to_str().unwrap(),
                "investigate",
                "run",
                "--config",
                config.to_str().unwrap(),
                "--json",
            ])
            .env("PATH", &path)
            .env("ORCHESTRATE_TEST_STALL_LANE", "lane-0002")
            .env(
                "ORCHESTRATE_TEST_STALL_RECONCILER",
                if stalled_reconciler { "yes" } else { "no" },
            )
            .current_dir(&f.path)
            .output()
            .unwrap();
        assert!(start.elapsed() < std::time::Duration::from_secs(8));
        assert!(
            run.status.success(),
            "{}",
            String::from_utf8_lossy(&run.stderr)
        );
        let value: serde_json::Value = serde_json::from_slice(&run.stdout).unwrap();
        assert_eq!(value["completed"], 2, "{value:#}");
        assert_eq!(value["failed"], 1);
        assert_eq!(value["requested"], 3);
        assert!(value.get("votes").is_none());
        let run_id = value["run_id"].as_str().unwrap();
        let lane = root
            .join("investigations")
            .join(run_id)
            .join("lanes/lane-0002");
        let outcome: serde_json::Value =
            serde_json::from_slice(&fs::read(lane.join("outcome.json")).unwrap()).unwrap();
        assert_eq!(outcome["timed_out"], true);
        assert!(!outcome["success"].as_bool().unwrap());
        assert!(
            fs::read_to_string(lane.join("transport.jsonl"))
                .unwrap()
                .contains("stalled-native-session")
        );
        assert!(lane.join("manifest.json").exists());
        if minimum == 1 && !stalled_reconciler {
            assert_eq!(value["completion"], "COMPLETE", "{value:#}");
            assert_eq!(
                value["reconciliation"]["findings"][0]["origins"]
                    .as_array()
                    .unwrap()
                    .len(),
                2
            );
        } else {
            assert_eq!(value["completion"], "INCOMPLETE", "{value:#}");
            assert_eq!(value["recommendation"], "INCONCLUSIVE");
        }
        let inspected = cli()
            .args([
                "--root",
                root.to_str().unwrap(),
                "investigate",
                "inspect",
                "--run",
                run_id,
                "--json",
            ])
            .env("PATH", "/unavailable-providers")
            .output()
            .unwrap();
        assert!(
            inspected.status.success(),
            "{}",
            String::from_utf8_lossy(&inspected.stderr)
        );
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&inspected.stdout).unwrap(),
            value
        );
    }
}
