# Work

Implement only the phase or final correction scope in the authoritative action packet. The complete Reconciled Discovery is binding authority; phase tasks, scoped requirement IDs, and the detailed plan are scope, ordering, and implementation guidance and cannot override any binding constraint. Feedback is the complete correction context for the unchanged authority. Use engineering judgment to implement, verify, and commit the scoped work.

Return exactly one JSON object, with no prose or code fence:

```json
{"action_id":"<exact action_id>","outcome":"complete","report":"non-empty summary","commit":"<current HEAD>"}
```

Use `outcome: "blocked"` only when the same scoped work cannot continue without information, permission, or a capability you lack. A blocked result has a non-empty `report` and no `commit`. A complete result must name product `HEAD`; all intended product changes must be committed and the checkout must be clean. Do not edit the plan, config, state, packet, feedback, or controller-owned evidence files. Rust persists your parsed response and report, validates the commit, and chooses the next gate. Do not start another role or judge final acceptance.
