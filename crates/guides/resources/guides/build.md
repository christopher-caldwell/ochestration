# Build

An explicit Build invocation is authorization to build one exact implementation-ready Reconciled Discovery now. Do not ask for another approval or confirmation. Build creates or reuses the exact Adoption receipt at this boundary.

Build requires the exact effort, the exact implementation-ready Reconciled Discovery, a detailed implementation plan, and the target repository. When the user supplied a Reconciled Discovery explicitly, bind that artifact; do not infer a different one.

Resolve the effort and Build directory:

```sh
orchestrate --root "<root>" status --effort "<effort>"
```

Read the rich Reconciled Discovery artifact as Build's authoritative contract. **Reconciled Discovery is binding WHAT; the detailed implementation plan is HOW and ordering.** The plan may describe approach, task grouping, ordering, and delivery phases only. It must not add or remove product requirements, weaken acceptance criteria, or contradict the Reconciled Discovery. If the plan materially conflicts with binding authority, stop before implementation and surface the conflict. Do not silently rewrite the plan or improvise around it.

The repository may have advanced since Discovery. Discovery remains frozen to its recorded baseline, while implementation must respect the current code and preserve the reconciled contract. Build is allowed when that Discovery baseline is an ancestor of current `HEAD`; it need not equal current `HEAD`.

A new Build requires a clean Git-visible product checkout before the worker takes ownership. Commit, move, or remove staged changes, unstaged changes, and ordinary untracked files before starting. The recorded Build starting commit/tree is the worker's starting boundary. Git-ignored local environment files are allowed. This check applies when Build state is first created, not when an active Build resumes.

## First preparation

Copy the detailed implementation plan into the Build directory, then materialize templates:

```sh
orchestrate --root "<root>" build scaffold --effort "<effort>"
```

This writes `plan.json` and `config.toml` and refuses to overwrite existing files. Fill `plan.json` with schema version 2, the exact Reconciled Discovery reference (`kind`, `artifact_id`, and `digest`), the relative detailed-plan path, and the ordered delivery-phase grouping. Each task belongs to one phase; task IDs do not create extra stops.

Configure worker and independent reviewer adapters separately in `config.toml`. Supported adapters are `codex`, `claude`, and `cursor`. Host configuration must already allow intended edits and checks; do not put credentials, command strings, or provider homes in this file.

## Existing prepared or active Build

If Build files or state already exist for this exact effort, do not scaffold again and do not replace the detailed plan, `plan.json`, or `config.toml`. Verify the existing exact Build authority and invoke or resume the driver. If the current invocation supplies a Reconciled Discovery or detailed plan that differs from the existing Build's frozen inputs, stop and report the mismatch; do not resume under different authority and do not replace the frozen files. Never delete or recreate Build state merely to satisfy this guide.

From the target repository, invoke the driver:

```sh
orchestrate --root "<root>" build --effort "<effort>"
```

The driver binds the exact Reconciled Discovery, creates or reuses Adoption, records current Build starting commit/tree separately from the Discovery baseline, verifies ancestry, and handles work, review, correction, implementation registration, and final Audit until completion or until it cannot safely continue automatically. An external user or infrastructure requirement is one common blocker. Do not perform those role turns yourself.

## A stopped Build

When the driver stops, it prints a durable `trigger`, the exact `stopped_action`, and the state path; the same facts are in Build state and the journal. Never hand-edit `state.json`, the action, or a receipt to move a stopped Build forward.

Resolve the stop with the exact stopped action:

```sh
orchestrate --root "<root>" build resolve --effort "<effort>" --action "<stopped_action>" --kind "<kind>" --note "<what changed>"
```

`--kind` names the intervention: `existing_authority_clarification` (the role already had authority), `environment_repair` (access, tooling or environment was repaired), `new_verification_evidence` (a final-scope Audit derived BLOCKED and new evidence is available), or `authority_change` (a proposed change to the adopted contract, which Build refuses and records with successor guidance). Add `--evidence` for each verification artifact you supply. Add `--confirm-not-running` only when you have confirmed that an accepted-but-uncertain provider action is no longer running; without that recorded confirmation Build will not create a fresh continuation for it. `--config` supplies new role configuration for future invocations and is accepted only with `environment_repair`; the original `config.toml` and its frozen digest are preserved, and prior records keep the settings they ran under.

Resolution records the intervention and authorizes one distinct continuation. It dispatches no provider action itself: run `orchestrate build` again to continue. An authority amendment must go through new Discovery and Reconcile and an explicitly linked successor Build.
