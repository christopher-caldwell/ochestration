---
name: prep-discovery-ticket
description: Prepare supplemental Discovery notes for a reviewed ticket without copying or interpreting the ticket itself.
disable-model-invocation: true
---

# Prepare ticket supplemental notes

Use this skill only when explicitly invoked. Read the user-selected raw Markdown file and supplied setup values. Do not edit the raw file, initialize Orchestration, start Discovery, inspect the target repository, or research beyond the supplied material.

Ask for genuinely missing or ambiguous target-project information. Resolve the store root from the supplied value, or use the current user's absolute `~/.orchestration` path. Resolve the project and output paths to absolute paths. Use a supplied effort slug, or propose a simple visible slug. Honor an explicit output path; otherwise write `<input-stem>.prepared.md` beside the input. Never overwrite either file without permission. Keep the output outside a new or uninitialized store; ask for another location if the default would be inside one.

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

Read the saved file back. In chat, provide its absolute path and exactly this form of command, shell-quoted for the actual path. Tell the user to paste the original ticket verbatim into the placeholder and review/edit the file before running it. Do not run the command:

```sh
orchestrate init --from-file "/actual/path/request.prepared.md"
```
