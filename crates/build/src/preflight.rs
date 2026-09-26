//! Bounded preflight for a prepared Build's execution path.
//!
//! Static preflight resolves the exact executable, attribute mapping and
//! settings the configured roles would use, from this machine's launch
//! environment.  It runs no provider inference, writes nothing, and reports
//! what it cannot establish as unknown instead of claiming a capability.
//!
//! Live preflight is optional and only ever runs after explicit operator
//! authorization.  It uses isolated owned Git fixtures, never the product
//! checkout, and retains its probe evidence.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use anyhow::{Context, Result, bail, ensure};
use orchestrate_contracts::digest_bytes;
use orchestrate_core::{Effort, Store, write_bytes_sync};
use serde::Serialize;
use serde_json::Value;

use crate::{
    BuildConfig, HostAdapter, Invocation, InvocationCompletion, InvocationObserver, RoleConfig,
    attribute_arguments_for, attribute_provenance, effective_config, provider_arguments, read_json,
};

/// How long one local `--version` probe may take before it is killed.  It is a
/// bound on preflight itself, not a provider timeout.
const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, Serialize)]
pub struct PreflightReport {
    pub mode: &'static str,
    pub effort: String,
    pub config_version: u32,
    pub roles: Vec<RolePreflight>,
    /// What this mode cannot establish, stated rather than assumed.
    pub unknown_capabilities: Vec<String>,
    pub live: Option<LivePreflightReport>,
}

#[derive(Clone, Debug, Serialize)]
pub struct RolePreflight {
    pub role: String,
    pub adapter: String,
    /// provider default / configured / not applicable for each attribute.
    pub attributes: serde_json::Value,
    /// The exact provider-native arguments this role would dispatch with.
    pub attribute_arguments: Vec<String>,
    /// The command form with the action prompt elided.
    pub command_form: Vec<String>,
    /// Non-secret environment overrides the controller applies at the process
    /// boundary.  It applies none for any adapter, so this is empty by
    /// construction and stays reported rather than assumed.
    pub environment_overrides: Vec<String>,
    pub executable: Option<String>,
    pub executable_found: bool,
    pub version: Option<String>,
    /// True only when the operator explicitly configured full access.
    pub explicit_full_access: bool,
    pub notes: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
pub struct LivePreflightReport {
    pub authorized: bool,
    pub disclosure: String,
    pub probes: Vec<LiveProbe>,
    pub evidence_dir: String,
}

#[derive(Clone, Debug, Serialize)]
pub struct LiveProbe {
    pub role: String,
    pub adapter: String,
    pub outcome: &'static str,
    pub detail: String,
    pub fixture: String,
    pub evidence: Vec<String>,
    /// Independently verified Git commits, populated for worker probes.
    pub fresh_commit: Option<String>,
    pub resumed_commit: Option<String>,
    pub resumed_parent: Option<String>,
    pub resumed_session_verified: bool,
}

/// Resolve one adapter to the executable its dispatch would launch.
pub(crate) fn program_for(adapter: &str) -> Result<&'static str> {
    match adapter {
        "codex" => Ok("codex"),
        "claude" => Ok("claude"),
        "cursor" => Ok("cursor-agent"),
        other => bail!("unsupported adapter {other}"),
    }
}

/// Resolve a program against the launch environment's own PATH.
pub(crate) fn resolve_executable(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|directory| directory.join(program))
        .find(|candidate| candidate.is_file())
}

/// The installed version, when the executable answers a bounded local
/// `--version`.  No provider inference is involved.
pub(crate) fn executable_version(executable: &Path) -> Option<String> {
    let mut child = Command::new(executable)
        .arg("--version")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + VERSION_PROBE_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(25)),
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Err(_) => return None,
        }
    }
    let output = child.wait_with_output().ok()?;
    let text = if output.stdout.is_empty() {
        String::from_utf8_lossy(&output.stderr).into_owned()
    } else {
        String::from_utf8_lossy(&output.stdout).into_owned()
    };
    text.lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .map(str::to_owned)
}

