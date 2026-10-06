# Build

## Human-controlled CLI boundary

Preparation, `$build`, and readiness discussion do not authorize any Orchestrate CLI command. Do not run `orchestrate build guide`, `prepare`, `scaffold`, `status`, `reset`, or the Build driver unless the human explicitly authorizes that exact operation. An explicit Build launch authorizes the Rust controller to run the Work ↔ Review phase loop through an exact Implementation candidate and then invoke the independent Audit stage automatically, with a single bounded Unblock detour; no per-transition or separate Audit approval is needed.

## Authority and preparation

Build runs Work ↔ Review across ordered phases and produces an exact Implementation candidate. Audit independently assesses that Implementation against the complete adopted Reconciled Discovery. These are distinct stages that the same unattended Rust driver coordinates; no separate process or orchestration command is required. Standalone Audit also accepts eligible registered Implementations from manual or external work without running Build.

Build implements one exact, implementation-ready Reconciled Discovery. That artifact is binding WHAT; the phase Markdown documents are HOW and ordering. The Discovery baseline must be an ancestor of product `HEAD`. A new Build starts only from a clean Git-visible checkout; ignored files are allowed.

The non-executing `orchestrate build prepare` command prints this guide. After separate authorization to scaffold, run `orchestrate build scaffold --effort <id>`. Fill the Build directory's `plan.json`, phase directories, and `config.toml`:

- `plan.json` schema 4 binds the exact Reconciled artifact and an ordered list of phase directory names, such as `phases: ["phase_01_foundation", "phase_02_delivery"]`. A phase is the dispatch, review, and checkpoint unit: a substantial, coherent implementation slice that can be built, meaningfully verified, and reviewed efficiently as one unit. Optimize boundaries for efficient Work → Review cycles and useful accepted checkpoints. Tasks are model-facing documents, never controller state.
- Each phase directory requires `phase.md`, describing its purpose, expected outcome, boundaries, implementation guidance, dependencies, and deliberate exclusions. Additional immediate `.md` files contain task/context guidance. Rust loads `phase.md` first, then the others in stable filename order, without recursion or prose parsing. The Planner chooses phases and task ordering/dependencies and makes each phase self-contained. Every role receives the complete binding Reconciled contract.
- Plan phases at a meaningful size between one task per Review and a near-whole-project mega-phase. Include related work and the tests or verification naturally associated with the slice; group multiple related tasks when together they form a better Work → Review unit. Task enumeration alone is not a reason to add another phase, and phase count follows the shape of the work rather than a numeric target. When substantial reviewable implementation has a natural checkpoint before later independently blocking work, such as external credentials, environment-specific integration, or real-service acceptance, separate that later work. This is conditional guidance, not a requirement to create extra phases in every plan.
- `config.toml` schema 5 names `worker` and `reviewer`, with optional `unblocker`. Each role has an `adapter` (`codex`, `claude`, or `cursor`) plus optional native `model`, neutral `effort`, and opaque `args`.
- The controller hashes only the exact `plan.json` bytes at initialization. Each explicit Build launch validates the machine plan and phase documents and loads configuration. Do not edit plan files while Rust is running. After a semantic blocked stop, the operator may refine phase Markdown before launching again; those documents are not hashed into durable state. Each provider invocation records the exact settings and arguments it used.

Author phase and task Markdown as implementation guidance, not a second product specification. Focus on implementation surfaces, technical approach, sequencing, dependencies, verification, risks, and review checkpoints; brief behavioral context is useful. For each substantial phase, say what its slice will establish, what meaningful evidence Review can inspect, and what remains for later phases or final verification. A requirement may span phases; a reviewed local phase does not establish full-contract acceptance. Do not label planning prose as binding, add or remove product behavior, restate requirements with stronger or weaker wording or missing conditions and exceptions, or invent acceptance conditions. When binding behavior matters, prefer references to relevant Reconciled requirement or constraint IDs over paraphrases; this does not require exhaustive requirement-to-phase mapping. Before treating a plan as ready, compare the complete Reconciled contract with every proposed phase document and example, especially the assertions and conditions behind acceptance commands and qualified exclusions. Correct conflicting phase guidance, including provider-specific examples and omission/default claims, rather than changing binding authority. Repair phase ordering, boundaries, routine implementation choices, and missing planned tests locally.

