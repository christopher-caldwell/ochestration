# Prepare Freeform Discovery Request

Use this only when explicitly invoked. Prepare creates the reviewed common request that every independent Discovery run will receive.

Read the user-selected freeform request exactly as supplied, along with optional supplemental context and setup values. Do not edit the source file.

Before writing, perform a lightweight intent challenge. Look for missing or ambiguous **user-owned intent** that could cause reasonable independent Discovery agents to proceed under materially different assumptions about behavior, scope, compatibility, allowed change, acceptance boundaries, or other consequential user decisions. Ask focused questions only when the answers could materially improve their common starting point. This is not a completeness checklist or fixed questionnaire; zero questions is valid. Ask only when a question is likely to prevent materially different assumptions between independent Discovery runs. Once further questioning is unlikely to materially improve their shared starting point, proceed with preparation.

Challenge only user intent. Do not inspect the target repository, source code, Git history, tests, runtime behavior, vendor documentation, or the web. Do not diagnose the problem, establish technical facts, compare implementation strategies, or recommend a solution. Those belong to Discovery.

Preserve each answer according to its meaning. A direct and unmistakable user requirement or prohibition may become a frozen constraint. Supplemental facts remain context; genuine uncertainty remains an explicit unknown for Discovery; hypotheses remain hypotheses. "I don't care", "use your judgment", and equivalent answers are deliberate delegation, not a reason to invent a requirement. Do not infer frozen constraints from implications, likely preferences, attributed statements, hypotheses, or technical interpretations. When an answer is useful but not unmistakably a constraint, preserve it in the body instead.

Resolve the store root from the supplied value, or expand the user's home directory and use the absolute `.orchestration` path, for example `/Users/name/.orchestration`. Resolve project and output paths to absolute paths. Use a supplied effort slug, or propose a simple visible slug: lowercase words separated by `-` or `_`, using only ASCII letters, numbers, `-`, `_`, and `.`; no spaces or hash suffixes.

Honor an explicit safe output path; otherwise write `<input-stem>.prepared.md` beside the input. Never overwrite the input or prepared file without permission. Never write the prepared request inside the Orchestrate store root or inside the target repository; ask for another location if the default would be there.

Write a separate UTF-8 Markdown file with frontmatter like this, quoting YAML values correctly:

```markdown
---
root: /absolute/path/to/.orchestration
project: /absolute/path/to/target-repository
effort: example-effort
request_kind: freeform
constraints: []
---
```

Organize the supplied request and preparation clarifications under headings appropriate to the actual content, such as Goal, Context, Explicit Unknowns, or Supporting Information. Do not force empty or irrelevant sections. Preserve every substantive supplied statement, speaker, uncertainty, condition, exception, scope distinction, conflict, and deliberate delegation. Do not invent new meaning. Before saving, check for omissions, invented requirements, altered certainty, merged speakers, inferred constraints, or lost conditions.

Read the saved file back and check that clarified intent has its actual authority. In chat, provide the absolute prepared-request path and ask the user to review/edit it before Discovery. Once reviewed, tell the user to invoke the installed `discovery` skill in the current host with this exact prepared-request path.

Do not initialize an effort or run `orchestrate init` yourself.
