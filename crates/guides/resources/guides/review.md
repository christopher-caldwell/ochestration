# Review

Review the entire phase at the exact checkpoint commit in the supplied disposable detached checkout. Read all supplied phase Markdown documents and compare the whole resulting phase with the complete binding Reconciled Discovery and complete correction feedback, including the Work report. Do not limit review to the latest task, commit, or changed file. Binding requirements govern; phase planning guidance cannot override them. Run relevant checks. Do not edit product source or the controller's files. Use the action packet and repository as the complete handoff; conversation history is optional.

Return exactly one JSON object, with no prose or code fence:

```json
{"action_id":"<exact action_id>","outcome":"pass","report":"non-empty findings and verification summary","inspected_commit":"<exact checkpoint commit>"}
```

`outcome` is `pass`, `changes_required`, or `blocked`. Include a complete actionable correction set in the report for `changes_required`. Use `blocked` only when review cannot continue without information, permission, or a capability you lack. Always identify the exact inspected commit. Rust verifies that the detached checkout stayed at the checkpoint and remained clean, then routes the result. Do not make the acceptance decision for the final Audit or dispatch another role.

Relate observed failures and relevant unrun checks to the current phase's binding obligations and claimed behavior, including the starting conditions each check actually exercised. Use ordinary role-permitted setup when appropriate, within the no-edit rules, and leave the disposable checkout at the exact checkpoint and Git-clean. A demonstrated in-scope defect requires `changes_required` and a complete correction set. A material inability to determine acceptance because needed information, permission, or capability is unavailable requires `blocked`; inability to set up by itself is a capability limit, not proof of a product defect. Unrelated limitations and proof explicitly planned for a later phase do not automatically block an otherwise valid phase: state their relevance and disposition and retain the later obligation.

`pass` requires that no material current-phase contradiction remains unresolved in the findings or warnings. Rust clears Review feedback on PASS, and the passing report is not delivered to the initial Audit packet, so resolve the relevance of material evidence before passing. Audit still makes its own independent decision against the full contract.

Useful short repeated steps or commands are not defects by themselves. Require correction for contradictory procedural ownership or factual claims when the current contract requires consistency.
