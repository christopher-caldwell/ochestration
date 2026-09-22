# Discovery

Input is one absolute prepared-request path. Resolve `orchestrate`, read the request frontmatter, then run:

```sh
orchestrate --root "<root>" init --from-file "<prepared-request>"
orchestrate --root "<root>" discovery prepare --effort "<effort>" \
  --host "<host>" [--provider "<provider>"] [--model "<model>"] [--model-effort "<session-or-effort>"]
```

`<root>` comes from the prepared-request frontmatter. Use the `effort` id printed by `init` for every later command. Initialization is safe to repeat from independent model windows only with the identical prepared request. The project plus human effort slug is one immutable unit: never rewrite the prepared request with answers obtained during Discovery and initialize it again under that slug. Record clarifications in this Discovery run. A genuinely different unit of work needs a deliberately different effort slug.

Every `discovery prepare` creates a new independent run and isolated workspace. Pass a truthful host. Supply provider, model, and model-effort only when known; never invent provider, model, or session identity. The command prints the run id, stable human source label, and workspace path. The workspace's `source/` directory is a clean detached checkout of the frozen baseline with Git history. Use `scratch/` or another temporary location for experiments; never alter or commit experiment artifacts in `source/`.

Discovery answers: **what should be built, and why?** Work only in that run workspace. Do not inspect sibling runs, parent records, previous Discovery results, a dirty checkout, the live target repository, or unrelated paths. Do not implement the change.

Start by reading `run.json`, `request.md`, and `context.json`. `run.json` defines this Discovery run: its effort, baseline, host, provider, model, and model effort. `request.md` is the original user input and must be read as written. `context.json` supplies the same frozen request for convenience plus `constraints`: explicit user constraints frozen before this Discovery from the reviewed prepared request. Do not extract new immutable constraints from a ticket. Answers obtained during Discovery are run-local evidence or user clarification; never write them back into the prepared request or frozen constraints for the same effort.

For user intent and scope, use this authority order: (1) explicit frozen user constraints; (2) explicit current user clarification; (3) the original ticket or freeform request. The original request is the default authority for requested behavior and scope; a later explicit user clarification overrides conflicting earlier intent. For factual claims, investigate: ticket or request assertions about current code, vendor behavior, runtime behavior, regressions, causes, or fallback behavior are not automatically true.

When `request_kind` is `ticket`, preserve its wording, distinctions, conditions, and explicit scope; do not silently summarize, generalize, narrow, or replace it. If credible evidence materially conflicts with requested behavior or scope, do not silently prefer either source: explain what the ticket says, what the evidence says, why it matters, and ask the user for a decision. Record the clarification as evidence.

When `request_kind` is `freeform`, treat the request as the user's stated goal and context, without inventing requirements that are not there. Ask the user when missing information creates a material product or engineering choice. Preserve and document any conflict resolved by later user clarification.

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

Before finalization, read the specification and graph together as the complete evidence a Reconcile model will receive without access to source or the host conversation:

- Check that every graph identifier used in the specification resolves to the intended node and that the specification and node agree. Link decisions and requirements to the Findings or recorded clarifications they actually rely on through `depends_on`; a prose reference does not create that dependency. Consolidate duplicated clarification records or connect their uses. A Finding may legitimately produce no Requirement, for example when recording a deferred observation; explain that disposition rather than inventing work.
- Keep observed behavior and its evidentiary limits in Findings, and the chosen remedy and its authority or tradeoffs in Decisions. An accepted Finding about what the code does does not by itself authorize changing that behavior. Preserve material compatibility evidence even when choosing to change the existing contract.
- Distinguish completed experiments, calculated examples, inspected code paths, and proposed tests. For an executed reproduction, preserve the invocation or procedure, relevant fixture inputs, and observed result in the public evidence, with the limits already described above. Expected output or a written walkthrough is not an observed execution result; a calculation about one step does not establish that the full flow was exercised.
- Check that exclusions and limitations remain consistent with the conclusions. Saying that a case is out of scope or needs no implementation change does not establish its cause. Preserve an unconfirmed cause as unconfirmed, even when the scope decision is settled.

This is a semantic publication review, not an additional graph status or a requirement that every investigation produce a change. Correct inconsistencies in the active run before publishing; structural validation alone does not establish that the evidence supports the conclusions.

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
