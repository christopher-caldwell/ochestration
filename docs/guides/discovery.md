# Discovery

Discovery is the investigation phase. Exact model instructions are `orchestrate discovery guide`.

The easiest way to use it is to open several fresh model windows and give each one the same
prepared request.

## Run one Discovery

In a fresh model window:

```text
$discovery "/absolute/path/to/request.prepared.md"
```

Then let it work.

The skill handles the Orchestrate CLI, creates its own isolated frozen source checkout, investigates
the repository, writes the evidence graph and technical specification, validates them, and
finalizes the result.

You do not need to initialize an effort or prepare a workspace manually.

## Run several independently

Three independent Discoveries are the normal default. Run additional independent Discoveries when
you deliberately want broader investigation or additional corroboration:

```text
Window 1 → $discovery "/same/request.prepared.md"
Window 2 → $discovery "/same/request.prepared.md"
Window 3 → $discovery "/same/request.prepared.md"
Window N → $discovery "/same/request.prepared.md"
```

Do not share one Discovery's conclusions with another. Their independence is useful.

## Questions are normal

Discovery may ask you a question when an unresolved decision could materially change what should
be built.

Answer that model window normally.

The answer stays inside that active Discovery run. Do not rewrite the prepared request or re-run
initialization under the same effort slug; the prepared request is immutable for that effort.

If a question remains materially unresolved, the Discovery may correctly finish as `BLOCKED`
instead of guessing. A finalized blocked artifact is immutable; if later information resolves it,
start a fresh Discovery under the same effort.

## What to save

When a Discovery finishes, it reports:

```text
run ID
human source label
artifact ID
outcome
published Discovery directory
```

For normal usage, save the **published Discovery directory**.

That directory is what you pass to Reconcile.

Only `IMPLEMENTATION_READY` Discoveries can be reconciled. Keep blocked results for reference, but
do not include them in the Reconcile input set.

## What Discovery produces internally

Each run records:

```text
Question
Finding
Decision
Requirement
```

plus a self-contained `technical-spec.md`.

Each finding records whether it was established by inspection, corroborated evidence, or a
controlled experiment. Experiments are reported with their setup, observation, scope, and limits;
the classification is not a score and does not automatically outrank other evidence.

You do not need to inspect those files during normal usage. They exist so Reconcile can deeply
analyze the finished Discovery without reopening the repository.

Next: [Reconcile](reconcile.md).
