# Capture a Build run for analysis

This example collects a Build run's evidence into a private ZIP useful for a later execution postmortem.

It is deliberately **outside the Orchestrate runtime model**. It does not choose actions, change controller state, add provider-specific behavior, or interpret the run. It records evidence that already exists and delegates the effort's own capture to `orchestrate build export`, which selects expected members before traversal and verifies what it archived.

## Run it

From the target repository:

```sh
EFFORT="<effort-id>" \
  bash docs/examples/capture-build-run.sh
```

Optional environment variables:

```sh
MODE=collect|run                           # default: collect
ORCHESTRATE_ROOT="$HOME/.orchestration"    # default
PROJECT_ROOT="/path/to/repository"         # default: current repository
CAPTURE_ROOT="$HOME/orchestrate-build-captures"  # default
```

`MODE=collect` (the default) dispatches nothing: it is safe on a stopped, blocked or historically failed run, and it makes zero Build and provider invocations. `MODE=run` additionally invokes the ordinary driver exactly as an operator would:

```sh
orchestrate --root "<root>" build --effort "<effort>"
```

and enables Rust backtraces only when those variables are not already set. Provider event streams are requested by the Build adapters and retained in each action's `transport.jsonl`; raw provider stderr in `provider-stderr.log`; per-dispatch facts in `invocation.json`. The wrapper invents no additional provider flags.

`CAPTURE_ROOT` must be outside the target repository. This matters because a new Build intentionally refuses to start from a checkout containing ordinary untracked files.

## What the archive contains

The resulting capture contains:

- `effort-evidence.zip`, the exporter's verified archive of the effort's durable evidence: authority and lineage bundles, plan/config and config history, state and migration evidence, the journal and resolutions, the requirement view and instructions, every applicable action's inputs/results/reports/transports/assessments plus invocation, stderr, ownership and cleanup records, and Git evidence sufficient to inspect the implementation;
- controller stdout and stderr from the outer invocation (`MODE=run`), or an explicit collect-only note;
- start/end timestamps, the Build process exit code and the export process exit code as separate facts;
- Orchestrate, Git, OS, and installed provider CLI versions;
- `status` before and after collection plus the effort journal;
- Git HEAD/branch/status before and after the Build;
- the committed Build diff and list of commits between the starting and ending HEAD;
- staged and unstaged diffs plus non-ignored untracked files left after the run;
- a self-contained Git bundle for repository history;
- a SHA-256 inventory of the capture; and
- `capture/manifest.json`, which records the process exit code and the final Orchestrate `operation_status` / `semantic_outcome` when available.

A process exit code of zero is **not** itself a Build verdict. Read `capture/manifest.json` or the controller JSON output. Orchestrate can report a semantic `BLOCKED` result without treating the CLI invocation as a process failure.

The Build exit code, the export exit code and the semantic outcome are independent. The wrapper exits nonzero when the export fails, and its manifest records all three; a successful Build never hides a failed export, and a failed export never rewrites the Build's own outcome. `effort-evidence.zip` exists only when the exporter verified that every selected member is present with its recorded hash, so its presence is a completeness claim while a ZIP that merely opens is not.

## Deliberate omissions

The wrapper does not sweep provider home/session directories such as all of `~/.codex` or `~/.claude`. Those locations can contain unrelated private conversations and their formats are provider-specific. The Build's provider-facing structured stdout remains in the action `transport.jsonl` files, and the controller state records observed session identities.

Contained Build review/audit/once-over checkouts are excluded from the exported effort by class, together with Discovery source and scratch workspaces. They are shared Git clones and bulky generated trees, not portable evidence by themselves. Their exact commits are recorded in the exported Build state and `git-evidence/checkout.json`, their committed source is preserved by `git-evidence/history.bundle` — which carries the whole history reachable from the recorded commit, needs no excluded baseline and restores into an empty repository even when the source never changed after Build started — and the tracked changes a stopped run left uncommitted travel as `git-evidence/partial-work.patch` while untracked files are listed by name only. The exporter discloses the excluded classes in its report rather than silently dropping them.

Ignored repository files are not copied. That avoids sweeping local environment files such as ignored credentials into a diagnostic archive.

## Limitations

This wrapper can only preserve information that Orchestrate, Git, or the provider processes expose. It cannot reconstruct an internal transient fact that was never emitted or persisted.

The capture is intentionally evidence-heavy and may contain source code, prompts, model output, error text, and other sensitive project information. Treat the ZIP as private and inspect it before sharing it outside the environment where the Build ran.

## Suggested analysis framing

Analyze the ZIP as an execution postmortem rather than as a fresh architecture review. Reconstruct the action/session/commit timeline, distinguish recorded evidence from agent claims, and evaluate scope fidelity, handoffs, review corrections, recovery behavior, verification, and repeated work. Recommend changes only when the captured run provides evidence for them.
