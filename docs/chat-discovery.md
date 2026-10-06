# Chat Discovery

Use this guide in a fresh ChatGPT conversation when collaborative Discovery is sufficient. Work with the user to determine what should be built and why. Challenge assumptions, identify material uncertainties, and distinguish explicit user requirements from your analysis and technical suggestions. Record meaningful decisions, rejected directions, compatibility concerns, and unresolved blockers. Describe only investigation or verification that actually occurred. This conversation is **one collaborative Discovery source**, never multiple independent runs.

When the user explicitly requests a handoff, create `chat-discovery.zip` with this small layout:

```text
chat-discovery.zip
├── discovery.json
└── build/                         optional
    └── phase_01_delivery/
        └── phase.md
```

`discovery.json` is UTF-8 JSON. Required fields are `effort` (a short human slug), `request` (the original request), `problem`, `selected_direction`, `evidence_summary`, and a nonempty `requirements` array. Each requirement has `id`, `text`, `acceptance`, and `authority` (`explicit_user` or `chat_analysis`), with optional `condition`. An `explicit_user` requirement also needs a nonempty `user_statement` quoting or faithfully recording the supporting user instruction; a `chat_analysis` requirement must not include `user_statement`. Optional arrays are `constraints`, `findings`, `decisions`, `disagreements`, `product_behavior_changed`, `product_behavior_unchanged`, `technical_behavior_changed`, `technical_behavior_unchanged`, `rejected_alternatives`, `implementation_risks`, `compatibility_concerns`, `caveats`, `advisory_technical_suggestions`, and `blockers`. Every optional array contains strings except `rejected_alternatives`, whose entries are objects with exactly `direction` and `reason`. In particular, portable `findings` and `decisions` are string arrays; do not copy graph-node shapes such as `{ "id": "F-1", "text": "..." }` into them. Use empty arrays or omit optional fields when there is nothing to record. Do not claim local repository inspection or experiments in `evidence_summary` unless they occurred. Resolve material blockers before import; a bundle with blockers cannot become implementation-ready.

### Choose the authority surface by what the statement governs

Every entry in `requirements[]` becomes part of the Reconciled implementation contract and receives Final Audit coverage. `authority` describes where the requirement comes from: `explicit_user` carries direct user authority, while `chat_analysis` is supported by this collaborative Discovery source. Both are binding downstream. In particular, `chat_analysis` does not mean advisory. A requirement must state an obligation about the resulting implementation, and its acceptance criteria must let an independent Audit assess that obligation against the registered implementation. Advisory implementation direction belongs in `advisory_technical_suggestions`, which remains non-binding.

Every entry in `constraints[]` is frozen into the Effort context and mechanically imported as a governing, frozen `GOV-N` requirement. It must be preserved by the implementation and receives Final Audit coverage. Use this field only for direct, unmistakable user constraints on the resulting implementation. User importance alone does not make a statement an implementation constraint. Do not use `constraints[]` for preferences, workflow instructions, phase plans, provider choices, planning notes, or technical suggestions.

For each requirement, its acceptance criteria, and each proposed constraint, ask:

> Could standalone Final Audit determine whether this obligation is satisfied by examining the registered implementation and performing appropriate verification, without relying on historical knowledge of how Build was conducted?

If yes, it may belong in implementation authority, provided it is truly binding and its source is represented accurately. If no, route it to the surface that owns it. Apply the test to the acceptance criteria too: a suitable requirement can have acceptance criteria that accidentally demand historical process evidence.

For example, “Existing Booking URLs must remain compatible” and “The public command name must remain unchanged” are implementation obligations when supported by the user or Discovery evidence. “All existing compatibility tests must pass” can be valid acceptance because Audit can run those tests against the resulting implementation. “Run the tests before Review” depends on Build history, so it is verification or Build-process guidance instead.

For example:

```json
{
  "effort": "status-page",
  "request": "Add a compact status page",
  "constraints": ["Keep the existing API"],
  "problem": "Status is hard to find",
  "selected_direction": "Add one compact status page",
  "evidence_summary": "The user chose this direction in conversation; current repository behavior was not inspected.",
  "findings": ["Current status is not visible on one compact page"],
  "decisions": ["Use one compact page rather than separate status views"],
  "requirements": [
    {"id": "R-1", "text": "Show status on one page", "acceptance": "The page displays current status", "authority": "chat_analysis"},
    {"id": "R-2", "text": "Keep the public command name", "acceptance": "Existing callers use the same command", "authority": "explicit_user", "user_statement": "Do not rename the existing command."}
  ],
  "rejected_alternatives": [
    {"direction": "Add separate status pages", "reason": "The selected direction is one compact page"}
  ]
}
```

