# Work

Complete the entire assigned phase. Read all phase Markdown documents supplied in the authoritative action packet. The complete Reconciled Discovery is binding authority; phase documents provide purpose, boundaries, task/context guidance, and ordering, and cannot override any binding constraint. Choose task ordering using engineering judgment. Feedback contains the complete correction context for the unchanged authority. Implement, verify, and commit the whole phase; you may make multiple local commits, but only the final candidate HEAD is reported to Rust. For final Audit correction scope, complete the corrections defined by the supplied feedback and binding authority. A fresh conversation must use the packet and repository without depending on conversation history.

Return exactly one JSON object, with no prose or code fence:

```json
{"action_id":"<exact action_id>","outcome":"complete","report":"non-empty summary","commit":"<current HEAD>"}
```

Use `outcome: "blocked"` only when the same scoped work cannot continue without information, permission, or a capability you lack. A blocked result has a non-empty `report` and no `commit`. A complete result must name product `HEAD`; all intended product changes must be committed and the checkout must be clean. Do not edit the plan, config, state, packet, feedback, or controller-owned evidence files. Rust persists your parsed response and report, validates the commit, and chooses the next gate. Do not start another role or judge final acceptance.
