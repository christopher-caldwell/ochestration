# Work

Complete the entire assigned phase. Read all phase Markdown documents supplied in the authoritative action packet. The complete Reconciled Discovery is binding authority; phase documents provide purpose, boundaries, task/context guidance, and ordering, and cannot override any binding constraint. Choose task ordering using engineering judgment. Feedback contains the complete correction context for the unchanged authority. Implement, verify, and commit the whole phase; you may make multiple local commits, but only the final candidate HEAD is reported to Rust. For final Audit correction scope, complete the corrections defined by the supplied feedback and binding authority. A fresh conversation must use the packet and repository without depending on conversation history.

In the existing report, connect each material claimed behavior checked in this phase to the evidence and result that establish it. State meaningful starting-environment conditions and limits, materially unverified claims, and verification deliberately left for a later phase. Existing-checkout compilation, a prebuilt binary, or copied-example execution establishes only what it exercised under those conditions; do not generalize it to fresh installation, a peer browser path, or full-contract completion. State the relevance and disposition of unrelated limitations and preserve later completion obligations; an unrelated optional failure or explicitly later verification does not by itself prevent completing an otherwise valid phase. This does not require an exhaustive environment inventory or a clean-install check for every phase, and it adds no response field.

Return exactly one JSON object, with no prose or code fence:

```json
{"action_id":"<exact action_id>","outcome":"complete","report":"non-empty summary","commit":"<current HEAD>"}
```

Use `outcome: "blocked"` only when the same scoped work cannot continue without information, permission, or a capability you lack. A blocked result has a non-empty `report` and no `commit`. A complete result must name product `HEAD`; all intended product changes must be committed and the checkout must be clean. Do not edit the plan, config, state, packet, feedback, or controller-owned evidence files. Rust persists your parsed response and report, validates the commit, and chooses the next gate. Do not start another role or judge final acceptance.