Optional `build/phase_XX_name/phase.md` files use two decimal digits and a nonempty name. They provide implementation guidance and ordering, not binding authority. Use Build planning for phase count and order, phase boundaries, Work → Review grouping, checkpoints, implementation sequencing, dependencies, and phase-local verification. When the user asks for phases, represent that request through the optional phase directories in the intended order; describe the work within each phase in its `phase.md`. For example, “Use two substantial reviewed phases” belongs in the phase decomposition, not in `requirements[]` or `constraints[]`.

Follow the [Build guide's phase-authoring semantics](../crates/guides/resources/guides/build.md#authority-and-preparation) when writing these optional files. The requirements and constraints in `discovery.json` become the binding Reconciled contract; phase Markdown must not become a second product specification or present planning prose as binding authority. Prefer references to relevant requirement IDs over rewriting their behavior when practical. For example, a phase for a hypothetical first-class role setting with requirement R-7 could say:

```md
## Relevant contract
- R-7

## Implementation
- Add the optional setting to role configuration.
- Translate supported values at the adapter boundary; preserve opaque native args.

## Verification
- Config parsing and adapter tests.
- Check provider-specific documentation examples.
```

### Review the complete handoff before packaging

Compare the actual settled conversation with the final `discovery.json` in both directions, whether or not optional phases are present. For every material settled decision, check that the appropriate existing fields preserve its subject, scope, starting conditions, exceptions, intended outcome, and a faithful scoped user statement when the user stated it. A broad paraphrase does not show that a precise decision survived. Conversely, check that every proposed binding obligation and acceptance detail has support in the conversation or Discovery analysis; do not strengthen examples or hide new acceptance in planning. Keep binding intent in requirement text, acceptance, condition, or `user_statement` where applicable, or in `constraints[]` only under its direct-user-constraint rules described above. Decisions, findings, risks, caveats, recaps, and phase prose cannot substitute for binding authority. Keep advisory direction in `advisory_technical_suggestions` or other appropriate planning/context fields.

Treat this handoff as the enumerated Build contract, not as a summary of the conversation. For explicit user requirements, preserve every materially distinct settled ask and decision that a Builder or Final Audit would need to distinguish. Consolidate repeated statements that describe the same obligation, but do not merge independently violable decisions, conditions, exceptions, starting states, or acceptance expectations into a broader requirement that loses those distinctions. Several user statements may support one requirement when they truly govern one obligation; this is fidelity, not a one-requirement-per-sentence rule.

Before packaging, inspect each `explicit_user` requirement for any material implementation boundary the user has not actually settled. Do not infer that implementation is open merely because the user stated an outcome, and do not promote a mechanism observed during investigation into binding authority merely because it worked. If whether a mechanism is required, forbidden, or left open would materially change what Build is allowed or required to do, ask one focused question about that explicit user requirement and do not create the ZIP until the answer is settled. An explicit answer that leaves the mechanism to Build is a valid boundary. Preserve useful non-binding mechanisms as findings or `advisory_technical_suggestions`. This clarification rule applies to explicit user requirements; do not turn `chat_analysis` requirements into another user questionnaire, and do not work backward from the request into speculative edge cases or additional requirements the user did not ask for.

Review requirements, acceptance, proposed constraints, and the settled conversation together. Confirm each binding item is an obligation on the resulting implementation that standalone Final Audit can assess without Build-history knowledge, is truly binding rather than advisory, and—if marked `explicit_user` or placed in `constraints[]`—was actually stated by the user. Preserve `explicit_user` versus `chat_analysis` attribution; never present analysis as a user statement. Route planning, configuration, advisory direction, or ordinary context to their existing surfaces. A later decision supersedes only the earlier subject it actually changes: withdrawing a proposed generator does not withdraw a settled manual tutorial, and allowing an optional copy shortcut does not make copy-and-run the canonical learning path. Challenge the assembled contract with a plausible wrong implementation: could it satisfy the stated checks while violating supported intent? Close a demonstrated gap in the relevant binding text and acceptance, without adding unrelated constraints. Do not invent schema fields, a decision ledger, a questionnaire, or another approval loop.

For each material acceptance obligation, distinguish the product behavior, what a check asserts about an observed response, and any external result that must succeed for completion. Resolve material acceptance or scope choices with the user in this conversation and record the supported result in the existing requirement, acceptance, condition, and evidence fields. For runnable installation or onboarding, acceptance concerns reaching the promised result from the documented starting state under the documented prerequisites, not merely listing or ordering commands. For example, running a prebuilt CLI against a copied example in an already built checkout proves that the checkout can run that example; it does not establish that a user can clone the source and complete the published install steps under their documented prerequisites. Preserve supported peer UI and CLI paths; an exercise of one does not prove the other. Put the resulting product property in binding authority and keep execution procedure/order in phase verification guidance. An independent check that the published instructions work is appropriate; requiring evidence that a particular historical Worker ran them is not implementation acceptance. Distinguish a health predicate, correct response handling, and any explicitly required healthy external outcome. Preserve required real-service proof, separately required successes, known-limit dispositions, and conditions on exclusions; a passing command or predicate alone does not establish a required external success. Where settled teaching intent allows a later transition to recheck an earlier invariant as part of observing the current state, preserve that permission without inventing an arbitrary-earlier-verifier command. Illustrative code remains illustration and does not create a mandatory live integration.

Read `discovery.json`, every optional phase document, and their examples together. Check that phase prose does not add, remove, strengthen, or weaken requirements or constraints, create alternate acceptance conditions, or lose conditions and exceptions. Check provider-specific examples and omission/default wording against precise binding behavior. Correct conflicting phase guidance before packaging; if authority itself is malformed, use the correction path below. This is semantic consistency, not a requirement-to-phase coverage exercise. Routine setup and later verification remain Build work: Discovery need not execute every future check, contact a live service for every code block, or test every platform. Not every unknown is a blocker; retain and resolve material blockers that prevent a coherent contract rather than deleting them to imply readiness. Chat Import rejects unresolved blockers rather than publishing normal Reconcile's `BLOCKED` outcome.

Provider and runtime choices belong to later Build configuration: Worker or Reviewer provider, model, reasoning/effort setting, and provider-native arguments. Chat Discovery intentionally does not emit `config.toml`; do not add configuration fields to the ZIP or encode these preferences in implementation authority or phase guidance. If the user states a provider/runtime preference, keep it outside the portable Chat Discovery artifact and report it separately in the surrounding chat/handoff message for later Build setup. The chat also cannot know the local Reconciled artifact ID; do not include `plan.json`.

Split a user statement that combines implementation authority and process direction according to what each part governs. For example:

```text
User: “Do not change the public API, and split the work into two reviewed phases.”

Binding implementation constraint:
- The public API remains compatible.

Build planning:
- Use two coherent reviewed phases.
```

Do not copy the combined sentence into `constraints[]`. Preserve only the implementation obligation there; express phase planning through the actual phase decomposition.

When supplying optional Build phases, use the same phase boundary as normal Build preparation: each phase should be a substantial, coherent implementation slice that can be built, meaningfully verified, and reviewed efficiently as one unit. Optimize for efficient Work → Review cycles and useful accepted checkpoints, and include the tests or verification naturally associated with the slice. Group multiple related tasks when together they make a stronger coherent Review unit; task enumeration alone does not call for another Review. Let phase count follow the work without a numeric target. If substantial reviewable implementation has a natural earlier checkpoint before later independently blocking work—such as external credentials, environment-specific integration, or real-service acceptance—plan that later work separately when useful; this is conditional, not a blanket extra-phase rule.

### Correcting malformed authority

Published authority is immutable. If Build or Audit reveals a malformed Reconciled Discovery, do not edit the published artifact or Build state, make phase prose override it, manufacture evidence for a historical orchestration requirement, or weaken Audit. Publish corrected authority through a supported workflow.

When reviewing findings after Build or Audit, classify each issue against what the contract actually requires: an existing-contract defect, lost or malformed authority, or a genuinely new suggestion. Correct an existing-contract defect through ordinary implementation work. Do not turn omitted source intent into unchanged-authority feedback or phase prose; restore missing binding intent through a supported corrected publication path. Keep new suggestions advisory until adopted through that path. A broad self-contained-path obligation can support correcting a real installation defect even when a stronger empty-directory teaching requirement was omitted. Consolidation alone does not prohibit useful short repetition.

If the frozen request or frozen `constraints[]` are wrong, create a new effort: those values cannot be removed by rerunning Reconcile, which preserves frozen constraints, and Chat Import cannot reuse an effort that already has artifacts or a Build plan. If the frozen request and context are correct but a derived requirement is malformed, publish corrected Discovery/Reconciled authority through a supported workflow; do not assume Chat Import can overwrite its existing effort. The exact route depends on how the corrected evidence is produced. Correct product code may be reused only when normal registration and ancestry rules permit it; corrected authority neither requires needless reimplementation nor grants unconditional reuse.

In the target Git repository, import with `orchestrate import ./chat-discovery.zip` (and `--root <store>` if needed). Import freezes the repository's current committed HEAD, publishes one Discovery and one Reconciled Discovery, and prepares the normal Build directory. Omitting phases is valid: import invents no phase, and the generated plan has an empty phase list until real phases are supplied. Build validation rejects that incomplete plan on launch. Review the result and stop. Producing or importing the ZIP does **not** authorize Build. A later explicit `$build` launch is required for Adoption, Work, Review, and Audit.
