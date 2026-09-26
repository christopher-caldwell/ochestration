# Architecture

```text
Prepare → Discovery × N → Reconcile → STOP

Later: explicit Build → Adoption → Work → Review → Audit → done
                                      ↑          │
                                      └─ correction
```

## Authority and instruction ownership

The prepared request and frozen user constraints establish intent. Discovery investigates; Reconcile publishes the binding contract. The detailed Build plan controls only implementation approach and ordering. Work, Review, and Audit make engineering judgments. Rust validates exact artifacts, commits, response schemas, and checkout state, then routes the fixed state machine. It does not infer engineering truth from reports.

Checked-in skills dispatch substantive work. Build is intentionally different: `$build`, readiness discussion, or preparation is not CLI authorization. The canonical human boundary is in the embedded [Build guide](../crates/guides/resources/guides/build.md). An explicit Build launch authorizes the full internal gate loop; individual transitions require no additional approval.

## Discovery and Reconcile

Each Discovery operates from the effort's frozen baseline and publishes an immutable evidence bundle. Reconcile binds an explicit set of finalized Discovery artifacts; it has no repository access and adds no unselected investigation. A Reconciled Discovery separates exhaustive binding requirements from advisory technical suggestions. Rust validates structure and lineage, not the truth of findings.

## Build state and routing

Build schema versions are intentionally breaking: plan 3, config 4, state 4. There is no migration path for earlier Build state. Initialization requires a clean Git-visible checkout, verifies that the Discovery baseline is an ancestor of current `HEAD`, creates/reuses Adoption, and records `HEAD` as both the starting point and first checkpoint. A digest binds the exact `plan.json` bytes and referenced detailed-plan bytes; each explicit launch validates the plan and loads configuration once.

The durable controller state contains the exact Reconciled and Adoption refs, starting commit, immutable plan digest, scope, gate, status, checkpoint commit, bounded handoff refs, optional Unblock context, current action id, adapter-tagged Worker and Reviewer sessions, implementation ref, completion refs, and a transition-relevant stop. Every role receives the complete Reconciled contract; phase requirement mappings are scope and ordering guidance. Action paths in state are relative to the Build directory. History is retained, but transitions use only state.

```text
phase Work ──complete──→ phase Review ──pass──→ next phase Work
     │                       │                         └─last phase→ final Audit
     │                       └─changes_required→ same phase Work
     ├─blocked→ Unblock ─retry→ same gate / checkpoint
     │                       └─external_requirement→ stopped; explicit relaunch
     └─malformed/provider/Git failure→ reset_required; explicit reset

final Audit ──pass──→ complete
          ├─changes_required──→ final Work → Audit
          └─unknown/missing coverage──→ Unblock
```

Review and Audit use detached disposable worktrees at the exact checkpoint. Work succeeds only when its reported commit exists, descends from the prior checkpoint, equals product `HEAD`, and leaves a clean tracked/untracked checkout. Review must return the inspected checkpoint and leave its worktree clean. Audit associates one exact submitted implementation artifact with its assessment; the existing Audit contract derives `PASS`, `CHANGES_REQUIRED`, or `BLOCKED`.

An explicit blocked result permits one Unblock detour in a disposable source checkout. A Work block is reset automatically because the provider completed and rejected its partial work. `retry` restores the checkpoint and requeues the original gate with originating context, original feedback, and Unblock guidance. A second block stops; no recursive Unblock is available. `external_requirement` is a clean stop. Explicit continuation preserves a clean operator repair, routes a descendant commit through Review or Audit, and starts a new attempt with one available Unblock detour. Provider failure, malformed output, Git invariant failure, or restart during `running` requires destructive `build reset`, which removes the current disposable worktree, runs `git reset --hard <checkpoint>` and `git clean -fd`, preserves ignored files, clears sessions, and requeues the same gate.

Every launch records the selected native adapter config and exact argv before atomically marking state `running`. The adapters handle executable selection, native argv ordering, cwd/prompt delivery, streaming raw transport and stderr to ordinary action files, final-response/session extraction, and process exit. Rust persists parsed `result.json`, `report.md`, and Audit `assessment.json`. There are no provider callbacks, heartbeat supervision, recovery ladders, or inferred continuation sessions.

## Artifacts and storage

Adoption binds one exact Reconciled artifact. Implementation records the Build start and target commits/trees and names Adoption and Reconciled ancestry. Audit publishes immutable assessment plus derived verdict. Bundle manifests hash payloads and record parent refs; the journal is diagnostic, while artifacts and Build state are authoritative. Store format and artifact schema version 6 remain independent of the breaking Build-local schemas.

The CLI retains `build guide`, `prepare`, `scaffold`, and `status`; adds `reset`; and removes resume, preflight, cleanup, export, resolve, and authority-amendment machinery. Optional OnceOver and evidence-output protocols are removed.
