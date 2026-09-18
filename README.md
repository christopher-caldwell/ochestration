# Orchestrate v0.1

Orchestrate is a local Rust CLI for an inspectable engineering chain:

```text
Discovery → Consensus → adopted Agreement → external implementation → Audit
```

It stores pipeline state outside the source project (default: `~/.orchestration`), freezes a committed Git baseline for each three-slot cohort, publishes immutable artifact bundles, and leaves Build execution explicitly external.

## Commands

```text
orchestrate init --project /path/to/repo --effort name --request "..."
orchestrate discovery prepare|run|finalize ...
orchestrate consensus finalize ...
orchestrate agreement adopt ...
orchestrate implementation register ...
orchestrate audit finalize ...
orchestrate status|inspect|lineage ...
orchestrate guide [discovery|consensus|audit]
orchestrate skills install --prefix /absolute/path
```

`discovery run` uses one explicitly selected absolute-path provider command. It receives only the request/context and its frozen, run-owned source workspace and must emit one JSON `Opinion`; malformed or failed responses cannot publish authority.

The machine-readable result always separates `operation_status` from `semantic_outcome`. `build` is deliberately not a CLI phase or installed skill: the v0.1 boundary is `implementation register`.

## Current qualification

Native macOS offline machinery has exercised a complete scripted chain, dirty-worktree exclusion, public-artifact tamper rejection, a failing Audit verdict, bundled guides, and skill installation outside the checkout. Live provider/host and semantic-regression qualification have not yet run, so this tree is **not fully qualified** under the supplied specification.
