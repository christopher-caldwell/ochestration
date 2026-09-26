// Retained source of the temporary live resume harness used for the
// 2026-09-25 Worker-commit validation (see ../README.md).  It was added to the
// `mod tests` body in crates/build/src/lib.rs so it could reach the private
// production `CommandAdapter`, run once with `--ignored`, and then removed
// again; it is kept here so the exact validation step is reproducible.
//
// Run (from the repository root, with a real provider available):
//   ORCHESTRATE_LIVE_STORE=<isolated store root> \
//   ORCHESTRATE_LIVE_EFFORT=<effort id> \
//   ORCHESTRATE_LIVE_FIXTURE=<disposable fixture cwd> \
//   ORCHESTRATE_LIVE_ACTION=<retention directory> \
//   ORCHESTRATE_LIVE_SESSION=<session id from the fresh worker invocation> \
//   cargo test -p orchestrate-build --lib \
//     live_worker_resume_commits_through_the_configured_adapter -- --ignored --nocapture

    /// Live resume harness for the Worker path (validation scaffolding, not part
    /// of the suite): it resumes the exact session a fresh Worker invocation
    /// returned, through the same production adapter and the effort's own
    /// configured role attributes, and requires the resumed turn to commit its
    /// second change.  Ignored by default; it spends real provider inference and
    /// needs an authorized, isolated effort.  Environment:
    ///   ORCHESTRATE_LIVE_STORE    isolated orchestration store root
    ///   ORCHESTRATE_LIVE_EFFORT   effort id inside that store
    ///   ORCHESTRATE_LIVE_FIXTURE  disposable Git fixture used as the worker cwd
    ///   ORCHESTRATE_LIVE_ACTION   retention directory for this invocation
    ///   ORCHESTRATE_LIVE_SESSION  session id the fresh Worker invocation returned
    #[test]
    #[ignore = "live provider validation; needs explicit authorization and a real session"]
    fn live_worker_resume_commits_through_the_configured_adapter() {
        struct Recorder(Option<String>);
        impl InvocationObserver for Recorder {
            fn accepted(&mut self) -> Result<()> {
                Ok(())
            }
            fn session_id(&mut self, session_id: &str) -> Result<()> {
                self.0 = Some(session_id.to_owned());
                Ok(())
            }
        }
        fn head(repo: &Path) -> String {
            let output = Command::new("git")
                .args(["rev-parse", "HEAD"])
                .current_dir(repo)
                .output()
                .unwrap();
            assert!(output.status.success(), "cannot read the fixture HEAD");
            String::from_utf8_lossy(&output.stdout).trim().to_owned()
        }
        fn show(repo: &Path, commit: &str) -> String {
            let output = Command::new("git")
                .args(["show", "--name-only", "--format=%s", commit])
                .current_dir(repo)
                .output()
                .unwrap();
            assert!(output.status.success(), "cannot read the fixture commit");
            String::from_utf8_lossy(&output.stdout).into_owned()
        }
        let variable = |name: &str| {
            std::env::var(name)
                .unwrap_or_else(|_| panic!("{name} is required by the live resume harness"))
        };
        let store = Store::open(variable("ORCHESTRATE_LIVE_STORE")).unwrap();
        let effort = store
            .load_effort(&variable("ORCHESTRATE_LIVE_EFFORT"))
            .unwrap();
        let build_dir = store.phase_dir(&effort, "build").unwrap();
        // The resumed dispatch runs under the effort's own effective worker
        // configuration, exactly as a re-dispatched Work action would.
        let worker = effective_config(&build_dir).unwrap().config.worker;
        let fixture = PathBuf::from(variable("ORCHESTRATE_LIVE_FIXTURE"));
        let action = PathBuf::from(variable("ORCHESTRATE_LIVE_ACTION"));
        fs::create_dir_all(&action).unwrap();
        let requested_session = variable("ORCHESTRATE_LIVE_SESSION");
        let invocation = Invocation {
            adapter: worker.adapter.clone(),
            role: "worker".into(),
            config: worker.clone(),
            cwd: fixture.clone(),
            build_dir: action.clone(),
            action: action.clone(),
            session_id: Some(requested_session.clone()),
            prompt: "This is an Orchestrate preflight resume probe, not product work. In this disposable fixture, create a file named `resume-worker.txt` containing the single line `resume worker commit`, stage it, and commit it with the message `resume worker commit`. Change nothing else. In your final response, briefly report whether the commit succeeded.".into(),
        };
        let arguments = provider_arguments(&invocation).unwrap();
        fs::write(
            action.join("emitted-arguments.json"),
            serde_json::to_vec_pretty(&serde_json::json!({
                "program": preflight::program_for(&invocation.adapter).unwrap(),
                "arguments": arguments,
                "requested_session": requested_session,
                "configured": {
                    "adapter": worker.adapter,
                    "model": worker.model,
                    "reasoning_effort": worker.reasoning_effort,
                    "permission": worker.permission,
                },
            }))
            .unwrap(),
        )
        .unwrap();
        let before = head(&fixture);
        let mut recorder = Recorder(None);
        let result = CommandAdapter.invoke(&invocation, &mut recorder).unwrap();
        let after = head(&fixture);
        let committed = after != before && show(&fixture, &after).contains("resume-worker.txt");
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "completion": format!("{:?}", result.completion),
                "requested_session": requested_session,
                "observed_session": recorder.0,
                "head_before": before,
                "head_after": after,
                "resume_commit_contains_expected_file": committed,
            }))
            .unwrap()
        );
        assert_eq!(result.completion, InvocationCompletion::Completed);
        assert_eq!(
            recorder.0.as_deref(),
            Some(requested_session.as_str()),
            "the resumed transport did not attest the requested session"
        );
        assert!(
            committed,
            "the resumed turn did not commit its expected file"
        );
    }
