# Review

Read, in order: the current `action.json`; the binding Reconciled Discovery at its `reconciled_discovery` path; the detailed implementation plan at its `detailed_plan` path; referenced feedback at its `feedback` path, when present; and the exact submitted `target_commit`. The working directory is a contained checkout of that commit. Independently inspect that commit and run relevant checks.

Reconciled Discovery is binding authority. The detailed plan is implementation guidance, and feedback is correction context for the unchanged authorized action. Check correctness, regressions, whether the assigned delivery phase was implemented, whether the implementation violates binding authority, and relevant tests/checks. Do not turn subjective preferences into mandatory corrections.

Read `binding_requirements` from `action.json` as the complete binding requirement view. Verify its source reference and digest identify the supplied Reconciled Discovery; preserve all requirement IDs, order, conditions, and acceptance criteria in your reasoning. Treat the original `goal` as historical context where it conflicts with selected current requirements. Use `phase_authority` for this phase's requirement IDs, completion evidence, and exclusions. A real contradiction must name the exact requirement, observed evidence, and conflict; do not treat a resolved prerequisite in historical wording as current authority.

Do not invent product requirements, expand the delivery-phase scope, require architecture or style changes that are not necessary for correctness or the authorized plan, change Reconciled authority, repair product source, change adopted scope, or edit controller state. A concrete defect or regression remains a valid finding even when it was not literally listed as a task.

Write a concrete report to the `report.md` path in `action.json`. When changes are required, the report is one complete actionable correction set for the unchanged scope. Then write the receipt at the `result.json` path:

```json
{"action_id": "<action_id>", "scope": "<scope>", "outcome": "pass", "commit": "<target_commit>"}
```

`outcome` is `pass`, `changes_required`, or `blocked`. `commit` must be the action's `target_commit`, the commit you actually inspected.
