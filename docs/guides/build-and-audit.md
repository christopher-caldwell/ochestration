# Build and Audit

Build is an unattended, fixed Rust-driven work/review loop. The adopted Reconciled Discovery still
defines **what** must be delivered; the approved detailed implementation plan supplies only the
ordered delivery-phase grouping.

## Prepare and run Build

Invoke `$build` after Adoption to copy the detailed plan and write contained `build/plan.json` and
`build/config.toml`. The Build skill uses the exact Adoption reference and never derives groups
from Markdown. Worker and reviewer sessions may use different configured adapters.

From the target repository, run:

```sh
orchestrate build
```

The controller resolves exactly one prepared or active effort, freezes the plan/config digests,
then continues phase work, independent phase review, corrections, implementation registration,
and final Audit. Use `--effort` only when automation has an explicit effort identity. Multiple
eligible efforts are reported instead of guessed.

Every role turn receives a controller-generated `action.json` and writes an immutable
`result.json` receipt. Workers commit reviewable changes; reviewers inspect a contained detached
checkout of that exact commit. If a provider stop is incomplete or malformed, Build first resumes,
then replaces the session, then records a bounded unblocker diagnosis if necessary. Re-running the
same command resumes the saved state; it does not repeat completed work.

## Audit

After the final phase passes, Build registers the exact commit and starts a fresh Audit session.
The Audit role locates the exact Reconciled Discovery → Adoption → Implementation chain, inspects
the immutable implementation snapshot, and checks every binding requirement once.

Technical suggestions do not create Audit requirements.

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
