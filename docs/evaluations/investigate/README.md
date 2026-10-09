# Investigation verification — October 8, 2026

Issue #12 adds an opt-in workflow; these checks do not authorize Build, replace canonical Audit,
or establish general model accuracy. Raw live inputs, provider transport and sealed run artifacts
remain outside this repository in the operator's orchestration verification directory.

Automated verification passed with `cargo test --workspace`, including the existing Build and Audit
regressions. Investigation tests cover fixed-cohort concurrency, identical frozen substantive inputs,
fresh sessions, retained failures, exact conformance authority, full coverage including `governing:
false`, native receipts, graph validation, aggregation, source drift and tampering. CLI tests cover
embedded guidance, prelaunch configuration rejection and native-process execution followed by
provider-free inspection. Formatting and strict workspace Clippy checks also pass.

## Bounded live consensus smoke

A disposable pinned Git fixture deliberately implemented `add(a, b)` as `a - b`; its supplied
acceptance check expected `add(2, 3) == 5`. Three fresh Codex lanes and two fresh Claude lanes launched
with a concurrency cap of five, followed by one fresh Codex reconciler. Lane instructions bounded
tool use and required tests to run in scratch copies. The original fixture's HEAD and working tree
were unchanged afterward.

The retained result was COMPLETE: five valid completed lanes, zero invalid/unavailable/failed lanes,
GO 0, NO_GO 5, UNKNOWN 0. The configured threshold was four and `threshold_met` was false. The
evidence recommendation was NO_GO. Offline `investigate inspect` verified the retained records and
reproduced the same structured result.

Provider versions were `codex-cli 0.160.0` and Claude Code `2.1.289`. Claude's transport reported
`claude-sonnet-5-5`; Codex did not emit an observed model name, which remains unknown. Requested
settings and observed metadata are retained separately.

Codex emitted completed command records with numeric statuses, including the failed acceptance
check. Claude's shell transport emitted output without numeric exit metadata. Those execution
claims were downgraded to unverified testimony, despite printed exit labels; source inspection and
Codex receipts supplied the primary defect evidence. Missing transport facts never become passing
runtime evidence.

## Cursor transport probe and limits

A separate fresh Cursor CLI session (`2026.09.28-64d2043`, observed model `GPT-5.5 272K Low`) ran one
bounded Python command that printed `123` and exited four. Its native `shellToolCall.result.failure`
contained `exitCode: 4`, output and working-directory facts. Receipt parsing handles this transport
and tests reject missing numeric status and background completion as verified execution.

Live wide aggregation, live conformance against a real effort, and a full Cursor investigation were
not exercised. Wide/open-ended and conformance behavior were verified with scripted provider
outputs. These smoke checks establish transport/controller interoperability on a small fixture;
semantic evidence quality remains attributed to the assessing agents.

## Focused correction review

The correction pass independently reviewed feature baseline `ead11bc991ee040e2e08a4c0ad2664fa77218754`
against `main` and the current issue brief. Classification and corrections:

- **Stalled providers — confirmed.** A scripted three-lane run sealed two valid graphs but never
  aggregated while the third provider stayed active, despite `min_completed = 1`. Investigation
  native calls now have a positive `provider_timeout_seconds` limit, default 1800 seconds, for
  both lanes and reconciliation. Deadline handling includes blocked stdin delivery. Timed-out
  calls fail without votes and retain partial native transport. Unix invocation process groups
  are stopped so ordinary tool children cannot continue writing. Build does not opt into the
  deadline or process-group policy; its wait and session behavior remain unchanged. No retry,
  resume, cancellation service or scheduler was added.
- **Execution authority — guidance ambiguity confirmed.** The blanket permission to run tests
  freely did not distinguish inspection-only requests. No unauthorized live execution was
  established by this review. Embedded guidance now makes the frozen request authoritative:
  tests/builds/executable probes require relevant bounded authorization, and absent or unclear
  authority means inspection plus an explicit verification limitation. The default conformance
  question and example request explicitly use inspection-only authority. Native permission flags
  do not expand it. This remains an instruction boundary, without a new security architecture.
- **Submodules — confirmed.** A minimal local parent repository with a committed submodule failed
  freezing with `source is not a file: deps/component`. Inventory now separates blob contents from
  gitlinks. Parent files remain frozen; exact submodule path/commit identities are retained in
  `repository.submodules`, with dependency contents explicitly unavailable to both agents and
  in the report. There is no fetch, recursive checkout or dependency manager.
