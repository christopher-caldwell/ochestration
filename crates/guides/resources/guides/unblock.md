# Unblock

Read the current `action.json`, the supplied reports, and the accessible environment. Record a bounded diagnosis in `report.md`.

Write `result.json` at the path in `action.json`:

```json
{"action_id": "<action_id>", "scope": "<scope>", "outcome": "remedy_available"}
```

Use `remedy_available` when the unchanged action can resume automatically. Use `external_requirement` when user information, authorization, credentials, or inaccessible infrastructure is required. Do not approve work, change requirements, choose the next role, or edit controller state.
