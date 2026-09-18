# Semantic evaluation corpus

These cases evaluate the phase guides and selected model adapters, not deterministic Rust behavior. Run the ignored harness only with explicitly selected provider commands:

```text
ORCHESTRATE_DISCOVERY_PROVIDER=/absolute/provider \
ORCHESTRATE_CONSENSUS_PROVIDER=/absolute/provider \
ORCHESTRATE_AUDIT_PROVIDER=/absolute/provider \
cargo test -p orchestrate --test semantic_evals -- --ignored --nocapture
```

Inspect every generated artifact manually. The corpus covers Discovery false-premise, hidden-edge-case, no-change, and product-blocker fixtures; Consensus equivalent-wording, condition-preservation, silence-versus-disagreement, useful-2-of-3, rotating-majority, and minority-only fixtures; and Audit good implementation, clear defect, missing verification, and optional-improvement fixtures.
