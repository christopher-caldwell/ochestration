# Orchestrate Discovery

Input is one absolute prepared-request path. Resolve `orchestrate`, read the request frontmatter, then run:

```sh
orchestrate --root "<root>" init --from-file "<prepared-request>"
orchestrate --root "<root>" discovery prepare --effort "<effort>" \
  --host "<host>" --provider "<provider>" --model "<model>" --model-effort "<session-or-effort>"
```

`<root>` comes from the prepared-request frontmatter. Use the `effort` id printed by `init` for every later command. Initialization is safe to repeat from independent model windows only with the identical prepared request. The project plus human effort slug is one immutable unit: never rewrite the prepared request with answers obtained during Discovery and initialize it again under that slug. Record clarifications in this Discovery run. A genuinely different unit of work needs a deliberately different effort slug.

Every `discovery prepare` creates a new run and isolated workspace; no slot, provider plan, or quorum is involved. Pass truthful host, provider, model, and model-effort provenance. The command prints the run id, stable human source label, and workspace path. The workspace's `source/` directory is a clean detached checkout of the frozen baseline with Git history. Use `scratch/` or another temporary location for experiments; never alter or commit experiment artifacts in `source/`.

Discovery answers: **what should be built, and why?** Work only in that run workspace. Do not inspect sibling runs, parent records, previous Discovery results, a dirty checkout, the live target repository, or unrelated paths. Do not implement the change.

Start by reading `run.json`, `request.md`, and `context.json`. `run.json` defines this Discovery run: its effort, baseline, host, provider, model, and model effort. `request.md` is the original user input and must be read as written. `context.json` supplies the same frozen request for convenience plus explicit user-supplied constraints. Constraints are explicit user-supplied clarifications or governing instructions; do not extract new immutable constraints from a ticket.

When `request_kind` is `ticket`, the ticket is the best available authority for requested behavior and intended scope. Preserve its wording, distinctions, conditions, and explicit scope; do not silently summarize, generalize, narrow, or replace it. A ticket is not infallible about facts. Claims about current code, vendor behavior, regressions, causes, or fallback behavior are hypotheses to investigate. If credible evidence materially conflicts with the ticket, do not silently prefer either source: explain what the ticket says, what the evidence says, why it matters, and ask the user for a decision. Record the clarification as evidence.

When `request_kind` is `freeform`, treat the request as the user's stated goal and context, without inventing requirements that are not there. Ask the user when missing information creates a material product or engineering choice. A later explicit user clarification takes precedence over a conflicting earlier ticket statement; preserve and document the conflict.

Record important questions, findings, decisions, and requirements as `graph/<id>.md`. Each file is Markdown with YAML frontmatter:

```markdown
---
id: Q-1
kind: question
status: open
depends_on: []
sources: []
required: true
mandatory: false
---

# Title

Body.
```

`kind` is `question`, `finding`, `decision`, or `requirement`. Ids use the prefixes `Q-`, `F-`, `D-`, and `R-`. Question statuses are only `open`, `answered`, `no_change`, and `blocked`. Findings use `accepted`, `rejected`, or `invalidated`. Decisions and requirements use `accepted` or `rejected`. Only a question may set `required: true`. Only a requirement may set `mandatory: true`. Link dependencies through `depends_on`. Every finding must set `verification` to `inspection`, `corroborated`, or `experiment`; non-findings use no verification value. Every accepted finding needs at least one source reference.

`inspection` means the conclusion primarily comes from source, configuration, history, documentation, or similar inspection. `corroborated` means independent forms of evidence support it without a direct controlled reproduction. `experiment` means a small controlled reproduction directly exercised the disputed behavior. This is evidence metadata, not a confidence score.

- `answered` means the question has an evidence-supported answer the Discovery can rely on. A required answered Question needs an accepted Finding that directly depends on it.
- `no_change` means the investigation establishes no implementation change or requirement.
- `blocked` means the answer is not established and can materially change requested behavior, implementation scope, compatibility, acceptance criteria, whether a requirement exists, or whether a proposed solution is safe or correct.
- `open` is still being investigated and is not a final disposition.

Evidence that only establishes "unknown," "not found in the repository," or "vendor behavior unconfirmed" is not an answer. Do not resolve material ambiguity by assumption. Investigate first; use relevant source, tests, Git history, authoritative external documentation, or authorized controlled experiments. When a material technical claim can reasonably be tested, strongly prefer the smallest practical isolated reproduction. For an `experiment` finding, explain the tested claim, setup or fixture, exercised variable or behavior, observation, what the result establishes, what it does not establish, and remaining limitations or assumptions. Do not design a contrived fixture to manufacture support. In particular, consider `git log`, `git blame`, and `git show` when current code does not explain environment-specific behavior, compatibility guards, constants, regressions, or historical intent. Do not perform meaningless Git archaeology.

If evidence answers a material question, record the answer. If it materially conflicts with the request, ask the user. Asking a question does not make the run blocked: record the answer and continue the same active run. If it remains unknown and the user can reasonably clarify it, ask directly in the current host conversation and record the clarification. Only a materially unresolved question at finalization should be `blocked`. A finalized blocked artifact is immutable and excluded from Reconcile; later information requires a fresh Discovery under the same frozen effort.

Use `technical-spec.md` as the public result. It must stand alone and cover the interpreted request, current behavior, recommendation, required and unchanged behavior, decisions and rationale, requirements and acceptance criteria, conditions, alternatives or disagreement, limitations, and verification.

Run `orchestrate discovery validate` whenever an intermediate check is useful:

```sh
orchestrate --root "<root>" discovery validate --effort "<effort>" --run "<run>"
```

You do not have to run it before finalization: `orchestrate discovery finalize` validates the workspace before it publishes anything.

```sh
orchestrate --root "<root>" discovery finalize --effort "<effort>" --run "<run>"
```

The outcome is derived mechanically and cannot be overridden. Any `blocked` Question produces a blocked result that is ineligible for Reconcile; otherwise the workspace must be implementation-ready, which requires every required Question to be `answered` or `no_change` and every mandatory Requirement to trace to accepted evidence.

Report the human source label, run ID, artifact ID, outcome, and published Discovery directory. `orchestrate status --effort "<effort-id>"` prints the effort and its internal IDs. The directory is `<root>/projects/<project-name>/efforts/<effort-slug>/discovery/<artifact-id>`. A blocked Discovery is valid but cannot be reconciled. Do not start Reconcile or edit the target repository. Stop after this phase.
