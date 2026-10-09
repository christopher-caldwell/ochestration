use anyhow::{Context, Result, bail, ensure};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    fs::{self, File},
    io::Write,
    path::Path,
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};

#[derive(Clone, Debug)]
pub struct Session {
    pub adapter: String,
    pub id: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InvocationRecord {
    pub adapter: String,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub args: Vec<String>,
    pub argv: Vec<String>,
    pub cwd: String,
    pub session_in: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_seconds: Option<u64>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct InvocationOutcome {
    pub success: bool,
    pub exit_code: Option<i32>,
    pub final_response: Option<String>,
    pub observed_session: Option<String>,
    pub stdout: String,
    pub stderr: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub timed_out: bool,
}
fn is_false(value: &bool) -> bool {
    !value
}

#[derive(Clone, Debug)]
pub struct InvocationPlan {
    pub record: InvocationRecord,
    program: &'static str,
    argv: Vec<String>,
    stdin_prompt: Option<String>,
    timeout: Option<Duration>,
    interrupt_signal: Option<Arc<AtomicUsize>>,
}

/// The sole provider seam: invoke one prepared call and return process and
/// provider-observed final-response facts. It makes deterministic controller
/// tests possible without adding policy callbacks to the adapter boundary.
pub trait InvocationApi: Sync {
    fn invoke(
        &self,
        plan: &InvocationPlan,
        cwd: &Path,
        action_dir: &Path,
    ) -> Result<InvocationOutcome>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessInvocationApi;

impl InvocationApi for ProcessInvocationApi {
    fn invoke(
        &self,
        plan: &InvocationPlan,
        cwd: &Path,
        action_dir: &Path,
    ) -> Result<InvocationOutcome> {
        invoke_process(plan, cwd, action_dir)
    }
}

pub fn prepare_invocation(
    config: &ProviderConfig,
    prompt: &str,
    cwd: &Path,
    session: Option<&Session>,
    policy_args: &[String],
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
            append_effort_args(&config.adapter, config.effort.as_deref(), &mut argv)?;
            argv.extend(policy_args.iter().cloned());
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
            append_effort_args(&config.adapter, config.effort.as_deref(), &mut argv)?;
            argv.extend(policy_args.iter().cloned());
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
            append_effort_args(&config.adapter, config.effort.as_deref(), &mut argv)?;
            argv.extend(policy_args.iter().cloned());
            argv.extend(args.iter().cloned());
            argv.push(prompt.into());
        }
        other => bail!("unsupported provider adapter {other:?}"),
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
        effort: config.effort.clone(),
        args,
        argv: argv.clone(),
        cwd: cwd.to_string_lossy().into_owned(),
        session_in: reusable.map(|session| session.id.clone()),
        timeout_seconds: None,
    };
    Ok(InvocationPlan {
        record,
        program,
        argv,
        stdin_prompt: None,
        timeout: None,
        interrupt_signal: None,
    })
}

fn append_effort_args(adapter: &str, effort: Option<&str>, argv: &mut Vec<String>) -> Result<()> {
    let Some(effort) = effort else {
        return Ok(());
    };

    match adapter {
        "codex" => {
            ensure!(
                matches!(
                    effort,
                    "none" | "minimal" | "low" | "medium" | "high" | "xhigh" | "max" | "ultra"
                ),
                "unsupported Codex effort {effort:?}; expected none, minimal, low, medium, high, xhigh, max, or ultra"
            );
            argv.push("-c".into());
            argv.push(format!("model_reasoning_effort=\"{effort}\""));
        }
        "claude" => {
            ensure!(
                matches!(
                    effort,
                    "low" | "medium" | "high" | "xhigh" | "max" | "ultracode"
                ),
                "unsupported Claude effort {effort:?}; expected low, medium, high, xhigh, max, or ultracode"
            );
            argv.extend(["--effort".into(), effort.into()]);
        }
        "cursor" => bail!(
            "provider adapter \"cursor\" does not support first-class effort; select an effort-bearing provider-native model with the model setting instead"
        ),
        other => bail!("unsupported provider adapter {other:?}"),
    }
    Ok(())
}

fn invoke_process(
    plan: &InvocationPlan,
    cwd: &Path,
    action_dir: &Path,
) -> Result<InvocationOutcome> {
    let stdout_path = action_dir.join("transport.jsonl");
    let stderr_path = action_dir.join("stderr.txt");
    let stdout_file = File::create(&stdout_path)
        .with_context(|| format!("cannot create {}", stdout_path.display()))?;
    let stderr_file = File::create(&stderr_path)
        .with_context(|| format!("cannot create {}", stderr_path.display()))?;
    let mut command = Command::new(plan.program);
    command
        .args(&plan.argv)
        .current_dir(cwd)
        .stdin(if plan.stdin_prompt.is_some() {
            Stdio::piped()
        } else {
            Stdio::inherit()
        })
        .stdout(Stdio::from(stdout_file))
        .stderr(Stdio::from(stderr_file));
    // Only bounded investigation calls own a process group. Build's process policy is unchanged.
    #[cfg(unix)]
    if plan.timeout.is_some() {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let deadline = plan
        .timeout
        .map(|timeout| {
            Instant::now()
                .checked_add(timeout)
                .context("provider timeout exceeds the supported clock range")
        })
        .transpose()?;
    let interrupted = || {
        plan.interrupt_signal
            .as_ref()
            .map_or(0, |signal| signal.load(Ordering::Relaxed))
    };
    if deadline.is_some() {
        ensure!(
            interrupted() == 0,
            "provider invocation interrupted before launch"
        );
    }
    let mut child = command
        .spawn()
        .with_context(|| format!("could not start {}", plan.program))?;
    let (status, stdin_result, timed_out) = if let Some(deadline) = deadline {
        // Delivering a large prompt must not block the deadline if the provider stops reading stdin.
        let writer = plan.stdin_prompt.as_ref().map(|prompt| {
            let mut stdin = child.stdin.take().expect("piped provider stdin");
            let prompt = prompt.clone();
            std::thread::spawn(move || stdin.write_all(prompt.as_bytes()))
        });
        let mut status = None;
        let timed_out = loop {
            let signal = interrupted();
            if signal != 0 {
                stop_bounded_provider(&mut child, &mut status)?;
                // Raw transport is already on disk. Do not join a blocked stdin writer.
                bail!(
                    "provider invocation interrupted by signal {signal}; retained output is partial"
                );
            }
            if status.is_none() {
                status = child
                    .try_wait()
                    .with_context(|| format!("could not wait for {}", plan.program))?;
            }
            if status.is_some() && writer.as_ref().is_none_or(|w| w.is_finished()) {
                break false;
            }
            if Instant::now() >= deadline {
                stop_bounded_provider(&mut child, &mut status)?;
                break true;
            }
            std::thread::sleep(Duration::from_millis(10));
        };
        let stdin_result = if let Some(writer) = writer {
            // Do not let an escaped descendant holding the pipe defeat a completed deadline.
            if timed_out {
                Ok(())
            } else {
                writer.join().expect("provider stdin writer panicked")
            }
        } else {
            Ok(())
        };
        (
            status.expect("provider process completed"),
            stdin_result,
            timed_out,
        )
    } else {
        let stdin_result = if let Some(prompt) = &plan.stdin_prompt {
            child
                .stdin
                .take()
                .context("provider stdin missing")?
                .write_all(prompt.as_bytes())
        } else {
            Ok(())
        };
        let status = child
            .wait()
            .with_context(|| format!("could not wait for {}", plan.program))?;
        (status, stdin_result, false)
    };
    stdin_result.context("could not deliver provider prompt on stdin")?;
    let stdout_bytes =
        fs::read(&stdout_path).with_context(|| format!("cannot read {}", stdout_path.display()))?;
    let stderr_bytes =
        fs::read(&stderr_path).with_context(|| format!("cannot read {}", stderr_path.display()))?;
    let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
    let mut stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
    if timed_out {
        stderr.push_str(&format!(
            "\norchestrate: provider invocation timed out after {} seconds\n",
            plan.record.timeout_seconds.expect("bounded invocation")
        ));
    }
    let (final_response, observed_session) = extract_response(&stdout);
    Ok(InvocationOutcome {
        success: status.success() && !timed_out,
        exit_code: status.code(),
        final_response,
        observed_session,
        stdout,
        stderr,
        timed_out,
    })
}

fn stop_bounded_provider(child: &mut Child, status: &mut Option<ExitStatus>) -> Result<()> {
    #[cfg(unix)]
    {
        use nix::{
            sys::signal::{Signal, killpg},
            unistd::Pid,
        };
        // The group can still own tools after the direct provider has exited.
        let pid = i32::try_from(child.id()).context("provider PID exceeds supported range")?;
        if let Err(error) = killpg(Pid::from_raw(pid), Signal::SIGKILL)
            && error != nix::errno::Errno::ESRCH
        {
            return Err(error).context("could not stop bounded provider process group");
        }
    }
    #[cfg(not(unix))]
    if status.is_none() {
        child.kill().context("could not stop bounded provider")?;
    }
    if status.is_none() {
        *status = Some(child.wait().context("could not reap bounded provider")?);
    }
    Ok(())
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
        other => bail!("unsupported provider adapter {other:?}"),
    };
    for arg in args {
        ensure!(
            !arg.is_empty(),
            "native arguments cannot contain empty arguments"
        );
        let flag = arg.split_once('=').map_or(arg.as_str(), |(flag, _)| flag);
        ensure!(
            !owned.contains(&flag),
            "native argument {arg:?} collides with adapter-owned transport, cwd, model, or session flags"
        );
    }
    Ok(())
}

pub fn extract_response(stdout: &str) -> (Option<String>, Option<String>) {
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
    if response.is_none()
        && let Ok(value) = serde_json::from_str::<Value>(stdout)
    {
        response = final_text(&value);
        session = session.or_else(|| {
            value
                .get("thread_id")
                .or_else(|| value.get("session_id"))
                .and_then(Value::as_str)
                .map(str::to_owned)
        });
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
    if let Some(item) = value.get("item")
        && item.get("type").and_then(Value::as_str) == Some("agent_message")
    {
        return content_text(item.get("content").or_else(|| item.get("text")));
    }
    if let Some(message) = value.get("message")
        && message.get("role").and_then(Value::as_str) == Some("assistant")
    {
        return content_text(message.get("content"));
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

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ProviderConfig {
    pub adapter: String,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub effort: Option<String>,
    #[serde(default)]
    pub args: Option<Vec<String>>,
}
impl InvocationPlan {
    /// Poll an explicitly supplied signal flag during bounded calls only.
    pub fn with_interrupt_signal(mut self, signal: Arc<AtomicUsize>) -> Self {
        self.interrupt_signal = Some(signal);
        self
    }
    pub fn program(&self) -> &str {
        self.program
    }
    /// Opt-in per-call bound. Callers without this setting retain the existing unbounded execution.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.record.timeout_seconds = Some(timeout.as_secs());
        self.timeout = Some(timeout);
        self
    }
    /// Investigation packets can exceed native argv limits. Build's existing argv transport stays unchanged.
    pub fn with_stdin_prompt(mut self) -> Result<Self> {
        if matches!(self.record.adapter.as_str(), "codex" | "claude") {
            let prompt = self.argv.pop().context("provider prompt missing")?;
            self.record.argv.pop();
            if self.record.adapter == "codex" {
                self.argv.push("-".into());
                self.record.argv.push("-".into());
            }
            self.stdin_prompt = Some(prompt);
        } else {
            ensure!(
                self.argv.last().is_none_or(|p| p.len() <= 96 * 1024),
                "Cursor packet exceeds bounded argv size; reduce request/evidence"
            );
        }
        Ok(self)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn timeout_bounds_blocked_stdin_and_stops_ordinary_tool_children() {
        let root = std::env::temp_dir().join(format!(
            "orchestrate-provider-timeout-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let config = ProviderConfig {
            adapter: "codex".into(),
            ..Default::default()
        };
        let unbounded = prepare_invocation(&config, "prompt", &root, None, &[]).unwrap();
        assert!(unbounded.timeout.is_none());
        assert!(unbounded.record.timeout_seconds.is_none());
        let mut plan = unbounded.with_timeout(Duration::from_secs(1));
        plan.program = "sh";
        plan.argv = vec!["-c".into(), "printf '%s\\n' '{\"type\":\"thread.started\",\"thread_id\":\"partial-session\"}'; (sleep 2; printf 'leaked' > leaked.txt) & sleep 60".into()];
        // Larger than the pipe: a provider which never reads cannot stall prompt delivery forever.
        plan.stdin_prompt = Some("x".repeat(1024 * 1024));
        let start = Instant::now();
        let outcome = invoke_process(&plan, &root, &root).unwrap();
        assert!(start.elapsed() < Duration::from_secs(5));
        assert!(outcome.timed_out && !outcome.success);
        assert_eq!(outcome.observed_session.as_deref(), Some("partial-session"));
        assert!(outcome.final_response.is_none());
        assert!(outcome.stderr.contains("timed out after 1 seconds"));
        assert!(root.join("transport.jsonl").exists());
        std::thread::sleep(Duration::from_millis(1300));
        assert!(
            !root.join("leaked.txt").exists(),
            "timed-out ordinary tool child kept running"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn interruption_stops_tools_even_after_provider_exit_and_prevents_later_launch() {
        use nix::{errno::Errno, sys::signal::kill, unistd::Pid};
        let root = std::env::temp_dir().join(format!(
            "orchestrate-provider-interrupt-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&root).unwrap();
        let signal = Arc::new(AtomicUsize::new(0));
        let mut plan = prepare_invocation(
            &ProviderConfig {
                adapter: "codex".into(),
                ..Default::default()
            },
            "prompt",
            &root,
            None,
            &[],
        )
        .unwrap()
        .with_timeout(Duration::from_secs(5))
        .with_interrupt_signal(Arc::clone(&signal));
        plan.program = "sh";
        plan.argv = vec!["-c".into(), "printf '%s' $$ > provider.pid; printf 'partial transport'; (sleep 1; printf leaked > leaked.txt) <&0 & exit 0".into()];
        plan.stdin_prompt = Some("x".repeat(1024 * 1024));
        std::thread::scope(|scope| {
            scope.spawn(|| {
                let deadline = Instant::now() + Duration::from_secs(3);
                loop {
                    if let Ok(pid) = fs::read_to_string(root.join("provider.pid"))
                        && let Ok(pid) = pid.parse::<i32>()
                        && kill(Pid::from_raw(pid), None) == Err(Errno::ESRCH)
                    {
                        signal.store(2, Ordering::Relaxed);
                        break;
                    }
                    assert!(Instant::now() < deadline, "direct provider was not reaped");
                    std::thread::sleep(Duration::from_millis(10));
                }
            });
            let error = invoke_process(&plan, &root, &root).unwrap_err();
            assert!(
                error.to_string().contains("interrupted by signal 2"),
                "{error:#}"
            );
        });
        assert_eq!(
            fs::read_to_string(root.join("transport.jsonl")).unwrap(),
            "partial transport"
        );
        std::thread::sleep(Duration::from_millis(1200));
        assert!(
            !root.join("leaked.txt").exists(),
            "ordinary child outlived interruption"
        );
        plan.argv = vec!["-c".into(), "printf launched > launched.txt".into()];
        assert!(
            invoke_process(&plan, &root, &root)
                .unwrap_err()
                .to_string()
                .contains("before launch")
        );
        assert!(!root.join("launched.txt").exists());
        fs::remove_dir_all(root).unwrap();
    }
}