/// Capture a model label only when the provider's structured transport reports
/// one. Claude Code reports it on assistant events; Cursor reports it during
/// stream initialization. The Codex JSON stream on the supported CLI does not
/// expose the effective model, so its observed value remains unknown.
pub(crate) fn observed_provider_model(adapter: &str, transport: &Path) -> Option<String> {
    let input = fs::read_to_string(transport).ok()?;
    input.lines().find_map(|line| {
        let event = serde_json::from_str::<Value>(line).ok()?;
        let model = match adapter {
            "claude" if event["type"] == "assistant" => event["message"]["model"].as_str(),
            "cursor" if event["type"] == "system" && event["subtype"] == "init" => {
                event["model"].as_str()
            }
            _ => None,
        }?;
        (!model.trim().is_empty()).then(|| model.to_owned())
    })
}

fn role_preflight(
    role: &str,
    config: &RoleConfig,
    cwd: &Path,
    build_dir: &Path,
) -> Result<RolePreflight> {
    let program = program_for(&config.adapter)?;
    let executable = resolve_executable(program);
    let mut notes = Vec::new();
    if executable.is_none() {
        notes.push(format!(
            "the {program} executable is not on this launch environment's PATH; a dispatch would fail with a spawn failure before any provider work"
        ));
    }
    if config.permission.as_deref() == Some("full_access") {
        notes.push(
            "this role is explicitly configured for full access; the provider's sandbox and approval prompts are bypassed for every dispatch of this role"
                .into(),
        );
    }
    if config.adapter == "claude" {
        notes.push(
            "no provider environment is injected: this role inherits the launching environment, so the backend and credential are whichever ones this host's own Claude Code configuration selects"
                .into(),
        );
    }
    if config.reasoning_effort.is_some() {
        notes.push(
            "the configured reasoning effort is forwarded verbatim; this adapter and model's acceptance of it is not established by a static check"
                .into(),
        );
    }
    let invocation = Invocation {
        adapter: config.adapter.clone(),
        role: role.into(),
        config: config.clone(),
        cwd: cwd.to_path_buf(),
        build_dir: build_dir.to_path_buf(),
        action: build_dir.to_path_buf(),
        session_id: None,
        prompt: "<prompt elided>".into(),
    };
    let mut command_form = vec![program.into()];
    command_form.extend(provider_arguments(&invocation)?);
    if let Some(last) = command_form.last_mut() {
        *last = "<prompt elided>".into();
    }
    Ok(RolePreflight {
        role: role.into(),
        adapter: config.adapter.clone(),
        attributes: attribute_provenance(config),
        attribute_arguments: attribute_arguments_for(role, config)?,
        command_form,
        // The controller applies no provider environment of its own for any
        // adapter, so every role runs against the backend the host and the
        // provider's own configuration already select.
        environment_overrides: Vec::new(),
        executable: executable
            .as_ref()
            .map(|path| path.to_string_lossy().into_owned()),
        executable_found: executable.is_some(),
        version: executable.as_deref().and_then(executable_version),
        explicit_full_access: config.permission.as_deref() == Some("full_access"),
        notes,
    })
}

/// Static preflight: the effective role configuration and this machine's own
/// executable resolution, with nothing dispatched and nothing written.
pub fn static_preflight(store: &Store, effort: &Effort) -> Result<PreflightReport> {
    let build_dir = store.phase_dir(effort, "build")?;
    let project_dir = store.project_for(effort)?.canonical_locator;
    let effective = effective_config(&build_dir)?;
    let config = &effective.config;
    let mut roles = vec![
        role_preflight("worker", &config.worker, &project_dir, &build_dir)?,
        role_preflight("reviewer", &config.reviewer, &project_dir, &build_dir)?,
    ];
    let unblocker = match &config.unblocker {
        Some(unblocker) => {
            let mut report = role_preflight("unblocker", unblocker, &project_dir, &build_dir)?;
            report
                .notes
                .push("configured independently of the reviewer".into());
            report
        }
        None => {
            let mut report =
                role_preflight("unblocker", &config.reviewer, &project_dir, &build_dir)?;
            report.notes.push(format!(
                "no unblocker is configured, so this is the documented fallback: Unblock turns use the reviewer's {} settings",
                config.reviewer.adapter
            ));
            report
        }
    };
    roles.push(unblocker);
    match &config.once_over {
        Some(once_over) => {
            let mut report = role_preflight("once_over", once_over, &project_dir, &build_dir)?;
            report
                .notes
                .push("the advisory once-over runs once per new commit submitted to Audit".into());
            roles.push(report);
        }
        None => roles.push(RolePreflight {
            role: "once_over".into(),
            adapter: String::new(),
            attributes: serde_json::Value::Null,
            attribute_arguments: Vec::new(),
            command_form: Vec::new(),
            environment_overrides: Vec::new(),
            executable: None,
            executable_found: false,
            version: None,
            explicit_full_access: false,
            notes: vec!["not configured; the advisory once-over never runs".into()],
        }),
    }
    Ok(PreflightReport {
        mode: "static",
        effort: effort.id.clone(),
        config_version: effective.version,
        roles,
        unknown_capabilities: vec![
            "whether the provider accepts the configured model".into(),
            "whether the provider accepts the configured reasoning effort".into(),
            "whether the role can commit inside the workspace its effective sandbox policy permits".into(),
            "whether the provider is authenticated and can reach its service".into(),
            "whether a role's checkout or the product repository remains writable under that policy".into(),
        ],
        live: None,
    })
}

