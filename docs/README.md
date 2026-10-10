# Orchestrate documentation

## Start here

- [Run guide](guides/run.md) — **the normal workflow; use this first**.
- [Installation](guides/agent-installation.md) — easiest agent-led install plus manual fallback.
- [Discovery walkthrough](guides/discovery-walkthrough.md) — a bounded synthetic bug, diagrams,
  and a manual procedure ending at Reconcile.

## Phase guides

- [Request preparation](guides/request-preparation.md) — clarify consequential user intent and review the common request before independent Discovery.
- [Discovery](guides/discovery.md) — run independent investigations and collect their outputs.
- [Reconcile](guides/reconcile.md) — turn selected Discovery outputs into one final contract.
- [Build and Audit](guides/build-and-audit.md) — run Build through Implementation registration, then independent Audit; also covers standalone Audit.
- [Investigations](guides/investigate.md) — opt-in consensus and wide evidence-backed reviews.
- [Ticket workflow](guides/ticket-workflow.md) — compact ticket-specific checklist.

## Reference

- [Architecture](architecture.md) — authority boundaries, artifacts, and deterministic rules.
- [Investigation verification](evaluations/investigate/README.md) — scripted coverage and the bounded live multi-provider smoke, including transport limitations.
- [Build reliability lessons](evaluations/build-reliability/lessons.md) — why the controller is checkpointed and historical state is not migrated.
- [Discovery/Reconcile and guidance evaluation procedure](evaluations/discovery-reconcile/README.md) — output-only evaluations for Discovery, Reconcile, Planner, Chat Discovery, and bounded Work/Review/Audit evidence cases, with an operator-only rubric. Raw evaluation inputs and records remain external; the issue-29 effort has a sanitized checked-in status summary.

## Historical design records

- [Discovery and Reconcile hardening](plans/discovery-reconcile-hardening.md) — original scope and acceptance criteria for implemented changes; model-quality evaluation remains pending.

The CLI is the versioned product and embeds the canonical workflow guides. A skill may request a
guide command only when the human explicitly authorizes that CLI operation. The `$build` skill is
deliberately non-executing: it uses the checked-in Build guide when available and does not invoke
the CLI, including for `guide` or `prepare`. Human docs explain the workflow without silently
starting a run. You normally should not need raw CLI commands for normal workflow phases.
