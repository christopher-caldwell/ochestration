# Audit

Audit answers: **does this exact implementation satisfy this exact adopted Reconciled Discovery?**

Audit is an independent stage, distinct from Build phase Review. Its boundary is an eligible registered Implementation, whether produced by Build, manual work, or another implementation path. The public Audit workflow does not require BuildState, phase state, Worker or Reviewer sessions, Build action directories, or a prior Build controller run.

The standalone path is Reconciled Discovery → Adoption → manual/external implementation → Implementation registration → Audit. Use the public `reconcile adopt` and `implementation register` operations to establish the exact chain before assessment. The unattended Rust driver may instead register its Build candidate and invoke Audit automatically under the explicit Build launch authorization; this does not change Audit’s responsibility or standalone eligibility.

Run `orchestrate status --effort "<effort>"` and identify one exact Reconciled Discovery → Adoption → Implementation chain. Never choose a "latest" chain. If the user did not identify an exact implementation and more than one plausible chain exists, stop and ask for the exact implementation/artifact rather than guessing. Read the reconciled contract, the adoption receipt, the implementation record, and the immutable snapshot at `<root>/snapshots/<target-commit>/source/`.

Read the immutable snapshot as evidence. If executable verification may write files, create or use a disposable checkout or copy of the exact registered target commit and run tests there. Never mutate the stored immutable snapshot. Audit must not invent product requirements, turn advisory technical suggestions into requirements, or fail implementation details that the binding Reconciled Discovery did not require. Do not edit source or authority.

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

`state` is `pass`, `fail`, `unknown`, or `not_applicable`. Pass and fail rows need evidence. Failures also need a correction. `not_applicable` also needs evidence establishing that the binding requirement's stated condition is false for this implementation/context. It must not waive an unconditional requirement because the assessor thinks it should not matter. For an unconditional binding requirement, use `pass`, `fail`, or `unknown`.

Rust, not the assessor, derives the verdict: any failure is `CHANGES_REQUIRED`; any unknown or missing row is `BLOCKED`; otherwise pass or justified not-applicable rows produce `PASS`. A partial or blocked implementation cannot pass.

When invoked through an unattended Build action, write the assessment and the action receipt only. The unattended Rust controller publishes the Audit and routes the derived verdict. Do not run `orchestrate audit finalize` in that case.

For standalone Audit, finalize with:

```sh
orchestrate --root "<root>" audit finalize --effort "<effort>" --bundle "<assessment.json>"
```

Report the derived `PASS`, `CHANGES_REQUIRED`, or `BLOCKED` verdict. Do not start another phase. Stop after the Audit result.