/// The disclosure an operator must see before a live probe runs.  It is
/// returned verbatim, and the probe does not run without the explicit
/// authorization that accompanies it.
pub fn live_preflight_disclosure(store: &Store, effort: &Effort) -> Result<String> {
    let build_dir = store.phase_dir(effort, "build")?;
    let config = effective_config(&build_dir)?.config;
    let mut roles = vec!["worker".to_owned(), "reviewer".to_owned()];
    roles.push(
        if config.unblocker.is_some() {
            "unblocker"
        } else {
            "reviewer (unblocker fallback)"
        }
        .into(),
    );
    if config.once_over.is_some() {
        roles.push("once_over".to_owned());
    }
    Ok(format!(
        "Live preflight dispatches real provider inferences for: {}. It spends the provider's own quota/tokens for each role, may take minutes, and writes only inside disposable fixtures created for the probe under the Build directory. It never touches the product checkout, Build state, credentials or provider configuration, and it never updates a CLI. Each worker probe is asked to commit inside its own fixture, so an edits-but-cannot-commit environment fails that probe.",
        roles.join(", ")
    ))
}

/// Live preflight against the real local adapters, as the CLI runs it.
pub fn live_preflight_with_local_adapters(
    store: &Store,
    effort: &Effort,
    authorized: bool,
) -> Result<LivePreflightReport> {
    live_preflight(store, effort, &crate::CommandAdapter, authorized)
}