Return upstream before launch only when planning cannot proceed honestly without changing binding meaning, resolving missing user authority, or assuming a material engineering fact. Name the exact Reconciled artifact and affected requirements, cite the conflicting or insufficient premises, explain why a planning edit cannot resolve them, and identify the missing decision or evidence. A synthesis defect resolvable from selected evidence needs an explicitly requested replacement Reconcile; a user-owned choice needs its actual answer through the existing authority path; a missing technical fact needs new isolated Discovery evidence and explicit selection of compatible Reconcile inputs. Planner feedback is a question or pointer, not engineering evidence for Reconcile. Reuse an effort only while its frozen request, constraints, baseline, and lineage remain valid; changed frozen inputs need a new effort. A replacement prelaunch plan may bind replacement authority, but an initialized Build keeps its exact plan and artifact binding. Do not edit published authority or treat a repository owner note as an implicit amendment.

Keep settled contract meaning, execution capability, and completion evidence distinct. A supported hard live gate can remain pending in an otherwise credible plan; absent credentials for an understood check call for setup or a hold before that action, not a new Discovery by default. Do not call a launch operationally ready when an immediately necessary permission or capability is absent. On a material upstream return, do not print the ready-to-launch command, start Build, or launch an automatic upstream retry loop. An unfinished prelaunch scaffold may remain.

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
schema_version = 5

[worker]
adapter = "codex"
model = "native-model-name" # optional
effort = "high"             # optional; omission adds no first-class effort setting
args = ["--search"]         # optional native arguments, appended after any first-class effort

[reviewer]
adapter = "claude"

# Optional; absent means Unblock uses reviewer settings, without a session.
# [unblocker]
# adapter = "cursor"
# model = "claude-opus-4-8-thinking-high" # provider-native effort variant; omit first-class effort
```

`effort` records provider-neutral intent, while accepted values depend on the selected adapter. Values must use their exact lowercase spelling; Build validates them against provider-level adapter support and does not check whether a particular model supports them. Omitting it adds no first-class effort setting; Build does not choose a default.

- Codex accepts `none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, and `ultra`, translated through its `model_reasoning_effort` config override.
- Claude accepts `low`, `medium`, `high`, `xhigh`, `max`, and `ultracode`, translated as `--effort <value>`.
- Cursor does not support the first-class `effort` setting. Set an effort-bearing provider-native value through `model`, for example `model = "claude-opus-4-8-thinking-high"`; Build passes that string through unchanged.

Accepted opaque `args` are appended unchanged after any translated first-class effort arguments. They may independently contain provider-native effort settings; Build does not inspect, normalize, reconcile, or prevent those settings. The invocation record preserves both the neutral configured effort and the exact translated argv.

After the Build is fully scaffolded and ready to launch, print the exact launch command using the actual resolved Orchestrate root and effort:

```sh
orchestrate --root "/actual/root" build --effort "actual-effort"
```

Do not print placeholders when those values are known. Do not run the command; printing it does not authorize Build execution, and launching Build requires separate explicit human authorization.

## Launch and durable state

Only an explicit launch starts provider work:

```sh
orchestrate --root "<root>" build --effort "<effort>"
```

The controller begins at the current `HEAD` checkpoint and routes fixed gates. Work completes the entire assigned phase and commits against the product repository. Review inspects the whole phase and Audit inspects the complete implementation at that exact commit in disposable detached worktrees. Review passes to the next phase; after the last phase passes, the controller registers the exact Implementation and invokes independent Audit. Review correction returns to Work with the complete report. Rust publishes the Audit against that Implementation and derives the verdict from its assessment. `PASS` permits reviewed completion. `CHANGES_REQUIRED` routes to final-scope Build Work, followed by registration of a new Implementation candidate and another Audit; unknown or missing coverage enters Unblock.

