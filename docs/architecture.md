# Architecture

```text
Discovery 1 ─┐
Discovery 2 ─┼──→ Reconciled Discovery → Adoption → Implementation → Audit
Discovery N ─┘
```

## Authority boundaries

The model window owns interaction: it investigates, asks the user material questions, reconciles
meaning, and evaluates implementation. Rust is a referee: it freezes exact inputs, creates
workspaces, validates structure and provenance, publishes immutable bundles, binds lineage, and
derives Audit verdicts. Rust never votes, ranks models, assigns confidence, launches a provider,
or determines engineering truth.

An effort freezes a reviewed request, explicit constraints, canonical target project, baseline
commit, and baseline tree. It intentionally does not freeze Discovery count, provider identity,
slots, or quorum. Provider/model metadata remains attached to each Discovery run as provenance.
Concurrent identical initialization writes a staged effort and atomically publishes it, so multiple
model windows converge on the same frozen identity.

## Discovery

Each Discovery gets a unique workspace with a clean detached `source/` checkout at the frozen
baseline. Its public artifact contains run provenance, evidence graph, technical specification, and
summary. The reconciler can never reopen the repository, so the public result must stand alone.

## Reconcile

Reconcile binds an explicit set of at least two unique finalized `IMPLEMENTATION_READY` Discovery
artifacts. Rust rejects duplicates, blocked artifacts, and cross-effort/context/baseline inputs;
the read-only binding publishes nothing. No later Discovery is inferred or added.

Reconcile is closed-world. Its allowed evidence is the frozen request/constraints and those exact
public Discovery artifacts. It may deeply compare agreement, disagreement, silence, omissions,
and strong minority evidence. It may not inspect source, Git, tests, web documentation, private
chats, or mutable workspaces.

One Reconciled Discovery bundle contains `reconciled-discovery.md`,
`reconciled-discovery.json`, and `manifest.json`; its parents are exactly the selected Discovery
artifacts. `reconciled-discovery.json` is the one authoritative contract and deterministically
renders `reconciled-discovery.md`; the reviewed document cannot add obligations outside it. The
contract separates exhaustive binding `requirements` from advisory `technical_suggestions`.
Every ordinary model-derived requirement and suggestion must trace to at least one selected
Discovery artifact (and, when supplied, an existing graph node). One source is enough; this is
provenance, not voting. Frozen explicit user constraints are mechanically added as governing
authority. An explicit Reconcile-time user clarification is distinct direct user authority; a
model cannot manufacture governing authority with a flag.

An empty `blocking_issues` list produces an implementation-ready result. Non-empty issues produce
`BLOCKED`; a blocked result cannot be adopted.

## Adoption, Build, and Audit

Adoption is a human authorization receipt for one exact Reconciled Discovery. Build remains
external. Registration records the adoption, reconciled artifact, starting baseline, exact target
commit/tree, producer declaration, and status while retaining an immutable snapshot.

Audit evaluates only the binding requirements from the exact Reconciled Discovery against that
exact implementation. It can inspect the snapshot and run tests, but cannot invent product
requirements or elevate advisory suggestions. One coverage row is required per binding
requirement. Any failure gives `CHANGES_REQUIRED`; otherwise unknown or missing coverage gives
`BLOCKED`; otherwise a submitted implementation passes.

## Storage

Stores use format and artifact schema version 5 and reject older stores rather than migrating them.
Phase directories are `discovery`, `reconcile`, `adoption`, `build`, and `audit`. Immutable bundle
manifests hash payloads and record exact parents. The journal is diagnostic; artifact manifests are
the authority lineage.
