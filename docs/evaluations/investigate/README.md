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
