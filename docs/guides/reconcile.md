# Reconcile

Reconcile turns the independent Discovery results you select into one final decision contract.
Exact model instructions are `orchestrate reconcile guide`.

It is intentionally a separate model phase: it analyzes the Discovery outputs deeply, but it does
not reopen the repository or perform another investigation.

## Run Reconcile

Collect at least two `IMPLEMENTATION_READY` Discovery directories, then open a fresh model window.

Invoke:

```text
$reconcile   "/path/to/discovery-1"   "/path/to/discovery-2"   "/path/to/discovery-3"
```

Add as many Discovery directories as you want considered.

Only the directories you explicitly pass are part of this Reconcile run.

## What it does

Reconcile compares the selected Discoveries and produces one **Reconciled Discovery**.

It may:

- combine complementary findings;
- recognize when different wording means the same thing;
- retain a strong finding that only one Discovery found;
- identify genuine disagreement or missing information;
- ask you a material intent question when the selected Discoveries cannot settle it.

It may not inspect source, Git history, tests, vendor documentation, web sources, private chats, or
mutable Discovery workspaces.

## What the output means

The Reconciled Discovery contains:

### Core result

The concise answer to:

> What are we actually going to do?

### Binding requirements

The auditable obligations Build must satisfy.

These are what Audit checks later.

### Advisory technical suggestions

Useful implementation direction found during Discovery.

Build may use them as a starting point, but they are not mandatory unless the underlying property
also appears as a binding requirement.

### Blocking issues

Anything the selected Discovery evidence and user clarification still could not resolve.

A blocked result cannot be approved for Build.

## Approval

The skill shows you the final `reconciled-discovery.md`.

If it asks whether you approve that result for Build, answer affirmatively only when the binding
requirements are what you actually want implemented.

Approval records the adoption receipt.

Before closing the window, keep:

```text
Effort ID
Reconciled document path
```

You will give both to the Build model.

Next: [Build and Audit](build-and-audit.md).
