# Unblock

Read the current `action.json`, then its exact `interrupted_action` and the paths in `interrupted_report`, `interrupted_result`, and `prior_feedback`. Read every referenced file that exists; the absence of a report or result is itself bounded evidence, not a reason to search elsewhere. Those fields are the evidence for the interrupted action; do not scan Build directories, guess which report is relevant, or choose a latest report. Inspect the accessible environment only as needed to diagnose whether that same action can resume. Record a bounded diagnosis in `report.md`.

Write `result.json` at the path in `action.json`:

```json
{"action_id": "<action_id>", "scope": "<scope>", "outcome": "remedy_available"}
```

Use `remedy_available` only when the same interrupted action can be retried without new user authority. Use `external_requirement` only when progress requires information, authorization, credentials, or inaccessible infrastructure the current system cannot supply. Do not approve work, change scope or requirements, select another phase or implementation direction, choose the next role, or edit controller state.
