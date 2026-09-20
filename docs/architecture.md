# Architecture

Orchestrate is intentionally a small deterministic shell around capable engineering agents.

The product is the authority chain:

```text
Discovery slots (codex, claude, cursor, ...)
            │
            ▼
        Consensus → adopted Agreement → external Build → Audit
```

Models perform semantic engineering work. Rust freezes inputs, validates mechanical invariants, binds exact artifacts together, and prevents a later phase from quietly changing what an earlier phase established.

## Design principle

A useful way to divide responsibility is:

### The model decides

- what the request means;
- whether a factual statement in a ticket is supported by evidence;
- which code, tests, history, documentation, or experiments matter;
- whether an ambiguity is material;
- what implementation direction the evidence supports;
- whether the Discovery results express the same underlying conclusion;
- whether an implementation satisfies a requirement.

### Rust decides

- which exact request and constraints define the effort;
- which exact Git commit every Discovery inspects;
- that the Orchestrate store and any explicit launch directory stay outside the target repository;
- whether a Discovery graph is structurally valid;
- whether a blocked question makes the Discovery blocked;
- whether every Consensus input belongs to the same cohort;
- whether every Consensus requirement meets the configured quorum;
- whether the entire mandatory package shares one common quorum;
- which exact Agreement was adopted;
- which exact implementation commit/tree was registered;
- whether Audit covers the Agreement completely;
- the final Audit verdict from the coverage states.

Rust does not attempt to judge engineering truth.

## 1. Effort, context, and cohort

An **effort** is one Orchestrate workflow for one engineering request.

Initialization freezes three things that matter downstream:

1. the reviewed request;
2. any explicit user constraints;
3. the target repository's current committed Git baseline.

The request and constraints form a context identity. A **cohort** binds that context to one
baseline commit and tree plus an ordered, provider-named slot set (default `codex`, `claude`,
`cursor`; extended via `--slot` or `--providers`) and a consensus `quorum`.

All Discovery runs in the cohort therefore investigate the same subject.

```text
Effort
├── Context
│   ├── request kind
│   ├── exact request
│   └── explicit constraints
└── Cohort
    ├── baseline commit
    ├── baseline tree
    ├── slots: codex, claude, cursor, ...
    └── quorum
```

A constraint is direct user authority. It is not intended to be an AI-extracted summary of a ticket.

## 2. Prepared request input

`orchestrate init --from-file` accepts a Markdown document with YAML frontmatter.

```markdown
---
root: /absolute/path/to/.orchestration
project: /absolute/path/to/repository
effort: example-effort
request_kind: ticket
constraints: []
slots: [codex, claude, cursor]
quorum: majority
---

# Discovery Request

...
```

Rust parses only the frontmatter. The Markdown body is opaque request content and is preserved as the effort request.

For ticket requests, the preparation skill deliberately leaves the original ticket for the user to paste manually. This prevents the preparation model from becoming an accidental ticket interpreter before Discovery starts.

## 3. Independent Discovery runs

Each cohort has one run workspace per declared slot. Slots are independent provider identities
(`codex`, `claude`, `cursor`, or a custom name such as `provider-x`).

`discovery prepare` creates a separate self-describing workspace for one slot:

```text
run.json
request.md
context.json
source/
graph/
technical-spec.md
```

`run.json` records the run identity and provenance, including the slot, effort, cohort, baseline, host, provider, model, and model effort.

`source/` is a clean detached Git checkout at the cohort baseline. It includes Git history, but excludes dirty and untracked state from the operator's working checkout.

The workspaces are isolated from one another. Independence is about peer reasoning: one slot must
not receive another slot's private work.

For a first-class parallel run, `discovery prepare-all` prepares every slot sequentially from a
provider plan and emits `launch.json`; the bundled launcher script then starts the headless
providers concurrently, each confined to its own workspace and log directory. The Rust CLI itself
still never launches model providers.

## 4. Discovery evidence graph

Discovery records the important reasoning path as a lightweight directed acyclic graph.

The four node kinds are:

```text
Question
Finding
Decision
Requirement
```

Typical flow:

```text
Question ─→ Finding ─→ Decision ─→ Requirement
               └──────────────────→ Requirement
```

Nodes may depend on multiple earlier nodes. Rust validates that referenced nodes exist and that dependencies do not form cycles.

### Question states

Questions use:

- `open` — still being investigated;
- `answered` — an evidence-supported answer is available;
- `no_change` — investigation establishes no implementation change or requirement;
- `blocked` — the answer is not established and can materially change the implementation contract.

Any blocked Question blocks the Discovery. A required open Question cannot become implementation-ready. A required answered Question must have an accepted Finding directly depending on it.

The intent is simple: a material unknown must not silently become an implementation assumption.

### Findings and requirements

Accepted Findings need at least one source reference. Mandatory accepted Requirements must trace to accepted evidence and cannot silently depend on rejected or invalidated evidence.

Rust validates those relationships. It does not decide whether the cited evidence is persuasive.

## 5. Finalized Discovery artifacts

A completed Discovery publishes an immutable public bundle. Its key human-readable output is `technical-spec.md`; the graph and run metadata explain how the run got there.

A Discovery is either:

```text
IMPLEMENTATION_READY
BLOCKED
```

Blocked Discoveries are preserved but cannot participate in implementation-ready Consensus.

