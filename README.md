# Orchestrate

Orchestrate is a local, deterministic workflow that carries one engineering request through four
boundaries: independent investigation, agreement, implementation, and verification.

```text
Discovery
   │
   │ finalized public artifacts
   ▼
Consensus
   │
   │ Agreement
   ▼
Build (external)
   │
   │ exact implementation
   ▼
Audit
```

The models do the engineering reasoning. Orchestrate supplies the deterministic boundaries around
that reasoning: frozen inputs, independent Discovery runs, an inspectable evidence graph, immutable
artifacts, exact Git revisions, and mechanical validation where it is useful.

## Installation

You need a Rust toolchain (`cargo`) and `git`. To run the parallel provider workflow you also need
the provider CLIs you plan to use (for example `codex` and `claude`).

Install the CLI from this checkout:

```sh
cargo install --path crates/cli --locked
```

Then install the checked-in skills into the provider you use:

```text
Codex       mkdir -p ~/.codex/skills && cp -R skills/* ~/.codex/skills/
Claude Code mkdir -p ~/.claude/skills && cp -R skills/* ~/.claude/skills/
Cursor      mkdir -p ~/.cursor/skills && cp -R skills/* ~/.cursor/skills/
```

Verify the installation:

```sh
orchestrate --version
orchestrate init --help
orchestrate discovery prepare-all --help
orchestrate guide discovery
orchestrate guide consensus
orchestrate guide audit
```

You can also install from an agent session: open this checkout in Codex, Claude Code, or Cursor and
say:

```text
Install the Orchestration CLI and all checked-in skills from this checkout for the provider I am using now.
```

Full installation details, including how to avoid overwriting an unrelated `orchestrate` binary,
are in the [installation guide](docs/guides/agent-installation.md).

## Quick start

### Fast ticket workflow

This is the recommended flow when you have a ticket and do not want to run CLI commands yourself.

1. Write your raw notes as a Markdown file.
2. In a provider session, invoke the prep skill and let it polish the notes without inventing
   meaning:

   ```text
   $prep-discovery-ticket
   ```

3. Open the generated prepared file and paste the original ticket verbatim into the placeholder.
4. In each provider slot, invoke Discovery with that file path:

   ```text
   $discovery "/absolute/path/to/request.prepared.md"
   ```

   In Claude Code or Cursor, use `/discovery` instead of `$discovery`.

   Answer the material questions that session asks. When a slot is needed, the skill asks which
   slot it should own.
5. Collect the finalized output directory from each Discovery session.
6. In one fresh provider session, reconcile all of them:

   ```text
   $reconcile "/path/to/codex/output" "/path/to/claude/output" "/path/to/cursor/output"
   ```

   In Claude Code or Cursor, use `/reconcile`.

   Space- or comma-separated paths both work.

   The result is the finalized Agreement document and its published path.

The `$discovery` and `$reconcile` skills run the underlying `orchestrate` commands for you. The
manual command path below is for inspection, debugging, and advanced use.

### Manual command path

### 1. Prepare a request

Use the prep skills to turn a ticket or freeform idea into a reviewed Markdown request.

```text
Existing ticket   $prep-discovery-ticket      (Codex)  or  /prep-discovery-ticket
Freeform request  $prep-discovery-freeform    (Codex)  or  /prep-discovery-freeform
```

For tickets, paste the original ticket into the generated placeholder yourself so the preparation
model never rewrites it. Review the file, then initialize the effort:

```sh
orchestrate init --from-file "/absolute/path/to/request.prepared.md"
export EFFORT_ID="PASTE-EFFORT-ID"
```

### 2. Run Discovery

Run one independent investigation per provider slot. By default a new effort uses `codex`,
`claude`, and `cursor`; you can name your own slots with `--slot` or a provider plan.

```text
Codex  → $orchestrate-discovery      (slot codex)
Claude → /orchestrate-discovery      (slot claude)
Cursor → /orchestrate-discovery      (slot cursor)
```

Each session investigates the frozen Git baseline, asks you when a material question is unresolved,
and publishes a finalized `technical-spec.md` plus an evidence graph.

