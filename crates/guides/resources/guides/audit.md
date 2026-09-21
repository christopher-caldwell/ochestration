# Orchestrate Audit

Audit answers: **does this exact implementation satisfy this exact adopted Reconciled Discovery?**

Run `orchestrate status --effort "<effort>"` and identify one exact Reconciled Discovery → Adoption → Implementation chain. Read the reconciled contract, the adoption receipt, the implementation record, and the immutable snapshot at `<root>/snapshots/<target-commit>/source/`.

Audit may inspect that exact implementation and run relevant tests. It must not invent product requirements, turn advisory technical suggestions into requirements, or fail implementation details that the binding Reconciled Discovery did not require. Do not edit source or authority.

Write `assessment.json` outside immutable published bundles. During unattended Build it lives under the supplied Build action's `artifacts/` directory; otherwise keep it outside the store. It names the exact `reconciled`, `adoption`, and `implementation` references from that chain and contains exactly one coverage row for every binding requirement. There are no coverage rows for technical suggestions.

```json
{
  "reconciled": {"kind": "reconciled_discovery", "artifact_id": "...", "digest": "..."},
  "adoption": {"kind": "adoption", "artifact_id": "...", "digest": "..."},
  "implementation": {"kind": "implementation", "artifact_id": "...", "digest": "..."},
  "coverage": [{
    "requirement_id": "R-1",
    "state": "pass",
    "rationale": "Why this row has this state.",
    "evidence": ["Bounded evidence."],
    "correction": ""
  }],
  "assessor_context": "What was inspected."
}
```

`state` is `pass`, `fail`, `unknown`, or `not_applicable`. Pass and fail rows need evidence. Failures also need a correction. Use `not_applicable` only with a justification for why the binding requirement does not apply to this implementation.

Rust, not the assessor, derives the verdict: any failure is `CHANGES_REQUIRED`; any unknown or missing row is `BLOCKED`; otherwise pass or justified not-applicable rows produce `PASS`. A partial or blocked implementation cannot pass.

When invoked through an unattended Build action, write the assessment and the action receipt only. Build publishes the Audit and routes the derived verdict. Do not run `orchestrate audit finalize` in that case.

For standalone Audit, finalize with:

```sh
orchestrate --root "<root>" audit finalize --effort "<effort>" --bundle "<assessment.json>"
```

Report the derived `PASS`, `CHANGES_REQUIRED`, or `BLOCKED` verdict. Do not start another phase. Stop after the Audit result.
