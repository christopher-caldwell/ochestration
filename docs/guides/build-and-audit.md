# Build and Audit

Build is an unattended, fixed Rust-driven work/review loop. Exact setup instructions are
`orchestrate build guide`. The adopted Reconciled Discovery still
defines **what** must be delivered; the approved detailed implementation plan supplies only the
ordered delivery-phase grouping.

## Prepare and run Build

Invoke `$build` after Adoption. It copies the detailed plan and fills `plan.json` and `config.toml`
from templates supplied by the installed CLI. Files you have already filled are left in place.
Worker and reviewer sessions may use different configured adapters. Exact setup rules, including
which adapters this CLI accepts, are supplied by `orchestrate build guide`.

`$build` invokes the driver after preparation; no second routine command is required. The
controller resolves exactly one prepared or active effort, then continues phase work, independent
phase review, corrections, implementation registration, and final Audit. Use `--effort` only when
automation has an explicit effort identity. Multiple eligible efforts are reported instead of guessed.

The driver gives each role the current action and the role guide from the CLI that is running.
It owns retries and corrections. It returns to you when the Build needs something only you can
supply, or when it cannot safely continue. You do not step the loop by hand.

## Audit

After the final phase passes, Build registers the implementation and runs Audit against the adopted
contract. Audit checks binding requirements. Technical suggestions stay advisory. Exact Audit
instructions are supplied by `orchestrate audit guide`.

The result is:

```text
PASS
CHANGES_REQUIRED
BLOCKED
```

- **PASS** — every binding requirement was verified.
- **CHANGES_REQUIRED** — at least one binding requirement failed.
- **BLOCKED** — Audit could not verify the full contract.

If Audit reports changes required, Build automatically routes the complete correction set to the
worker, registers the new exact commit, and runs a fresh Audit. A final PASS is derived from the
published Audit artifact, not a role's prose.

Back to the [full run guide](run.md).
