# Orchestrate documentation

Orchestrate guides an engineering request through four explicit boundaries:

```text
Discovery → Consensus → adopted Agreement → external Build → Audit
```

Use these docs based on what you are trying to do.

## Run Orchestrate

- [Complete run guide](guides/run.md) — the shortest end-to-end path from a prepared request through Audit.
- [Prepare a request](guides/request-preparation.md) — use the ticket or freeform prep skill, review the generated Markdown, and initialize an effort.
- [Discovery](guides/discovery.md) — prepare and run the three independent Discovery investigations, handle questions, validate, and finalize them.
- [Consensus and Agreement](guides/consensus.md) — reconcile the three finalized Discovery results, review the Agreement, and adopt it.
- [Build and Audit](guides/build-and-audit.md) — implement outside Orchestrate, register the exact Git commit, and audit it against the adopted Agreement.
- [Agent-led installation](guides/agent-installation.md) — install the CLI and five checked-in skills for Codex, Claude Code, or Cursor.

## Understand Orchestrate

- [Architecture](architecture.md) — the authority chain, artifacts, evidence graph, Git freezing, majority rule, Audit verdicts, storage, and what Rust does versus what the models decide.

## Model-facing phase guides

The installed CLI carries the authoritative instructions used by models during each phase:

```sh
orchestrate guide discovery
orchestrate guide consensus
orchestrate guide audit
```

The human-facing guides in this directory explain how to operate the workflow. The CLI guides define how the model should behave inside each phase.
