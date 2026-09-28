# Build and Audit

Build runs the deterministic checkpointed `Work ↔ Review` phase loop and produces an exact Implementation candidate. The unattended Rust driver then invokes the independent Audit stage against that Implementation before reporting reviewed completion. Build and Audit are distinct responsibilities coordinated by the same driver; no second process or orchestration command is needed. The exact Reconciled Discovery is binding **what**; phase Markdown is **how** and ordering. A Build launch creates or reuses Adoption for that exact authority. The canonical workflow and human CLI boundary live in [the embedded Build guide](../../crates/guides/resources/guides/build.md).

## Prepare

`$build`, a readiness discussion, or preparation does not authorize an Orchestrate command. Explicitly authorize any CLI operation. `orchestrate build prepare` only prints help; `scaffold` writes templates but does not launch provider work.

Schema versions are deliberately breaking: `plan.json` 4, `config.toml` 4, and state 5. The plan names the exact Reconciled ref and ordered phase directory names. Each phase requires `phase.md`; other immediate Markdown files are task/context documents. Rust includes their contents in each phase handoff, with `phase.md` first and the rest sorted by filename. Rust does not parse task semantics or assign requirements. Every role receives the complete binding Reconciled contract. Only exact machine-plan bytes are hashed at initialization; phase Markdown can be refined after a semantic blocked stop. Do not edit plan files while Rust runs. Config uses `worker`, `reviewer`, and optional `unblocker`; each has an adapter (`codex`, `claude`, or `cursor`) and optional native `model` and opaque `args`. Each explicit Build launch validates the plan and loads config. No old Build schema is migrated.

Start a new Build only from a clean Git-visible checkout. The Discovery baseline must be an ancestor of `HEAD`. The current `HEAD` becomes the first checkpoint. Build runs until it completes or reaches a durable stop.

## Unattended controller routing

- Work implements the entire current phase (or final Audit correction), commits, and reports that exact `HEAD`.
- Review reviews the entire phase at the exact checkpoint in a disposable detached checkout. Pass advances to the next phase; after the last phase passes, Rust registers the exact Implementation and invokes independent Audit; `changes_required` routes to Work for the same scope with the complete report.
- Audit assesses the exact registered implementation and returns a complete coverage assessment. Rust delegates verdict derivation to the Audit contract: `PASS` permits reviewed completion; `CHANGES_REQUIRED` routes to final-scope Build Work with report and assessment, then a new registered Implementation and another Audit; unknown or missing coverage routes to Unblock.
- An explicit `blocked` result enters Unblock once with the originating gate, scope, feedback, and a disposable source checkout. `retry` restores the checkpoint and requeues that gate with original correction plus Unblock guidance. Another block or Unblock `blocked` stops cleanly as `stopped / blocked`; there is no second automatic Unblock. Reports and originating gate context are preserved; explicit continuation preserves a clean operator repair and starts a new bounded attempt.

Provider errors, malformed output, Git invariant failures, and restart after durable `running` state require destructive reset. `build reset` validates the checkpoint, removes the current disposable checkout, hard-resets tracked state, cleans ordinary untracked files with `git clean -fd`, preserves ignored files, and requeues the same gate. After a semantic blocker is addressed, an explicit Build launch preserves a clean operator repair and continues at the appropriate Work/Review/Audit boundary in one action.

Review and Audit may share a Reviewer session only when its adapter matches; Work has a separate optional Worker session. Sessions live only in the running Rust process and never in durable state. Every new launch starts fresh conversations using self-contained packets. Unblock is always sessionless. Provider stdout and stderr stream into ordinary action files while an invocation runs. The existing `orchestrate build --effort <id>` invocation always shows a live status and append-only event history on stderr; stdout remains exactly one JSON command result with the same outcome and exit semantics. The display replaces the old compact transition lines and adds no command, wrapper requirement, or persisted display files. Provider evidence remains in the ordinary action files. `build status` reports durable state only and does not inspect historical action directories to infer routing.

The status frame shows recorded status and gate, phase or final scope, the full current action ID when present, and a display-only checkpoint prefix. The 20-cell bar counts reviewed phases, never tasks. A full reviewed-phase bar does not mean final Audit passed: final-scope Work or Unblock may still be pending or running. Only recorded completion says Build complete. Prior reviewed phases are summarized from saved scope, not replayed from historical actions. Recorded Running does not prove process liveness; long actions retain their last recorded view without timers or invented progress. Stops show `blocked` or `reset_required`; inspect the full stop detail with `orchestrate build status --effort <id>`.

Terminal output updates a small status frame in place while retaining event history. By default, piped stderr is plain append-only output: an initial snapshot, flushed live event lines, and a final snapshot, with no ANSI or cursor movement. `ORCHESTRATE_BUILD_COLOR=auto|always|never` takes precedence over `NO_COLOR`: explicit `auto` uses whether stderr is a terminal, `always` forces colored in-place output even under a pipe (including ANSI and cursor control), and `never` stays plain even on a terminal. When the override is unset, nonempty `NO_COLOR` selects plain output; empty `NO_COLOR` leaves automatic detection enabled. Unsupported override values conservatively select plain output. The fixed layout may wrap in narrow terminals; use `never` for append-only output. Status, scaffold, and reset retain their existing behavior, including reset's `RESET` stderr line.

## Independent and standalone Audit

Build answers whether all planned phases have been implemented and reviewed, producing an exact Implementation candidate. Audit answers whether that exact Implementation satisfies the complete binding Reconciled Discovery.

Audit is independently usable through the public [Audit workflow](../../crates/guides/resources/guides/audit.md), including `$audit` / `orchestrate audit guide` and `orchestrate audit finalize`. An eligible registered Implementation is its boundary; it does not require BuildState, phase state, Worker or Reviewer sessions, Build action directories, or a prior Build controller run. Manual and external work follow this path:

```text
Reconciled Discovery → Adoption → manual / external / other implementation
  → Implementation registration → Audit
```

Use `reconcile adopt` for the exact Reconciled Discovery and `implementation register` for the exact Adoption and target commit before running the public Audit workflow. During an authorized unattended launch, Rust performs registration and invokes Audit without another human confirmation.

An Audit assessment contains the exact Reconciled, Adoption, and Implementation references plus one coverage row per binding requirement. The existing Audit contract publishes the immutable report and derives `PASS`, `CHANGES_REQUIRED`, or `BLOCKED`; Build does not interpret report prose. Technical suggestions remain advisory.

See [the full run guide](run.md) for the complete workflow.
