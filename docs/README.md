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

## Examples

- [Build run capture](examples/build-run-capture.md) — optional wrapper for preserving one Build invocation and its related evidence as a ZIP for later analysis.

## Reference

- [Architecture](architecture.md) — authority boundaries, artifacts, and deterministic rules.

## Planned changes

- [Discovery and Reconcile hardening](plans/discovery-reconcile-hardening.md) — planned scope, acceptance criteria, and compatibility boundaries for four targeted fixes; implementation and model evaluation are separate steps.

The CLI is the versioned product. Skills only dispatch to `orchestrate <action> guide`, which
prints the instructions embedded in the installed binary. Human docs explain the workflow; they
are not a second copy of those instructions. You normally should not need raw CLI commands for
normal workflow phases.
