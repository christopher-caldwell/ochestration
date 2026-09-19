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
orchestrate init --project /path/to/repo --effort name --request-file ./TICKET.md --request-kind ticket
orchestrate init --from-file /absolute/path/prepared-request.md
orchestrate discovery prepare|validate|run|finalize ...
orchestrate consensus finalize ...
orchestrate agreement adopt ...
orchestrate implementation register ...
orchestrate audit finalize ...
orchestrate journal|status|inspect|lineage ...
orchestrate guide [discovery|consensus|audit]
orchestrate skills install --prefix /absolute/path
```

`--request-file` preserves the exact UTF-8 request and requires `--request-kind ticket|freeform`; inline requests default to `freeform`. `--constraint` is an explicit user clarification or governing instruction, not a derived ticket interpretation.

## Prepared Discovery input

For a reviewable request file, explicitly invoke either `prep-discovery-ticket` (supplemental notes plus a deliberate ticket placeholder) or `prep-discovery-freeform` (the whole freeform request). Each skill writes a separate Markdown file with absolute `root`, `project`, `effort`, `request_kind`, and optional `constraints` frontmatter. Review and edit that file before initializing:

```sh
orchestrate init --from-file "/absolute/path/prepared-request.md"
```

The file body is preserved exactly as the request. Ticket input is rejected until its literal original-ticket placeholder is replaced. `--from-file` cannot be mixed with `--root`, project, effort, request, request-file, request-kind, or constraint flags. Preparation never initializes an effort or starts Discovery.

`orchestrate skills install` remains a legacy three-phase helper; it is not the complete skill-set installation route. To install the CLI and all five skills for the current provider, use the prompt in [the agent-led installation guide](docs/guides/agent-installation.md).

`discovery prepare` creates a self-describing run workspace containing `run.json`, `request.md`, `context.json`, a clean detached Git `source/` checkout at the frozen baseline, a technical-spec template, and Markdown/frontmatter evidence nodes under `graph/`. `discovery run` uses `--provider-command` for one explicitly selected absolute-path provider executable and records its host/provider/model provenance during preparation.

The machine-readable result always separates `operation_status` from `semantic_outcome`. `build` is deliberately not a CLI phase or installed skill: the v0.1 boundary is `implementation register`.

Artifacts use schema v4. Existing stores use an intentionally incompatible format and are rejected rather than migrated. `manifest.json` hashes every public file; the journal is diagnostic only and may be absent after a hard crash without invalidating authority.

## Qualification

The deterministic suite covers the frozen-source, graph, consensus, adoption, implementation, Audit, and journal boundaries. Semantic-provider fixtures are opt-in and should be reviewed manually.
