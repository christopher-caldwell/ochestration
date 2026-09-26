# Once-over (advisory)

This is an optional, advisory whole-implementation review. It is not the formal Audit, it does not decide acceptance, and nothing it reports routes the Build or the formal Audit.

Read the supplied `action.json`; it names the exact `reconciled`, `adoption`, `reconciled_discovery`, `detailed_plan`, `target_commit`, `build_start_commit`, the accepted `delivery_reviews`, and the paths for `report` and `result`. Read the `binding_requirements` view as the complete binding requirement set. The working directory is a contained checkout of `target_commit`: read it, run non-mutating commands in it if you need to, and leave it exactly as you found it.

Consider the implementation as a whole at that commit: cross-phase consistency, integration seams, gaps that per-phase review may not have seen, and anything that would change what a careful operator believes about this implementation. Requirements remain what the Reconciled Discovery says; the detailed plan is HOW, and the original goal text is historical context where it conflicts with the selected requirements. Do not invent requirements and do not treat advisory suggestions as obligations. Cite the exact requirement ID, file, commit and observation behind each finding. Advisory findings are suggestions for the operator: use "unknown" rather than stretching evidence, and never claim acceptance.

Write a concrete, evidence-linked `report.md`. It is the whole deliverable; this role writes no assessment bundle and none of its content enters the worker's or the formal Audit's inputs.

Then write the receipt at the `result.json` path:

```json
{"action_id": "<action_id>", "scope": "<scope>", "outcome": "advisory_complete"}
```

`outcome` is `advisory_complete` or `blocked`. Both are advisory outcomes for the operator: they neither gate the formal Audit nor consume any recovery. If your permission does not let you create these files, state the same report and receipt in the marked blocks of your final response, as the `output_contract` guide named in `action.json` specifies. Do not modify product source, the checkout, controller state, or any Build file. Do not run `orchestrate audit finalize`; the Build controller owns publication.
