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
        let path = std::env::temp_dir().join(format!(
            "orchestrate-investigate-cli-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
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
