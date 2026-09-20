---
name: discovery
description: Run one independent Orchestrate Discovery from a prepared request and return its finalized public artifact.
disable-model-invocation: true
---

# Discovery

Input is one absolute prepared-request path. Resolve `orchestrate`, read the request frontmatter,
then run:

```sh
orchestrate --root "<root>" init --from-file "<prepared-request>"
orchestrate --root "<root>" discovery prepare --effort "<effort>"
orchestrate --root "<root>" guide discovery
```

The prepared request freezes the request, explicit constraints, target project, baseline commit,
and baseline tree. Initialization is safe to repeat from independent model windows. Every
`discovery prepare` creates a new run and isolated workspace; no slot, provider plan, or quorum is
involved.

Read `run.json`, `request.md`, and `context.json`, then investigate only the clean detached
`source/` checkout. Ask the user in this model window whenever a material choice is unresolved.
Write the self-contained `technical-spec.md` and evidence graph under `graph/`, following the
Discovery guide. Do not inspect sibling runs, previous Discovery results, or the live target
repository. Do not implement the change.

Validate when helpful, then finalize:

```sh
orchestrate --root "<root>" discovery validate --effort "<effort>" --run "<run>"
orchestrate --root "<root>" discovery finalize --effort "<effort>" --run "<run>"
```

Report the run ID, artifact ID, outcome, and published Discovery directory. A blocked Discovery is
valid but cannot be reconciled. Stop after Discovery; never begin Reconcile automatically.
