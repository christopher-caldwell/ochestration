# Review

Read the current `action.json`. Independently inspect and test only its exact submitted commit. The working directory is a contained checkout of that commit. Run relevant checks.

Do not repair product source, change the adopted scope, or edit controller state.

Write a concrete report to the `report.md` path in `action.json`. When changes are required, the report is the complete correction set. Then write the receipt at the `result.json` path:

```json
{"action_id": "<action_id>", "scope": "<scope>", "outcome": "pass", "commit": "<target_commit>"}
```

`outcome` is `pass`, `changes_required`, or `blocked`. `commit` must be the action's `target_commit`, the commit you actually inspected.
