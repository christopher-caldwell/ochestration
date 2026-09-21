# Build and Audit

Build is an unattended, fixed Rust-driven work/review loop. The adopted Reconciled Discovery still
defines **what** must be delivered; the approved detailed implementation plan supplies only the
ordered delivery-phase grouping.

## Prepare and run Build

Invoke `$build` after Adoption to copy the detailed plan and write contained `build/plan.json` and
`build/config.toml`. The Build skill uses the exact Adoption reference and never derives groups
from Markdown. Worker and reviewer sessions may use different configured adapters.

Supported adapters are `codex`, `claude`, and `cursor`. Their host configuration must already
permit unattended edits and checks; Build never adds force or permission-bypass flags. OpenCode is
not supported by this release.

`$build` invokes the driver after preparation; no second routine command is required. The
controller resolves exactly one prepared or active effort, freezes the plan/config digests,
then continues phase work, independent phase review, corrections, implementation registration,
and final Audit. Use `--effort` only when automation has an explicit effort identity. Multiple
eligible efforts are reported instead of guessed.

Every role turn receives exact absolute paths to its controller-generated `action.json` and
contained instruction. Workers commit reviewable changes; reviewers inspect a contained detached
checkout of that exact commit. If a provider stop is incomplete or malformed, Build first resumes
the same session, then replaces it, then asks a fresh session-less unblocker for a diagnosis. A
remedy automatically resumes the interrupted action; only an external requirement returns control
to the user. Each recovery step is tried once, so a failing scope reaches a decision instead of
looping, and an invocation whose acceptance state cannot be determined is never resent — Build
stops and names its transport log instead. The raw command is the recovery interface and does not
repeat completed work.

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
