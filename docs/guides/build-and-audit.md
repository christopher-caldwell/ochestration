# Build and Audit

Build is intentionally external to Orchestrate.

The Reconciled Discovery tells Build **what must be accomplished**. Build decides **how to
implement it**.

## Build handoff

Give your coding model:

- the target repository;
- the reconciled document path;
- the effort ID.

A useful prompt is:

> Implement the adopted Reconciled Discovery at
> `/absolute/path/to/reconciled-discovery.md`.
>
> Treat every binding requirement as authority. Technical suggestions are advisory starting
> points, not mandatory implementation choices unless the underlying property is also a binding
> requirement.
>
> Run the relevant tests. When the implementation is complete, commit it and register that exact
> commit with Orchestrate for effort `<effort-id>`.

The Build model can make its own detailed implementation plan. That plan does not replace the
Reconciled Discovery.

The underlying registration command is:

```sh
orchestrate implementation register --effort "<effort-id>"
```

You normally let the Build model run it.

Registration freezes the exact implementation commit and tree for Audit.

## Audit

After Build has committed and registered the implementation, open a fresh model window:

```text
$audit
```

Give it:

```text
Effort: <effort-id>
```

The Audit skill locates the exact:

```text
Reconciled Discovery
→ Adoption
→ Registered Implementation
```

It inspects the immutable implementation snapshot and checks every binding requirement exactly
once.

Technical suggestions do not create Audit requirements.

The result is:

```text
PASS
CHANGES_REQUIRED
BLOCKED
```

- **PASS** — every binding requirement was verified.
- **CHANGES_REQUIRED** — at least one binding requirement failed.
- **BLOCKED** — Audit could not verify the full contract.

If Audit reports changes required, return to Build using the same adopted Reconciled Discovery,
make the correction, commit it, register the new implementation, and Audit again.

Back to the [full run guide](run.md).
