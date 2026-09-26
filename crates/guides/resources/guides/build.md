# Build

## Human-controlled CLI boundary

Preparation, `$build`, and readiness discussion do not authorize any Orchestrate CLI command. Do not run `orchestrate build guide`, `prepare`, `scaffold`, `status`, `reset`, or the Build driver unless the human explicitly authorizes that exact operation. An explicit Build launch authorizes the Rust controller to run the Work ↔ Review phase loop through an exact Implementation candidate and then invoke the independent Audit stage automatically, with a single bounded Unblock detour; no per-transition or separate Audit approval is needed.

## Authority and preparation

Build runs Work ↔ Review across ordered phases and produces an exact Implementation candidate. Audit independently assesses that Implementation against the complete adopted Reconciled Discovery. These are distinct stages that the same unattended Rust driver coordinates; no separate process or orchestration command is required. Standalone Audit also accepts eligible registered Implementations from manual or external work without running Build.

Build implements one exact, implementation-ready Reconciled Discovery. That artifact is binding WHAT; the phase Markdown documents are HOW and ordering. Do not let the phase Markdown documents add, remove, or weaken requirements. The Discovery baseline must be an ancestor of product `HEAD`. A new Build starts only from a clean Git-visible checkout; ignored files are allowed.

The non-executing `orchestrate build prepare` command prints this guide. After separate authorization to scaffold, run `orchestrate build scaffold --effort <id>`. Fill the Build directory's `plan.json`, phase directories, and `config.toml`:

- `plan.json` schema 4 binds the exact Reconciled artifact and an ordered list of phase directory names, such as `phases: ["phase_01_foundation", "phase_02_delivery"]`. A phase is the dispatch, review, and checkpoint unit. Tasks are model-facing documents, never controller state.
- Each phase directory requires `phase.md`, describing its purpose, expected outcome, boundaries, implementation guidance, dependencies, and deliberate exclusions. Additional immediate `.md` files contain task/context guidance. Rust loads `phase.md` first, then the others in stable filename order, without recursion or prose parsing. The Planner chooses phases and task ordering/dependencies and makes each phase self-contained. Every role receives the complete binding Reconciled contract.
- `config.toml` schema 4 names `worker` and `reviewer`, with optional `unblocker`. Each role has an `adapter` (`codex`, `claude`, or `cursor`) and optional native `model` and opaque `args`.
- The controller hashes only the exact `plan.json` bytes at initialization. Each explicit Build launch validates the machine plan and phase documents and loads configuration. Do not edit plan files while Rust is running. After a semantic blocked stop, the operator may refine phase Markdown before launching again; those documents are not hashed into durable state. Each provider invocation records the exact settings and arguments it used.

Example phase layout (task filenames are the Planner's choice):

```text
build/
  plan.json
  config.toml
  phase_01_foundation/
    phase.md
    task_01.md
    task_02.md
  phase_02_delivery/
    phase.md
    notes.md
```

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

The controller begins at the current `HEAD` checkpoint and routes fixed gates. Work completes the entire assigned phase and commits against the product repository. Review inspects the whole phase and Audit inspects the complete implementation at that exact commit in disposable detached worktrees. Review passes to the next phase; after the last phase passes, the controller registers the exact Implementation and invokes independent Audit. Review correction returns to Work with the complete report. Rust publishes the Audit against that Implementation and derives the verdict from its assessment. `PASS` permits reviewed completion. `CHANGES_REQUIRED` routes to final-scope Build Work, followed by registration of a new Implementation candidate and another Audit; unknown or missing coverage enters Unblock.

A provider's explicit `blocked` result enters Unblock once in a disposable source checkout. `retry` restores the repository to its last checkpoint and retries the same gate with the originating context, original correction, and Unblock guidance. If the retry blocks again, or Unblock returns `blocked`, Build stops as `stopped / blocked`; it does not start another Unblock. Work partial changes are discarded back to the saved checkpoint. Blocker reports and guidance remain ordinary feedback. After addressing it, explicitly launch `build --effort <id>` to continue. Continuation never resets the product checkout: a clean unchanged checkout retries the gate, while a clean descendant commit becomes a candidate for Review or Audit. Dirty or unrelated repository state is rejected without deleting it. Explicit continuation starts a new attempt with one available Unblock detour.

Provider failure, malformed output, an invariant failure, or a restart while an action was marked `running` requires explicit destructive reset. `build reset --effort <id>` removes the current disposable worktree, restores the recorded checkpoint with `git reset --hard` and `git clean -fd`, preserves ignored files, and requeues the same gate. Inspect the durable facts with `build status --effort <id>`. Status dispatches nothing.

Worker and Reviewer sessions are optional conveniences held only in the running Rust process; Audit may share Reviewer and Unblock is always fresh. Sessions are absent from state.json and disappear when Rust exits. Every action packet is a complete handoff, including phase documents, binding authority, checkpoint, and feedback, so a new launch starts fresh conversations.

The supported operating model is one person running one local Rust Build process against a project. No daemon or multi-controller coordination is required.

The controller streams provider stdout and stderr into the action directory while the process runs, then writes parsed `result.json`, `report.md`, and Audit `assessment.json`; provider roles return a single JSON object and do not own controller evidence. Action history remains in the Build directory. Stderr carries compact gate/transition messages; stdout carries one JSON command result. A stopped result is not a completed Build.

Build state schema 5 and the current plan/config versions are intentionally strict. Older versions are unsupported; start a new Build from current accepted artifacts rather than adding a migration or recovery ladder.
