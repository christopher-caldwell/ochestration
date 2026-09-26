# Live role completion and provider mappings

Date: 2026-09-25. This record covers the live observation that a real Build's Work, Review, advisory
once-over and formal Audit complete through production dispatch and receipt consumption, and the
provider mappings the dispatched roles used. Raw transcripts stayed outside the repository;
[redacted-invocations.json](redacted-invocations.json) retains the non-secret facts: requested
settings, mapped arguments, whether the controller injected any provider environment, the
provider-reported model where the stream carries one, and what the controller persisted from each
role's own response.

## Live Builds

Two whole Builds of the same fixture completed, one per read-only adapter, both
`BUILD COMPLETE` with a derived `PASS`:

| Run | Worker | Read-only roles | Result |
| --- | --- | --- | --- |
| 1 | claude `workspace_write` | cursor `read_only` (`--mode ask`) | `audit-cc453ec4ad9b566d` PASS; worker commit `32921c3c…` |
| 2 | claude `workspace_write` | codex `read_only` (real `sandbox_mode="read-only"`) | `audit-6196aae295219713` PASS; worker commit `63458c38…` |

In both, every read-only action's `result.json`, `report.md` and — for the Audit — `assessment.json`
were parsed from the role's own final response and persisted by the controller, which is what the
role's permission contract otherwise prevents. The Codex read-only roles ran under Codex's own
read-only sandbox, so nothing they "wrote" could have reached the filesystem.

### Run 1 detail: `claude` worker, `cursor` read-only roles

```
ORCHESTRATE_LIVE_WORKER_ADAPTER=claude ORCHESTRATE_LIVE_REVIEWER_ADAPTER=cursor \
  cargo test -p orchestrate --test e2e \
    opt_in_live_build_completes_through_read_only_role_evidence -- --ignored --nocapture
```

The fixture is a disposable store and repository with one binding requirement (`R-1`: `hello.txt`
contains exactly `hello`), one delivery phase and one task. Configuration: worker `claude`
`standard`/`workspace_write`; reviewer, unblocker and once-over `cursor` `standard`/`read_only`.

Result: `BUILD_COMPLETE`, formal Audit `audit-cc453ec4ad9b566d` derived `PASS` with a coverage row
that cites the registered snapshot and the verification checkout. Observations:

| Action | Role and mapping | Evidence |
| --- | --- | --- |
| work (`P1`) | claude, `--model deepseek-flash --permission-mode acceptEdits --settings {"sandbox":{"enabled":true,"allowUnsandboxedCommands":false}}` | Wrote `result.json` and `report.md` itself and committed `32921c3cd890358de8ba1fc3acc17cebff25f360`; the controller took nothing from its transport. |
| review (`P1`) | cursor, `--model gpt-5.5-low --trust --mode ask --sandbox enabled` | Wrote no file. The controller parsed `orchestrate-receipt` and `orchestrate-report` from its final response, persisted both, and validated the receipt against the exact action, scope and target commit. |
| once_over (`final`) | cursor, same mapping | Same path; the outcome `advisory_complete` was recorded from the persisted receipt. |
| final_audit (`final`) | cursor, same mapping | Same path plus `orchestrate-assessment`; the assessment's lineage was validated and `finalize_audit_with_run_id` published the verdict from it. |
| unblocker | cursor, `read_only` | Configured and statically preflighted, and live-probed in the preflight run below; not dispatched in either Build, because neither stopped. The read-only Unblock evidence path is covered deterministically (`read_only_once_over_and_unblock_complete_from_their_own_final_response`). |

Every dispatched role recorded `mapped.environment = {}`: the controller injected no provider
environment. The `claude` role's provider-reported model was `deepseek-flash` and the `cursor`
roles' was `GPT-5.5 272K Low`, both from the providers' own streams — the DeepSeek-backed Claude
Code configuration on this host is selected by that host's own configuration, not by the
controller. The product repository was left with the worker's commit and a clean status.

### Run 2 detail: the same Build with `codex` read-only roles

Same fixture and worker; reviewer, once-over and unblocker configured `codex`
`standard`/`read_only`, dispatched as
`--model gpt-5.5 -c model_reasoning_effort=low -c sandbox_mode="read-only" -c approval_policy="never"`.
Result: `BUILD_COMPLETE`, `audit-6196aae295219713` `PASS`, worker commit `63458c38…`. Each read-only
action persisted `result.json` and `report.md` from its own response, and the formal Audit its
`assessment.json` as well — from inside a real read-only sandbox, where a write would have failed.
`codex`'s stream does not report a model label, so `observed_model` is null there, as recorded.