- **Lane versus cohort eligibility — intentional stages, ambiguous label.** Duplicate-session
  lanes were correctly excluded and offline inspection recomputed that exclusion. Counts were
  not wrong. Sealed lane records now say `stage = lane_graph` and `graph_valid`; cohort records
  and result lanes retain final eligibility and its rejection reason. Old lane `eligible` fields
  remain readable as lane-stage validation. Immutable graphs and manifests are not rewritten.
- **Skill count — confirmed documentation omission.** README now lists `investigate`. A staging
  installation produced seven dispatchers, and the README list exactly matched the installed set.

Regression checks exercised real scripted subprocesses for stalled lanes, unmet completion minima,
stalled reconciliation, retained transport and offline inspection; a blocked-input/process-group
probe; a pinned local submodule; graph-valid but cohort-invalid duplicate sessions; and legacy
configuration encoding. Full workspace tests, formatting and strict all-target Clippy pass,
including existing Audit and Build regressions. Parallel test fixture directory names now include
an atomic sequence after an observed timestamp collision.

The original sealed five-lane live smoke was inspected offline with providers unavailable and
returned an identical structured result, without rewriting its records. No fresh live model run
was performed in this correction pass. Unix timeout cleanup was exercised on macOS; Windows
termination and descendants deliberately escaping their process group were not exercised or
claimed as an isolation guarantee. No remaining merge blocker was identified by these checks.

## Interrupted provider cleanup correction

The finding was independently reproduced against `2f61ab061d1de15ac8dc4e5ba69c76d77284a1e9`
with the actual CLI and a scripted native provider, without live models or external services.
Two concurrent providers each launched an ordinary tool child; both continuously wrote heartbeat
files. Sending SIGINT to the controller's process group, SIGINT directly to the controller, or
SIGTERM directly to the controller terminated Orchestrate while both providers and both tools
continued writing during the observation window. A one-second provider deadline stopped the same
process groups correctly. Retained partial transport survived, and offline inspection already
reported the interrupted runs as INCOMPLETE/INCONCLUSIVE.

The regression mechanism was the separate Unix process group introduced for bounded investigation
calls: terminal SIGINT targets the controller's foreground group, and controller termination does
not invoke the provider's timeout loop or automatically terminate its children. There was no
SIGINT/SIGTERM handler. Literal pseudo-terminal Ctrl+C probes were inconclusive on this host;
controller-group SIGINT supplies the confirmed signal-delivery reproduction.

Only the CLI's Unix `investigate run` branch now installs SIGINT/SIGTERM flag handlers, before lane
threads start. An explicitly supplied shared signal flag reaches bounded provider plans through
the existing invocation seam. Each active call polls it and reuses timeout's group SIGKILL and
direct-child reaping, without joining a blocked stdin writer. Interruption is a failed attempt;
raw transport/stderr remain on disk. The coordinator joins active lanes, stops further batches and
reconciliation, and leaves the run without a final manifest. Previously sealed lane evidence is
retained, and existing offline inspection returns INCOMPLETE/INCONCLUSIVE. A typed interruption
error identifies the signal and available artifact directory; the CLI returns 130 or 143.
Repeated signals update the flag without bypassing cleanup. Existing entry points, invocation
traits, artifact schemas, Build execution/session behavior and canonical Audit authority remain
unchanged. Library entry points do not install process-global signal handlers.

Verification on macOS with Rust/Cargo 1.94.0:

- `cargo test --locked --workspace`: 177 tests passed, zero failures or ignored tests; doc-tests
  also passed. This includes the existing Build/Audit regressions and unchanged successful native
  investigation/evidence round-trip and timeout assertions.
- Five new CLI regression tests exercise controller-group SIGINT, direct SIGINT/SIGTERM, concurrent
  lanes, blocked stdin, repeated signals, reconciliation interruption, completed evidence plus
  queued lanes with `min_completed` already met, and isolation between two running investigations.
  Assertions verify conventional exits, reaped providers, stopped tool heartbeats, retained raw
  transport/stderr, absent final manifests, skipped launches, and honest offline inspection.
- A new core test interrupts after the direct provider has exited and been reaped while an ordinary
  child still holds stdin open; the child cannot perform its scheduled write. An already-set flag
  also prevents a later provider launch.
- `cargo fmt --all -- --check`, `cargo clippy --locked --workspace --all-targets -- -D warnings`,
  and `git diff --check`: passed.

No automatic retry, resume, process supervisor or persistent registry was added. SIGKILL, OS
crashes, SIGHUP and descendants deliberately escaping their process group remain outside the
cleanup guarantee. Windows termination behavior is unchanged and was not exercised. No live
provider billing was incurred.
