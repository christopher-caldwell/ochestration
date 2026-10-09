# Architecture

```text
Normal:
Prepare → Discovery × N → Reconcile ─┐
                                     ├→ Reconciled Discovery → STOP
Chat:
Conversation → import as 1 Discovery ┘

Later, explicitly:
Reconciled Discovery → Adoption → Build → Implementation → Audit
```

## Authority and instruction ownership

The prepared request and frozen user constraints establish intent. Discovery investigates; Reconcile publishes the binding contract. `plan.json` names the exact Reconciled Discovery and ordered phase directories. Phase Markdown describes implementation approach, ordering, boundaries, and task/context guidance. Rust routes work by phase but does not parse that prose. Work, Review, and Audit make engineering judgments. Rust validates exact artifacts, commits, response schemas, and checkout state, then routes the fixed state machine. It does not infer engineering truth from reports.

Checked-in skills dispatch substantive work. Build is intentionally different: `$build`, readiness discussion, or preparation is not CLI authorization. The canonical human boundary is in the embedded [Build guide](../crates/guides/resources/guides/build.md). An explicit Build launch authorizes Rust to run the Work ↔ Review phase loop, register the exact Implementation candidate, and invoke independent Audit automatically; individual transitions and Audit invocation require no additional approval.

## Discovery and Reconcile

Normal Discovery operates from the effort's frozen baseline and publishes an immutable evidence bundle. Normal Reconcile binds at least two distinct finalized implementation-ready Discovery artifacts; it has no repository access and adds no unselected investigation. The optional [Chat Discovery import](chat-discovery.md) records one collaborative conversation as one Discovery source, then publishes an ordinary Reconciled Discovery from it. The importer captures the target repository's committed HEAD as the Effort baseline and prepares Build files, but does not create Adoption or Build state. A Reconciled Discovery separates exhaustive binding requirements from advisory technical suggestions. Rust validates structure and lineage, not the truth of findings.

## Build and Audit responsibilities

Build answers whether all planned phases have been implemented and reviewed, producing an exact Implementation candidate. Audit independently assesses that exact Implementation against the complete binding adopted Reconciled Discovery. The same unattended Rust controller may coordinate both stages in sequence and react to the Audit verdict. This requires neither a separate OS process nor a second orchestration command. Rust keeps the train moving without an AI orchestrating transitions.

Standalone Audit remains available through its public workflow for any eligible registered Implementation, including manual or external work:

```text
Reconciled Discovery → Adoption → manual / external / other implementation
  → Implementation registration → Audit
```

Audit does not require BuildState, phase state, Worker or Reviewer sessions, Build action directories, or a prior Build controller run. The internal Final Audit guide is an instruction surface used by the unattended controller to invoke this independent assessment, not another phase reviewer.

## Build state and controller routing

Build schema versions are intentionally breaking: plan 4, config 4, state 5. There is no migration path for earlier Build state. Initialization requires a clean Git-visible checkout, verifies that the Discovery baseline is an ancestor of current `HEAD`, creates/reuses Adoption, and records `HEAD` as both the starting point and first checkpoint. A digest binds only the exact `plan.json` bytes; each explicit launch validates the plan and loads configuration once.

The durable controller state contains the exact Reconciled and Adoption refs, starting commit, immutable plan digest, scope, gate, status, checkpoint commit, bounded handoff refs, optional Unblock context, current action id, implementation ref, completion refs, and a transition-relevant stop. Every role receives the complete Reconciled contract; ordered phase directory names route Work/Review, and their Markdown documents are model-facing guidance. Action paths in state are relative to the Build directory. History is retained, but transitions use only state.

```text
phase Work ──complete──→ phase Review
     │                       ├─pass, more phases→ next phase Work
     │                       ├─pass, last phase→ register Implementation → Audit
     │                       └─changes_required→ same phase Work
     ├─blocked→ Unblock ─retry→ same gate / checkpoint
     │                       └─blocked→ stopped; explicit relaunch
     └─malformed/provider/Git failure→ reset_required; explicit reset

Audit ──PASS──→ reviewed completion
      ├─CHANGES_REQUIRED──→ final-scope Build Work → register new Implementation → Audit
      └─unknown/missing coverage──→ Unblock
```

