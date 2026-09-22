# Final Audit

Read the supplied `action.json`. It names the exact `reconciled`, `adoption`, and `implementation` references plus resolved paths for `reconciled_discovery`, `adoption_receipt`, `implementation_record`, `registered_snapshot`, `verification_checkout`, `assessment`, `report`, and `result`. Read those supplied files and the `feedback` path when present; do not inspect controller state or search the store. Inspect only that registered implementation.

Read the immutable registered snapshot as evidence. Run executable verification only in the supplied disposable `verification_checkout`; commands that write files must never mutate the stored immutable snapshot. Do not invent product requirements, turn advisory technical suggestions into requirements, or fail implementation details the binding Reconciled Discovery did not require. Feedback, when present, is correction context for the unchanged authority and scope.

The assessment names those exact references and contains exactly one coverage row for every binding requirement. There are no coverage rows for technical suggestions.

```json
{
  "reconciled": {"kind": "reconciled_discovery", "artifact_id": "...", "digest": "..."},
  "adoption": {"kind": "adoption", "artifact_id": "...", "digest": "..."},
  "implementation": {"kind": "implementation", "artifact_id": "...", "digest": "..."},
  "coverage": [{
    "requirement_id": "R-1",
    "state": "pass",
    "rationale": "Why this row has this state.",
    "evidence": ["Bounded evidence."],
    "correction": ""
  }],
  "assessor_context": "What was inspected."
}
```

`state` is `pass`, `fail`, `unknown`, or `not_applicable`. Pass and fail rows need evidence. Failures also need a correction. Use `not_applicable` only when the binding requirement itself is conditional and its stated condition is demonstrably false for this implementation/context. It never means that an unconditional requirement seems unimportant. For an unconditional binding requirement, use `pass`, `fail`, or `unknown`. Copy `reconciled`, `adoption`, and `implementation` from `action.json`; do not look them up from controller state.

Write `assessment.json`, `report.md`, and `result.json` at the exact paths in `action.json`:

```json
{"action_id": "<action_id>", "scope": "<scope>", "outcome": "complete"}
```

`outcome` is `complete` or `blocked`. Rust derives the verdict from the assessment. Do not run `orchestrate audit finalize`: unattended Build owns immutable Audit publication and verdict routing. Do not edit product source or controller state.
