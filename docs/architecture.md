# Architecture

```text
Discovery 1 ─┐
Discovery 2 ─┼──→ Reconciled Discovery → STOP
Discovery N ─┘

Later: explicit Build → Adoption → Implementation → Audit
```

## Instruction ownership

Checked-in skills are stable dispatchers. Each one runs `orchestrate <action> guide` and follows
whatever the installed binary prints. Those guides, and the Build `plan.json` / `config.toml`
templates, are Markdown and text compiled into the binary. Host files (`AGENTS.md`, `CLAUDE.md`,
and the Cursor orchestration rule) only explain how to install or update that CLI and those
dispatchers.

Work, review, final Audit, and unblock are internal Build roles. They are not installed skills.
Whenever Build starts or resumes, the driver writes the role guides embedded in the running
CLI into the Build directory, replacing any previously generated copies.

## Authority boundaries

The model window owns interaction: it investigates, asks the user material questions, reconciles
meaning, and evaluates implementation. Rust is a referee: it freezes exact inputs, creates
workspaces, validates structure and provenance, publishes immutable bundles, binds lineage,
drives the fixed Build work/review loop, and derives Audit verdicts. Rust never votes, ranks
models, assigns confidence, or determines engineering truth.

An effort freezes a reviewed request, explicit constraints, canonical target project, baseline
commit, and baseline tree. It intentionally does not freeze Discovery count, provider identity,
slots, or quorum. Provider/model metadata remains attached to each Discovery run as provenance.
The canonical project plus human effort slug identifies one immutable unit. Concurrent identical
initialization converges on it; changed request kind, body, or constraints under that slug hard-fail.

## Discovery

Each Discovery gets a stable human provenance label and a unique workspace with a clean detached `source/` checkout at the frozen
baseline. Its public artifact contains run provenance, evidence graph, technical specification, and
summary. Findings classify verification as inspection, corroborated, or experiment. The reconciler
can never reopen the repository, so the public result must stand alone.

## Reconcile

Reconcile binds an explicit set of at least two unique finalized `IMPLEMENTATION_READY` Discovery
artifacts. Rust rejects duplicates, blocked artifacts, and cross-effort/context/baseline inputs;
the read-only binding publishes nothing. No later Discovery is inferred or added.

Reconcile is closed-world. Its only engineering evidence is the exact public Discovery artifacts
that were selected. Frozen explicit constraints remain direct pre-Discovery user authority, which
Rust mechanically preserves as governing requirements; they are not engineering evidence discovered
by Reconcile. The frozen request and context remain available for the effort's goal, identity, and
lineage, and to ensure the selected artifacts answer the same effort, but may not be reinterpreted
as another source of technical investigation. Reconcile-time user clarification is likewise direct
user authority, not engineering evidence. Exact model limits are supplied by `orchestrate reconcile
guide`.

One Reconciled Discovery bundle contains `reconciled-discovery.md`,
`reconciled-discovery.json`, and `manifest.json`; its parents are exactly the selected Discovery
artifacts. `reconciled-discovery.json` is the one authoritative contract and deterministically
renders `reconciled-discovery.md`; the reviewed document cannot add obligations outside it. The
contract separates exhaustive binding `requirements` from advisory `technical_suggestions`.
Rust requires each ordinary requirement and suggestion to trace to a selected Discovery artifact.
One source is provenance, not a vote. Rust adds frozen explicit user constraints as governing
authority. An explicit Reconcile-time user clarification is recorded as direct user authority.

An empty `blocking_issues` list produces an implementation-ready result. Non-empty issues produce
`BLOCKED`. Reconcile then stops without Adoption or Build approval.

## Adoption, Build, and Audit

Explicit Build invocation is authorization and creates or reuses Adoption for one exact Reconciled
Discovery. A prepared Build stores its exact authority, phase/task grouping, host settings, controller state, and durable
role reports in the external store. Rust moves one worker and independent reviewer through whole
delivery phases; task IDs do not create extra stops. Registration records the adoption, reconciled
artifact, Discovery baseline, actual Build starting commit/tree, exact target commit/tree, producer declaration, and status while
retaining an immutable snapshot.

Audit evaluates the binding requirements from the exact Reconciled Discovery against that exact
implementation. Rust requires one coverage row per binding requirement and derives the verdict:
any failure gives `CHANGES_REQUIRED`; otherwise unknown or missing coverage gives `BLOCKED`;
otherwise a submitted implementation passes.

## Storage

Stores use format and artifact schema version 6 and reject older stores rather than migrating them.
Project and effort directories use safe human names; internal project, effort, context, artifact,
commit, tree, and lineage identities remain in metadata. Project-name collisions across different
canonical repositories fail clearly.
Phase directories are `discovery`, `reconcile`, `adoption`, `build`, and `audit`. Immutable bundle
manifests hash payloads and record exact parents. The journal is diagnostic; artifact manifests are
the authority lineage.
