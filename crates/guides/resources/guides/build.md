# Build

## Human-controlled CLI boundary

Preparation, `$build`, and readiness discussion do not authorize any Orchestrate CLI command. Do not run `orchestrate build guide`, `prepare`, `scaffold`, `status`, `reset`, `resume`, or the Build driver unless the human explicitly authorizes that exact operation. An explicit Build launch authorizes the Rust controller to own the complete internal Work → Review → Audit loop and its single bounded Unblock detour; no per-gate approval is needed.

## Authority and preparation

Build implements one exact, implementation-ready Reconciled Discovery. That artifact is binding WHAT; the detailed plan is HOW and ordering. Do not let the detailed plan add, remove, or weaken requirements. The Discovery baseline must be an ancestor of product `HEAD`. A new Build starts only from a clean Git-visible checkout; ignored files are allowed.

The non-executing `orchestrate build prepare` command prints this guide. After separate authorization to scaffold, run `orchestrate build scaffold --effort <id>`. Fill the Build directory's `plan.json`, `implementation-plan.md`, and `config.toml`:

- `plan.json` schema 3 binds the exact Reconciled artifact, detailed-plan path, and ordered phases. Each phase has an `id`, `tasks`, and referenced `requirement_ids`.
- `config.toml` schema 4 names `worker` and `reviewer`, with optional `unblocker`. Each role has an `adapter` (`codex`, `claude`, or `cursor`) and optional native `model` and opaque `args`.
- The controller hashes the exact plan and detailed-plan bytes at initialization. Editing either later stops the run; configuration is read again before every gate and each invocation records the exact settings and arguments it used.

Example config:

```toml
schema_version = 4

[worker]
adapter = "codex"
model = "native-model-name" # optional
args = ["--search"]         # optional native arguments

[reviewer]
adapter = "claude"

# Optional; absent means Unblock uses reviewer settings, without a session.
# [unblocker]
# adapter = "cursor"
```

## Launch and durable state

Only an explicit launch starts provider work:

```sh
orchestrate --root "<root>" build --effort "<effort>"
```

The controller begins at the current `HEAD` checkpoint and routes fixed gates. Work commits against the product repository. Review and Audit inspect that exact commit in disposable detached worktrees. Review passes to the next phase or final Audit; correction returns to Work with the complete report. Audit publishes the exact implementation lineage and derives the verdict from its assessment. A failed assessment routes to final-scope Work; unknown or missing coverage enters Unblock.

A provider's explicit `blocked` result enters Unblock once. `retry` restores the repository to its last checkpoint and retries the same gate with both the original correction and Unblock guidance. If the retry blocks again, Build stops; it does not start another Unblock. `external_requirement` stops cleanly. After addressing it, explicitly run `build resume --effort <id>`, then explicitly launch `build --effort <id>` to continue. Resume itself dispatches no provider.

Provider failure, malformed output, an invariant failure, or a restart while an action was marked `running` requires explicit reset. `build reset --effort <id>` removes the current disposable worktree, restores the recorded checkpoint with `git reset --hard` and `git clean -fd`, preserves ignored files, clears sessions, and requeues the same gate. Inspect the durable facts with `build status --effort <id>`. Status dispatches nothing.

The controller writes parsed `result.json`, `report.md`, and Audit `assessment.json`; provider roles return a single JSON object and do not own controller evidence. Action history remains in the Build directory. Stderr carries compact gate/transition messages; stdout carries one JSON command result. A stopped result is not a completed Build.

Schema 3 Build state and older plan/config versions are intentionally unsupported. Start a new Build from current accepted artifacts; no migration or recovery ladder is provided.
