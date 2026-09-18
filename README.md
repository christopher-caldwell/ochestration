# Orchestrate v0.1

The controlling roadmap is [the v0.1 alignment specification](spec/ALIGNMENT.md). Earlier material under `spec/orchestrate-specification/` is retained as design history and test inspiration, not the controlling contract.

Orchestrate is a local Rust CLI for an inspectable engineering chain:

```text
Discovery → Consensus → adopted Agreement → external implementation → Audit
```

It stores pipeline state outside the source project (default: `~/.orchestration`), freezes a committed Git baseline for each three-slot cohort, publishes immutable multi-file bundles, records an append-only non-authoritative journal, and leaves Build execution explicitly external.

## Commands

```text
orchestrate init --project /path/to/repo --effort name --request "..."
orchestrate discovery prepare|validate|run|finalize ...
orchestrate consensus finalize ...
orchestrate agreement adopt ...
orchestrate implementation register ...
orchestrate audit finalize ...
orchestrate journal|status|inspect|lineage ...
orchestrate guide [discovery|consensus|audit]
orchestrate skills install --prefix /absolute/path
```

`discovery prepare` creates a run workspace containing `context.json`, committed `source/`, a technical-spec template, and Markdown/frontmatter evidence nodes under `graph/`. `discovery run` uses one explicitly selected absolute-path provider command; its JSON protocol is internal and it produces the same workspace bundle.

The machine-readable result always separates `operation_status` from `semantic_outcome`. `build` is deliberately not a CLI phase or installed skill: the v0.1 boundary is `implementation register`.

Artifacts use schema v3. Existing stores use an intentionally incompatible format and are rejected rather than migrated. `manifest.json` hashes every public file; the journal is diagnostic only and may be absent after a hard crash without invalidating authority.

## Qualification

The deterministic suite covers the frozen-source, graph, consensus, adoption, implementation, Audit, and journal boundaries. Semantic-provider fixtures are opt-in and should be reviewed manually.
