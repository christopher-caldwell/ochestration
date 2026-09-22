# Orchestrate documentation

## Start here

- [Run guide](guides/run.md) — **the normal workflow; use this first**.
- [Installation](guides/agent-installation.md) — easiest agent-led install plus manual fallback.

## Phase guides

- [Request preparation](guides/request-preparation.md) — prepare a ticket or freeform request.
- [Discovery](guides/discovery.md) — run independent investigations and collect their outputs.
- [Reconcile](guides/reconcile.md) — turn selected Discovery outputs into one final contract.
- [Build and Audit](guides/build-and-audit.md) — run the unattended Build loop through registration and final verification.
- [Ticket workflow](guides/ticket-workflow.md) — compact ticket-specific checklist.

## Reference

- [Architecture](architecture.md) — authority boundaries, artifacts, and deterministic rules.

The CLI is the versioned product. Skills only dispatch to `orchestrate <action> guide`, which
prints the instructions embedded in the installed binary. Human docs explain the workflow; they
are not a second copy of those instructions. You normally should not need raw CLI commands for
normal workflow phases.
