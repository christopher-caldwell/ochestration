# Work

Read the current `action.json`. It names the scope, task ids, working directory, detailed plan, optional feedback path, and the exact paths for `report.md` and `result.json`.

Implement the whole assigned delivery phase in that working directory, run relevant checks, and commit the intended product changes. When the action references feedback, address that complete valid correction set for the unchanged scope instead of starting a new phase.

Write `report.md`, then write `result.json` at the path in `action.json`:

```json
{"action_id": "<action_id>", "scope": "<scope>", "outcome": "complete", "commit": "<HEAD>"}
```

`outcome` is `complete`, `incomplete`, or `blocked`. A `complete` receipt must name the commit that is current HEAD, and the worktree must not contain other tracked changes. Use `blocked` only when the phase cannot continue without information, permission, or a capability you do not have.

Do not edit `state.json`, choose another phase, or start another role.
