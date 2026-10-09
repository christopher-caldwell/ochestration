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
#[test]
fn scripted_native_run_and_offline_inspect_round_trip_json() {
    use std::os::unix::fs::PermissionsExt;
    let f = Fixture::new();
    let bin = f.path.join("bin");
    fs::create_dir(&bin).unwrap();
    let script = bin.join("codex");
    fs::write(&script,r##"#!/bin/sh
set -eu
cat >/dev/null
if [ -f lane-input.json ]; then
  lane=$(sed -n 's/ *"lane_id": "\([^"]*\)".*/\1/p' lane-input.json)
  digest=$(sed -n 's/ *"input_digest": "\([^"]*\)".*/\1/p' lane-input.json)
  result="{\"schema_version\":1,\"lane_id\":\"$lane\",\"input_digest\":\"$digest\",\"observations\":[{\"id\":\"O\",\"kind\":\"inspection\",\"source\":{\"source_id\":\"request\",\"path\":\"request.md\",\"line\":1},\"observed\":\"bounded question\",\"environment\":\"static\",\"depends_on\":[]}],\"findings\":[{\"id\":\"F\",\"claim\":\"request is present\",\"scope\":\"request\",\"applicability\":\"request identity\",\"impact\":\"bounded fixture\",\"material\":false,\"negative\":false,\"uncertainty\":\"none\",\"depends_on\":[\"O\"]}],\"challenges\":[],\"conclusion\":{\"id\":\"C\",\"answer\":\"request is present\",\"position\":\"GO\",\"depends_on\":[\"F\"],\"limitations\":[]}}"
else
  digest=$(sed -n 's/ *"input_digest": "\([^"]*\)".*/\1/p' evidence.json | head -1)
  result="{\"schema_version\":1,\"input_digest\":\"$digest\",\"findings\":[{\"id\":\"K\",\"proposition\":\"request is present\",\"scope\":\"request\",\"origins\":[\"lane-0001/F\"],\"supporting_observations\":[\"lane-0001/O\"],\"contradicting_observations\":[],\"disposition\":\"supported\",\"rationale\":\"bounded request source\",\"material\":false,\"negative\":false,\"demonstrated\":false,\"limitations\":[]}],\"challenges\":[],\"essential_findings\":[\"K\"],\"answer\":\"request is present\",\"limitations\":[]}"
fi
escaped=$(printf '%s' "$result" | sed 's/\\/\\\\/g; s/"/\\"/g')
printf '{"type":"result","result":"%s","session_id":"fresh-fixture-session"}\n' "$escaped"
"##).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
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
