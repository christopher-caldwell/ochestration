# Prepare Ticket Discovery Request

Use this only when explicitly invoked. Read the user-selected raw Markdown file and supplied setup values. Do not edit the raw file, initialize Orchestrate, start Discovery, inspect the target repository, or research beyond the supplied material.

Ask for genuinely missing or ambiguous target-project information. Resolve the store root from the supplied value, or expand the user's home directory and use the absolute `.orchestration` path, for example `/Users/name/.orchestration`. Resolve the project and output paths to absolute paths. Use a supplied effort slug, or propose a simple visible slug: lowercase words separated by `-` or `_`, using only ASCII letters, numbers, `-`, `_`, and `.`; no spaces or hash suffixes. Honor an explicit safe output path; otherwise write `<input-stem>.prepared.md` beside the input. Never overwrite either file without permission. Never write the prepared request inside the Orchestrate store root or inside the target repository; ask for another location if the default would be there.

Write a separate UTF-8 Markdown file with this frontmatter, quoting YAML values correctly:

```markdown
---
root: /absolute/path/to/.orchestration
project: /absolute/path/to/target-repository
effort: example-effort
request_kind: ticket
constraints: []
---

# Discovery Request

## Original Ticket

<!-- PASTE ORIGINAL TICKET VERBATIM HERE -->

## Additional User Context
```

Organize only supplemental material under useful headings such as Additional User Context, Explicit Unknowns, and Supporting Context. Do not ask for, fetch, reproduce, rewrite, summarize, or reconstruct the original ticket. If the raw file contains an explicitly labelled ticket excerpt, leave it out of the output and tell the user to paste the original ticket themselves; if the boundary is unclear, ask which material is supplemental.

Preserve every substantive supplied statement, speaker, uncertainty, condition, exception, scope, and conflict. A direct, unmistakable user instruction may also be copied verbatim to `constraints`; do not turn attributed statements, hypotheses, or interpretations into constraints. Before saving, check for omissions, invented requirements, altered certainty, merged speakers, or lost conditions.

Read the saved file back. In chat, provide its absolute path. Tell the user to paste the original ticket verbatim into the placeholder and review/edit the file, then invoke the installed `discovery` skill in the current host with this exact prepared-request path.

Do not initialize an effort or run `orchestrate init` yourself.
