# Capture a Build run for analysis

This example wraps one normal `orchestrate build` invocation and creates a private ZIP with the evidence useful for a later execution postmortem.

It is deliberately **outside the Orchestrate runtime model**. It does not choose actions, change controller state, add provider-specific behavior, or interpret the run. It only records and copies evidence that already exists around the invocation.

## Run it

From the target repository:

```sh
EFFORT="<effort-id>" \
  bash docs/examples/capture-build-run.sh
```

Optional environment variables:

```sh
ORCHESTRATE_ROOT="$HOME/.orchestration"   # default
PROJECT_ROOT="/path/to/repository"        # default: current repository
CAPTURE_ROOT="$HOME/orchestrate-build-captures"  # default
```

`CAPTURE_ROOT` must be outside the target repository. This matters because a new Build intentionally refuses to start from a checkout containing ordinary untracked files.

The wrapper leaves the normal Build invocation unchanged:

```sh
orchestrate --root "<root>" build --effort "<effort>"
```

It only enables Rust backtraces when those variables are not already set. Provider event streams are already requested by the Build adapters and retained in each action's `transport.jsonl`; the wrapper does not invent additional provider tracing flags.

## What the archive contains

The resulting capture contains:

- controller stdout and inherited stderr from the outer Build invocation;
- start/end timestamps and process exit code;
- Orchestrate, Git, OS, and installed provider CLI versions;
- `status` before and after the Build plus the effort journal;
- the complete effort directory, including Build state, action files, reports, receipts, transport logs, Reconciled authority, Adoption, Audit, and upstream artifacts;
- the store marker and project metadata;
- every store source snapshot whose commit is referenced by the copied effort;
- Git HEAD/branch/status before and after the Build;
- the committed Build diff and list of commits between the starting and ending HEAD;
- staged and unstaged diffs plus non-ignored untracked files left after the run;
- a self-contained Git bundle for repository history;
- a SHA-256 inventory of the capture; and
- `capture/manifest.json`, which records the process exit code and the final Orchestrate `operation_status` / `semantic_outcome` when available.

A process exit code of zero is **not** itself a Build verdict. Read `capture/manifest.json` or the controller JSON output. For example, Orchestrate can report a semantic `BLOCKED` result without treating the CLI invocation as a process failure.

## Deliberate omissions

The wrapper does not sweep provider home/session directories such as all of `~/.codex` or `~/.claude`. Those locations can contain unrelated private conversations and their formats are provider-specific. The Build's provider-facing structured stdout remains in the action `transport.jsonl` files, and the controller state records observed session identities.

Contained Build review/audit checkouts named `source` and `verification` are also removed from the copied effort. They are shared Git clones and are not portable evidence by themselves. Their committed source is preserved by the referenced store snapshots and `git/repository.bundle`.

Ignored repository files are not copied. That avoids sweeping local environment files such as ignored credentials into a diagnostic archive.

## Limitations

This wrapper can only preserve information that Orchestrate, Git, or the provider processes expose. It cannot reconstruct an internal transient fact that was never emitted or persisted.

The capture is intentionally evidence-heavy and may contain source code, prompts, model output, error text, and other sensitive project information. Treat the ZIP as private and inspect it before sharing it outside the environment where the Build ran.

## Suggested analysis framing

Analyze the ZIP as an execution postmortem rather than as a fresh architecture review. Reconstruct the action/session/commit timeline, distinguish recorded evidence from agent claims, and evaluate scope fidelity, handoffs, review corrections, recovery behavior, verification, and repeated work. Recommend changes only when the captured run provides evidence for them.
