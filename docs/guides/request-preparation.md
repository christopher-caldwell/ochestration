# Request preparation

Preparation creates the one reviewed file that every Discovery run will receive. Before those
independent investigations diverge, Prepare may ask a few focused questions about consequential
missing user intent. It does not investigate the repository or decide technical solutions. Zero
questions is normal when the request is clear. Exact model instructions are
`orchestrate prep-discovery-ticket guide` or `orchestrate prep-discovery-freeform guide`.

You normally use one of two skills:

```text
Existing ticket  → $prep-discovery-ticket
Freeform request → $prep-discovery-freeform
```

## Existing ticket

Provide the original ticket as a Markdown file. You may also provide supplemental context, which
can be very small:

```markdown
The issue has been reproduced in production.

I do not know whether staging is expected to match production.
```

Invoke:

```text
$prep-discovery-ticket
```

Give the model:

```text
Ticket: /absolute/path/to/ticket.md
Optional context: /absolute/path/to/notes.md
Project: /absolute/path/to/repository
Effort: short-visible-name
```

The model reads the ticket, may ask about missing user decisions, and creates a prepared file that
preserves the original ticket verbatim. Review the prepared file before Discovery.

## Freeform request

Put the full request and context in a Markdown file and invoke:

```text
$prep-discovery-freeform
```

Give it:

```text
Request: /absolute/path/to/request.md
Project: /absolute/path/to/repository
Effort: short-visible-name
```

The skill may ask about consequential missing user intent, then organizes the request without
inventing new meaning. Review the prepared file before Discovery.

## What the prepared file contains

Both skills create a prepared Markdown file. Its frontmatter records the absolute store root, the
absolute target project, the effort slug, whether the request is a ticket or freeform, and any
direct, unmistakable user constraints. Other answers, including uncertainty and deliberate
delegation, stay in the body according to their meaning. Keep the prepared request outside both
the Orchestrate store root and target repository. You do not normally edit that frontmatter. The
exact file shape is in the preparation guide.

The important thing is simple:

> Every independent Discovery window gets the exact same prepared request file.

The project plus effort slug identifies one frozen unit of work. Once initialized, changing the
request kind, body, or constraints under that effort slug is rejected. Clarifications answered
during Discovery belong in that Discovery run, not in a rewritten prepared file.

Next: [Discovery](discovery.md).
