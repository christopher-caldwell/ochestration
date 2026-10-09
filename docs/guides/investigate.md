# Multi-lane investigations

Investigations are opt-in reviews of one frozen question. They do not launch Build or publish a
canonical Audit result. Read `orchestrate investigate guide` for the embedded configuration,
evidence protocol and authority rules.

- **Consensus** reports the configured GO threshold separately from the evidence recommendation.
  A demonstrated material minority defect prevents GO even when the numeric threshold is met.
- **Wide** retains all credible distinct findings surfaced by completed lanes, including single-lane
  findings and contradictions. It provides no majority score or blanket claim of correctness.

Copy [the consensus example](../../examples/investigate/consensus.toml) or
[the wide example](../../examples/investigate/wide.toml), write the actual review request, and select
native adapters and settings. Repository input requires a full commit ID. A conformance review
requires exact Reconciled Discovery, Adoption and registered Implementation references.

```sh
orchestrate --root /absolute/orchestration/root investigate run --config /absolute/config.toml
orchestrate --root /absolute/orchestration/root investigate inspect --run investigation-... --json
```

Fresh lanes run concurrently in separate directories under the orchestration root. Like Discovery,
they have frozen source checkouts and scratch space. Tests and experiments require authorization
in the frozen request and belong in scratch; inspection-only requests grant no execution authority.
Lane instructions prohibit source/Git changes, peer-report access and production mutation. This
uses the existing local-agent trust model, with no added OS sandbox or lock system.

Inspect retained request/config/input identities, provider invocation records, original responses,
normalized command receipts, graph validation, manifests, reconciliation and report. Static source,
captured execution and testimony remain distinct. Mechanical validation establishes graph integrity,
not semantic truth. Native tool records without complete exit/completion facts cannot establish a
passing runtime check.

Each lane and reconciler call has a positive `provider_timeout_seconds` limit (default 1800 seconds).
Timed-out calls retain partial transport and count as failed attempts, so stalled providers cannot
indefinitely block aggregation. Build retains its existing execution behavior.

Git submodule paths and pinned commits appear in `repository.submodules`. Dependency contents are
not fetched or frozen; evidence about those contents remains unavailable.

A lane's `validation.json` describes graph validation before cohort checks. Final eligibility lives
in `cohort-validation.json` and `result.lanes`; a valid graph can be excluded for a duplicate session.
Sealed lane artifacts remain unchanged, and offline inspection verifies both stages.

Runs have no automatic retry or resume. A fresh retry creates a separate cohort/run. Partial files
survive failures; offline inspect reports an unfinished run as INCOMPLETE/INCONCLUSIVE. All output
stays local and can contain private source or evidence.
