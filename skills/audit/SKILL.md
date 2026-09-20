---
name: audit
description: Audit an exact registered implementation against the binding requirements of an adopted Reconciled Discovery.
disable-model-invocation: true
---

# Audit

Run `orchestrate guide audit`, then inspect `orchestrate status --effort "<effort>"` to identify
one exact Reconciled Discovery → Adoption → Implementation chain. Read the reconciled contract,
the adoption receipt, implementation record, and the immutable snapshot at
`<root>/snapshots/<target-commit>/source/`.

Audit may inspect that exact implementation and run relevant tests. It must not invent product
requirements, turn advisory technical suggestions into requirements, or fail implementation
details that the binding Reconciled Discovery did not require.

Write `assessment.json` outside the store. It names the exact `reconciled`, `adoption`, and
`implementation` references and contains exactly one coverage row for every binding requirement.
There are no coverage rows for technical suggestions. Pass and fail rows need evidence; failures
also need a correction. Finalize with:

```sh
orchestrate --root "<root>" audit finalize --effort "<effort>" --bundle "<assessment.json>"
```

Report the derived `PASS`, `CHANGES_REQUIRED`, or `BLOCKED` verdict and stop.