/// Optional live preflight.  It runs only with explicit operator
/// authorization; without it the call refuses before any inference.
pub fn live_preflight(
    store: &Store,
    effort: &Effort,
    adapter: &dyn HostAdapter,
    authorized: bool,
) -> Result<LivePreflightReport> {
    let disclosure = live_preflight_disclosure(store, effort)?;
    ensure!(
        authorized,
        "live preflight needs explicit operator authorization before it may spend provider inference. {disclosure}"
    );
    let build_dir = store.phase_dir(effort, "build")?;
    let effective = effective_config(&build_dir)?;
    let config = &effective.config;
    let evidence_dir = build_dir
        .join("preflight")
        .join(format!("live-{}", crate::now_ms()));
    fs::create_dir_all(&evidence_dir)?;
    let mut probes = Vec::new();
    for (role, kind) in [
        ("worker", ProbeKind::Commit),
        ("reviewer", ProbeKind::ReadOnly),
        (
            if config.unblocker.is_some() {
                "unblocker"
            } else {
                "reviewer"
            },
            ProbeKind::ReadOnly,
        ),
    ] {
        if probes.iter().any(|probe: &LiveProbe| probe.role == role) {
            continue;
        }
        let probe = run_live_probe(store, effort, adapter, role, kind, &evidence_dir)?;
        probes.push(probe);
    }
    if config.once_over.is_some() {
        probes.push(run_live_probe(
            store,
            effort,
            adapter,
            "once_over",
            ProbeKind::ReadOnly,
            &evidence_dir,
        )?);
    }
    Ok(LivePreflightReport {
        authorized,
        disclosure,
        probes,
        evidence_dir: evidence_dir.to_string_lossy().into_owned(),
    })
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ProbeKind {
    /// The role must be able to change and commit inside its own fixture.
    Commit,
    /// The role must read, run a command, and leave no changes behind.
    ReadOnly,
}

/// A token unique to one probe run.  It is written only into the probe's own
/// fixture file, never into the prompt, so a role that reports it has
/// demonstrably read the fixture rather than produced a plausible answer.
fn probe_token(role: &str) -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let material = format!(
        "{role}|{nanos}|{}|{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    );
    format!(
        "orchestrate-probe-{}",
        &digest_bytes(material.as_bytes())[..16]
    )
}

/// One isolated owned Git fixture per probe.  It is created by the controller,
/// used by exactly one role, and never points at the product checkout.
fn live_fixture(evidence_dir: &Path, role: &str, token: &str) -> Result<PathBuf> {
    let fixture = evidence_dir.join(format!("fixture-{role}"));
    fs::create_dir_all(&fixture)?;
    for args in [
        vec!["init"],
        vec!["config", "user.email", "preflight@orchestrate.invalid"],
        vec!["config", "user.name", "Orchestrate preflight"],
    ] {
        fixture_git(
            &fixture,
            &args,
            &format!("prepare the {role} probe fixture"),
        )?;
    }
    fs::write(
        fixture.join("README.md"),
        format!("orchestrate preflight fixture\nprobe token: {token}\n"),
    )?;
    for args in [vec!["add", "."], vec!["commit", "-m", "preflight fixture"]] {
        fixture_git(&fixture, &args, &format!("commit the {role} probe fixture"))?;
    }
    Ok(fixture)
}

/// Run one fixture Git command with its output captured.  The CLI's stdout is
/// a single JSON value, so fixture bookkeeping may never inherit the process
/// stdout; a failure carries the captured stderr instead.
fn fixture_git(fixture: &Path, args: &[&str], what: &str) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(fixture)
        .output()
        .with_context(|| format!("cannot run git {args:?} in {}", fixture.display()))?;
    ensure!(
        output.status.success(),
        "cannot {what}: {}",
        String::from_utf8_lossy(&output.stderr).trim()
    );
    Ok(())
}

#[derive(Default)]
struct ProbeObserver {
    session_id: Option<String>,
}
impl InvocationObserver for ProbeObserver {
    fn accepted(&mut self) -> Result<()> {
        Ok(())
    }
    fn session_id(&mut self, session_id: &str) -> Result<()> {
        self.session_id = Some(session_id.to_owned());
        Ok(())
    }
}

/// Retain the exact non-secret launch facts for a live probe. The argument
/// vector comes from the same builder used by CommandAdapter; only the prompt
/// is elided. Provider environment contents and credentials are never stored.
fn retain_probe_invocation(
    invocation: &Invocation,
    observed_session: Option<&str>,
    completion: &str,
) -> Result<()> {
    let program = program_for(&invocation.adapter)?;
    let mut command_form = vec![program.to_owned()];
    command_form.extend(crate::provider_arguments(invocation)?);
    if let Some(last) = command_form.last_mut() {
        *last = "<prompt elided>".into();
    }
    // The controller injects no provider environment for any adapter, so the
    // launched provider inherits the launching environment and selects its
    // backend from its own configuration; neither is retained here.
    let environment = serde_json::json!({});
    let record = serde_json::json!({
        "schema_version": 1,
        "role": invocation.role,
        "adapter": invocation.adapter,
        "mode": if invocation.session_id.is_some() { "resumed" } else { "fresh" },
        "session": {
            "requested": invocation.session_id,
            "resumed": invocation.session_id.is_some(),
            "observed": observed_session,
        },
        "requested": attribute_provenance(&invocation.config),
        "mapped": {
            "arguments": attribute_arguments_for(&invocation.role, &invocation.config)?,
            "environment": environment,
        },
        "observed": {
            "model": observed_provider_model(
                &invocation.adapter,
                &invocation.action.join("transport.jsonl"),
            ),
            "reasoning_effort": Value::Null,
            "session": observed_session,
        },
        "executable": {
            "program": program,
            "path": resolve_executable(program).map(|path| path.to_string_lossy().into_owned()),
            "version": resolve_executable(program).as_deref().and_then(executable_version),
        },
        "command": {
            "form": command_form,
            "working_directory": invocation.cwd,
            "directory_grants": [invocation.build_dir],
            "prompt_sha256": digest_bytes(invocation.prompt.as_bytes()),
            "prompt_bytes": invocation.prompt.len(),
        },
        "completion": completion,
        "credential_note": "credential values are resolved by the local provider and are never retained here",
        "environment_note": "the controller sets no provider environment for this probe: the launched provider inherits the launching environment and selects its own backend from its own configuration",
    });
    write_bytes_sync(
        &invocation.action.join("invocation.json"),
        &serde_json::to_vec_pretty(&record)?,
    )
}

