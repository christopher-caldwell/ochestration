# Orchestrate implementation readiness

Date: 2026-09-25. This addendum records the completed provider-neutral configuration, bounded
provider validation, read-only role evidence, historical recovery validation, installation, and
portability review. The requirement matrix and detailed Codex observations remain in
[requirement-evidence.md](requirement-evidence.md) and [live-codex/](live-codex/). Live role
completion and the historical recovery exercise are in [live-roles/](live-roles/) and
[live-recovery/](live-recovery/). Concrete adapter contracts are in
[provider-mappings.md](../../guides/provider-mappings.md).

| Readiness area | Evidence |
| --- | --- |
| Human-controlled Build CLI | `$build` dispatcher plus embedded/versioned guides prohibit implicit CLI calls; Build help has a non-executing `prepare` path. Focused coverage is in `crates/cli/tests/architecture.rs` and `crates/cli/tests/e2e.rs`. Every discoverable Build dispatcher on this host matches the checkout (see Installation). |
| Neutral settings and compatibility | Schema-v3 mappings live at the execution edge in `attribute_arguments_for`; `role_attributes_route_exactly_and_unsupported_values_fail_clearly` covers mapping and rejection, including the explicit `full_access` value. Frozen schema-v2 config remains readable without in-place migration. |
| Read-only role completion | Two live Builds completed Work, Review, advisory once-over and formal Audit through production dispatch — one with `cursor` read-only roles, one with `codex` read-only roles inside a real read-only sandbox — with each role's receipts, reports and assessment persisted from its own final response ([live-roles/](live-roles/)). Deterministic tests cover the stop and precedence branches. |
| Provider setup and live capability | Claude Code/DeepSeek, Cursor and Codex roles verified live with no controller-injected provider environment; redacted production adapter arguments are retained in [live-roles/redacted-invocations.json](live-roles/redacted-invocations.json). |
| Historical recovery/status | The actual interrupted records were migrated and continued on an isolated, timestamp-preserving copy, with the original files proven unchanged and one compatibility defect fixed ([live-recovery/](live-recovery/)). |
| Installation and portability | Installed PATH binary/help and six dispatchers verified in all three host skill directories. Rust source/manifests/scripts contain no host-OS branches. |

## Provider-neutral configuration

The schema-v3 vocabulary is `adapter`, `model_strength` (`standard` or `strong`),
`reasoning_effort` (`low`, `medium`, or `high`), and `permission` (`read_only`, `workspace_write`,
or the explicit `full_access`). Formal Audit uses the Reviewer configuration. This example is
provider-neutral at the role level and uses combinations verified by static preflight:

```toml
schema_version = 3

[worker]
adapter = "cursor"
model_strength = "standard"
reasoning_effort = "medium"
permission = "workspace_write"

[reviewer]
adapter = "codex"
model_strength = "standard"
reasoning_effort = "low"
permission = "read_only"

[unblocker]
adapter = "claude"
model_strength = "standard"
reasoning_effort = "medium"
permission = "read_only"
```

