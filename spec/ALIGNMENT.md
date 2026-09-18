# Orchestration v0.1 Alignment Specification

This document is the controlling implementation contract for v0.1. Earlier specifications in `spec/orchestrate-specification/` are historical design material only where they conflict with this document.

The product guides capable agents through Discovery, Consensus, external Build, and Audit. Deterministic Rust code is a referee: it freezes inputs, preserves immutable artifacts, validates structural evidence and contradiction rules, and retains diagnostic execution history. It does not decide engineering meaning.

v0.1 requires one frozen request-plus-constraint context and committed Git baseline per three-slot cohort; isolated supplied Discovery workspaces; substantive technical specifications with lightweight DAG evidence graphs; whole-package Consensus majority; explicit Agreement adoption; exact implementation registration; coverage-derived Audit verdicts; immutable artifact lineage; and an append-only non-authoritative journal.

The intended public chain is:

```text
Discovery A/B/C → Consensus → adopted Agreement → external implementation → Audit
```

The detailed accepted behavior is implemented and tested in the workspace contracts, phase crates, CLI guides, and integration tests. Build scheduling, distributed writers, dashboards, generic workflow engines, and mandatory live-model release gates are deferred.