For a one-shot parallel run, declare a provider plan, prepare all slots in sequence, and launch the
providers concurrently — see [Parallel Discovery](docs/guides/parallel-discovery.md). The store,
workspaces, prompts, logs, and launch manifest all live outside the target repository; Discovery
never writes into the repository it is investigating, and the CLI rejects a store root or launch
directory placed inside that repository.

### 3. Reconcile and adopt

After every Discovery has finalized, run Consensus in one fresh session:

```text
$orchestrate-consensus        (Codex)  or  /orchestrate-consensus
```

Consensus resolves the eligible Discovery artifacts, produces an Agreement candidate, and stops.
Review the Agreement, then explicitly adopt it:

```sh
orchestrate agreement adopt --effort "$EFFORT_ID" --authorization-label "YOUR-NAME"
```

### 4. Build, register, and audit

Implement outside Orchestrate using whatever coding agent or process you prefer, then commit the
exact implementation and register it:

```sh
orchestrate implementation register --effort "$EFFORT_ID"
```

Finally run Audit against the adopted Agreement:

```text
$orchestrate-audit            (Codex)  or  /orchestrate-audit
```

The complete end-to-end path with every raw command is in the
[run guide](docs/guides/run.md).

## Key concepts

### An effort freezes the inputs

An **effort** is one workflow for one request. Initialization freezes the reviewed request, explicit
user constraints, the target repository's current Git commit and tree, the ordered provider
**slots**, and the Consensus **quorum**. Everything downstream compares against that frozen state.

### Discovery investigates; it does not implement

Every Discovery slot receives the same request and the same frozen baseline, but not the other
slots' private reasoning. It is expected to challenge factual assumptions with evidence and to ask
you — or finish blocked — rather than silently guess.

### The evidence graph explains why

Discovery records its reasoning as a directed graph:

```text
Question → Finding → Decision → Requirement
```

Orchestrate validates the graph's structure (no cycles, referenced nodes exist, accepted findings
have sources). The model decides what the evidence means; Rust decides whether the record is
structurally sound.

### Consensus reconciles; it does not re-investigate

Consensus reads the finalized Discovery artifacts and determines what they genuinely agree on. A
mandatory requirement needs at least `quorum` supporting slots, and the whole package must share a
common quorum, so a plan cannot be assembled from incompatible rotating majorities. Silence is not
disagreement.

### The Agreement is the implementation contract

Consensus produces an Agreement candidate, but that candidate is not Build authority until you
explicitly adopt it. Adoption is the one irreversible, human-checked transition from recommendation
to implementation.

### Audit evaluates an exact implementation

Build stays external. Orchestrate records the exact commit and tree you register, then Audit
evaluates that implementation against the exact adopted Agreement, requirement by requirement. The
final `PASS`, `CHANGES_REQUIRED`, or `BLOCKED` verdict is derived mechanically from coverage.

## Documentation

- [Complete run guide](docs/guides/run.md) — full manual end-to-end command path.
- [Ticket workflow](docs/guides/ticket-workflow.md) — fast ticket path with `$prep-discovery-ticket`, `$discovery`, and `$reconcile`.
- [Parallel Discovery](docs/guides/parallel-discovery.md) — run providers concurrently, then reconcile.
- [Example provider plan](docs/examples/providers.example.toml) — commented template for Codex, Claude Code, Cursor, and Provider X.
- [Prepare a request](docs/guides/request-preparation.md) — ticket and freeform preparation.
- [Discovery](docs/guides/discovery.md) — running investigations and handling questions.
- [Consensus and Agreement](docs/guides/consensus.md) — reconciling and adopting.
- [Build and Audit](docs/guides/build-and-audit.md) — registering and auditing.
- [Installation](docs/guides/agent-installation.md) — agent-led install details.
- [Architecture](docs/architecture.md) — authority boundaries, storage, lineage, and what Rust decides.

The model-facing phase instructions are available directly from the CLI:

```sh
orchestrate guide discovery
orchestrate guide consensus
orchestrate guide audit
```
