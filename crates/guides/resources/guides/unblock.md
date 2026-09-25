# Unblock

Read the current `action.json`, then its exact `interrupted_action` and the paths in `interrupted_report`, `interrupted_result`, and `prior_feedback`. It also supplies the controller's durable `trigger`, `process_completion`, `receipt_validation` and `recovery_remaining` facts plus any `current_audit`, `assessment`, and `unresolved_requirement_ids`; treat those as the recorded reason the role stopped. Read every referenced file that exists; the absence of a report or result is itself bounded evidence, not a reason to search elsewhere. Those fields are the evidence for the interrupted action; do not scan Build directories, guess which report is relevant, or choose a latest report. Inspect the accessible environment only as needed to diagnose whether that same action can resume. Record a bounded diagnosis in `report.md`; a remedy must name what changes on retry, and regenerating an unchanged completed assessment is not a remedy.

Write `result.json` at the path in `action.json`:

```json
{"action_id": "<action_id>", "scope": "<scope>", "outcome": "remedy_available"}
```

Use `remedy_available` only when the same interrupted action can be retried without new user authority. Use `external_requirement` only when progress requires information, authorization, credentials, or inaccessible infrastructure the current system cannot supply.

Unblock is diagnosis-only. Do not modify product source, run mutating commands against the product checkout, commit changes, edit Build/controller files, or perform the proposed remedy yourself. Describe an available remedy in `report.md`; the controller will resume the interrupted role to perform it. Do not approve work, change scope or requirements, select another phase or implementation direction, or choose the next role.

Use `binding_requirements` in the interrupted action as current authority, together with its exact `reconciled` reference. Treat the original `goal` as historical context when it conflicts with selected current requirements. Diagnose the interrupted role against its exact `phase_authority`; distinguish a real missing decision/access/capability from a resolved prerequisite. Any reported conflict must name the requirement ID and the observed evidence establishing the conflict.