fn completion_category(result: &Result<crate::InvocationResult>) -> &'static str {
    match result {
        Ok(result) => match result.completion {
            InvocationCompletion::Completed => "completed",
            InvocationCompletion::SpawnFailed { .. } => "spawn_failed",
            InvocationCompletion::FailedBeforeAcceptance { .. } => "failed_before_acceptance",
            InvocationCompletion::AcceptedButIncomplete { .. } => "accepted_but_incomplete",
        },
        Err(_) => "adapter_error",
    }
}

fn run_live_probe(
    store: &Store,
    effort: &Effort,
    adapter: &dyn HostAdapter,
    role: &str,
    kind: ProbeKind,
    evidence_dir: &Path,
) -> Result<LiveProbe> {
    let build_dir = store.phase_dir(effort, "build")?;
    let config = effective_config(&build_dir)?.config;
    // The probe dispatches under the role's own effective configuration.
    let role_config = role_config_for_role(&config, role)?;
    let token = probe_token(role);
    let fixture = live_fixture(evidence_dir, role, &token)?;
    let action_dir = evidence_dir.join(format!("probe-{role}"));
    fs::create_dir_all(&action_dir)?;
    let report_path = action_dir.join("report.md");
    let prompt = match kind {
        ProbeKind::Commit => format!(
            "This is an Orchestrate preflight probe, not product work. In this disposable fixture: create a file named `preflight-{role}.txt` containing the single line `orchestrate preflight`, stage it, and commit it with the message `preflight {role} commit`. Change nothing else. In your final response, briefly report whether the commit succeeded."
        ),
        // The read probe must produce output a no-op cannot fake: the token is
        // in the fixture only, and the commit id is only obtainable by reading
        // or running a command in the fixture. The role must not write a report
        // because its sandbox is read-only; the controller retains its final
        // response from the transport log.
        ProbeKind::ReadOnly => (
            "This is an Orchestrate preflight probe, not product work. In this disposable fixture: read README.md and run `git rev-parse HEAD`. Change nothing at all and make no commit. Return exactly two lines in your final response: first the probe token from README.md, then the commit id printed by the command. Do not write a file."
        )
            .to_string(),
    };
    let before = git_head(&fixture);
    let invocation = Invocation {
        adapter: role_config.adapter.clone(),
        role: role.into(),
        config: role_config.clone(),
        cwd: fixture.clone(),
        build_dir: action_dir.clone(),
        action: action_dir.clone(),
        session_id: None,
        prompt,
    };
    let mut observer = ProbeObserver::default();
    let result = adapter.invoke(&invocation, &mut observer);
    retain_probe_invocation(
        &invocation,
        observer.session_id.as_deref(),
        completion_category(&result),
    )?;
    let fixture_limits = "this verifies capability inside an isolated controller-owned fixture only; it does not certify the product checkout, the effective permission policy, the accepted model or the requested reasoning effort";
    let mut evidence = vec![
        report_path.to_string_lossy().into_owned(),
        action_dir
            .join("transport.jsonl")
            .to_string_lossy()
            .into_owned(),
        action_dir
            .join("invocation.json")
            .to_string_lossy()
            .into_owned(),
        action_dir.to_string_lossy().into_owned(),
    ];
    let mut fresh_commit = None;
    let mut resumed_commit = None;
    let mut resumed_parent = None;
    let mut resumed_session_verified = false;
    let (outcome, detail) = match result {
        Ok(result) => {
            match result.completion {
                InvocationCompletion::Completed => {
                    let after = git_head(&fixture);
                    let retained = retained_probe_report(&action_dir, &report_path);
                    let report_present = retained
                        .as_deref()
                        .is_some_and(|text| !text.trim().is_empty());
                    if !report_present {
                        // No retained output means there is nothing to verify, so no
                        // capability is claimed from a silent process.
                        (
                            "no_probe_evidence",
                            format!(
                                "the role's process completed but no readable report was retained at {}; a probe with no output cannot demonstrate a capability. {fixture_limits}",
                                report_path.display()
                            ),
                        )
                    } else {
                        match kind {
                            ProbeKind::Commit => {
                                let file = fixture.join(format!("preflight-{role}.txt"));
                                let committed = file.is_file()
                                    && after != before
                                    && after.as_deref().is_some_and(|commit| {
                                        git_parent(&fixture, commit).as_deref() == before.as_deref()
                                            && git_file_at(
                                                &fixture,
                                                commit,
                                                &format!("preflight-{role}.txt"),
                                            )
                                            .as_deref()
                                                == Some("orchestrate preflight\n")
                                            && git_changed_paths(&fixture, commit).as_deref()
                                                == Some(
                                                    vec![format!("preflight-{role}.txt")]
                                                        .as_slice(),
                                                )
                                    })
                                    && git_dirty(&fixture).is_empty();
                                if committed {
                                    fresh_commit = after.clone();
                                    match observer.session_id.as_deref() {
                                        Some(session_id) => {
                                            let resume_dir = action_dir.join("resume");
                                            fs::create_dir_all(&resume_dir)?;
                                            let resume_prompt = format!(
                                                "Continue this same preflight session. In this same disposable Git fixture, create `preflight-{role}-resumed.txt` containing exactly `orchestrate resumed preflight`, stage only that file, and commit it with the message `preflight {role} resumed commit`. Do not amend or alter the first commit. Report the second commit id printed by `git rev-parse HEAD`."
                                            );
                                            let resumed = Invocation {
                                                adapter: role_config.adapter.clone(),
                                                role: role.into(),
                                                config: role_config.clone(),
                                                cwd: fixture.clone(),
                                                build_dir: action_dir.clone(),
                                                action: resume_dir.clone(),
                                                session_id: Some(session_id.to_owned()),
                                                prompt: resume_prompt,
                                            };
                                            let mut resume_observer = ProbeObserver::default();
                                            let resume_result =
                                                adapter.invoke(&resumed, &mut resume_observer);
                                            retain_probe_invocation(
                                                &resumed,
                                                resume_observer.session_id.as_deref(),
                                                completion_category(&resume_result),
                                            )?;
                                            evidence.extend([
                                                resume_dir
                                                    .join("transport.jsonl")
                                                    .to_string_lossy()
                                                    .into_owned(),
                                                resume_dir
                                                    .join("invocation.json")
                                                    .to_string_lossy()
                                                    .into_owned(),
                                                resume_dir.to_string_lossy().into_owned(),
                                            ]);
                                            match resume_result {
                                                Ok(completion)
                                                    if completion.completion
                                                        == InvocationCompletion::Completed =>
                                                {
                                                    let resumed_head = git_head(&fixture);
                                                    let parent = resumed_head.as_deref().and_then(
                                                        |commit| git_parent(&fixture, commit),
                                                    );
                                                    let contents = resumed_head
                                                        .as_deref()
                                                        .and_then(|commit| {
                                                            git_file_at(
                                                                &fixture,
                                                                commit,
                                                                &format!(
                                                                    "preflight-{role}-resumed.txt"
                                                                ),
                                                            )
                                                        });
                                                    let first_contents =
                                                        after.as_deref().and_then(|commit| {
                                                            git_file_at(
                                                                &fixture,
                                                                commit,
                                                                &format!("preflight-{role}.txt"),
                                                            )
                                                        });
                                                    if let (
                                                        Some(first),
                                                        Some(second),
                                                        Some(actual_parent),
                                                    ) = (after.clone(), resumed_head, parent)
                                                    {
                                                        let exact = actual_parent == first
                                                        && second != first
                                                        && first_contents.as_deref() == Some("orchestrate preflight\n")
                                                        && contents.as_deref() == Some("orchestrate resumed preflight\n")
                                                        && git_changed_paths(&fixture, &second).as_deref() == Some(vec![format!("preflight-{role}-resumed.txt")].as_slice())
                                                        && git_dirty(&fixture).is_empty()
                                                        && git_head(&fixture).as_deref() == Some(second.as_str());
                                                        if exact {
                                                            resumed_commit = Some(second.clone());
                                                            resumed_parent =
                                                                Some(actual_parent.clone());
                                                            resumed_session_verified = true;
                                                            (
                                                                "fresh_and_resumed_commits_verified",
                                                                format!(
                                                                    "the role made the first commit {first}, then resumed session {session_id} and made {second} with parent {actual_parent}; independent Git reads verified both file contents and the parent edge. {fixture_limits}"
                                                                ),
                                                            )
                                                        } else {
                                                            (
                                                                "resumed_commit_unverified",
                                                                format!(
                                                                    "the resumed call completed, but independent Git checks did not verify the expected second file/content and direct parent {first}; observed head={second}, parent={actual_parent}. {fixture_limits}"
                                                                ),
                                                            )
                                                        }
                                                    } else {
                                                        (
                                                            "resumed_commit_unverified",
                                                            format!(
                                                                "the resumed call completed, but Git did not expose the expected second commit and parent. {fixture_limits}"
                                                            ),
                                                        )
                                                    }
                                                }
                                                Ok(completion) => (
                                                    "resume_incomplete",
                                                    format!(
                                                        "the resumed provider action did not complete: {:?}. {fixture_limits}",
                                                        completion.completion
                                                    ),
                                                ),
                                                Err(error) => (
                                                    "resume_error",
                                                    format!(
                                                        "the resumed provider action failed: {error:#}. {fixture_limits}"
                                                    ),
                                                ),
                                            }
                                        }
                                        None => (
                                            "resume_session_missing",
                                            format!(
                                                "the first worker commit succeeded, but its transport did not provide a session id to resume; no resumed commit is claimed. {fixture_limits}"
                                            ),
                                        ),
                                    }
                                } else {
                                    (
                                        "cannot_commit",
                                        format!(
                                            "the role's process completed but the fixture has no committed probe change ({before:?} -> {after:?}); an environment that can edit files but cannot commit fails this probe. {fixture_limits}"
                                        ),
                                    )
                                }
                            }
                            ProbeKind::ReadOnly => {
                                let dirty = git_dirty(&fixture);
                                let text = retained.unwrap_or_default();
                                let read_proved = text.contains(&token);
                                let command_proved =
                                    after.as_deref().is_some_and(|commit| text.contains(commit));
                                if !dirty.is_empty() || after != before {
                                    (
                                        "modified_fixture",
                                        format!(
                                            "the probe left changes in its fixture: {dirty}; a review-only role must not modify its checkout. {fixture_limits}"
                                        ),
                                    )
                                } else if read_proved && command_proved {
                                    (
                                        "read_and_command_verified",
                                        format!(
                                            "the retained report carries both the fixture's own probe token and the commit id the command printed, and the fixture is unchanged at {after:?}; {fixture_limits}"
                                        ),
                                    )
                                } else {
                                    let missing = match (read_proved, command_proved) {
                                        (false, false) => {
                                            "neither the fixture's probe token nor the commit id the command prints"
                                        }
                                        (false, true) => {
                                            "the fixture's probe token, so the fixture file was not read"
                                        }
                                        _ => {
                                            "the commit id the requested command prints, so no comparable command output was produced"
                                        }
                                    };
                                    (
                                        "no_verifiable_probe_output",
                                        format!(
                                            "the role's process completed and left its fixture unchanged, but the retained report does not contain {missing}; a no-op or generic answer is not read/command capability, so none is claimed. {fixture_limits}"
                                        ),
                                    )
                                }
                            }
                        }
                    }
                }
                InvocationCompletion::SpawnFailed { detail } => ("cannot_launch", detail),
                InvocationCompletion::FailedBeforeAcceptance { detail } => {
                    ("failed_before_acceptance", detail)
                }
                InvocationCompletion::AcceptedButIncomplete { detail } => ("incomplete", detail),
            }
        }
        Err(error) => ("probe_error", format!("{error:#}")),
    };
    // Probe evidence is retained with the report and the observed fixture state.
    let evidence = vec![
        report_path.to_string_lossy().into_owned(),
        action_dir
            .join("transport.jsonl")
            .to_string_lossy()
            .into_owned(),
        action_dir.to_string_lossy().into_owned(),
    ];
    Ok(LiveProbe {
        role: role.into(),
        adapter: role_config.adapter.clone(),
        outcome,
        detail,
        fixture: fixture.to_string_lossy().into_owned(),
        evidence,
        fresh_commit,
        resumed_commit,
        resumed_parent,
        resumed_session_verified,
    })
}

