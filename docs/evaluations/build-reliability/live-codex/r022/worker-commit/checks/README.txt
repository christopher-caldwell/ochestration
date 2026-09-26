Relevant checks for the live Worker-commit validation and its defect fix
Run 2026-09-25T20:24:28Z on the working tree at 946787b plus the uncommitted build-reliability changes,
with SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk (host linking workaround, not a code fact).

## Full workspace suite (final tree, temporary harness removed, rustfmt applied)
  running 2 tests	test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  running 11 tests	test result: ok. 11 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.33s
  running 31 tests	test result: ok. 31 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 7.18s
  running 0 tests	test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  running 104 tests	test result: ok. 104 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 58.91s
  running 12 tests	test result: ok. 12 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  running 2 tests	test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.24s
  running 6 tests	test result: ok. 6 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  running 4 tests	test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  running 0 tests	test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  running 0 tests	test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  running 0 tests	test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  running 0 tests	test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  running 0 tests	test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  running 0 tests	test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  running 0 tests	test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  running 0 tests	test result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.00s
  process exit code: 0 (no failures in any binary); full log: checks/full-suite.log

## Regression test for the stdout defect, pre-fix and post-fix
  test: crates/cli/tests/e2e.rs::live_preflight_keeps_stdout_a_single_json_value
  pre-fix (fixture Git output inherited stdout): exit 101, FAILED
    live preflight stdout was not one JSON value (expected value at line 1 column 1): Initialized empty Git repository in /private/tmp/claude-502/orchestrate-live-p
  post-fix: exit 0, 1 passed (checks/regression-test-post-fix.log)

## Live resume harness (temporary, run once)
  run: cargo test -p orchestrate-build --lib live_worker_resume_commits_through_the_configured_adapter -- --ignored --nocapture
  result: 1 passed (see ../resume/harness.log); source retained at ../settings/live-resume-harness.rs
