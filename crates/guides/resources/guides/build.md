# Build

An explicit Build invocation is authorization to build one exact implementation-ready Reconciled Discovery now. Do not ask for another approval or confirmation. Build creates or reuses the exact Adoption receipt at this boundary.

Build requires the exact effort, the exact implementation-ready Reconciled Discovery, a detailed implementation plan, and the target repository. When the user supplied a Reconciled Discovery explicitly, bind that artifact; do not infer a different one.

Resolve the effort and Build directory:

```sh
orchestrate --root "<root>" status --effort "<effort>"
```

Read the rich Reconciled Discovery artifact as Build's authoritative contract. **Reconciled Discovery is binding WHAT; the detailed implementation plan is HOW and ordering.** The plan may describe approach, task grouping, ordering, and delivery phases only. It must not add or remove product requirements, weaken acceptance criteria, or contradict the Reconciled Discovery. If the plan materially conflicts with binding authority, stop before implementation and surface the conflict. Do not silently rewrite the plan or improvise around it.

The repository may have advanced since Discovery. Discovery remains frozen to its recorded baseline, while implementation must respect the current code and preserve the reconciled contract. Build is allowed when that Discovery baseline is an ancestor of current `HEAD`; it need not equal current `HEAD`.

## First preparation

Copy the detailed implementation plan into the Build directory, then materialize templates:

```sh
orchestrate --root "<root>" build scaffold --effort "<effort>"
```

This writes `plan.json` and `config.toml` and refuses to overwrite existing files. Fill `plan.json` with schema version 2, the exact Reconciled Discovery reference (`kind`, `artifact_id`, and `digest`), the relative detailed-plan path, and the ordered delivery-phase grouping. Each task belongs to one phase; task IDs do not create extra stops.

Configure worker and independent reviewer adapters separately in `config.toml`. Supported adapters are `codex`, `claude`, and `cursor`. Host configuration must already allow intended edits and checks; do not put credentials, command strings, or provider homes in this file.

## Existing prepared or active Build

If Build files or state already exist for this exact effort, do not scaffold again and do not replace the detailed plan, `plan.json`, or `config.toml`. Verify the existing exact Build authority and invoke or resume the driver. Never delete or recreate Build state merely to satisfy this guide.

From the target repository, invoke the driver:

```sh
orchestrate --root "<root>" build --effort "<effort>"
```

The driver binds the exact Reconciled Discovery, creates or reuses Adoption, records current Build starting commit/tree separately from the Discovery baseline, verifies ancestry, and handles work, review, correction, implementation registration, and final Audit until completion or until it cannot safely continue automatically. An external user or infrastructure requirement is one common blocker. Do not perform those role turns yourself.