A provider's explicit `blocked` result enters Unblock once in a disposable source checkout. `retry` restores the repository to its last checkpoint and retries the same gate with the originating context, original correction, and Unblock guidance. If the retry blocks again, or Unblock returns `blocked`, Build stops as `stopped / blocked`; it does not start another Unblock. Work partial changes are discarded back to the saved checkpoint. Blocker reports and guidance remain ordinary feedback. After addressing it, explicitly launch `build --effort <id>` to continue. Continuation never resets the product checkout: a clean unchanged checkout retries the gate, while a clean descendant commit becomes a candidate for Review or Audit. Dirty or unrelated repository state is rejected without deleting it. Explicit continuation starts a new attempt with one available Unblock detour.

Provider failure, malformed output, an invariant failure, or a restart while an action was marked `running` requires explicit destructive reset. Before reset, inspect and save any work you need: `orchestrate build reset --effort <id>` removes the current disposable worktree, restores the recorded checkpoint with `git reset --hard` and `git clean -fd`, discards tracked edits, removes non-ignored untracked files and directories, preserves ignored files, and requeues the same gate. `orchestrate build status --effort <id>` displays durable facts and recovery guidance without dispatching work; append `--json` for the structured status envelope.

Worker session continuity is optional and process-local. Work may reuse the latest usable matching-adapter ID within a phase; when Review passes, the controller drops it before advancing. The first final-scope correction Work therefore starts fresh, while later final-scope correction Work may reuse the latest Worker ID. Every phase Review and internal final Audit starts fresh, and their output IDs are not retained. Unblock is always sessionless. Sessions are absent from state.json and disappear when Rust exits. Every action packet is a complete handoff, including phase documents, binding authority, checkpoint, and feedback, so a new launch starts fresh conversations.

The supported operating model is one person running one local Rust Build process against a project. No daemon or multi-controller coordination is required.

The controller streams provider stdout and stderr into the action directory while the process runs, then writes parsed `result.json`, `report.md`, and Audit `assessment.json`; provider roles return a single JSON object and do not own controller evidence. Action history remains in the Build directory. The existing `orchestrate build --effort <id>` invocation always shows a live status and append-only event history on stderr; stdout remains exactly one JSON command result with the same outcome and exit semantics. The display replaces the old compact transition lines and adds no command, wrapper requirement, or persisted display files. Provider evidence remains in the ordinary action files.

The status frame shows recorded status and gate, phase or final scope, the full current action ID when present, and a display-only checkpoint prefix. The 20-cell bar counts reviewed phases, never tasks. A full reviewed-phase bar does not mean final Audit passed: final-scope Work or Unblock may still be pending or running. Only recorded completion says Build complete. Prior reviewed phases are summarized from saved scope, not replayed from historical actions. Recorded Running does not prove process liveness; long actions retain their last recorded view without timers or invented progress. `build status` is human-readable by default and shows stop detail, available evidence paths, and stop-specific next steps; `build status --json` prints the structured envelope for scripts. `reset_required` guidance explains the cleanup before suggesting reset, while `blocked` guidance directs the operator to feedback and explicit relaunch.

Terminal output updates a small status frame in place while retaining event history. By default, piped stderr is plain append-only output: an initial snapshot, flushed live event lines, and a final snapshot, with no ANSI or cursor movement. `ORCHESTRATE_BUILD_COLOR=auto|always|never` takes precedence over `NO_COLOR`: explicit `auto` uses whether stderr is a terminal, `always` forces colored in-place output even under a pipe (including ANSI and cursor control), and `never` stays plain even on a terminal. When the override is unset, nonempty `NO_COLOR` selects plain output; empty `NO_COLOR` leaves automatic detection enabled. Unsupported override values conservatively select plain output. The fixed layout may wrap in narrow terminals; use `never` for append-only output. Status, scaffold, and reset retain their existing behavior, including reset's `RESET` stderr line.

Build state schema 5 and the current plan/config versions are intentionally strict. Older versions are unsupported; start a new Build from current accepted artifacts.
