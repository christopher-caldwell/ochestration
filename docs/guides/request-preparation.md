# Request preparation

Preparation creates the one file that every Discovery run will receive. Exact model instructions
are `orchestrate prep-discovery-ticket guide` or `orchestrate prep-discovery-freeform guide`.

You normally use one of two skills:

```text
Existing ticket  → $prep-discovery-ticket
Freeform request → $prep-discovery-freeform
```

## Existing ticket

Create a Markdown file containing only the supplemental context you want to add.

It can be very small:

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
Notes: /absolute/path/to/notes.md
Project: /absolute/path/to/repository
Effort: short-visible-name
```

The skill creates a prepared file with a ticket placeholder.

Paste the **original ticket verbatim** into that placeholder yourself, then save and review the
file.

Do not summarize the ticket for the prep model. The point of this step is to keep the original
ticket authoritative.

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

The skill organizes the request without inventing new meaning and returns the prepared file.

## What the prepared file contains

Both skills create a prepared Markdown file. Its frontmatter records the store root, the target
project, the effort name, whether the request is a ticket or freeform, and any explicit
constraints. You do not normally edit that frontmatter. The exact file shape is in the preparation
guide.

The important thing is simple:

> Every independent Discovery window gets the exact same prepared request file.

Next: [Discovery](discovery.md).
