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

An Orchestrate effort runs independent Discovery investigations over the same request and Git
baseline. The installed phase skills own the normal lifecycle, so you work in provider sessions
instead of typing every command. A common assignment is:

```text
Discovery codex  → Codex  → $orchestrate-discovery
Discovery claude → Claude → /orchestrate-discovery
Discovery cursor → Cursor → /orchestrate-discovery
```

For a one-shot, first-class parallel run, declare a provider plan and launch every provider
concurrently — see [Parallel Discovery](docs/guides/parallel-discovery.md).

Each `orchestrate-discovery` session investigates the frozen baseline independently, asks you
questions when material information is missing, and finalizes the run for you. It reports the run
ID, the Discovery artifact ID, and the outcome.

Once every Discovery artifact is complete:

```text
$orchestrate-consensus → review → explicit adopt → external Build
    → orchestrate implementation register --effort "$EFFORT" → $orchestrate-audit
```

Consensus resolves the eligible Discovery artifacts before it reconciles them and binds those same
artifacts at finalization, and registration infers the canonical project and the sole Adoption
receipt, so the typed surface stays small. See the [step-by-step run guide](docs/guides/run.md)
for the full workflow and the raw commands for manual use.

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

Consensus consumes the finalized Discovery results and determines what they genuinely agree on.

A strict-majority conclusion is valid. Silence is not disagreement. Orchestrate prevents a final
package from being assembled out of incompatible rotating majorities.

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
* **[Parallel Discovery guide](docs/guides/parallel-discovery.md)** — run the same request across providers concurrently, then reconcile.
* **[Discovery run guide](docs/guides/discovery.md)** — running the independent investigations and handling user questions.
* **[Consensus and Agreement guide](docs/guides/consensus.md)** — reconciling the results and adopting the contract.
* **[Build and Audit guide](docs/guides/build-and-audit.md)** — registering the exact implementation and auditing it.
* **[Installation guide](docs/guides/agent-installation.md)** — how provider-led installation works.
* **[Architecture](docs/architecture.md)** — artifacts, authority boundaries, evidence graph, frozen Git state, lineage, and the role of the Rust referee.

The model-facing phase instructions are also available directly from the CLI:

```sh
orchestrate guide discovery
orchestrate guide consensus
orchestrate guide audit
```
