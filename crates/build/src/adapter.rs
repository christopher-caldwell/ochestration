use crate::state::{RoleConfig, Session};
use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{path::Path, process::Command};

#[derive(Clone, Debug, Serialize)]
pub struct InvocationRecord {
    pub adapter: String,
    pub model: Option<String>,
    pub args: Vec<String>,
    pub argv: Vec<String>,
    pub cwd: String,
    pub session_in: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InvocationOutcome {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub final_response: Option<String>,
    pub observed_session: Option<String>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Clone, Debug)]
pub struct InvocationPlan {
    pub record: InvocationRecord,
    program: &'static str,
    argv: Vec<String>,
}

/// The sole provider seam: invoke one prepared call and return process and
/// provider-observed final-response facts. It makes deterministic controller
/// tests possible without adding policy callbacks to the adapter boundary.
pub trait InvocationApi: Sync {
    fn invoke(&self, plan: &InvocationPlan, cwd: &Path) -> Result<InvocationOutcome>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessInvocationApi;

impl InvocationApi for ProcessInvocationApi {
    fn invoke(&self, plan: &InvocationPlan, cwd: &Path) -> Result<InvocationOutcome> {
        invoke_process(plan, cwd)
    }
}

pub fn prepare_invocation(
    config: &RoleConfig,
    prompt: &str,
    cwd: &Path,
    session: Option<&Session>,
) -> Result<InvocationPlan> {
    let args = config.args.clone().unwrap_or_default();
    validate_native_args(&config.adapter, &args)?;
    let reusable = session.filter(|session| session.adapter == config.adapter);
    let mut argv = Vec::new();
    match config.adapter.as_str() {
        "codex" => {
            argv.push("exec".into());
            argv.push("--json".into());
            argv.push("-C".into());
            argv.push(cwd.to_string_lossy().into_owned());
            if let Some(session) = reusable {
                argv.push("resume".into());
                argv.push(session.id.clone());
            }
            if let Some(model) = &config.model {
                argv.push("--model".into());
                argv.push(model.clone());
            }
            argv.extend(args.iter().cloned());
            argv.push(prompt.into());
        }
        "claude" => {
            argv.extend([
                "-p".into(),
                "--verbose".into(),
                "--output-format".into(),
                "stream-json".into(),
            ]);
            if let Some(session) = reusable {
                argv.extend(["--resume".into(), session.id.clone()]);
            }
            if let Some(model) = &config.model {
                argv.extend(["--model".into(), model.clone()]);
            }
            argv.extend(args.iter().cloned());
            // --add-dir accepts variadic directories; stop option parsing so
            // the prompt is not consumed as another directory.
            argv.push("--".into());
            argv.push(prompt.into());
        }
        "cursor" => {
            argv.extend(["-p".into(), "--output-format".into(), "stream-json".into()]);
            if let Some(session) = reusable {
                argv.extend(["--resume".into(), session.id.clone()]);
            }
            if let Some(model) = &config.model {
                argv.extend(["--model".into(), model.clone()]);
            }
            argv.extend(args.iter().cloned());
            argv.push(prompt.into());
        }
        other => bail!("unsupported Build adapter {other:?}"),
    }
    let program = match config.adapter.as_str() {
        "codex" => "codex",
        "claude" => "claude",
        "cursor" => "cursor-agent",
        _ => unreachable!(),
    };
    let record = InvocationRecord {
        adapter: config.adapter.clone(),
        model: config.model.clone(),
        args,
        argv: argv.clone(),
        cwd: cwd.to_string_lossy().into_owned(),
        session_in: reusable.map(|session| session.id.clone()),
    };
    Ok(InvocationPlan {
        record,
        program,
        argv,
    })
}

fn invoke_process(plan: &InvocationPlan, cwd: &Path) -> Result<InvocationOutcome> {
    let output = Command::new(plan.program)
        .args(&plan.argv)
        .current_dir(cwd)
        .output()
        .with_context(|| format!("could not start {}", plan.program))?;
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let (final_response, observed_session) = extract_response(&stdout);
    Ok(InvocationOutcome {
        success: output.status.success(),
        exit_code: output.status.code(),
        final_response,
        observed_session,
        stdout,
        stderr,
    })
}

pub fn validate_native_args(adapter: &str, args: &[String]) -> Result<()> {
    let owned = match adapter {
        "codex" => &["exec", "resume", "--json", "-C", "--cd", "--model"][..],
        "claude" => &[
            "--",
            "--print",
            "-p",
            "--verbose",
            "--output-format",
            "--resume",
            "--model",
        ][..],
        "cursor" => &[
            "-p",
            "--output-format",
            "--resume",
            "--model",
            "--workspace",
        ][..],
        other => bail!("unsupported Build adapter {other:?}"),
    };
    for arg in args {
        ensure!(
            !arg.is_empty(),
            "native arguments cannot contain empty arguments"
        );
        let flag = arg.split_once('=').map_or(arg.as_str(), |(flag, _)| flag);
        ensure!(
            !owned.contains(&flag),
            "native argument {arg:?} collides with adapter-owned Build transport, cwd, model, or session flags"
        );
    }
    Ok(())
}

fn extract_response(stdout: &str) -> (Option<String>, Option<String>) {
    let mut response = None;
    let mut session = None;
    for line in stdout.lines() {
        let Ok(value) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if session.is_none() {
            session = value
                .get("thread_id")
                .or_else(|| value.get("session_id"))
                .or_else(|| value.get("conversation_id"))
                .and_then(Value::as_str)
                .filter(|value| !value.is_empty())
                .map(str::to_owned);
        }
        if let Some(text) = final_text(&value) {
            response = Some(text);
        }
    }
    if response.is_none() {
        if let Ok(value) = serde_json::from_str::<Value>(stdout) {
            response = final_text(&value);
            session = session.or_else(|| {
                value
                    .get("thread_id")
                    .or_else(|| value.get("session_id"))
                    .and_then(Value::as_str)
                    .map(str::to_owned)
            });
        }
    }
    (response, session)
}

fn final_text(value: &Value) -> Option<String> {
    let event_type = value
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if matches!(
        event_type,
        "result" | "turn.completed" | "response.completed"
    ) {
        for key in ["result", "final_response", "text"] {
            if let Some(text) = value.get(key).and_then(Value::as_str) {
                return Some(text.to_owned());
            }
        }
    }
    if event_type == "item.completed" {
        let item = value.get("item")?;
        if item.get("type").and_then(Value::as_str) == Some("agent_message") {
            return content_text(item.get("content").or_else(|| item.get("text")));
        }
    }
    if let Some(item) = value.get("item") {
        if item.get("type").and_then(Value::as_str) == Some("agent_message") {
            return content_text(item.get("content").or_else(|| item.get("text")));
        }
    }
    if let Some(message) = value.get("message") {
        if message.get("role").and_then(Value::as_str) == Some("assistant") {
            return content_text(message.get("content"));
        }
    }
    None
}

fn content_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let parts = items
                .iter()
                .filter_map(|item| {
                    if item.get("type").and_then(Value::as_str) == Some("text") {
                        item.get("text").and_then(Value::as_str)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            (!parts.is_empty()).then(|| parts.join("\n"))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_final_responses_and_sessions_from_native_json_streams() {
        for (body, expected) in [
            (r#"{"type":"result","result":"{}","session_id":"s1"}"#, "{}"),
            (
                r#"{"type":"item.completed","item":{"type":"agent_message","content":[{"type":"text","text":"{}"}]}}"#,
                "{}",
            ),
            (
                r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"{}"}]}}"#,
                "{}",
            ),
        ] {
            let (response, _) = extract_response(body);
            assert_eq!(response.as_deref(), Some(expected));
        }
        assert_eq!(
            extract_response(r#"{"type":"result","result":"ok","session_id":"s1"}"#)
                .1
                .as_deref(),
            Some("s1")
        );
    }

    #[test]
    fn native_args_only_reject_adapter_owned_flags() {
        validate_native_args("codex", &["--search".into(), "hello".into()]).unwrap();
        assert!(validate_native_args("codex", &["--json".into()]).is_err());
    }

    #[test]
    fn invocation_records_use_native_order_and_only_reuse_matching_sessions() {
        let codex = RoleConfig {
            adapter: "codex".into(),
            model: Some("native-model".into()),
            args: Some(vec!["--search".into()]),
        };
        let codex_session = Session {
            adapter: "codex".into(),
            id: "thread-1".into(),
        };
        let codex_call =
            prepare_invocation(&codex, "prompt", Path::new("/repo"), Some(&codex_session)).unwrap();
        assert_eq!(codex_call.record.session_in.as_deref(), Some("thread-1"));
        assert_eq!(codex_call.record.argv[0], "exec");
        assert_eq!(codex_call.record.argv[1], "--json");
        assert_eq!(codex_call.record.argv[2..4], ["-C", "/repo"]);
        assert_eq!(codex_call.record.argv[4], "resume");
        assert_eq!(codex_call.record.argv[5], "thread-1");
        assert!(codex_call.record.argv.contains(&"--json".into()));
        assert!(codex_call.record.argv.contains(&"--model".into()));
        assert_eq!(codex_call.record.argv.last().unwrap(), "prompt");

        let claude = RoleConfig {
            adapter: "claude".into(),
            model: None,
            args: None,
        };
        let claude_call =
            prepare_invocation(&claude, "prompt", Path::new("/repo"), Some(&codex_session))
                .unwrap();
        assert_eq!(claude_call.record.session_in, None);
        assert!(!claude_call.record.argv.iter().any(|arg| arg == "--resume"));

        let same_claude_session = Session {
            adapter: "claude".into(),
            id: "session-2".into(),
        };
        let claude_resumed = prepare_invocation(
            &claude,
            "prompt",
            Path::new("/repo"),
            Some(&same_claude_session),
        )
        .unwrap();
        assert!(
            claude_resumed
                .record
                .argv
                .windows(2)
                .any(|pair| pair == ["--resume", "session-2"])
        );
        assert_eq!(claude_resumed.record.argv.last().unwrap(), "prompt");
        assert_eq!(
            claude_resumed.record.argv[claude_resumed.record.argv.len() - 2],
            "--"
        );
    }
}
