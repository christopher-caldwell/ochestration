# Orchestrate

Orchestrate is a local workflow for taking an engineering request through independent investigation, agreement, implementation, and verification.

```text
Discovery
   │
   │ finalized public artifacts
   ▼
Consensus
   │
   │ Agreement
   ▼
Build
   │
   │ exact implementation
   ▼
Audit
```

Models do the engineering reasoning. Orchestrate provides the deterministic boundaries around that work: frozen inputs, independent Discovery runs, inspect-able evidence, immutable artifacts, exact Git revisions, and mechanical validation where it is useful.

## Quick start

### 1. Install

Open this repository in Codex, Claude Code, or Cursor and say:

```text
Install the Orchestration CLI and skills from this checkout for the provider I am using now.
```

This installs the `orchestrate` CLI and the included preparation and phase skills.

### 2. Prepare a request

For an existing engineering ticket, invoke:

```text
# Codex
$prep-discovery-ticket

# Claude / Cursor
/prep-discovery-ticket
```

For a freeform request, invoke:

```text
# Codex
$prep-discovery-freeform

# Claude / Cursor
/prep-discovery-freeform
```

The skill creates a reviewable Markdown request file. For ticket workflows, you paste the original ticket into the generated placeholder yourself so the preparation model never rewrites it.

Review the file, then initialize the effort:

```sh
orchestrate init --from-file "/absolute/path/to/request.prepared.md"
```

### 3. Run the workflow

An Orchestrate effort uses three independent Discovery runs over the same request and Git baseline.

A common setup is:

```text
Discovery A → Codex
Discovery B → Claude Code
Discovery C → Cursor
```

Each investigator works independently, can inspect source and Git history, and may ask you questions when material information is missing.

Once all three Discovery artifacts are complete:

```text
Discovery A ─┐
Discovery B ─┼─→ Consensus → adopt Agreement → Build → Audit
Discovery C ─┘
```

See the [step-by-step run guide](docs/guides/run.md) for the exact commands and workflow.

## Key concepts

### Discovery is investigation, not implementation

Each Discovery receives the same reviewed request and the same frozen Git baseline, but not the other investigators' reasoning.

Discovery is expected to challenge factual assumptions with evidence. If an unanswered question could materially change what should be built, it should ask you or finish as blocked rather than silently guessing.

### The evidence graph explains why

Discovery records important:

```text
Question → Finding → Decision → Requirement
```

Orchestrate validates the graph's structure. The model remains responsible for deciding what the evidence actually means.

### Consensus reconciles; it does not re-investigate

Consensus consumes the three finalized Discovery results and determines what they genuinely agree on.

A useful 2-of-3 conclusion is valid. Silence is not disagreement. Orchestrate prevents a final package from being assembled out of incompatible rotating majorities.

### The Agreement is the implementation contract

Consensus produces an Agreement candidate.

It does not authorize implementation until you explicitly adopt it.

Build itself remains external: use whichever coding agent or development process you want.

### Audit evaluates an exact implementation

After Build, Orchestrate records the exact Git commit and tree.

Audit evaluates that exact implementation against the exact adopted Agreement, requirement by requirement.

The final verdict is derived mechanically from Audit coverage.

## Documentation

Start here:

* **[Complete run guide](docs/guides/run.md)** — step-by-step from request preparation through Audit.
* **[Discovery run guide](docs/guides/discovery.md)** — running the three independent investigations and handling user questions.
* **[Consensus and Agreement guide](docs/guides/consensus.md)** — reconciling the three results and adopting the contract.
* **[Build and Audit guide](docs/guides/build-and-audit.md)** — registering the exact implementation and auditing it.
* **[Installation guide](docs/guides/agent-installation.md)** — how provider-led installation works.
* **[Architecture](docs/architecture.md)** — artifacts, authority boundaries, evidence graph, frozen Git state, lineage, and the role of the Rust referee.

The model-facing phase instructions are also available directly from the CLI:

```sh
orchestrate guide discovery
orchestrate guide consensus
orchestrate guide audit
```