`Gate::Audit` remains the controller’s routing point for invoking the independent Audit stage. Stage separation does not require the driver to exit after the last phase Review.

Review and Audit use detached disposable worktrees at the exact checkpoint. Work succeeds only when its reported commit exists, descends from the prior checkpoint, equals product `HEAD`, and leaves a clean tracked/untracked checkout. Review must return the inspected checkpoint and leave its worktree clean. Audit associates one exact submitted implementation artifact with its assessment; the existing Audit contract derives `PASS`, `CHANGES_REQUIRED`, or `BLOCKED`.

An explicit blocked result permits one Unblock detour in a disposable source checkout. A Work block is reset automatically because the provider completed and rejected its partial work. `retry` restores the checkpoint and requeues the original gate with originating context, original feedback, and Unblock guidance. A second block stops as `stopped / blocked`, preserving its report and the originating gate context; no recursive Unblock is available. Unblock `blocked` is also a clean stop. Explicit continuation preserves a clean operator repair, routes a descendant commit through Review or Audit, and starts a new attempt with one available Unblock detour. Provider failure, malformed output, Git invariant failure, or restart during `running` requires destructive `build reset`, which removes the current disposable worktree, runs `git reset --hard <checkpoint>` and `git clean -fd`, preserves ignored files, and requeues the same gate.

Phase packets include `phase.md` first, then other immediate Markdown documents in filename order. Rust does not parse their prose. Phase Markdown is not hashed and may change between a semantic stop and the next launch; the ordered machine plan remains frozen. Optional Worker session continuity is process-local and limited to one phase; Review PASS drops the Worker ID before the next phase or final scope. The first final-scope correction Work starts fresh, and later corrections may reuse the latest Worker ID. Every Review and internal Audit starts fresh and does not retain its output session ID. Unblock is sessionless. Sessions are not durable, and a new Rust invocation starts fresh from its complete packets. The supported operating model is one person, one local machine, and one Build process at a time.

Every launch records the selected native adapter config and exact argv before atomically marking state `running`. The adapters handle executable selection, native argv ordering, cwd/prompt delivery, streaming raw transport and stderr to ordinary action files, final-response/session extraction, and process exit. Rust persists parsed `result.json`, `report.md`, and Audit `assessment.json`.

## Artifacts and storage

Adoption binds one exact Reconciled artifact. Implementation records the start and target commits/trees and names Adoption and Reconciled ancestry; it may be registered by the Build controller or through the public registration workflow. Audit publishes immutable assessment plus derived verdict. Bundle manifests hash payloads and record parent refs; the journal is diagnostic, while artifacts and Build state are authoritative. Store format and artifact schema version 6 remain independent of the breaking Build-local schemas.

`orchestrate lineage` walks artifact parentage: Discovery artifacts → Reconciled Discovery → Adoption → Implementation → Audit. The Effort separately holds the original request, frozen context/constraints, and baseline. Together these provide traceability from user intent to the reviewed implementation; the original request is not an additional artifact in the lineage command’s graph.

## Opt-in investigations

`investigate run --config <path>` freezes one request/config/input identity, launches a fixed cohort
of fresh provider sessions in separate source/scratch workspaces, validates and seals each review
evidence graph, then calls one fresh evidence reconciler. Consensus votes are calculated by Rust
against the configured cohort; evidence recommendations separately preserve material objections.
Wide retains distinct supported, contested, insufficient and rejected findings with provenance.

Run-local version-1 manifests and results live under `<root>/investigations/<run-id>` and do not
change store/artifact schema 6 or canonical Audit lineage. Exact conformance refs use the existing
all-requirement coverage contract and verdict derivation without publishing an Audit. Generic
prompt/artifact reviews need no Effort or Build state. Inspect is offline and read-only.

Native transport and argv mechanics live in core's provider module; Build retains its own gate
policy and worker session continuity. Lanes are concurrent, single-invocation trusted local agents
following Discovery's source/scratch instructions. There is no new OS sandbox, file-lock system,
automatic retry or resume. Retained native receipts establish actual execution facts where the
adapter supplies them; missing facts remain testimony/unknown.
