# Prepare Freeform Discovery Request

Use this only when explicitly invoked. Read the user-selected raw Markdown file and supplied setup values. Do not edit the raw file, initialize Orchestrate, start Discovery, inspect the target repository, or research beyond the supplied material.

Ask for genuinely missing or ambiguous target-project information. Resolve the store root from the supplied value, or expand the user's home directory and use the absolute `.orchestration` path, for example `/Users/name/.orchestration`. Resolve the project and output paths to absolute paths. Use a supplied effort slug, or propose a simple visible slug: lowercase words separated by `-` or `_`, using only ASCII letters, numbers, `-`, `_`, and `.`; no spaces or hash suffixes. Honor an explicit safe output path; otherwise write `<input-stem>.prepared.md` beside the input. Never overwrite either file without permission. Never write the prepared request inside the Orchestrate store root or inside the target repository; ask for another location if the default would be there.

Write a separate UTF-8 Markdown file with frontmatter like:

```markdown
---
root: /absolute/path/to/.orchestration
project: /absolute/path/to/target-repository
effort: example-effort
request_kind: freeform
constraints: []
---
```

Organize the whole supplied request with headings appropriate to its content, such as Goal, Context, Explicit Unknowns, and Supporting Information. Do not add a ticket placeholder. Preserve every substantive supplied statement, speaker, uncertainty, condition, exception, scope, and conflict. A direct, unmistakable user instruction may also be copied verbatim to `constraints`; do not turn attributed statements, hypotheses, or interpretations into constraints. Before saving, check for omissions, invented requirements, altered certainty, merged speakers, or lost conditions.

Read the saved file back. In chat, provide its absolute path, ask the user to review/edit it, then invoke the installed `discovery` skill in the current host with this exact prepared-request path.

Do not initialize an effort or run `orchestrate init` yourself.
