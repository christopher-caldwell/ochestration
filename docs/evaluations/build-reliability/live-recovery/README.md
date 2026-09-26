# Historical recovery validation

Date: 2026-09-25. This record covers the supported recovery sequence exercised against an isolated
copy of the interrupted Build `effort-7c0de108b37c9d6c`
(`reconcile-44dfd4294fe678ae`, scope B-recovery-audit). It replaces the earlier claim that "there
was no schema-v3 state migration to perform": the original records do lack `migration_version`, and
the additive migration is exercised here.

The original effort was never opened by a controller, resumed, edited or migrated. Its Build state,
config, plan and Reconciled artifact still hash to
`0337e3a8…`, `f13474b5…`, `80e9e13c…` and `b0b47c87…` exactly as recorded, its `state.json` and
`journal.jsonl` keep their original modification times, and its interrupted action directory still
holds only the partial `action.json` and `transport.jsonl` it had.

## Isolation

`prepare-isolated-environment.sh` builds a directory outside the store:

- `<work>/store` — the store marker and the project's records copied byte for byte with
  timestamps (`cp -a`); the unrelated project in the same store is not copied.
- `<work>/product` — a fresh clone of the product repository with its own object store, so nothing
  the validation commits can reach the real repository.
- The copied `project.json`'s `canonical_locator` is rewritten to `<work>/product`, so every
  project and store path the controller resolves points into the isolated directory. Nothing else
  in the copied records is rewritten; the frozen plan, config, digests and artifact references are
  the originals.

```sh
sh docs/evaluations/build-reliability/live-recovery/prepare-isolated-environment.sh \
  /private/tmp/orchestrate-historical-recovery-20260925 \
  /Users/christophercaldwell/Code/projects/ai_orchestration

ORCHESTRATE_HISTORICAL_RECOVERY_ROOT=/private/tmp/orchestrate-historical-recovery-20260925 \
SDKROOT=/Library/Developer/CommandLineTools/SDKs/MacOSX26.5.sdk \
  cargo test -p orchestrate-build --test historical_recovery -- --ignored --nocapture
```

`crates/build/tests/historical_recovery.rs` is that harness. It dispatches no provider: a
deterministic adapter performs the continuation inside the isolated checkout, as the assignment
allows.

## Sequence and result

Starting state, as recorded: schema 3, no `migration_version`, `phase_index` 1 of 7 phases, action
`act-1790305441538-67205-3` (work, scope `B-recovery-audit`) with `dispatch: running` and session
`01a0d67c-0071-7373-bb19-16e740c8beac`, and no durable stop.

| Step | Observation |
| --- | --- |
| First start | Recorded the interrupted acceptance instead of dispatching: `trigger = uncertain_acceptance`, `process_completion = "accepted; provider completion is uncertain"`. Zero adapter dispatches, so the uncertain action was not resent and no `transport.completed` marker appeared. |
| Migration | `build/evidence/state-v3-original.json` holds the original state bytes exactly; the journal gained one `build_state_migrated` event; `migration_version = 1`; plan digest, phase index, accepted phases, action identity and the interrupted action's partial transport all survived unchanged. |
| Resolution without confirmation | Refused: "confirm the provider is no longer running". |
| Resolution with `--confirm-not-running` | Applied as `existing_authority_clarification`; the continuation action is a distinct id whose kind is `work` and whose scope is the interrupted `B-recovery-audit`; it carries the interrupted action as its recorded predecessor. |
| Continuation | Work, review and the formal Audit for the interrupted scope and for phases C–G completed; the Build printed `BUILD COMPLETE — delivery finished and formal Audit audit-… derived Pass`. |
| No replay | No dispatch named the already-accepted phase `A-authority`; no dispatch targeted the interrupted action's directory. |
| Repeated recovery | The same resolution re-submitted is idempotent: one resolution record, one `build_resolved` journal event, the same continuation action, no second transition. A completed Build added nothing. |

## Defect found and fixed

The exercise found a real compatibility defect: a frozen plan written as prose did not survive
resume.

- `**Requirements:** …, R-041.` — the sentence-final period was read as part of `R-041`, so the
  controller rejected its own frozen plan with "phase authority for B-recovery-audit references an
  unknown binding requirement" and stopped the continuation as a controller failure.
- `**Requirements:** …, R-042 plus cross-cutting acceptance for R-001–R-036.` — a clause naming ids
  was read as one identifier, so phase G failed the same way after phases B–F had completed.

Either failure made a resumable historical Build permanently unresumable, because the plan is a
frozen input that cannot be edited. The fix tokenizes each entry into the identifiers it names
(letters, a hyphen, then a digit) and checks those, keeping the plan's own wording in what roles
receive. `prose_requirement_lists_resolve_to_the_identifiers_they_name` covers both forms, and
still requires that an identifier that is written but does not exist is rejected.

## Limits

- The continuation is a deterministic adapter, so this record proves the controller sequence —
  migration, stop, confirmation-gated resolution, continuation, idempotence — not live model
  behaviour on the historical effort.
- The isolated run rewrote one field (`project.json`'s `canonical_locator`). Every other copied byte
  is the original, which the digest checks above are taken against.
- The original effort remains stopped and untouched, exactly as it was; this exercise neither
  completes nor continues it.
