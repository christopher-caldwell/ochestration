# Build and Audit

Build is the deterministic checkpointed `Work → Review → Audit` gate runner. The exact Reconciled Discovery is binding **what**; the detailed plan is **how** and ordering. A Build launch creates or reuses Adoption for that exact authority. The canonical workflow and human CLI boundary live in [the embedded Build guide](../../crates/guides/resources/guides/build.md).

## Prepare

`$build`, a readiness discussion, or preparation does not authorize an Orchestrate command. Explicitly authorize any CLI operation. `orchestrate build prepare` only prints help; `scaffold` writes templates but does not launch provider work.

Schema versions are deliberately breaking: `plan.json` 3, `config.toml` 4, and state 4. The plan names the exact Reconciled ref, detailed-plan path, and ordered phases with tasks and requirement IDs. Those mappings guide scope and order; every role receives the complete binding Reconciled contract. Its exact bytes and the detailed-plan bytes are hashed at initialization and cannot change during the Build. Config uses `worker`, `reviewer`, and optional `unblocker`; each has an adapter (`codex`, `claude`, or `cursor`) and optional native `model` and opaque `args`. Each explicit Build launch validates the plan and loads config once. No old Build schema is migrated.

Start a new Build only from a clean Git-visible checkout. The Discovery baseline must be an ancestor of `HEAD`. The current `HEAD` becomes the first checkpoint. Build runs until it completes or reaches a durable stop.

## Gate behavior

- Work implements only the current phase (or final Audit correction), commits, and reports that exact `HEAD`.
- Review inspects the exact checkpoint in a disposable detached checkout. Pass advances to the next phase or final Audit; `changes_required` routes to Work for the same scope with the complete report.
- Audit assesses the exact registered implementation and returns a complete coverage assessment. Rust delegates verdict derivation to the Audit contract: pass completes; failed requirements route to final Work with report and assessment; unknown or missing coverage routes to Unblock.
- An explicit `blocked` result enters Unblock once with the originating gate, scope, feedback, and a disposable source checkout. `retry` restores the checkpoint and requeues that gate with original correction plus Unblock guidance. Another block stops; there is no second Unblock. `external_requirement` stops for an operator; explicit continuation preserves a clean operator repair and starts a new bounded attempt.

Provider errors, malformed output, Git invariant failures, and restart after durable `running` state require destructive reset. `build reset` validates the checkpoint, removes the current disposable checkout, hard-resets tracked state, cleans ordinary untracked files with `git clean -fd`, preserves ignored files, clears sessions, and requeues the same gate. After an external requirement is addressed, an explicit Build launch preserves a clean operator repair and continues at the appropriate Work/Review/Audit boundary in one action.

Review and Audit share a Reviewer session only when its adapter matches; Work has a separate Worker session. Unblock is always sessionless. Provider stdout and stderr stream into ordinary action files while an invocation runs. Stderr contains compact transition messages; stdout contains one JSON result. `build status` reports durable state only and does not inspect historical action directories to infer routing.

## Audit

An Audit assessment contains the exact Reconciled, Adoption, and Implementation references plus one coverage row per binding requirement. The existing Audit contract publishes the immutable report and derives `PASS`, `CHANGES_REQUIRED`, or `BLOCKED`; Build does not interpret report prose. Technical suggestions remain advisory.

See [the full run guide](run.md) for the complete workflow.
