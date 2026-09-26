# Build and Audit

Build is the deterministic checkpointed `Work → Review → Audit` gate runner. The exact Reconciled Discovery is binding **what**; phase Markdown is **how** and ordering. A Build launch creates or reuses Adoption for that exact authority. The canonical workflow and human CLI boundary live in [the embedded Build guide](../../crates/guides/resources/guides/build.md).

## Prepare

`$build`, a readiness discussion, or preparation does not authorize an Orchestrate command. Explicitly authorize any CLI operation. `orchestrate build prepare` only prints help; `scaffold` writes templates but does not launch provider work.

Schema versions are deliberately breaking: `plan.json` 4, `config.toml` 4, and state 5. The plan names the exact Reconciled ref and ordered phase directory names. Each phase requires `phase.md`; other immediate Markdown files are task/context documents. Rust includes their contents in each phase handoff, with `phase.md` first and the rest sorted by filename. Rust does not parse task semantics or assign requirements. Every role receives the complete binding Reconciled contract. Only exact machine-plan bytes are hashed at initialization; phase Markdown can be refined after a semantic blocked stop. Do not edit plan files while Rust runs. Config uses `worker`, `reviewer`, and optional `unblocker`; each has an adapter (`codex`, `claude`, or `cursor`) and optional native `model` and opaque `args`. Each explicit Build launch validates the plan and loads config. No old Build schema is migrated.

Start a new Build only from a clean Git-visible checkout. The Discovery baseline must be an ancestor of `HEAD`. The current `HEAD` becomes the first checkpoint. Build runs until it completes or reaches a durable stop.

## Gate behavior

- Work implements the entire current phase (or final Audit correction), commits, and reports that exact `HEAD`.
- Review reviews the entire phase at the exact checkpoint in a disposable detached checkout. Pass advances to the next phase or final Audit; `changes_required` routes to Work for the same scope with the complete report.
- Audit assesses the exact registered implementation and returns a complete coverage assessment. Rust delegates verdict derivation to the Audit contract: pass completes; failed requirements route to final Work with report and assessment; unknown or missing coverage routes to Unblock.
- An explicit `blocked` result enters Unblock once with the originating gate, scope, feedback, and a disposable source checkout. `retry` restores the checkpoint and requeues that gate with original correction plus Unblock guidance. Another block or Unblock `blocked` stops cleanly as `stopped / blocked`; there is no second automatic Unblock. Reports and originating gate context are preserved; explicit continuation preserves a clean operator repair and starts a new bounded attempt.

Provider errors, malformed output, Git invariant failures, and restart after durable `running` state require destructive reset. `build reset` validates the checkpoint, removes the current disposable checkout, hard-resets tracked state, cleans ordinary untracked files with `git clean -fd`, preserves ignored files, and requeues the same gate. After a semantic blocker is addressed, an explicit Build launch preserves a clean operator repair and continues at the appropriate Work/Review/Audit boundary in one action.

Review and Audit may share a Reviewer session only when its adapter matches; Work has a separate optional Worker session. Sessions live only in the running Rust process and never in durable state. Every new launch starts fresh conversations using self-contained packets. Unblock is always sessionless. Provider stdout and stderr stream into ordinary action files while an invocation runs. Stderr contains compact transition messages; stdout contains one JSON result. `build status` reports durable state only and does not inspect historical action directories to infer routing.

## Audit

An Audit assessment contains the exact Reconciled, Adoption, and Implementation references plus one coverage row per binding requirement. The existing Audit contract publishes the immutable report and derives `PASS`, `CHANGES_REQUIRED`, or `BLOCKED`; Build does not interpret report prose. Technical suggestions remain advisory.

See [the full run guide](run.md) for the complete workflow.
