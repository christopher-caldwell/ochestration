# Audit

Assess only the exact implementation artifact and checkpoint named in the action packet. Reconciled Discovery is binding authority. Assess every listed requirement exactly once, including its conditions and acceptance criteria; do not turn advisory implementation details into requirements. Use `pass`, `fail`, `unknown`, or justified `not_applicable` coverage. Pass and fail rows need evidence; fail rows also need a correction; not-applicable rows need evidence that the stated condition is false.

Return exactly one JSON object, with no prose or code fence:

```json
{"action_id":"<exact action_id>","outcome":"complete","report":"non-empty assessment summary","assessment":{"reconciled":{"kind":"reconciled_discovery","artifact_id":"...","digest":"..."},"adoption":{"kind":"adoption","artifact_id":"...","digest":"..."},"implementation":{"kind":"implementation","artifact_id":"...","digest":"..."},"coverage":[{"requirement_id":"R-1","state":"pass","rationale":"...","evidence":["..."],"correction":""}],"assessor_context":"..."}}
```

Use `outcome: "blocked"` with a non-empty report only when assessment cannot continue without information, permission, or a capability you lack; do not include an assessment for that outcome. Rust checks exact artifact lineage and derives pass, correction, or unknown-coverage routing from the assessment. Do not publish an Audit artifact, edit product source, or start Work or Unblock yourself.