## Finding: Cursor's `plan` mode cannot serve a role that owes a report

The first live Build of the same fixture stopped with `trigger = invalid_receipt` on its formal
Audit, and its advisory once-over was recorded as `invalid_receipt`. The transports show why, and
the cause is provider-side:

- Cursor's `--mode plan` is an approval flow. Both actions ended by emitting
  `interaction_query`/`createPlanRequestQuery` — "here is the plan, approve it" — which the
  headless CLI auto-answers with an empty plan. The turn ends there, so the role never stated its
  artifacts. The review in that same run happened to include its blocks and passed.
- `cursor-agent --help` documents both read-only modes: `plan` ("analyze, propose plans, no edits")
  and `ask` ("Q&A style for explanations and questions (read-only)").

The `read_only` mapping now selects `--mode ask`, which is read-only without an approval turn.
Re-validated: the existing live preflight for Cursor still reports `fresh_and_resumed_commits_verified`
for a worker and `read_and_command_verified` for the read-only roles, and the live Build above then
completed.

## Codex: the ChatGPT session was fine, the inherited proxy was not

A first attempt at the Codex criteria failed with
`unexpected status 401 Unauthorized: Incorrect API key provided: sk-svcac…` from
`https://chatgpt.com/backend-api/codex/responses`, for every Codex invocation including a plain
`codex exec` with no Orchestrate involved. That is *not* an account or credential problem, and
`~/.codex/auth.json` is not at fault: it reports `auth_mode: "chatgpt"`, `OPENAI_API_KEY: null`, and
unexpired ChatGPT tokens, and no OpenAI key exists in the shell environment or in `config.toml`.

The cause is the environment the command inherited. The Claude Code app's terminal panel exports
`HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY` pointing at its own local proxy, and requests to the Codex
responses endpoint through that proxy are answered with the key error above. Running the same
command with those variables removed works normally — `codex exec --json "…"` returns
`agent_message: ready` and `turn.completed`, and the Codex live runs above then pass:

```sh
env -u HTTPS_PROXY -u HTTP_PROXY -u ALL_PROXY -u https_proxy -u http_proxy \
  env ORCHESTRATE_LIVE_ADAPTER=codex cargo test -p orchestrate --test e2e \
    opt_in_live_provider_preflight_checks_commit_resume_and_roles -- --ignored --nocapture
```

Codex authenticated with its ChatGPT session throughout; no API key was configured, and no
credential or provider configuration was changed. Orchestrate does not scrub or set provider
network environment — the controller injects nothing for any adapter, and the proxy is an artifact
of the environment the command was launched from, not of a role's configuration.

With that environment, the Codex criteria are met: a Worker under the explicit
`permission = "full_access"` (`--dangerously-bypass-approvals-and-sandbox`) committed fresh
(`2f6e5e26…`) and then on the same resumed session (`f0ca3ff4…`, whose parent is the fresh commit),
and Reviewer/Unblocker probes under `sandbox_mode="read-only"` verified read and command capability
without changing the fixture.

## Live preflight for all three adapters

`opt_in_live_provider_preflight_checks_commit_resume_and_roles` passed for `claude`, `cursor` and
`codex`. Each run probes the worker (fresh commit, then a resumed-session commit) and the read-only
reviewer and unblocker, and compares the invocation record's mapping against the static preflight
report. Outcomes: `fresh_and_resumed_commits_verified` for each worker,
`read_and_command_verified` for each read-only role, and `mapped.environment = {}` everywhere. The
Claude runs report `deepseek-flash` and the Cursor runs `GPT-5.5 272K Low` from the providers' own
streams; Codex's stream reports no model label. All of it is in
[redacted-invocations.json](redacted-invocations.json).

## Deterministic coverage of the same paths

The live Build exercises one worker, one reviewer, one advisory once-over and one formal Audit with
no stop. The following deterministic tests cover the branches a passing live run does not reach;
their adapters are fakes, so they prove controller behavior and not model behavior.

- `read_only_roles_complete_a_build_from_their_own_final_response` — a whole Work → Review → Audit
  Build in which no read-only role writes a file.
- `read_only_once_over_and_unblock_complete_from_their_own_final_response` — a blocked Audit, the
  read-only diagnosis that authorizes exactly one reassessment, and the advisory once-over.
- `a_read_only_response_without_a_valid_receipt_stops_instead_of_being_inferred` — a prose-only
  response stops with `invalid_receipt`; no verdict is published.
- `a_written_receipt_outranks_a_transported_block` — a file a role wrote is never overridden by a
  response block.
- `marked_blocks_are_found_by_fence_rules` — the exact info string, the closing-fence rule and the
  last-complete-block rule.
