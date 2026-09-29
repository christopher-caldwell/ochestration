# Reconcile

Input is two or more explicitly supplied finalized Discovery output directories. Resolve that exact set before semantic work:

1. Read only the supplied Discovery directories.
2. Read each directory's `manifest.json` and `run.json` to derive its exact artifact ID.
3. Derive the common Orchestrate root and effort information from those supplied directories.
4. Confirm that every supplied artifact resolves to one common frozen effort, context, and baseline.
5. Only then call `reconcile inputs` with those exact artifact IDs.

Do not infer another Discovery artifact, choose a latest run, or scan for additional eligible Discoveries. Only explicitly supplied inputs belong to this Reconcile.

```sh
orchestrate --root "<root>" reconcile inputs --effort "<effort>" \
  --discovery "<artifact-1>" --discovery "<artifact-2>"
```

Require `kind: discovery`, outcome `IMPLEMENTATION_READY`, and one common effort, context, and frozen baseline. Read each selected bundle's `manifest.json`, `run.json`, `discovery.json`, `technical-spec.md`, and public evidence graph. Use its stable source label—such as `claude_ab12cd34` or `codex_ef56ab78`—when discussing agreement or disagreement, and preserve the label-to-artifact mapping.

Reconcile is synthesis only. Its engineering evidence is exactly the selected finalized public Discovery outputs. Frozen explicit constraints are direct pre-existing user authority: Rust mechanically preserves them as governing requirements, but they are not engineering evidence. The frozen request and context may be used only for goal/context, identity, lineage, and confirming the selected artifacts answer the same frozen effort; do not reinterpret them as a source of technical investigation. A Reconcile-time user answer is direct user authority, not engineering evidence. Do not inspect the target repository, any Discovery `source/` workspace, private chats, mutable workspaces, tests, Git history, vendor documentation, or the web. Do not run tests or experiments, and do not introduce a technical theory from outside the supplied outputs. Other store metadata may be used only for identity, lineage, baseline, and artifact validation.

Answer: **given only these Discovery outputs, what do they collectively establish, and what is the best-supported direction?** Produce one leading direction. Compare agreement, evidence quality, applicability, limitations, and disagreement; agreement count is informative but is not voting. `experiment` is not an automatic winner over `inspection` or `corroborated`. Evaluate whether an experiment tested the disputed claim, used a representative fixture, encoded the answer, omitted variables, overreached its observation, or conflicts with other strong evidence. When summarizing verification methods, preserve the classifications recorded by the cited Discovery findings. You may evaluate evidence strength, relevance, limitations, and quality, but must not relabel an inspection as an experiment or otherwise upgrade a recorded method.

For every `evidence_synthesis` entry, set `verification_methods` to the distinct classifications of its directly cited Finding nodes, exactly. A direct Finding citation has both a selected `discovery_artifact_id` and that artifact's Finding `node_id`; do not infer methods from other nodes, other Findings in the artifact, or dependencies. Artifact-only citations and citations to Questions, Decisions, or Requirements may be used for context, but do not grant a verification method. When an entry has no direct Finding citations, use `"verification_methods": []`; rendered output will state that the entry is not attributed to specific Finding nodes. Rejected and invalidated Findings may still be cited and retain their recorded classification.

With exactly two inputs, never invent a majority or choose by model identity. Choose the better-supported direction when the evidence distinguishes them. If a genuine material tie or missing user-authority choice remains, ask the user before finalization and record the answer through `user_clarification`. If competent Discovery should have surfaced the question, identify it as a Discovery coverage failure. User answers are direct authority, not new engineering evidence. After an answer, still converge on one direction.

After selecting a direction, review the assembled contract's requirements, acceptance, conditions, unchanged behavior, exclusions, evidence limits, and relevant risks together. Check the meaning supported by the selected outputs, not only whether each source ID exists. In particular, distinguish a health predicate, a real check that verifies correct handling of the observed response, and a completion gate requiring the external target to respond healthy. A passing acceptance command does not by itself identify which assertion it contains. Preserve qualified exclusions rather than turning "not without X" into "never" or assuming X automatically authorizes a remedy. Keep binding obligations in requirements, acceptance, and conditions; risks and caveats may explain their consequences but cannot waive or create them.

When a known limitation intersects acceptance, state the supported disposition and its completion consequence. Preserve a hard external prerequisite when selected evidence or user authority establishes it, even if its result remains pending; future failure alone does not make synthesis unready. Preserve a response-handling boundary when that is what the authority establishes, without adding an external-success gate. If a material user-owned choice remains, ask and record its actual answer under the authority rules above. If settlement instead needs missing engineering evidence, name the missing question in `blocking_issues`; do not obtain new evidence inside Reconcile or treat a later Planner report as a Discovery source. Do not require fresh approval when existing authority settles the meaning or proof that all future checks will pass.

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

The requirements must exhaustively decompose every binding, implementation-affecting obligation and include precise acceptance criteria and conditions. Preserve intentionally unchanged behavior, evidence mapping, experiment summaries and limitations, credible disagreement, risks, compatibility concerns, caveats, and useful advisory suggestions. Include a rejected alternative only when selected Discovery evidence actually established that competing direction; cite that evidence. Do not invent hypothetical alternatives to make the artifact look complete. Use an empty `rejected_alternatives` array when no meaningful competing direction was established. Use empty arrays when another category genuinely has nothing to report; do not omit fields.

Ordinary requirements, evidence synthesis, rejected alternatives, and technical suggestions may cite only the exact selected Discovery artifacts, and referenced node IDs must exist. Never set `requirement.governing` or `frozen_user_constraint`; Rust adds frozen constraints mechanically. An actual Reconcile-time user answer may instead authorize a requirement with empty `source_refs` and `user_clarification` containing the exact answer. It cannot also cite Discovery sources.

Rust deterministically renders `reconciled-discovery.md` from this contract. Do not submit separate specification Markdown. Technical suggestions remain advisory; make an architectural property binding only when it must be audited. A non-empty `blocking_issues` list is reserved for a material issue that cannot responsibly be resolved from selected evidence or user authority.

Finalize against the same exact IDs:

```sh
orchestrate --root "<root>" reconcile finalize --effort "<effort>" \
  --discovery "<artifact-1>" --discovery "<artifact-2>" \
  --bundle "<absolute path to reconcile-proposal.json>"
```

After finalization succeeds, use the actual published outcome for the chat handoff, then stop. Do not inspect code, create Adoption, ask for Build approval, or start Build. For `IMPLEMENTATION_READY`, print the exact Build handoff; this does not itself start Build. The ready response is concise:

```text
Result: <one clear selected direction>

Key caveat: <only if materially important>

Artifact: <absolute path>

Next:
$build "/absolute/path/to/reconciled-discovery.md" and scaffold the Build. You are authorized to run the Build scaffold operation for this effort.
```

For `BLOCKED`, instead report the exact published artifact path, each material unresolved issue, and the user decision or upstream engineering evidence needed. Stop without the normal Build/scaffold handoff or a claim that the artifact is implementation-ready. If finalization fails, report the failure without inventing a published artifact or outcome.

`Artifact:` and any ready `Next:` path must use the actual absolute path to `reconciled-discovery.md`, not a bundle directory or a placeholder. The ready scaffold authorization applies only to the Build scaffold operation; it does not authorize Build execution or the driver. The artifact tells Build why; chat tells the user the answer.
