# Build and Audit

Build is an unattended, fixed Rust-driven work/review loop. Its canonical workflow is embedded in
the CLI and in `crates/guides/resources/guides/build.md`. `$build`, a request to prepare, or a
readiness discussion does not authorize chat to run an Orchestrate CLI command. The non-executing
CLI help path is `orchestrate build prepare`; a human must explicitly ask before chat runs that
command. The exact Reconciled Discovery defines **what** must be delivered; the detailed
implementation plan provides implementation approach and ordered delivery-phase grouping.

Reconciled Discovery is binding **what**. The detailed implementation plan is **how** and ordering.
If they materially conflict, Reconciled Discovery wins, but Build stops before implementation rather
than silently changing the plan or improvising around the conflict.

## Prepare and run Build

The chat may prepare files only when the human explicitly authorizes those writes. Running the Rust
driver requires explicit authorization for the exact Build operation; `$build` alone never starts
or resumes it and never runs paid preflight. The driver binds the Reconciled Discovery, creates or
reuses Adoption, captures current Build starting HEAD/tree, and verifies that the frozen Discovery
baseline is its ancestor. The repository may advance between Discovery and Build. Build requires a
clean Git-visible checkout when starting a new Build: no staged or unstaged tracked changes or
ordinary untracked files. Ignored local environment files are allowed. Existing Build files and
state are resumed rather than re-scaffolded.

Worker and reviewer sessions may use different configured adapters. Once an explicitly authorized
driver launch begins, the Rust controller owns phase work, independent phase review, corrections,
implementation registration, and final Audit without asking approval at every internal transition.
Use `--effort` only when automation has an explicit effort identity. Multiple eligible efforts are
reported instead of guessed.

The driver gives each role the current action and the role guide from the CLI that is running.
It owns retries and corrections. It returns to you when the Build needs something only you can
supply, or when it cannot safely continue automatically. You do not step the loop by hand.

## Roles and attributes

Schema-v3 `config.toml` declares worker and independent reviewer, with optional separate unblocker
and advisory once-over. Each role uses provider-neutral `model_strength = "standard"|"strong"`,
`reasoning_effort = "low"|"medium"|"high"`, and
`permission = "read_only"|"workspace_write"|"full_access"`, where `full_access` is the explicit
unrestricted mapping a role only ever receives when an operator writes it.
The [provider translation reference](provider-mappings.md) gives the complete per-setting mapping —
model, effort, permission and transport — for each adapter. Unsupported values fail before dispatch with role, field, value, and adapter.
Schema-v2 frozen configurations remain readable with their original provider-native meaning; never
edit a frozen config in place. A stopped v2 Build can record a v3 config through the
`environment_repair` resolution overlay. Final Audit uses reviewer settings; an absent unblocker
falls back to them; an absent once-over means it never runs.

Before product-changing work, `orchestrate build preflight --effort "<effort>"` resolves each
role's executable and version, the exact arguments it would dispatch, and what cannot be
established locally. The optional live probe spends real provider inference inside disposable
owned fixtures and runs only with `--live --authorize-live`; without that authorization it refuses
and prints the cost and side effects.

## Watching and stopping

`orchestrate build status --effort "<effort>"` is read-only. It reports the current phase, action,
scope, dispatch state, configured role attributes, accepted phases out of the plan's total,
invocation elapsed time, last provider activity, the durable stop, and the latest Audit's verdict
and unresolved requirement IDs. Controller liveness comes from the lock; it is reported separately
from whether a provider process may still be running. A Build's stdout is one JSON result; progress,
transitions and heartbeats go to stderr so a redirected run still parses.

A stop is a recorded acceptance decision, not a crash. The driver prints the trigger, the stopped
action and the state path, and never retries an uncertain provider action by itself. Resolve it
with `orchestrate build resolve` (see the canonical Build guide); resolution records your
intervention, preserves the original failure and corrections, and authorizes one continuation.
A proposed change to the adopted contract is refused and directed to a linked successor Build.

## Cleanup and export

After an action's evidence is durable, `orchestrate build cleanup --effort "<effort>" [--dry-run]`
releases generated Cargo products inside checkouts the controller recorded and owns. It removes
known generated output entry by entry, so a fixture the action's own evidence cites, an unfamiliar
profile, or any file the controller cannot identify as its own generated output is preserved and
reported — with the exact path and reason — instead of being deleted as part of a directory. It
never touches tracked source, history, evidence, your product checkout, or provider homes, and its
failures are reported separately from Build outcomes.

`orchestrate build export --effort "<effort>" --output "<path>.zip"` derives the expected members
from durable state and from what each writer's own history obliges before traversing anything,
excludes bulky generated and scratch trees by class, and verifies membership and hashes by reading
the archive back before promoting it. A completed action's lost packet, transport or receipt fails
the collection; an interrupted or older action's genuinely absent record is disclosed as such.
Referenced evidence is included — nested Build records, live-probe directories, and the evidence a
resolution names, checked against the digest that record published. Each export owns its scratch
paths, so an unrelated file under a similar name is reported and left untouched. The archive's Git
evidence restores into an empty repository without the excluded baseline, including uncommitted
partial work. Export status is reported separately from the Build's semantic outcome: a BLOCKED
Build can export successfully, and a PASS Build cannot hide a failed export. A valid ZIP is not by
itself proof of a complete capture.

`docs/examples/capture-build-run.sh` wraps either an existing stopped run (the default
`MODE=collect`, which dispatches nothing) or a fresh `MODE=run` invocation, and reports the Build
exit code, the export exit code and the semantic outcome as separate facts.

## Provenance and offline usage

Every dispatch keeps a passive record beside its action: the action id, kind and scope with the
role and adapter, fresh, resumed or replacement mode, the session this dispatch *asked for* held
separately from the session the provider's own stream reported, the requested attributes and
whether each came from configuration or is the provider's own default, the resolved executable
path, version and digest, the exact argv the launch used with only the prompt elided, dispatch and
completion times with the monotonic elapsed duration, the process exit or launch failure, the
output locations, and the byte copy and digest of the exact instruction that invocation received.
Published Implementation and Audit artifacts record the adapter, model and effort that produced
them; an unset attribute stays unknown rather than becoming a frozen effective value. Raw provider
transport is retained verbatim and is never normalized by the controller.

Usage analysis happens offline over that evidence. `docs/examples/usage-accounting.py` reads the
retained `transport.jsonl` files from an exported effort or a capture ZIP and converts the Codex
CLI 0.156.1 cumulative `turn.completed` counters into per-observation deltas across resumed
sessions, suppresses transport copied into more than one capture, marks counter resets, reports
actions with no usage as unknown rather than zero, and keeps cache-read and reasoning tokens as
subsets that are never added twice. Its documented fixtures run with
`python3 docs/examples/usage-accounting.py --self-test`. These are provider-reported token counts,
never a cost, billing, latency or subscription figure.

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
