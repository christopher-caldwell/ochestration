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

The result must converge on one leading direction and may:

- combine complementary findings;
- treat different wording as the same finding;
- keep a strong finding that only one Discovery found;
- surface genuine disagreement or missing information;
- ask you a material intent question when the selected Discoveries cannot settle it.

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

A blocked result cannot be built.

## Stop boundary

The skill shows you the final `reconciled-discovery.md`, reports the selected direction and path,
and stops. It does not ask for Build approval, create Adoption, or begin Build.

Before closing the window, keep:

```text
Effort ID
Reconciled document path
```

Later, an explicit Build invocation uses these to authorize Build.

Next: [Build and Audit](build-and-audit.md).
