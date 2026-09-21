# Final Audit

Read the supplied `action.json`. It names the exact `reconciled`, `adoption`, and `implementation` references to assess, and the exact paths for your output. Inspect only that registered implementation. You may read the immutable snapshot and run relevant tests. Do not invent product requirements, turn advisory technical suggestions into requirements, or fail implementation details the binding Reconciled Discovery did not require.

Write `assessment.json` in the action directory, beside the receipt. It names those exact references and contains exactly one coverage row for every binding requirement. There are no coverage rows for technical suggestions.

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

`state` is `pass`, `fail`, `unknown`, or `not_applicable`. Pass and fail rows need evidence. Failures also need a correction. Copy `reconciled`, `adoption`, and `implementation` from `action.json`; do not look them up from controller state.

Write `report.md` and `result.json` at the paths in `action.json`:

```json
{"action_id": "<action_id>", "scope": "<scope>", "outcome": "complete"}
```

`outcome` is `complete` or `blocked`. Rust derives the verdict from the assessment. Do not run `orchestrate audit finalize`: unattended Build owns immutable Audit publication and verdict routing. Do not edit product source or controller state.
