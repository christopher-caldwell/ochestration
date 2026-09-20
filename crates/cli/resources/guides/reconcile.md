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
  "blocking_issues": [],
  "reconciliation_md": "# Reconciled Discovery\\n..."
}
```

The binding core must state required outcomes, changed and unchanged behavior, constraints, meaningful edge cases, and acceptance criteria. Each model-derived requirement and each technical suggestion needs at least one source from the exact selected input set. A governing user constraint is direct authority and is added mechanically by Rust.

Technical suggestions are advisory. Promote an architectural property to a binding requirement only when that property itself must be audited. If a material question cannot be resolved from the selected artifacts or explicit user direction, use a non-empty `blocking_issues` list rather than inventing an answer.

Finalize with the same exact selectors passed to `inputs`. An implementation-ready Reconciled Discovery may be shown to the user and adopted only after explicit affirmative approval. A blocked result must not be adopted.
