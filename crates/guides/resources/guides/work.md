# Work

Read, in order: the current `action.json`; the binding Reconciled Discovery at its `reconciled_discovery` path; the detailed implementation plan at its `detailed_plan` path; and referenced feedback at its `feedback` path, when present. Then implement only the assigned delivery phase. `action.json` names the scope, task IDs, working directory, exact authority references, and output paths.

Reconciled Discovery is binding authority. The detailed plan is implementation guidance. Feedback is correction context for the unchanged authorized action; it does not change authority, requirements, or scope. Implement the whole assigned delivery phase in the working directory, run relevant checks, and commit the intended product changes. When the action references feedback, address that complete valid correction set instead of starting a new phase.

Read `binding_requirements` from `action.json` as the complete binding requirement view. It is mechanically projected from the exact Reconciled Discovery named by `reconciled`, and includes its source JSON SHA-256. Preserve requirement order, conditions, acceptance criteria, governing flags, and provenance fields; do not omit conditional entries. Treat the original `goal` text in Reconciled Discovery as historical context when it conflicts with selected current requirements. `phase_authority` identifies this phase's requirement IDs, completion evidence, and deliberate exclusions; it scopes the plan's HOW and does not replace the full binding contract.

When you find a genuine conflict, cite the exact requirement ID, the specific observed fact or evidence, and how they conflict. Identify the actual missing decision, access, or capability. Do not raise a resolved prerequisite as a blocker based only on stale historical wording.

When this action continues, replaces or remediates an earlier one, `action.json` also names `predecessor`: the exact prior action, its report, result/receipt and transport (or their recorded absence), the observed ending commit, `handoff` (`continuation`, `replacement`, `remedy` or `resolution`) and, for a remedy, `remedy`. Read those references instead of searching the Build directory, and determine recorded work and outstanding checks yourself: a predecessor's claims, including any test it says it ran, remain unverified and are not completion. An early failure in a predecessor's conditional command chain means its later commands were not executed. Environment workarounds it recorded are re-checkable execution context, not requirements. `feedback` stays the original correction you must address; `remedy` is diagnosis context and never replaces it.

Write `report.md`, then write `result.json` at the path in `action.json`:

```json
{"action_id": "<action_id>", "scope": "<scope>", "outcome": "complete", "commit": "<HEAD>"}
```

`outcome` is `complete`, `incomplete`, or `blocked`. A `complete` receipt must name the commit that is current HEAD, and the worktree must not contain other tracked changes. Use `blocked` only when the phase cannot continue without information, permission, or a capability you do not have.

Controller-owned material is read-only. Do not edit `action.json`, `state.json`, role instructions, Build plan/configuration, feedback reports, Reconciled authority artifacts, controller metadata, choose another phase, or start another role. You may modify intended product source, required generated product artifacts, your own `report.md`, and your own `result.json` only.
