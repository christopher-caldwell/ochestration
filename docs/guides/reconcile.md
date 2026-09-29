# Reconcile

Reconcile turns the independent Discovery results you select into one final decision contract.
Exact model instructions are `orchestrate reconcile guide`.

It is intentionally a separate model phase: it analyzes the Discovery outputs deeply, but it does
not reopen the repository or perform another investigation. That repository-access boundary is a
model instruction; Rust mechanically limits admissible Reconcile evidence to the selected artifact
lineage but does not sandbox the model's filesystem.

Its only engineering evidence is the selected finalized Discovery outputs. Frozen explicit
constraints remain direct user authority and are mechanically preserved as governing requirements;
the frozen request and context may establish the effort's goal, identity, and lineage. Neither is a
second source of engineering investigation. A Reconcile-time user clarification is also direct user
authority, not engineering evidence.

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
It also checks that combining their requirements, acceptance criteria, exclusions, and known
limitations preserves the meaning the selected outputs support. A passing live acceptance command
does not alone say whether an external target must respond healthy or whether its observed response
must be classified correctly. A supported hard gate can remain pending without making synthesis
blocked; an unresolved material decision or missing engineering evidence cannot be filled in by
Reconcile's own investigation.

The result must converge on one leading direction and may:

- combine complementary findings;
- treat different wording as the same finding;
- keep a strong finding that only one Discovery found;
- surface genuine disagreement or missing information;
- ask you a material intent question only when the selected Discoveries genuinely cannot settle it.

That question is exceptional recovery: Discovery should ideally have surfaced it earlier. Repeated
Reconcile questions signal Discovery coverage or guidance to improve, not a conversational phase to
extend.

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

The skill reports the final outcome and stops. An `IMPLEMENTATION_READY` result includes the exact
`reconciled-discovery.md` path and normal Build-scaffold handoff. A `BLOCKED` result includes its
published artifact, unresolved issue, and needed upstream decision or evidence, without a ready
Build handoff. It does not ask for Build approval, create Adoption, or begin Build.

Before closing the window, keep:

```text
Effort ID
Reconciled document path
```

Only an implementation-ready artifact can enter Build. A later explicit Build invocation uses
its exact artifact and effort to authorize Build.

For `IMPLEMENTATION_READY`, next: [Build and Audit](build-and-audit.md).
