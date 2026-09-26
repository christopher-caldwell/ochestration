# Orchestrate documentation

## Start here

- [Run guide](guides/run.md) — **the normal workflow; use this first**.
- [Installation](guides/agent-installation.md) — easiest agent-led install plus manual fallback.

## Phase guides

- [Request preparation](guides/request-preparation.md) — clarify consequential user intent and review the common request before independent Discovery.
- [Discovery](guides/discovery.md) — run independent investigations and collect their outputs.
- [Reconcile](guides/reconcile.md) — turn selected Discovery outputs into one final contract.
- [Build and Audit](guides/build-and-audit.md) — run the unattended Build loop through registration and final verification.
- [Ticket workflow](guides/ticket-workflow.md) — compact ticket-specific checklist.

## Reference

- [Architecture](architecture.md) — authority boundaries, artifacts, and deterministic rules.
- [Build reliability lessons](evaluations/build-reliability/lessons.md) — why the controller is checkpointed and historical state is not migrated.
- [Discovery/Reconcile evaluation procedure](evaluations/discovery-reconcile/README.md) — repeatable, output-only evaluation using explicitly supplied attempts, with a separate operator rubric.

## Planned changes

- [Discovery and Reconcile hardening](plans/discovery-reconcile-hardening.md) — planned scope, acceptance criteria, and compatibility boundaries for four targeted fixes; implementation and model evaluation are separate steps.

The CLI is the versioned product and embeds the canonical workflow guides. A skill may request a
guide command only when the human explicitly authorizes that CLI operation. The `$build` skill is
deliberately non-executing: it uses the checked-in Build guide when available and does not invoke
the CLI, including for `guide` or `prepare`. Human docs explain the workflow without silently
starting a run. You normally should not need raw CLI commands for normal workflow phases.
