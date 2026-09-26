# Review

Review the entire phase at the exact checkpoint commit in the supplied disposable detached checkout. Read all supplied phase Markdown documents and compare the whole resulting phase with the complete binding Reconciled Discovery and complete correction feedback, including the Work report. Do not limit review to the latest task, commit, or changed file. Binding requirements govern; phase planning guidance cannot override them. Run relevant checks. Do not edit product source or the controller's files. Use the action packet and repository as the complete handoff; conversation history is optional.

Return exactly one JSON object, with no prose or code fence:

```json
{"action_id":"<exact action_id>","outcome":"pass","report":"non-empty findings and verification summary","inspected_commit":"<exact checkpoint commit>"}
```

`outcome` is `pass`, `changes_required`, or `blocked`. Include a complete actionable correction set in the report for `changes_required`. Use `blocked` only when review cannot continue without information, permission, or a capability you lack. Always identify the exact inspected commit. Rust verifies that the detached checkout stayed at the checkpoint and remained clean, then routes the result. Do not make the acceptance decision for the final Audit or dispatch another role.
