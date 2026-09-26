# Live Worker commit and resume through the normal configuration → adapter → provider path

Date: 2026-09-25. This record closes R-022's live Worker-commit criterion and
validates a resumed Worker dispatch on the same session, both through the
production path: the effort's own Build configuration, mapped by
`attribute_arguments`, launched by the real `codex` executable. No launcher,
wrapper or sandbox injection is on the execution path; every provider call was
bounded, explicitly authorized, and directed at disposable controller-owned Git
fixtures inside an isolated orchestration store. The product repository and its
history, the interrupted live Build effort, global provider configuration and
credentials were not changed.

## Requested configuration

The effort's `build/config.toml` (retained at [settings/config.toml](settings/config.toml)):

| role | adapter | model | reasoning_effort | permission |
| --- | --- | --- | --- | --- |
| worker | codex | gpt-5.6-luna | medium | full_access |
| reviewer | codex | gpt-5.6-sol | low | omitted |
| unblocker | codex | gpt-5.6-sol | low | omitted |
| once_over | — | — | — | not configured |

These are *requested* settings. What the provider attested is in the results;
what remains unattested is listed under [Limits](#limits).

## Environment

- Source-built CLI: `orchestrate 0.0.1`, `target/debug/orchestrate`,
  sha256 `3501dfbc9d2d043f8c59f8a0b3ad30f8ee389c80fb95d7c7098af511210663c9`
  (working tree at `946787b` plus the uncommitted build-reliability changes).
- Provider executable on the launch PATH: `/Users/christophercaldwell/.nvm/versions/node/v22.13.1/bin/codex`,
  sha256 `61b0194f3bb6534439c8d26a3ed57d0805f84b884588b761795323eeb92fcf70`,
  `codex-cli 0.156.1` (`codex --version`). No `temporary-codex-launcher` or other
  wrapper was on PATH in the environment that ran the validation.
- Isolated store: `<TMPDIR>/orch-live-worker-commit/store` (`$TMPDIR` on this host
  is `/private/tmp/claude-502`), effort `live-worker-commit`
  (`effort-cc4061f79de28a23`), initialized against a disposable fixture project
  repo. Nothing under `~/.orchestration` was read or written for this effort.
- Provider sessions were persisted by the host's normal Codex session record,
  because the resume test requires the exact fresh session; no provider
  configuration or credential was changed.

Full environment facts, including the PATH the CLI ran under:
[settings/env-facts.txt](settings/env-facts.txt) and
[settings/terminal-env.txt](settings/terminal-env.txt).

## Emitted provider arguments

Static `build preflight` (the same `attribute_arguments` mapping dispatch uses)
reported, for the worker:

```
--model gpt-5.6-luna -c model_reasoning_effort=medium --dangerously-bypass-approvals-and-sandbox
```

with `explicit_full_access: true`; reviewer and unblocker reported no permission
argument at all, so a configuration that omits `permission` is never escalated.
Full reports: [static-preflight.json](static-preflight.json) and, for the
pre-fix run, [attempt1-pre-fix/static-preflight.json](attempt1-pre-fix/static-preflight.json).

The flag is the installed CLI's own provider-native argument; `codex exec --help`
and `codex exec resume --help` both document it
([settings/codex-flag-help.txt](settings/codex-flag-help.txt)):

```
--dangerously-bypass-approvals-and-sandbox
    Skip all confirmation prompts and execute commands without sandboxing.
```

The full fresh-dispatch argv is that mapping appended to the adapter's
`exec --json -C <cwd> --add-dir <build_dir>` prefix. The resumed dispatch's exact
argv, captured from the production `provider_command` construction by the resume
harness, is retained at [resume/emitted-arguments.json](resume/emitted-arguments.json):

```
codex exec --json -C <fixture-worker> --add-dir <resume-worker> \
  resume 01a0da36-77c1-70e2-9c4a-02f342ab7163 \
  --model gpt-5.6-luna -c model_reasoning_effort=medium \
  --dangerously-bypass-approvals-and-sandbox <prompt>
```

## Exact commands

Static preflight (works with no provider call):

```
orchestrate --root <store> build preflight --effort live-worker-commit
```

Live preflight, run from the operator's own shell so the provider could reach its
service (the validating session's Bash sandbox denies the provider's network
egress and its `~/.codex` state writes):

```
orchestrate --root /private/tmp/claude-502/orch-live-worker-commit/store \
  build preflight --effort live-worker-commit --live --authorize-live
```

Resume, through the production adapter (`CommandAdapter::invoke`, the same call
the Build driver makes when it re-dispatches a Work action with the session it
recorded):

```
ORCHESTRATE_LIVE_STORE=<store> ORCHESTRATE_LIVE_EFFORT=live-worker-commit \
ORCHESTRATE_LIVE_FIXTURE=<live>/fixture-worker \
ORCHESTRATE_LIVE_ACTION=<live>/resume-worker \
ORCHESTRATE_LIVE_SESSION=01a0da36-77c1-70e2-9c4a-02f342ab7163 \
cargo test -p orchestrate-build --lib \
  live_worker_resume_commits_through_the_configured_adapter -- --ignored --nocapture
```

The harness was temporarily added to the `mod tests` body in
`crates/build/src/lib.rs` so it could call the private production adapter; it was
removed again after the run. Its exact source is retained at
[settings/live-resume-harness.rs](settings/live-resume-harness.rs).

## Results

All three live probes completed with provider exit code 0 and empty provider
stderr; each fixture's controller identity authored its commits, so the commits
could not have come from the operator's own Git identity.

| probe | controller outcome | provider exit |
| --- | --- | --- |
| worker (fresh) | `worker_commit_verified` | 0 |
| reviewer | `read_and_command_verified` | 0 |
| unblocker | `read_and_command_verified` | 0 |

Controller result (stdout is a single JSON value, `semantic_outcome: READ_ONLY`):
[live-preflight-result.json](live-preflight-result.json). Raw provider
transports, retained reports, exit records and stderr for each role:
[probe-worker/](probe-worker/), [probe-reviewer/](probe-reviewer/),
[probe-unblocker/](probe-unblocker/).

### Fresh Worker

- Fixture baseline `8ec6c77edb99b106754b15201358ccf599f317f3` (`preflight fixture`);
  fresh commit `4fc39df2b08c738e61180aee1f423255403caf1e` (`preflight worker commit`),
  one file changed, `preflight-worker.txt`, content exactly `orchestrate preflight\n`.
- Fixture working tree clean after the probe.
- Session id on the transport: `01a0da36-77c1-70e2-9c4a-02f342ab7163`.
- The worker's own report (`probe-worker/report.md`): "Commit succeeded:
  `4fc39df preflight worker commit`. Only `preflight-worker.txt` was added."

### Resumed Worker

- Resumed the exact fresh session id; the resumed transport's `thread_id` is the
  same `01a0da36-77c1-70e2-9c4a-02f342ab7163`, and the adapter's session observer
  reported the same id (see `resume/harness.log`).
- Resumed commit `36e3bf06f302fa254688bd6257dc010d8544aaa6` (`resume worker commit`),
  parent is the fresh commit, one file changed, `resume-worker.txt`, content
  exactly `resume worker commit\n`; fixture working tree clean.
- Configured attributes at resume time were read from the effort's own effective
  configuration (config version 1, unchanged since the fresh dispatch): codex /
  gpt-5.6-luna / medium / full_access — see `resume/emitted-arguments.json`.
- Raw transport, exit record and stderr: [resume/](resume/); harness output
  including `head_before`/`head_after`: [resume/harness.log](resume/harness.log).

### Independent verification

Both commits were verified with Git by the validating session, not by the
provider or the controller: commit ids and parents, per-commit file lists, exact
file bytes from `git cat-file`, clean `git status --porcelain`, and a re-check
from the retained self-contained Git bundle (including file sha256) —
[independent-verification.txt](independent-verification.txt). The fixture's own
files and the bundle are retained under [fixtures/](fixtures/).

## Defect found and fixed

The live run exposed a real defect: the preflight's fixture bookkeeping (`git
init` and `git commit` for each fixture) inherited the CLI's stdout, so
`build preflight --live --authorize-live` printed Git chatter before its JSON
result and could not be parsed as one JSON value. The prior retained evidence had
the same leak (`../read-only-transport-rerun/cli-stdout.log`), where it had to be
worked around by hand when extracting the JSON.

The fix captures the fixture Git commands' output in `live_fixture`
(`crates/build/src/preflight.rs`) and reports a failure with the captured stderr
instead of writing it to the process stdout. Regression coverage:
`live_preflight_keeps_stdout_a_single_json_value` in `crates/cli/tests/e2e.rs`
runs the real CLI with a stub `codex` on PATH, so no inference is needed; it
failed before the fix with the exact Git chatter shown in
[attempt1-pre-fix/controller-stdout-raw.log](attempt1-pre-fix/controller-stdout-raw.log)
and passes after. The pre-fix live run is retained under
[attempt1-pre-fix/](attempt1-pre-fix/): it also committed (`6eb7015`) and is what
produced the stale-stdout evidence.

## Limits

1. **Requested versus provider-attested.** The transport attests the session id,
   exit status, event completion and usage; it does not report the effective
   model, effective reasoning effort, or effective sandbox/approval policy. The
   flags were accepted (the CLI documents them and the calls completed), but
   acceptance is not attestation.
2. **This host's Codex default is already full access.** The host's own
   `~/.codex/config.toml` sets `sandbox_mode = "danger-full-access"` and
   `approval_policy = "never"`. The explicit mapping is what the validation
   checks — the correct provider-native argument is emitted from configuration
   and the provider performs the commit under it — but the run cannot separate
   "the explicit flag did it" from "the host default would have allowed it too".
   What the earlier runs do show is that the probe discriminates: with an
   injected `workspace-write` policy the same probe could not commit
   (`../read-only-transport-rerun/`).
3. **Reviewer and unblocker probes do not prove read-only enforcement.** With
   `permission` omitted, no privilege flag is added (that is the preserved
   behaviour), so those roles ran under the host's provider default. Their
   `read_and_command_verified` results prove they read the fixture token and
   returned the exact `git rev-parse HEAD` output while leaving the fixture
   unchanged; they do not prove a read-only sandbox would have prevented writes.
4. **Single observations.** Each role ran once per attempt on disposable
   fixtures; these are capability observations on this host and CLI version, not
   guarantees of model behaviour.
5. **Provider-side host effects.** Codex persisted its normal session records
   under `~/.codex` (required for the resume test) and this host's configured
   Codex `notify` hook may have run on turn end; no provider configuration,
   credential or the product repository was changed.

Provider-reported usage for every call in this validation is recorded in
[../../usage-metadata.json](../../usage-metadata.json).
