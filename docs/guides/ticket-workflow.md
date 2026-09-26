# Ticket workflow

Use this when the work starts from an existing ticket.

## 1. Prepare

Provide the original ticket as a Markdown file, with optional supplemental context, then run:

```text
$prep-discovery-ticket
```

Give it the ticket file, optional context, target repository, and a short effort slug.

Prepare may ask focused questions about consequential missing user intent. It preserves the
original ticket verbatim in the prepared file. Review that file before Discovery. Technical
investigation begins in Discovery.

## 2. Discover

Open three fresh model windows. Add more independent Discoveries deliberately when broader
investigation or corroboration is useful.

In every window:

```text
$discovery "/absolute/path/request.prepared.md"
```

Collect the **published Discovery directory** from every `IMPLEMENTATION_READY` result.

## 3. Reconcile

Open a fresh model window:

```text
$reconcile "/path/discovery-1" "/path/discovery-2" "/path/discovery-3" ...
```

Review the Reconciled Discovery. Reconcile stops after reporting the selected direction and artifact.

Keep the reconciled document path and effort ID.

## 4. Build

Later, use `$build` to prepare the effort and detailed implementation plan. `$build` does not run
the CLI or start the driver; explicitly authorize the exact Build CLI operation when ready.

Build runs the delivery phases, reviews, corrections, implementation registration, and the final
Audit until it finishes or cannot safely continue automatically. A user or infrastructure need is
one common blocker.

That is the full ticket workflow.

For the expanded version, see [Run Orchestrate](run.md).
