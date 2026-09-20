---
name: reconcile
description: Reconcile two or more explicitly supplied finalized Orchestrate Discovery artifacts into one authoritative Reconciled Discovery.
disable-model-invocation: true
---

# Reconcile

Input is two or more finalized Discovery output directories. Read each `manifest.json`,
`discovery.json`, and public files. Require `kind: discovery`, `IMPLEMENTATION_READY`, and one
common effort, context, and frozen baseline. Extract exactly the supplied artifact IDs; do not add
later or otherwise available Discoveries.

Resolve the set before semantic work:

```sh
orchestrate --root "<root>" reconcile inputs --effort "<effort>" \
  --discovery "<artifact-1>" --discovery "<artifact-2>"
```

Include every user-supplied artifact exactly once. Read `orchestrate guide reconcile` and then
reason only from the frozen request and constraints plus those exact finalized public Discovery
artifacts. Never inspect source, Git history, tests, web or vendor docs, private chats, or mutable
workspaces. Reconcile may preserve a strong minority finding; agreement count is information, not
an eligibility rule.

Write `reconcile-proposal.json` outside the repository and artifact store. It must distinguish
binding requirements from advisory `technical_suggestions`. Every model-derived requirement and
suggestion needs at least one source reference to a selected Discovery artifact; a referenced node
ID must exist there. If the selected evidence cannot settle a material decision, ask the user when
appropriate or use `blocking_issues` rather than inventing an answer.

Finalize against the same exact IDs:

```sh
orchestrate --root "<root>" reconcile finalize --effort "<effort>" \
  --discovery "<artifact-1>" --discovery "<artifact-2>" \
  --bundle "<absolute path to reconcile-proposal.json>"
```

Show the resulting `reconciled-discovery.md` to the user. Explain the binding result separately
from advisory technical suggestions. Ask whether they explicitly approve Build against this exact
artifact. Only after an affirmative answer may you run:

```sh
orchestrate --root "<root>" reconcile adopt --effort "<effort>" \
  --reconciled "<artifact-id>" --authorization-label "<user label>"
```

Never adopt automatically. A blocked Reconciled Discovery cannot be adopted.
