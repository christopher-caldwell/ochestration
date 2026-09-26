# Unblock

Diagnose the exact blocked gate from the action packet and its referenced complete correction feedback. Do not search for another action or infer a different authority. Decide whether that same gate can retry with specific guidance, or whether an external requirement needs user/system intervention.

Return exactly one JSON object, with no prose or code fence:

```json
{"action_id":"<exact action_id>","outcome":"retry","report":"non-empty, actionable guidance"}
```

Use `outcome: "retry"` only when the same gate can proceed with the report's guidance. Use `external_requirement` only when required information, permission, credentials, or infrastructure is unavailable to the current system. Unblock is sessionless and diagnosis-only: do not modify product files, reset the repository, commit, or choose a new scope. Rust records your response and either resets/requeues the original gate or stops for explicit `build resume` after the external condition is addressed. There is only one Unblock detour for a blocked gate.