| Adapter and installed version | Requested generic settings | Production mapping and live result |
| --- | --- | --- |
| Claude Code 2.1.282 backed by the host's configured DeepSeek endpoint | Worker `standard / medium / workspace_write`; Reviewer and Unblocker `read_only` | Worker requested `deepseek-flash`, `--effort high`, `--permission-mode acceptEdits`, and Claude sandbox settings with unsandboxed commands denied. Reviewer and Unblocker used `--permission-mode plan` under the same sandbox. Fresh commit `9213aa164594057abe3d2735bf5b92081844e3ca`; resumed-session commit `77fa6699de69a948161a27d58da765852b9faee5` directly followed it. Re-verified 2026-09-25 through the production adapter with no controller-injected environment: the provider's own stream reported `deepseek-flash` for every role, fresh and resumed. |
| Cursor CLI 2026.09.23-86fc751 | Worker `standard / medium / workspace_write`; Reviewer and Unblocker `read_only` | Worker requested `--model gpt-5.5-medium --trust --force --sandbox enabled`; fresh and resumed commits `24c750d6…` and `bf8b5ccc…` directly followed one another. Re-verified 2026-09-25 with the read-only mapping now `--trust --mode ask --sandbox enabled`: `fresh_and_resumed_commits_verified` for the worker and `read_and_command_verified` for the read-only roles, with no injected environment. Cursor's other read-only mode, `plan`, ends its turn requesting plan approval, which a role whose deliverable is its own report never receives; see [live-roles/](live-roles/). |
| Codex CLI 0.156.1 | Reviewer and Unblocker `read_only`; a Worker needs the explicit `full_access` | Static mapping uses `exec --json` with `sandbox_mode=read-only`, `approval_policy=never`, and the configured reasoning-effort override; `full_access` maps to `--dangerously-bypass-approvals-and-sandbox` on fresh and resumed calls alike. `workspace_write` for a Codex Worker is rejected before dispatch: Codex protects Git metadata in `workspace-write`, so that narrow pairing cannot meet the commit contract, and the error names `full_access` as the explicit alternative rather than widening the permission. Observed 2026-09-25: a Worker under `full_access` committed fresh (`2f6e5e26…`) and on the same resumed session (`f0ca3ff4…`), and a whole Build ran its Review, once-over and formal Audit under `read_only` inside Codex's own sandbox. Codex then returned `401 Unauthorized` on a later attempt; the cause was the Claude Code app's proxy environment inherited by the launching shell, not the account — see [live-roles/](live-roles/). |

For every commit-capable provider that was observed, the production adapter made the fresh and
resumed calls, the resume used the session reported by the fresh call, and Git independently
verified each commit's parent, exact fixture files/content, and final status. No temporary launcher
injected permission flags and no human committed on the Worker's behalf. Invocation evidence
retains requested values, redacted argument arrays, the environment the controller set (nothing, for
every adapter), and the provider-reported model where the stream provides it;
[the retained argument record](live-roles/redacted-invocations.json) omits prompts, session
identifiers, credentials, and account identity.
The Claude CLI's accepted flags describe Claude Code's local controls, not all backend capabilities;
the backend is whichever one that host's Claude Code configuration selects, and DeepSeek's
Anthropic-compatible endpoint and effort behavior are documented in the provider mapping guide.

Schema-v2 frozen configurations remain readable without mutation. New schema-v3 configurations use
an explicit recorded overlay for a stopped Build; no native model or permission is guessed during
migration. Unsupported adapter/value pairs stop before dispatch with the role, field, value, and
adapter in the error.

## Historical recovery validation

The original Build `effort-7c0de108b37c9d6c` was never resumed, edited or migrated, then and now.
Its immutable state, config, plan, and Reconciled artifact digests are respectively:

```text
0337e3a80d3796cf7487b73220a883fb33243c3ff12e6ccb4a78414a6d666fd3
f13474b58463866817ae5192dcd410bfe207e0290ffc94a454ea3115c687f93f
80e9e13c409b53d4b32bca50ac9e407767d820c7051f2e0dcafd824703bccb4a
b0b47c8706698347b49249c6397e72725f8529f5c8a8b3ed1ed691342ad222e2
```

The supported authority-amendment operation recorded refusal `amend-aa465390f0c71227ddc91aa7`,
which authorizes a successor rather than editing frozen requirements.

**Correction to the earlier record.** An earlier pass reported that "there was no schema-v3 state
migration to perform". That was wrong: the historical `state.json` is schema 3 but carries no
`migration_version`, and the additive migration applies to it. It also validated only that
resolution was refused because no durable stop existed, without exercising migration or
continuation.

The migration and continuation are now exercised against an isolated copy of the actual records at
`/private/tmp/orchestrate-historical-recovery-20260925`: the store records copied byte for byte with
timestamps, a disposable clone of the product repository, and the copied project record repointed at
that clone. The original effort was not opened by a controller. The sequence, driven by
`crates/build/tests/historical_recovery.rs` with a deterministic adapter instead of a provider:

1. The first start recorded the interrupted acceptance as a durable stop
   (`trigger = uncertain_acceptance`) and dispatched nothing, so the uncertain action was not resent
   and no completion marker appeared.
