# Prepare a Discovery request

Request preparation gives you a reviewable input before an effort is created.

The preparation model is allowed to improve **structure**, not **meaning**.

```text
raw notes
   ↓
prep skill
   ↓
prepared Markdown
   ↓
human review/edit
   ↓
orchestrate init --from-file ...
```

There are separate skills for tickets and freeform requests because the trust boundary is different.

## Ticket requests

Use `prep-discovery-ticket` when an existing ticket is the best available statement of requested behavior.

The prep model should receive your supplemental notes, not the ticket itself. Give it:

- the raw Markdown file containing your notes/context;
- the target repository path;
- the effort name you want to use, if you already have one;
- an output path only when you care where the prepared file is written.

The skill creates a file like:

```markdown
---
root: /Users/me/.orchestration
project: /Users/me/code/my-project
effort: age-377-rerun
request_kind: ticket
constraints: []
slots: [codex, claude, cursor]
quorum: majority
---

# Discovery Request

## Original Ticket

<!-- PASTE ORIGINAL TICKET VERBATIM HERE -->

## Additional User Context

...

## Explicit Unknowns

...
```

The prep skill does **not** fetch, summarize, reproduce, or reinterpret the original ticket.

### Before Discovery

Open the prepared file and:

1. paste the original ticket verbatim under `Original Ticket`;
2. review the organized supplemental context;
3. correct anything whose wording, certainty, speaker, condition, or scope changed;
4. inspect `constraints` and ensure every entry is an explicit user instruction rather than an inferred ticket requirement.

Orchestrate rejects a ticket file while the literal ticket placeholder is still present.

Then run Discovery in each provider session. The `discovery` skill initializes or reuses the
shared effort for you:

```text
$discovery "/absolute/path/to/request.prepared.md"
```

In Claude Code or Cursor, invoke `/discovery` with the same path.

For manual or debugging use, the same file can be initialized directly:

```sh
orchestrate init --from-file "/absolute/path/to/request.prepared.md"
```

## Freeform requests

Use `prep-discovery-freeform` when the request itself is primarily your notes or prose rather than a separate authoritative ticket.

The prep skill may organize the whole request into headings such as:

```text
Goal
Context
Explicit Unknowns
Supporting Information
```

It must preserve:

- substantive statements;
- who said what;
- uncertainty and confidence;
- conditions and exceptions;
- scope;
- conflicts between supplied statements.

It must not invent requirements or silently resolve ambiguity.

Review the generated file, then run Discovery in each provider session:

```text
$discovery "/absolute/path/to/request.prepared.md"
```

In Claude Code or Cursor, invoke `/discovery` with the same path.

## Frontmatter fields

Prepared files contain exactly the machine-facing setup values Orchestrate needs:

```yaml
root: /absolute/path/to/.orchestration
project: /absolute/path/to/target-repository
effort: short-effort-name
request_kind: ticket
constraints: []
slots: [codex, claude, cursor]
quorum: majority
```

`root` and `project` must be absolute paths.

`request_kind` is either `ticket` or `freeform`.

`constraints` should normally be empty. Use it only for direct user instructions that should govern the workflow independently of Discovery voting.

`slots` is the ordered provider slot list; it defaults to `codex`, `claude`, `cursor` when omitted.

`quorum` is `majority` or an integer from `2` through the slot count; it defaults to strict majority when omitted.

The Markdown body after the closing `---` is preserved as the actual request.

## What happens at initialization

`orchestrate init --from-file ...` freezes:

- the prepared request body;
- request kind;
- explicit constraints;
- target repository identity;
- the target repository's current committed Git commit and tree.

That baseline becomes the common source for every Discovery slot in the cohort.

If you are intentionally reproducing an earlier run, make sure the target repository is checked out at the desired baseline before initialization.

## Next step

Continue with [Discovery](discovery.md).
