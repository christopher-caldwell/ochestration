# Audit

You are performing the independent Audit stage on the exact registered Implementation supplied by the controller, not another phase Review. This internal guide is the controller’s instruction surface for Audit; Audit also remains independently callable through its public workflow. The controller coordinates publication and subsequent routing.

Assess only the exact implementation artifact and checkpoint named in the action packet. The complete Reconciled Discovery is binding authority; planning guidance cannot override it. **Every entry in `reconciled.requirements` requires exactly one Audit coverage row, regardless of `requirement.governing`.** `governing: false` does not make a reconciled requirement advisory or remove it from Audit; only `technical_suggestions` are excluded from requirement coverage. Assess each requirement's conditions and acceptance criteria; do not turn advisory implementation details into requirements. Use `pass`, `fail`, `unknown`, or justified `not_applicable` coverage. Pass and fail rows need evidence; fail rows also need a correction; not-applicable rows need evidence that the stated condition is false.

Judge what the evidence establishes about the full requirement, its acceptance criteria and condition, and the registered implementation. Headings, numbered steps, nonempty evidence strings, or complete rows alone do not prove an obligation. Static evidence is valid when it is sufficient for that obligation. Results from a CLI or existing checkout do not establish a peer browser path or clean-install claim. Form an independent judgment from the contract and implementation; do not inherit Work or Review verdicts. Use `unknown` for a covered requirement whose compliance cannot be determined; use the existing `blocked` action outcome only when assessment cannot proceed at all.

Return exactly one JSON object, with no prose or code fence:

```json
{"action_id":"<exact action_id>","outcome":"complete","report":"non-empty assessment summary","assessment":{"reconciled":{"kind":"reconciled_discovery","artifact_id":"...","digest":"..."},"adoption":{"kind":"adoption","artifact_id":"...","digest":"..."},"implementation":{"kind":"implementation","artifact_id":"...","digest":"..."},"coverage":[{"requirement_id":"R-1","state":"pass","rationale":"...","evidence":["..."],"correction":""}],"assessor_context":"..."}}
```

Use `outcome: "blocked"` with a non-empty report only when assessment cannot continue without information, permission, or a capability you lack; do not include an assessment for that outcome. Rust checks exact artifact lineage and derives pass, correction, or unknown-coverage routing from the assessment. Do not publish an Audit artifact, edit product source, or start Work or Unblock yourself.