/// Read provider-written probe output when available; otherwise preserve the
/// provider's completed final message from the controller-owned transport log.
/// Read-only roles cannot write a sidecar report under a read-only sandbox, so
/// making that write a prerequisite would defeat their command probe even when
/// the transport already contains the unique token and command result.
fn retained_probe_report(action_dir: &Path, report_path: &Path) -> Option<String> {
    if let Some(report) = fs::read_to_string(report_path)
        .ok()
        .filter(|report| !report.trim().is_empty())
    {
        return Some(report);
    }

    let messages = crate::transport_messages(&action_dir.join("transport.jsonl"));
    if messages.is_empty() {
        return None;
    }

    let report = messages.join("\n\n");
    write_bytes_sync(report_path, report.as_bytes()).ok()?;
    Some(report)
}

/// The effective configuration one role name runs under, so a probe cannot
/// dispatch under different attributes than the Build itself would use.
fn role_config_for_role<'a>(config: &'a BuildConfig, role: &str) -> Result<&'a RoleConfig> {
    match role {
        "worker" => Ok(&config.worker),
        "reviewer" => Ok(&config.reviewer),
        "unblocker" => Ok(match &config.unblocker {
            Some(unblocker) => unblocker,
            None => &config.reviewer,
        }),
        "once_over" => config
            .once_over
            .as_ref()
            .context("once_over is not configured"),
        other => bail!("unknown role {other}"),
    }
}

