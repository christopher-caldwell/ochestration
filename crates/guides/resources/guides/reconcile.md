# Reconcile guide

Input is two or more explicitly selected finalized Discovery output directories. Resolve the exact set before semantic work:

```sh
orchestrate --root "<root>" reconcile inputs --effort "<effort>" \
  --discovery "<artifact-1>" --discovery "<artifact-2>"
```

Require `kind: discovery`, outcome `IMPLEMENTATION_READY`, and one common effort, context, and frozen baseline. Read each selected bundle's `manifest.json`, `run.json`, `discovery.json`, `technical-spec.md`, and public evidence graph. Use its stable source label—such as `claude_ab12cd34` or `codex_ef56ab78`—when discussing agreement or disagreement, and preserve the label-to-artifact mapping.

Reconcile is synthesis only. Its engineering evidence is exactly the selected finalized public Discovery outputs. Frozen explicit constraints are direct pre-existing user authority: Rust mechanically preserves them as governing requirements, but they are not engineering evidence. The frozen request and context may be used only for goal/context, identity, lineage, and confirming the selected artifacts answer the same frozen effort; do not reinterpret them as a source of technical investigation. A Reconcile-time user answer is direct user authority, not engineering evidence. Do not inspect the target repository, any Discovery `source/` workspace, private chats, mutable workspaces, tests, Git history, vendor documentation, or the web. Do not run tests or experiments, and do not introduce a technical theory from outside the supplied outputs. Store metadata may be used only for identity, lineage, baseline, and artifact validation.

Answer: **given only these Discovery outputs, what do they collectively establish, and what is the best-supported direction?** Produce one leading direction. Compare agreement, evidence quality, applicability, limitations, and disagreement; agreement count is informative but is not voting. `experiment` is not an automatic winner over `inspection` or `corroborated`. Evaluate whether an experiment tested the disputed claim, used a representative fixture, encoded the answer, omitted variables, overreached its observation, or conflicts with other strong evidence.

With exactly two inputs, never invent a majority or choose by model identity. Choose the better-supported direction when the evidence distinguishes them. If a genuine material tie or missing user-authority choice remains, ask the user before finalization and record the answer through `user_clarification`. If competent Discovery should have surfaced the question, identify it as a Discovery coverage failure. User answers are direct authority, not new engineering evidence. After an answer, still converge on one direction.

Write `reconcile-proposal.json` outside the repository and published bundles. It is the single authoritative structured contract. Include:

```json
{
  "core_result": "One clear selected direction.",
  "problem": "The original problem as established by Discovery.",
  "product_behavior_changed": ["..."],
  "product_behavior_unchanged": ["..."],
  "technical_behavior_changed": ["..."],
  "technical_behavior_unchanged": ["..."],
  "requirements": [{
    "requirement": {"id": "R-1", "text": "Binding behavior.", "acceptance": "Precise verification.", "condition": null, "governing": false},
    "source_refs": [{"discovery_artifact_id": "discovery-...", "node_id": "R-1"}]
  }],
  "evidence_synthesis": [{
    "id": "E-1",
    "conclusion": "Significant conclusion.",
    "source_refs": [{"discovery_artifact_id": "discovery-...", "node_id": "F-1"}],
    "verification_methods": ["experiment"],
    "evidence_summary": "What the selected evidence establishes and why it applies.",
    "limitations": "What it does not establish and remaining assumptions."
  }],
  "disagreements": ["Important disagreement and its disposition."],
  "rejected_alternatives": [{
    "direction": "Strongest rejected direction.",
    "reason": "Why the selected direction is better supported.",
    "source_refs": [{"discovery_artifact_id": "discovery-...", "node_id": "F-2"}]
  }],
  "implementation_risks": ["..."],
  "compatibility_concerns": ["..."],
  "caveats": ["..."],
  "technical_suggestions": [{
    "id": "TS-1",
    "text": "Advisory implementation direction.",
    "source_refs": [{"discovery_artifact_id": "discovery-...", "node_id": "F-3"}]
  }],
  "blocking_issues": []
}
```

The requirements must exhaustively decompose every binding, implementation-affecting obligation and include precise acceptance criteria and conditions. Preserve intentionally unchanged behavior, evidence mapping, experiment summaries and limitations, credible disagreement, rejected alternatives, risks, compatibility concerns, caveats, and useful advisory suggestions. Use empty arrays when a category genuinely has nothing to report; do not omit fields.

Ordinary requirements, evidence synthesis, rejected alternatives, and technical suggestions may cite only the exact selected Discovery artifacts, and referenced node IDs must exist. Never set `requirement.governing` or `frozen_user_constraint`; Rust adds frozen constraints mechanically. An actual Reconcile-time user answer may instead authorize a requirement with empty `source_refs` and `user_clarification` containing the exact answer. It cannot also cite Discovery sources.

Rust deterministically renders `reconciled-discovery.md` from this contract. Do not submit separate specification Markdown. Technical suggestions remain advisory; make an architectural property binding only when it must be audited. A non-empty `blocking_issues` list is reserved for a material issue that cannot responsibly be resolved from selected evidence or user authority.

Finalize against the same exact IDs:

```sh
orchestrate --root "<root>" reconcile finalize --effort "<effort>" \
  --discovery "<artifact-1>" --discovery "<artifact-2>" \
  --bundle "<absolute path to reconcile-proposal.json>"
```

Then stop. Do not inspect code, create Adoption, ask for Build approval, start Build, or suggest continuing immediately. The normal chat response is concise:

```text
Result: <one clear selected direction>

Key caveat: <only if materially important>

Artifact: <absolute path>
```

The artifact tells Build why; chat tells the user the answer.
