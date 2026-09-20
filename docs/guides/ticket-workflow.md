# Ticket workflow

This is the shortest path for the common case: start from a ticket, run several Discovery slots,
and reconcile them into one finalized Agreement without typing raw CLI commands.

## 1. Write raw notes

Create a Markdown file with your supplemental notes and context. You do not need special
formatting.

## 2. Polish without inventing meaning

In a provider session, invoke:

```text
$prep-discovery-ticket
```

Give the model:

- the raw notes file;
- the target repository path;
- the store root, if it is not `~/.orchestration`;
- the effort slug, if you have one;
- the provider slot names and quorum, if they differ from `codex`, `claude`, `cursor` and `majority`.

The skill writes a prepared file such as:

```text
/Users/you/orchestration/acme/request.prepared.md
```

## 3. Paste the original ticket

Open the prepared file and replace the ticket placeholder with the original ticket verbatim:

```markdown
## Original Ticket

...paste the ticket exactly here...
```

Do not let the preparation model summarize or reinterpret the ticket.

## 4. Run Discovery in each provider

In each provider session, invoke:

```text
Codex:       $discovery "/absolute/path/to/request.prepared.md"
Claude/Cursor: /discovery "/absolute/path/to/request.prepared.md"
```

The skill asks which slot it should own if that is not obvious, initializes or reuses the shared
effort, prepares a private workspace, investigates, asks you material questions, and finalizes the
run.

Each session reports:

```text
output directory: /Users/you/.orchestration/projects/.../efforts/.../discovery/discovery-...
run ID:           run-...
artifact ID:      discovery-...
outcome:          IMPLEMENTATION_READY or BLOCKED
```

Repeat for each provider slot.

## 5. Reconcile the output directories

In one fresh provider session, pass every output directory to:

```text
Codex:        $reconcile \
  "/path/to/codex/output" \
  "/path/to/claude/output" \
  "/path/to/cursor/output"

Claude/Cursor: /reconcile \
  "/path/to/codex/output" \
  "/path/to/claude/output" \
  "/path/to/cursor/output"
```

Paths may be separated by spaces or commas.

The skill:

- verifies each directory is a finalized `IMPLEMENTATION_READY` Discovery artifact;
- confirms the artifacts belong to the same effort and cover the full cohort;
- runs `orchestrate consensus inputs` with the exact artifact IDs;
- reads the public specifications and writes a proposal;
- runs `orchestrate consensus finalize`;
- reports the finalized Agreement document path.

Review the Agreement before adopting it. Adoption remains a separate explicit authority boundary.

## 6. What the CLI does underneath

The skills use these commands so you do not have to type them:

```text
orchestrate init --from-file "<prepared file>"
orchestrate discovery prepare ...
orchestrate guide discovery
orchestrate discovery finalize ...

orchestrate consensus inputs ...
orchestrate guide consensus
orchestrate consensus finalize ...
```

The deterministic rules, frozen Git baseline, isolated workspaces, evidence graph, quorum checks,
and immutable artifact lineage are unchanged.

## Next steps

After the Agreement is finalized and reviewed, adopt it and continue with Build and Audit using the
[complete run guide](run.md).
