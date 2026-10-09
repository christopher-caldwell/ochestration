use crate::state::{Gate, RoleConfig};
use anyhow::Result;
#[cfg(test)]
use orchestrate_core::provider::extract_response;
pub use orchestrate_core::provider::{
    InvocationApi, InvocationOutcome, InvocationPlan, InvocationRecord, ProcessInvocationApi,
    Session, validate_native_args,
};
#[cfg(test)]
use serde_json::Value;
use std::path::Path;
#[derive(Default)]
pub struct RuntimeSessions {
    pub worker: Option<Session>,
}
// Claude's sandbox is the shell boundary; the Bash tool grant only avoids
// CLI approval requests, including for linked-worktree Git writes. Do not set
// blockReadsOutsideWorkingDirectories here: it also hid the user's installed
// Rust toolchain from sandboxed Cargo commands in a live probe.
const CLAUDE_WORKER_SANDBOX_SETTINGS: &str = r#"{
  "sandbox": {
    "enabled": true,
    "autoAllowBashIfSandboxed": true,
    "allowUnsandboxedCommands": false,
    "failIfUnavailable": true
  }
}"#;

pub fn prepare_invocation(
    config: &RoleConfig,
    gate: &Gate,
    prompt: &str,
    cwd: &Path,
    session: Option<&Session>,
) -> Result<InvocationPlan> {
    let native = orchestrate_core::provider::ProviderConfig {
        adapter: config.adapter.clone(),
        model: config.model.clone(),
        effort: config.effort.clone(),
        args: config.args.clone(),
    };
    let args = config.args.as_deref().unwrap_or_default();
    let policy = if config.adapter == "claude"
        && *gate == Gate::Work
        && !has_explicit_claude_permission_policy(args)
    {
        vec![
            "--permission-mode".into(),
            "acceptEdits".into(),
            "--permission-prompts".into(),
            "none".into(),
            "--settings".into(),
            CLAUDE_WORKER_SANDBOX_SETTINGS.into(),
        ]
    } else {
        vec![]
    };
    orchestrate_core::provider::prepare_invocation(&native, prompt, cwd, session, &policy)
}
fn has_explicit_claude_permission_policy(args: &[String]) -> bool {
    args.iter().any(|arg| {
        let flag = arg.split_once('=').map_or(arg.as_str(), |(flag, _)| flag);
        matches!(
            flag,
            "--permission-mode"
                | "--allowedTools"
                | "--allowed-tools"
                | "--dangerously-skip-permissions"
                | "--allow-dangerously-skip-permissions"
                | "--permission-prompts"
                | "--permission-prompt-tool"
                | "--settings"
                | "--setting-sources"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn worker_settings(argv: &[String]) -> Value {
        let settings = argv
            .windows(2)
            .find(|pair| pair[0] == "--settings")
            .expect("Claude Worker baseline has --settings");
        serde_json::from_str(&settings[1]).expect("Claude Worker settings are valid JSON")
    }

    fn assert_claude_worker_baseline(argv: &[String]) {
        assert!(
            argv.windows(2)
                .any(|pair| pair == ["--permission-mode", "acceptEdits"])
        );
        assert!(
            argv.windows(2)
                .any(|pair| pair == ["--permission-prompts", "none"])
        );
        // Sandboxed Bash is auto-allowed by `autoAllowBashIfSandboxed`. A blanket
        // allow rule would also approve `sandbox.excludedCommands`, which run
        // outside the sandbox (verified live against Claude Code 2.1.289).
        assert!(
            !argv
                .iter()
                .any(|arg| { matches!(arg.as_str(), "--allowedTools" | "--allowed-tools") })
        );
        let sandbox = worker_settings(argv)["sandbox"].clone();
        assert_eq!(sandbox["enabled"], true);
        assert_eq!(sandbox["autoAllowBashIfSandboxed"], true);
        assert_eq!(sandbox["allowUnsandboxedCommands"], false);
        assert_eq!(sandbox["failIfUnavailable"], true);
    }

    fn assert_no_claude_worker_baseline(argv: &[String]) {
        assert!(
            !argv
                .windows(2)
                .any(|pair| pair == ["--permission-mode", "acceptEdits"])
        );
        assert!(
            !argv
                .windows(2)
                .any(|pair| pair == ["--permission-prompts", "none"])
        );
    }

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
            effort: None,
            args: Some(vec!["--search".into()]),
        };
        let codex_session = Session {
            adapter: "codex".into(),
            id: "thread-1".into(),
        };
        let codex_call = prepare_invocation(
            &codex,
            &Gate::Work,
            "prompt",
            Path::new("/repo"),
            Some(&codex_session),
        )
        .unwrap();
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
            effort: None,
            args: None,
        };
        let claude_call = prepare_invocation(
            &claude,
            &Gate::Review,
            "prompt",
            Path::new("/repo"),
            Some(&codex_session),
        )
        .unwrap();
        assert_eq!(claude_call.record.session_in, None);
        assert!(!claude_call.record.argv.iter().any(|arg| arg == "--resume"));
        assert!(
            !claude_call
                .record
                .argv
                .iter()
                .any(|arg| arg == "--permission-mode")
        );
        assert_no_claude_worker_baseline(&claude_call.record.argv);

        let same_claude_session = Session {
            adapter: "claude".into(),
            id: "session-2".into(),
        };
        let claude_resumed = prepare_invocation(
            &claude,
            &Gate::Work,
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
        assert_eq!(claude_resumed.record.args, Vec::<String>::new());
        assert_claude_worker_baseline(&claude_resumed.record.argv);
    }

    #[test]
    fn claude_worker_permissions_are_defaulted_only_for_work_and_respect_explicit_policy() {
        let restricted = RoleConfig {
            adapter: "claude".into(),
            model: Some("claude-native-model".into()),
            effort: Some("high".into()),
            args: Some(vec!["--disallowedTools".into(), "Bash(git push *)".into()]),
        };
        let work = prepare_invocation(&restricted, &Gate::Work, "prompt", Path::new("/repo"), None)
            .unwrap();
        assert_eq!(work.record.args, ["--disallowedTools", "Bash(git push *)"]);
        assert!(
            work.record
                .argv
                .windows(2)
                .any(|pair| pair == ["--effort", "high"])
        );
        assert!(
            work.record
                .argv
                .windows(2)
                .any(|pair| pair == ["--permission-mode", "acceptEdits"])
        );
        assert!(
            work.record
                .argv
                .windows(2)
                .any(|pair| pair == ["--permission-prompts", "none"])
        );
        assert!(
            work.record
                .argv
                .windows(2)
                .any(|pair| pair[0] == "--settings")
        );
        assert!(
            !work
                .record
                .argv
                .iter()
                .any(|arg| matches!(arg.as_str(), "--allowedTools" | "--allowed-tools"))
        );
        assert!(
            work.record
                .argv
                .windows(2)
                .any(|pair| pair == ["--disallowedTools", "Bash(git push *)"])
        );
        let prompt_separator = work.record.argv.iter().position(|arg| arg == "--").unwrap();
        let defaults_end = work
            .record
            .argv
            .iter()
            .position(|arg| arg == "--settings")
            .unwrap()
            + 2;
        assert!(defaults_end < prompt_separator);
        assert_eq!(work.record.argv.last().unwrap(), "prompt");

        let explicit_policies = [
            vec!["--permission-mode".into(), "plan".into()],
            vec!["--permission-mode=plan".into()],
            vec!["--allowedTools".into(), "Read".into()],
            vec!["--allowedTools=Read".into()],
            vec!["--allowed-tools".into(), "Read".into()],
            vec!["--allowed-tools=Read".into()],
            vec!["--dangerously-skip-permissions".into()],
            vec!["--allow-dangerously-skip-permissions".into()],
            vec!["--permission-prompts".into(), "host".into()],
            vec![
                "--permission-prompt-tool".into(),
                "permissions.approve".into(),
            ],
            vec![
                "--settings".into(),
                r#"{"sandbox":{"enabled":false}}"#.into(),
            ],
            vec!["--settings={\"sandbox\":{\"enabled\":false}}".into()],
            vec!["--setting-sources".into(), "project".into()],
            vec!["--setting-sources=project".into()],
        ];
        for args in explicit_policies {
            let config = RoleConfig {
                adapter: "claude".into(),
                model: None,
                effort: None,
                args: Some(args.clone()),
            };
            let invocation =
                prepare_invocation(&config, &Gate::Work, "prompt", Path::new("/repo"), None)
                    .unwrap();
            assert_eq!(invocation.record.args, args);
            assert_no_claude_worker_baseline(&invocation.record.argv);
            let separator = invocation
                .record
                .argv
                .iter()
                .position(|arg| arg == "--")
                .unwrap();
            let native_args_start = separator - args.len();
            assert!(
                !invocation.record.argv[..native_args_start]
                    .iter()
                    .any(|arg| matches!(
                        arg.as_str(),
                        "--permission-mode"
                            | "--allowedTools"
                            | "--allowed-tools"
                            | "--dangerously-skip-permissions"
                            | "--allow-dangerously-skip-permissions"
                            | "--permission-prompts"
                            | "--permission-prompt-tool"
                            | "--settings"
                            | "--setting-sources"
                    )),
                "native policy args should suppress every injected Worker setting: {args:?}"
            );
            assert_eq!(
                &invocation.record.argv[separator - args.len()..separator],
                args
            );
        }

        for gate in [Gate::Review, Gate::Audit, Gate::Unblock] {
            let invocation =
                prepare_invocation(&restricted, &gate, "prompt", Path::new("/repo"), None).unwrap();
            assert!(!invocation.record.argv.iter().any(|arg| {
                matches!(arg.as_str(), "--permission-mode" | "--permission-prompts")
            }));
            assert_no_claude_worker_baseline(&invocation.record.argv);
            assert!(
                invocation
                    .record
                    .argv
                    .windows(2)
                    .any(|pair| pair == ["--disallowedTools", "Bash(git push *)"])
            );
        }
    }

    #[test]
    fn worker_permission_baseline_does_not_change_codex_or_cursor_invocations() {
        for adapter in ["codex", "cursor"] {
            let config = RoleConfig {
                adapter: adapter.into(),
                model: None,
                effort: None,
                args: None,
            };
            let invocation =
                prepare_invocation(&config, &Gate::Work, "prompt", Path::new("/repo"), None)
                    .unwrap();
            assert_no_claude_worker_baseline(&invocation.record.argv);
            assert!(!invocation.record.argv.iter().any(|arg| {
                matches!(
                    arg.as_str(),
                    "--permission-mode" | "--permission-prompts" | "--settings"
                )
            }));
            if adapter == "codex" {
                assert_eq!(&invocation.record.argv[..2], ["exec", "--json"]);
                assert_eq!(invocation.record.argv.last().unwrap(), "prompt");
            } else {
                assert_eq!(
                    &invocation.record.argv[..3],
                    ["-p", "--output-format", "stream-json"]
                );
                assert_eq!(invocation.record.argv.last().unwrap(), "prompt");
            }
        }
    }

    #[test]
    fn codex_effort_values_map_before_opaque_native_args() {
        for effort in [
            "none", "minimal", "low", "medium", "high", "xhigh", "max", "ultra",
        ] {
            let native_override = "model_reasoning_effort=\"low\"";
            let config = RoleConfig {
                adapter: "codex".into(),
                model: Some("model-that-build-does-not-inspect".into()),
                effort: Some(effort.into()),
                args: Some(vec!["-c".into(), native_override.into()]),
            };
            let invocation =
                prepare_invocation(&config, &Gate::Work, "prompt", Path::new("/repo"), None)
                    .unwrap();
            let translated = format!("model_reasoning_effort=\"{effort}\"");
            let effort_positions = invocation
                .record
                .argv
                .windows(2)
                .enumerate()
                .filter_map(|(index, pair)| {
                    (pair[0] == "-c" && pair[1].starts_with("model_reasoning_effort="))
                        .then_some(index)
                })
                .collect::<Vec<_>>();
            assert_eq!(effort_positions.len(), 2, "{effort}");
            assert!(effort_positions[0] < effort_positions[1], "{effort}");
            let model_index = invocation
                .record
                .argv
                .windows(2)
                .position(|pair| pair == ["--model", "model-that-build-does-not-inspect"])
                .unwrap();
            assert!(model_index < effort_positions[0], "{effort}");
            assert_eq!(invocation.record.argv[effort_positions[0] + 1], translated);
            assert_eq!(
                invocation.record.argv[effort_positions[1] + 1],
                native_override
            );
            assert_eq!(invocation.record.effort.as_deref(), Some(effort));
            let recorded = serde_json::to_value(&invocation.record).unwrap();
            assert_eq!(recorded["effort"], effort);
        }
    }

    #[test]
    fn claude_effort_values_map_before_opaque_native_args() {
        for effort in ["low", "medium", "high", "xhigh", "max", "ultracode"] {
            let config = RoleConfig {
                adapter: "claude".into(),
                model: Some("model-that-build-does-not-inspect".into()),
                effort: Some(effort.into()),
                args: Some(vec!["--effort".into(), "low".into()]),
            };
            let invocation =
                prepare_invocation(&config, &Gate::Work, "prompt", Path::new("/repo"), None)
                    .unwrap();
            let effort_positions = invocation
                .record
                .argv
                .windows(2)
                .enumerate()
                .filter_map(|(index, pair)| (pair[0] == "--effort").then_some(index))
                .collect::<Vec<_>>();
            assert_eq!(effort_positions.len(), 2, "{effort}");
            assert!(effort_positions[0] < effort_positions[1], "{effort}");
            let model_index = invocation
                .record
                .argv
                .windows(2)
                .position(|pair| pair == ["--model", "model-that-build-does-not-inspect"])
                .unwrap();
            assert!(model_index < effort_positions[0], "{effort}");
            assert_eq!(invocation.record.argv[effort_positions[0] + 1], effort);
            assert_eq!(invocation.record.argv[effort_positions[1] + 1], "low");
            assert_eq!(invocation.record.effort.as_deref(), Some(effort));
        }
    }

    #[test]
    fn unsupported_effort_values_fail_exactly_and_cursor_keeps_opaque_models() {
        for adapter in ["codex", "claude"] {
            for effort in ["HIGH", "High", "custom"] {
                let config = RoleConfig {
                    adapter: adapter.into(),
                    model: None,
                    effort: Some(effort.into()),
                    args: None,
                };
                let error =
                    prepare_invocation(&config, &Gate::Work, "prompt", Path::new("/repo"), None)
                        .unwrap_err();
                assert!(error.to_string().contains("unsupported"));
                assert!(error.to_string().contains(effort));
            }
        }

        let cursor_effort = RoleConfig {
            adapter: "cursor".into(),
            model: Some("claude-opus-4-8-thinking-high".into()),
            effort: Some("high".into()),
            args: None,
        };
        let error = prepare_invocation(
            &cursor_effort,
            &Gate::Work,
            "prompt",
            Path::new("/repo"),
            None,
        )
        .unwrap_err();
        assert!(
            error
                .to_string()
                .contains("does not support first-class effort")
        );

        let cursor_native_model = RoleConfig {
            adapter: "cursor".into(),
            model: Some("claude-opus-4-8-thinking-high".into()),
            effort: None,
            args: None,
        };
        let invocation = prepare_invocation(
            &cursor_native_model,
            &Gate::Work,
            "prompt",
            Path::new("/repo"),
            None,
        )
        .unwrap();
        assert!(
            invocation
                .record
                .argv
                .windows(2)
                .any(|pair| pair == ["--model", "claude-opus-4-8-thinking-high"])
        );
        assert_eq!(invocation.record.effort, None);
        assert!(serde_json::to_value(&invocation.record).unwrap()["effort"].is_null());
    }

    #[test]
    fn omitted_effort_adds_no_provider_effort_arguments() {
        for adapter in ["codex", "claude", "cursor"] {
            let config = RoleConfig {
                adapter: adapter.into(),
                model: None,
                effort: None,
                args: None,
            };
            let invocation =
                prepare_invocation(&config, &Gate::Review, "prompt", Path::new("/repo"), None)
                    .unwrap();
            assert!(!invocation.record.argv.iter().any(|arg| arg == "--effort"));
            assert!(
                !invocation
                    .record
                    .argv
                    .iter()
                    .any(|arg| arg.starts_with("model_reasoning_effort="))
            );
            assert_eq!(invocation.record.effort, None);
        }
    }
}