Private conversation transcripts are not part of the authority chain. The public artifact should be sufficient for a later model to understand the Discovery result.

## 6. Consensus

Consensus consumes exactly one finalized, eligible Discovery artifact from every slot in the same cohort.

The exact artifact set is resolved before reconciliation starts, an ambiguous slot is resolved with
the user first, and finalization binds those same artifacts. A Consensus lineage therefore always
names the artifacts the proposal was actually derived from, and a Discovery artifact that becomes
eligible later cannot change them.

It does not re-investigate the repository. Its job is semantic reconciliation:

- normalize equivalent conclusions;
- distinguish silence from disagreement;
- preserve material conditions;
- retain meaningful dissent;
- identify which slots support each proposed requirement.

### Quorum package rule

Each Consensus-derived mandatory requirement needs at least `quorum` supporters. `quorum` defaults
to strict majority (`floor(N/2)+1`) and is frozen at initialization.

Orchestrate also intersects the supporter sets across the entire mandatory package. The
intersection must itself contain at least `quorum` slots.

For example:

```text
R1 supporters = {codex, claude}
R2 supporters = {claude, cursor}

common intersection = {claude}
```

Each row individually has a majority, but the package does not. Orchestrate therefore rejects it as an implementation-ready package. This prevents a Consensus model from assembling a plan that no two Discovery runs actually support as a whole.

Explicit user constraints are governing requirements, not votes.

## 7. Agreement and adoption

An eligible Consensus result produces an Agreement candidate.

The Agreement is still not Build authority until the user explicitly adopts it.

```text
Consensus
   ↓
Agreement candidate
   ↓
explicit adoption
   ↓
Build authority
```

The adoption artifact binds the exact Agreement artifact. This is the human control point between analysis and implementation.

## 8. Build is external

Orchestrate does not schedule or execute implementation in v0.1.

The adopted Agreement can be given to Codex, Claude, Cursor, another tool, or a human engineer.

When Build is complete, the implementation is committed to Git and registered with Orchestrate. Registration verifies that the target repository matches the effort and that the implementation commit descends from the adopted Agreement's baseline.

Orchestrate then records:

```text
starting baseline
exact target commit
exact target tree
implementation status
```

It also retains an immutable source snapshot for Audit.

## 9. Audit

Audit evaluates:

```text
exact Agreement
+
exact adoption
+
exact registered implementation
```

The assessor supplies one coverage row per Agreement requirement:

- `pass`;
- `fail`;
- `unknown`;
- `not_applicable` with justification.

A pass or failure must include evidence. A failure also includes a bounded correction.

Rust derives the verdict:

```text
any fail
    → CHANGES_REQUIRED

otherwise any unknown or missing row
    → BLOCKED

otherwise, if implementation is fully submitted
    → PASS
```

A partial or blocked implementation cannot pass.

The assessor does not independently choose the overall verdict.

## 10. Immutable bundles and lineage

Finalized phase outputs are published as immutable bundles. Each bundle contains a manifest that hashes its public payloads and records parent artifact references.

Those parent references form the authoritative lineage.

Conceptually:

```text
Audit
├── Agreement
│   └── Consensus
│       ├── Discovery codex
│       ├── Discovery claude
│       └── Discovery cursor
│           (one Discovery artifact per cohort slot)
├── Adoption
│   └── Agreement
└── Implementation
    └── Adoption
```

`orchestrate lineage` reconstructs that relationship from the artifact manifests.

## 11. Storage and journal

The default store is `~/.orchestration`.

The store and any explicit parallel Discovery launch directory must be outside the target
repository. `orchestrate init` and `orchestrate discovery prepare-all` reject those paths when they
fall inside the repository being investigated, so Orchestrate does not write generated workspace,
prompt, log, or manifest files into the codebase it is studying.

Conceptually it contains:

```text
projects/
  <project-id>/
    efforts/
      <effort-id>/
        request.md
        effort.json
        project.json
        journal.jsonl
        discovery/
        consensus/
        agreement/
        build/
        audit/
snapshots/
  <commit>/
    source/
    source-manifest.json
```

The journal records operational events such as workspace preparation, finalization, adoption, implementation registration, and Audit completion.

The journal is deliberately **not authoritative**. Deleting or losing the journal does not invalidate finalized artifacts or change the authority chain.

Run identity belongs in `run.json`; authority belongs in finalized artifacts and their lineage.

## 12. Interactive phase operation

Phase work happens in an interactive provider session. The installed phase skills (`orchestrate-discovery`, `orchestrate-consensus`, `orchestrate-audit`) drive the normal lifecycle: they run the deterministic CLI commands while the model investigates, reasons, and asks the user questions in the host conversation.

The CLI deliberately does not launch model providers. There are no `run` subcommands, callbacks, resumable subprocess protocols, session IDs, provider adapters, polling loops, or embedded chat machinery, because the provider host already supplies the interaction layer.

Running the underlying commands by hand remains supported for debugging, inspection, and scripted use. The deterministic phase rules are the same either way.

## 13. Explicit v0.1 boundaries

The current product intentionally does not include:

- internal Build execution;
- task scheduling;
- distributed workers;
- a generic workflow engine;
- dashboards or a web UI;
- semantic confidence scoring;
- a database-backed evidence system;
- Rust logic that attempts to decide engineering meaning.
