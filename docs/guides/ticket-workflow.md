# Ticket workflow

Use this when the work starts from an existing ticket.

## 1. Prepare

Create a small Markdown file with any supplemental context, then run:

```text
$prep-discovery-ticket
```

Give it the notes file, target repository, and a short effort name.

Paste the original ticket verbatim into the prepared file's placeholder and save it.

## 2. Discover

Open 3–5 fresh model windows.

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

Later, explicitly use `$build` with the effort and detailed implementation plan.

Build runs the delivery phases, reviews, corrections, implementation registration, and the final
Audit until it finishes or needs something only you can provide.

That is the full ticket workflow.

For the expanded version, see [Run Orchestrate](run.md).
