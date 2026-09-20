# Reconcile guide

Reconcile is a closed-world semantic phase. Determine the strongest defensible answer to **what are we actually going to do?** using only the frozen request and constraints plus the exact finalized Discovery artifacts selected by `orchestrate reconcile inputs`.

Do not inspect the target repository, a `source/` checkout, Git history, tests, web or vendor documentation, private Discovery chats, or mutable workspaces. Do not acquire new engineering evidence. You may analyze the selected artifacts deeply: compare their reasoning, recognize equivalent findings, distinguish silence from disagreement, retain a strong minority finding, combine complementary findings, and identify genuine gaps. Agreement count is informative, never an acceptance rule.

Write `reconcile-proposal.json` outside the repository and published artifact directories. Its shape is:

```json
{
  "core_result": "The concise authoritative result.",
  "requirements": [{
    "requirement": {"id": "R-1", "text": "Binding behavior.", "acceptance": "How Audit can verify it.", "condition": null, "governing": false},
    "source_refs": [{"discovery_artifact_id": "discovery-...", "node_id": "R-1"}]
  }],
  "technical_suggestions": [{
    "id": "TS-1",
    "text": "Optional implementation direction.",
    "source_refs": [{"discovery_artifact_id": "discovery-...", "node_id": "F-2"}]
  }],
  "blocking_issues": []
}
```

`core_result` is the concise authoritative answer to what will be done. `requirements` are the exhaustive, auditable decomposition of every implementation-affecting obligation in that result; each has acceptance criteria. `technical_suggestions` are advisory only.

An ordinary model-derived requirement must have at least one `source_refs` entry from the exact selected input set. A technical suggestion likewise needs at least one selected Discovery source. Never set `requirement.governing` to `true` or `frozen_user_constraint` to `true`: Rust rejects model-supplied governing authority. Frozen effort constraints are added mechanically as governing requirements.

When an explicit answer from the user during this Reconcile conversation resolves a material intent choice, it may be represented as direct user authority instead:

```json
"source_refs": [],
"user_clarification": "The exact user answer that resolved this requirement."
```

Use this only for an actual Reconcile-time user answer, never for new engineering evidence. A requirement with `user_clarification` cannot also cite Discovery sources. It does not permit source, Git, tests, experiments, web, vendor documentation, private Discovery conversations, or mutable workspaces. Rust records it as governing authority only through this explicit mechanism.

Rust deterministically renders `reconciled-discovery.md` from the finalized structured contract. Do not submit Markdown for that document or rely on prose outside the structured `core_result`, requirements, suggestions, and blocking issues. The document the user reviews is therefore the same contract Audit reads.

Technical suggestions are advisory. Promote an architectural property to a binding requirement only when that property itself must be audited. If a material question cannot be resolved from the selected artifacts or explicit user direction, use a non-empty `blocking_issues` list rather than inventing an answer.

Finalize with the same exact selectors passed to `inputs`. An implementation-ready Reconciled Discovery may be shown to the user and adopted only after explicit affirmative approval. A blocked result must not be adopted.