2. Migration preserved the original state bytes at `build/evidence/state-v3-original.json`, journaled
   one `build_state_migrated` event, set `migration_version = 1`, and kept the frozen plan and config
   digests, the accepted phase boundary, and the interrupted action's partial transport.
3. A resolution without `--confirm-not-running` was refused; the same resolution with it applied as
   `existing_authority_clarification` and created a distinct continuation action for the interrupted
   scope.
4. The continuation completed the interrupted scope and the remaining phases and reached
   `BUILD COMPLETE` with a derived `Pass`. No already-accepted phase was replayed.
5. Repeating the resolution was idempotent: one record, one journal event, one continuation.

The exercise found a real compatibility defect and it is fixed: a frozen plan that writes its
requirement list as prose (`…, R-041.` or `…, R-042 plus cross-cutting acceptance for R-001–R-036.`)
was rejected by the controller's own phase-authority check, which made a resumable historical Build
permanently unresumable because the plan is a frozen input. The check now reads the identifiers each
entry names and keeps the plan's wording. Details, digests and limits are in
[live-recovery/](live-recovery/).

## Installation and portability

The installed PATH executable is `/Users/christophercaldwell/.cargo/bin/orchestrate`, rebuilt from
this checkout after the final Rust changes with `cargo install --path crates/cli --locked --force`.
Version: `orchestrate 0.0.1`. `orchestrate build --help` exposes `guide`, `prepare`, `scaffold`,
`status`, `preflight`, `cleanup`, `export`, `resolve`, and `amend-authority`.

`scripts/install-skills.sh` then installed the six checked-in dispatchers into all three
discoverable host skill directories on this machine — `~/.claude/skills`, `~/.agents/skills` and
`~/.codex/skills` — and each of the eighteen `SKILL.md` files was verified byte-identical to the
checkout. `~/.codex/skills/build/SKILL.md` had still instructed a model to run
`orchestrate build guide`; it now carries the same human-controlled boundary as the others, and no
discoverable Build dispatcher authorizes a CLI invocation. Unrelated skills in those directories
(`ruff-boundaries.md`, `tach.md`, `independent-consensus-audit`, `taskledger`) were preserved. There
is one canonical workflow guide, `crates/guides/resources/guides/build.md`, embedded in the CLI and
printed only by the explicitly requested non-executing help path.

The Rust sources contain no OS-specific `cfg` conditions, `target_os`/`target_arch` branches,
`cfg!` checks, or `std::os::{unix,windows}` use. `cargo fmt --all -- --check` and the full workspace
tests passed on this host. Only the `aarch64-apple-darwin` Rust target is installed, so this records
a portable source/API review and macOS test run, not Linux or Windows runtime evidence. The local
`SDKROOT` override used for this host's test/install commands is a toolchain workaround; it is not
part of the repository code or runtime.

The workspace run reported 116 Build tests passed, 32 CLI end-to-end tests passed (two opt-in live
tests now remain ignored), 11 architecture tests passed, and the other crate suites passed. The
opt-in historical-recovery test passed against the isolated copy of the real records.

Two environment notes, neither of them a code defect:

1. **Codex requests fail when the Claude Code app's proxy environment is inherited.** With
   `HTTPS_PROXY`/`HTTP_PROXY`/`ALL_PROXY` pointing at that app's local proxy, the Codex responses
   endpoint answers with `401 Unauthorized: Incorrect API key provided: sk-svcac…` even though Codex
   is authenticated with its ChatGPT session (`auth_mode: "chatgpt"`, no stored API key, unexpired
   tokens) and no key exists in the environment or config. Removing those variables for the command
   (`env -u HTTPS_PROXY -u HTTP_PROXY -u ALL_PROXY -u https_proxy -u http_proxy …`) makes Codex work
   and the Codex criteria pass. Orchestrate does not scrub or set provider network environment; the
   controller injects nothing for any adapter.
2. **Cursor read-only roles now use `--mode ask`.** The previously documented `--mode plan` ends its
   turn requesting plan approval, so an advisory once-over or formal Audit under it could complete
   without stating its artifacts; the live Build observed exactly that before the mapping changed.