fn git_head(repo: &Path) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(repo)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git_parent(repo: &Path, commit: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["rev-list", "--parents", "-n", "1", commit])
        .current_dir(repo)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()?
        .split_whitespace()
        .nth(1)
        .map(str::to_owned)
}

fn git_file_at(repo: &Path, commit: &str, path: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["show", &format!("{commit}:{path}")])
        .current_dir(repo)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

fn git_changed_paths(repo: &Path, commit: &str) -> Option<Vec<String>> {
    let output = Command::new("git")
        .args(["diff-tree", "--no-commit-id", "--name-only", "-r", commit])
        .current_dir(repo)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(
        String::from_utf8(output.stdout)
            .ok()?
            .lines()
            .map(str::to_owned)
            .collect(),
    )
}

fn git_dirty(repo: &Path) -> String {
    Command::new("git")
        .args(["status", "--porcelain=v1", "--untracked-files=all"])
        .current_dir(repo)
        .output()
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|error| format!("<unreadable: {error}>"))
}

/// True when the Build directory holds a valid effective configuration, so the
/// CLI can fail a probe request before creating anything.
pub fn effective_config_summary(store: &Store, effort: &Effort) -> Result<(u32, BuildConfig)> {
    let build_dir = store.phase_dir(effort, "build")?;
    let effective = effective_config(&build_dir)?;
    Ok((effective.version, effective.config))
}

/// Write one preflight report as evidence beside the Build directory.
pub fn retain_report(store: &Store, effort: &Effort, report: &PreflightReport) -> Result<PathBuf> {
    let build_dir = store.phase_dir(effort, "build")?;
    let dir = build_dir.join("preflight");
    fs::create_dir_all(&dir)?;
    let path = dir.join(format!("preflight-{}.json", crate::now_ms()));
    write_bytes_sync(&path, &serde_json::to_vec_pretty(report)?)?;
    Ok(path)
}

/// Read back one retained preflight report, used by tests and status.
pub fn read_report(path: &Path) -> Result<serde_json::Value> {
    read_json(path)
}
