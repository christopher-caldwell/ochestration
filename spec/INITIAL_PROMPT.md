You are implementing **Orchestrate v0.1** as a new Rust application.

Your job is to take the supplied specification package from specification to a finished, tested first delivery.

Do not treat this as a design exercise, prototype, partial scaffold, or migration of the old Python tools. Continue through implementation, verification, packaging, and qualification until the first-version scope defined by the specification is complete, or until a genuine external blocker prevents a required test from being executed.

# Governing product model

This is the north star:

```text
Discovery
   │
   │ finalized public artifact
   ▼
Consensus
   │
   │ Agreement artifact
   ▼
Build
   │
   │ exact implementation
   ▼
Audit
```

Every architecture choice, abstraction, persistence mechanism, CLI command, model call, skill, and test exists to make that chain reliable.

Do not allow implementation machinery to become the product.

A successful implementation must make it possible to establish, mechanically and inspectably:

1. These are the exact Discovery opinions that were produced against the exact selected request and committed source baseline.
2. This Consensus result accurately represents their coherent agreement without inventing majority support or silently adding minority scope.
3. The user adopted these exact Agreement bytes.
4. This is the exact implementation registered against that Agreement.
5. This Audit assessed that exact Agreement/implementation pair using attributable evidence.
6. The complete chain remains reconstructable after process restart without using chat history as authority.

# Specification package

Locate and read the complete contents of these files before designing implementation details:

1. `ORCHESTRATE-SPEC.md`
2. `VERIFICATION.md`
3. `TECHNICAL-GUIDANCE.md`
4. `requirements.json`

Read `ORCHESTRATE-SPEC.md` completely first.

It is the **authoritative product specification**.

`VERIFICATION.md` expands its required acceptance cases and traceability. It does not create new product behavior.

`TECHNICAL-GUIDANCE.md` is advisory. Follow it where it is useful, but you may choose a simpler or stronger implementation when it satisfies the product requirements better.

`requirements.json` is an index/checklist derived from the product specification. It is not an independent authority and must not override the prose specification.

If these documents conflict, use this precedence:

```text
ORCHESTRATE-SPEC.md
        ↓
VERIFICATION.md
        ↓
TECHNICAL-GUIDANCE.md
        ↓
requirements.json
```

Do not begin substantive implementation until you understand the whole first-version acceptance contract.

# This is a new product

Do not preserve compatibility with:

* the existing Python Discovery CLI;
* Independent Consensus Audit;
* Taskledger;
* Flow;
* Build Skills;
* their databases;
* their command names;
* their schemas;
* their output bundles;
* their scoring formulas;
* their migration histories;
* their provider abstractions;
* their internal data models.

Those systems are references documenting lessons already incorporated into the specification.

You may inspect them when a specification requirement benefits from seeing how an earlier problem was solved or how a failure occurred.

Never treat their implementation as normative merely because it already exists.

Prefer the requirements in the Orchestrate specification over copying prior machinery.

# Product boundaries

Implement one Rust product, one repository, one distributed executable, and explicit phase boundaries.

The canonical executable is:

```text
orchestrate
```

The first delivery includes:

* Discovery;
* Consensus;
* Agreement candidate publication;
* explicit Agreement adoption;
* external implementation registration;
* Audit;
* inspection/status/lineage;
* external orchestration storage;
* skills/guides;
* integrity and recovery behavior;
* scripted machinery tests;
* actual packaged CLI tests;
* required native macOS qualification;
* required live-host and semantic qualification where the configured providers are available.

The first delivery does **not** include an internal Build engine.

Do not implement Taskledger, Flow, a scheduler, task graph, TUI, worker pool, automatic integration system, or speculative Build architecture.

The Build boundary must work through external implementation registration exactly as required by the specification.

Reserve Build as a future phase without advertising an implementation that does not exist.

# External operational storage

Operational state and pipeline artifacts belong outside the target repository.

Use the specification's external-store model, conceptually:

```text
~/.orchestration/
  <project>/
    <effort>/
      discovery/
      consensus/
      build/
      audit/
```

Human labels are not sufficient identity. Distinct repositories or efforts with colliding names must remain distinct.

Do not require `.gitignore` changes.

Do not write Discovery reproductions, scratch data, orchestration records, Audit evidence, or run state into the target repository.

Do not modify the target repository's Git configuration or administration merely to support Orchestrate.

The target repository is evidence/source material, not Orchestrate's database.

# Source baseline requirements

For one comparison cohort:

1. Resolve the committed source baseline once.
2. Bind the cohort to that exact baseline.
3. Prepare each Discovery investigator from the committed material represented by that baseline.
4. Never copy a dirty working directory and call it the committed baseline.
5. Never clean, reset, overwrite, or discard the user's uncommitted work.
6. Do not re-resolve `HEAD` independently for each Discovery slot.
7. Consensus must reject or clearly disqualify supposedly equivalent opinions that are actually bound to incompatible request/source baselines.

A Discovery experiment or reproduction must run in run-owned disposable material outside the target repository.

The source repository must not be contaminated by an earlier investigation in a way that later Discovery runs can observe as project source.

# Independence requirements

One Discovery run represents one investigator-owned opinion.

Do not build a nested Discovery committee or Discovery-local consensus system.

The intended workflow is:

```text
Discovery A ─┐
Discovery B ─┼── Consensus
Discovery C ─┘
```

Do not confuse external storage with access isolation.

At minimum, Orchestrate must enforce **supplied-context exclusion**: sibling Discovery outputs are not injected, copied, retrieved through Orchestrate, or included in the prepared source/context of another investigator.

If the actual host cannot enforce filesystem-level exclusion from sibling records, record that limitation honestly. Do not claim access-enforced independence when only cooperative isolation was available.

A contaminated run is retained historically but cannot silently count as a clean independent opinion.

# Phase boundaries are public-artifact boundaries

Downstream phases consume finalized public artifacts, not upstream private working state.

Enforce the dependency direction in the Rust design.

Conceptually:

```text
Discovery internals
        │
        ▼
Discovery public artifact
        │
        ▼
Consensus
        │
        ▼
Agreement artifact
       / \
      /   \
 Build     Audit
 boundary
```

Consensus must not depend on Discovery's private database/runtime.

Audit must not depend on Consensus's private runtime or a future Build implementation.

A downstream consumer must continue to function if the upstream working database/private state disappears but its valid finalized public artifact remains.

Use Rust crate/API boundaries where useful, but also test dependency declarations because a crate boundary alone does not prevent someone from editing Cargo dependencies.

Share mechanical infrastructure only when it is genuinely common.

Good shared candidates include:

* identities;
* hashing;
* canonical generated-record encoding;
* public artifact publication;
* path handling;
* bounded process execution;
* provider transport primitives.

Do not create a universal semantic graph or common semantic lifecycle merely because Discovery, Consensus, and Audit all contain things that can be called "claims" or "evidence."

# Discovery responsibilities

Discovery must investigate a questionable engineering request against the selected source and produce a substantive finalized opinion.

Preserve the important behavioral safeguards from the specification:

* requested behavior and observed behavior are not automatically the same;
* ticket assertions are not source truth;
* facts, interpretations, proposals, and observations remain distinguishable;
* missing product authority may block;
* delegated engineering choices normally become research/recommendations rather than unnecessary user questions;
* assumptions are visible and scoped;
* contrary evidence matters;
* dependent conclusions become stale when their support changes;
* research can legitimately conclude "no change is required";
* adopted behavior has acceptance/verification criteria;
* meaningful unresolved limitations remain visible;
* the recommendation receives substantive adversarial challenge before finalization.

Do not recreate the old Discovery bookkeeping protocol merely because it existed.

The model should spend its effort investigating the engineering problem, not operating an audit database.

The machinery must nevertheless reject a finalization that lacks required current support or still contains a blocking product decision.

The public Discovery artifact must be useful to a human and to Consensus without requiring access to private run internals.

Record actual run provenance, including where available:

* host/source;
* actual model;
* actual reasoning/effort setting;
* Orchestrate version;
* guide/version identity;
* request/context identity;
* source baseline;
* timestamps;
* run identity.

If actual model or effort metadata is unavailable, record it as unknown. Do not promote a declared value into an observed fact.

# Consensus responsibilities

Consensus receives the three selected finalized Discovery opinions plus governing request/context.

It does **not** perform another investigation of the source repository.

It must semantically compare the opinions and propose one coherent Agreement candidate.

Preserve these critical rules:

* one logical Discovery run is one opinion;
* aliases/copies are not extra votes;
* silence is not disagreement;
* related recommendations are not automatically identical recommendations;
* conditions are part of the recommendation;
* explicit opposition remains opposition;
* user/governing requirements cannot be voted away;
* minority-only additions remain non-normative;
* rejected alternatives cannot enter mandatory scope through prose;
* separate 2/3 majorities do not establish 2/3 support for their union;
* the mandatory package must have one common supporting majority;
* a concrete objection capable of invalidating an essential premise cannot simply be outvoted;
* useful coherent 2–1 agreement may be actionable;
* an empty unanimous generality is not preferable to a supported useful answer merely because it scores better;
* a genuine lack of coherent agreement is a valid result and must not be disguised as an execution failure.

For a build-ready Agreement candidate, all three intended Discovery slots must be eligible under the first-version specification.

The model proposes semantic positions and the candidate package.

The machinery verifies structurally checkable properties such as:

* opinion identities;
* distinct eligible slots;
* parent/baseline compatibility;
* supporter counts;
* whole-package supporter intersection;
* valid references;
* required governing coverage;
* absence of normative material that is not represented in validated adopted records.

Render normative Agreement content from validated adopted records.

Do not allow unchecked free-form prose to silently impose additional Build requirements.

Do not inherit the prototype's numerical confidence scoring system.

Report support, disagreement, conditions, and descriptive scoped confidence as required by the specification.

# Agreement adoption

Consensus finalization and user adoption are separate.

A model-generated Agreement cannot authorize itself.

Build authority begins only after an immutable adoption record identifies the exact eligible Agreement revision/digest and the attributable authorization.

Do not implement a force flag that:

* bypasses missing participation;
* converts minority scope into authority;
* resolves missing product meaning;
* changes the finalized Agreement in place.

A user-requested contract change produces a new upstream contract/revision.

# Build boundary for v0.1

Do not create an internal builder.

Implement external implementation registration.

The registered implementation must bind:

* the exact adopted Agreement identity/digest;
* the selected source baseline;
* the exact resulting implementation commit/tree/material identity;
* producer/process declaration;
* status;
* attributable checks/evidence;
* known deviations;
* unresolved questions.

The system must distinguish an implementation producer saying "tests passed" from a check Orchestrate actually observed.

A no-change implementation may legitimately identify the original source baseline as the implementation target.

An implementation registration is not a conformance decision.

# Audit responsibilities

Audit evaluates one exact adopted Agreement and one exact registered implementation.

Freeze those identities before assessment.

The assessor must be fresh from the builder.

Audit must provide an honest disposition for every required Agreement behavior.

Distinguish at least:

* demonstrated conformance;
* demonstrated violation;
* missing/unavailable verification;
* Agreement/authority defect;
* non-applicable where the contract supports that state.

One demonstrated required-behavior violation prevents PASS.

Lack of evidence is not automatically evidence of a defect.

A model preference, architecture taste, speculative hardening opportunity, or unrequested enhancement is not a required finding.

A required finding must identify:

* the Agreement requirement;
* concrete supporting evidence;
* consequence;
* bounded correction.

Audit does not modify code.

Audit does not rewrite the Agreement.

Audit does not automatically start another phase.

A corrected implementation is registered as another exact target and receives another explicitly requested Audit.

# Evidence execution

Implement the bounded check-runner behavior required by the specification.

The auditor should not have to pretend it ran tests it could not run.

Checks must execute only within approved, disposable, run-owned resources and against the exact target being assessed.

Receipts must preserve enough information to know what actually occurred.

Do not infer success from:

* reassuring stdout;
* exit code zero alone;
* a worker/model statement;
* a test command that actually ran zero tests.

Classify infrastructure failure separately from behavioral test failure.

Never execute arbitrary destructive commands merely because model prose suggested them.

# Publication and recovery

Mutable working state and finalized public authority are different.

Design finalization so consumers can distinguish:

* running;
* failed;
* interrupted/unknown;
* finalized.

A final-looking file existing is not sufficient evidence of finalization.

Public artifacts are immutable after successful finalization.

Later invalidation/revision creates another record; it does not rewrite historical bytes.

Test interruption at the meaningful publication boundaries.

A restart must never:

* fabricate success;
* turn a half-written bundle into authority;
* duplicate a Discovery vote;
* silently repeat an uncertain external effect;
* replace a missing parent with another convenient artifact.

The first version must make honest process-crash guarantees. Do not overclaim power-loss durability unless you implement and qualify it separately.

# CLI

Design and freeze a coherent human-facing CLI contract before depending on it throughout the implementation.

Required product capabilities include:

* project/effort/cohort setup or selection;
* Discovery;
* Consensus;
* Agreement adoption;
* external implementation registration;
* Audit;
* status;
* inspect;
* lineage;
* guide;
* safe skill installation/update behavior.

The exact subverbs and flags are yours to design unless specified.

Do not require humans to compose raw JSON for normal use.

Machine-readable output must distinguish:

* invocation/operational success;
* semantic result;
* authorization/adoption state.

For example, successfully producing `CHANGES_REQUIRED` is not a crashed command.

Unknown phase/input must fail explicitly. The CLI must never guess the user's phase.

Read-only operations must not require a model/provider login.

# Skills and guides

Ship explicit thin phase skills for the implemented phases:

```text
orchestrate-discovery
orchestrate-consensus
orchestrate-audit
```

Do not advertise a functioning Build skill until an internal Build phase exists.

Each skill maps deterministically to:

```text
orchestrate guide discovery
orchestrate guide consensus
orchestrate guide audit
```

The wrapper must not inspect the user's wording to guess a phase.

Maintain one canonical semantic body where possible while allowing genuinely required host-specific metadata.

Operator guides and internal managed-role prompts are different concerns.

The Consensus/Audit operator skill should not dump the reconciler/assessor's internal semantic prompt into the parent model and ask it to reproduce the pipeline.

The installed executable and packaged guides must remain version-compatible and work without access to the source checkout.

# Provider implementation

Implement only the provider capability required by the specification.

Do not build a general provider framework.

The initial managed provider adapter must have a narrow responsibility:

* launch the configured role;
* enforce available capability restrictions;
* bound execution;
* parse protocol output;
* retain diagnostics;
* report actual observed identity/usage where available;
* return typed success/failure/unknown.

It does not decide semantic correctness.

Do not silently switch provider/model/effort when the requested configuration is unavailable.

Do not invent token/cost values.

A provider process exiting zero does not by itself mean the semantic operation succeeded.

# Rust architecture

Use Rust because it helps enforce the new product boundaries, not because the product requires maximum abstraction.

Use a Cargo workspace where it provides clear boundaries.

Do not create one crate for every noun or layer.

Prefer vertical, behavior-focused modules within phase crates.

Establish mechanically testable dependency policy so forbidden cross-phase private dependencies fail even if someone later makes a symbol public or adds an alias/feature/re-export.

Before implementing downstream consumers, define and test the relevant public artifact schemas and their versioning/integrity rules.

Do not rely only on round-trip tests for serialization/hashing. Use independently calculated fixed vectors to catch common-mode encoding bugs.

# Implementation order

Do not attempt to implement every phase superficially in parallel.

Use vertical milestones that demonstrate real product behavior.

Recommended order:

## Milestone 1 — Foundation

Implement:

* repository/project identity;
* effort/request identity;
* context revision;
* cohort identity;
* external store;
* exact committed source baseline capture/materialization;
* safe run creation;
* public artifact envelope;
* digest/version/reference semantics;
* finalization/publication boundary;
* inspect/status/lineage primitives;
* crash/restart handling for these mechanics.

Before proceeding, prove:

* no target-repo pollution;
* dirty user work remains untouched;
* same-basename repositories do not collide;
* source baseline is frozen once;
* wrong parent/baseline is rejected;
* partial publication is not accepted;
* restart preserves honest state.

## Milestone 2 — Full scripted vertical chain

Implement enough real public operations to exercise:

```text
3 scripted Discovery runs
        ↓
Consensus
        ↓
Agreement candidate
        ↓
Adoption
        ↓
External implementation registration
        ↓
Audit
```

Do this using scripted semantic boundaries first.

Use the real:

* CLI;
* Git handling;
* filesystem;
* persistence;
* public artifact parser;
* finalizer;
* integrity verification;
* process boundaries where applicable.

The scripted helper must not write final authoritative artifacts directly.

Get both of these journeys passing:

1. known-good implementation → PASS;
2. known-bad implementation → CHANGES_REQUIRED → corrected target → PASS.

Also implement the no-change journey.

## Milestone 3 — Provider and phase skills

Add:

* actual managed provider adapter;
* packaged guides;
* phase skills;
* installer/update behavior;
* protocol-level provider emulator tests;
* bounded live low-strength host smoke where available.

## Milestone 4 — Semantic qualification

Run the semantic fixtures required by `VERIFICATION.md` using the actual intended phase instructions and selected profiles.

Include:

* false ticket premise;
* justified no-change;
* missing product authority;
* conditions;
* equivalent language;
* coherent 2–1 result;
* rotating majorities;
* minority-scope insertion;
* concrete minority counterexample;
* missing verification;
* known implementation violation;
* fixed implementation;
* Agreement defect.

Do not rerun failing cases until they happen to pass.

Retain every qualification trial.

# Test strategy is part of the implementation

The test suite is not cleanup after coding.

Build it alongside the functionality.

Use the four test layers from `VERIFICATION.md`:

```text
D — deterministic rule/fixed-vector/generated-state tests
I — real CLI/filesystem/Git/process integration tests
H — installed-host live smoke
S — semantic regression evaluation
```

A test mapped to a requirement does not satisfy that requirement until the test has actually executed at the required boundary.

For every consequential guarantee, implement both positive and adversarial cases.

The test suite must catch deliberate defects such as:

* skipping artifact verification;
* accepting an unpublished draft;
* re-resolving moving HEAD;
* including peer Discovery output;
* treating silence as support;
* using supporter union instead of whole-package intersection;
* allowing untracked normative prose;
* omitting an Audit coverage obligation;
* accepting changed Agreement bytes under an old adoption;
* relabeling a receipt;
* publishing authority before its payload is committed;
* deleting an unowned path during cleanup.

Do not optimize for line coverage percentages.

Optimize for proving the behavioral guarantees.

# Required ordinary Rust checks

Establish normal repository commands and keep them green.

At minimum, unless a technically justified exception is documented:

```bash
cargo fmt --check
cargo check --workspace --all-targets
cargo test --workspace --all-targets
cargo clippy --workspace --all-targets -- -D warnings
cargo build --release
```

Add feature flags to these commands if your actual workspace design requires them.

Do not weaken warnings/tests simply to obtain a green build.

Also run the packaged executable from outside the source checkout.

# Machinery qualification

Run the complete mandatory deterministic/integration case set from `VERIFICATION.md`.

Do not report the tool complete because unit tests passed.

Exercise:

* actual CLI binary;
* actual filesystem;
* actual Git repositories;
* actual subprocess/parser boundary;
* actual persistence;
* actual finalization;
* actual restart;
* actual source snapshot;
* actual installer/package resources.

Use a fresh disposable test root for every trial.

Offline tests must not use real model credentials or external network.

Test helpers may control semantic outputs but may not bypass production validators or publishers.

Use independent expected values and examiner-only fixtures outside model-visible material.

# Live-model permission

For this implementation effort, you are authorized to run **bounded, low-strength live-model tests** needed for installed-host smoke and small E2E qualification when the relevant local provider/host is already configured and available.

Requirements:

* use finite predeclared attempts;
* use small synthetic fixtures;
* record actual provider/model/effort where observable;
* do not repeatedly rerun until a preferred answer appears;
* do not silently substitute a stronger/different model;
* do not expose unrelated local files or secrets;
* retain failures honestly.

Low-strength success qualifies the host/integration path only.

For semantic-quality requirements, also run the profiles actually intended for real Discovery/Consensus/Audit work when available, as specified by `VERIFICATION.md`.

If a required live provider or host surface is unavailable, complete all machinery work possible and report that qualification lane as `BLOCKED` or `NOT EXECUTED`. Do not mark it passed.

# macOS qualification

The first supported native environment is macOS.

Run the native path, process, cancellation, filesystem, install, and recovery cases required by the specification.

Do not infer macOS qualification from Linux CI or successful Rust compilation.

Specifically exercise real path canonicalization behavior and avoid assumptions like `/var` and `/private/var` being distinct logical roots when the platform says otherwise.

Do not claim Linux or Windows support unless those environments are actually qualified.

# Work discipline

Do not stop after:

* creating Cargo.toml;
* establishing crates;
* writing interfaces;
* creating TODOs;
* implementing only the happy path;
* writing tests without running them;
* producing a plan.

Continue through the first-version definition of done.

Make ordinary technical decisions yourself using the product specification and repository evidence.

Ask the user only when a missing product decision would materially change required behavior and the specification does not already settle it.

Do not ask the user to choose:

* crate names;
* storage libraries;
* serialization libraries;
* error types;
* internal folder layouts;
* routine implementation patterns.

Those are implementation decisions.

Prefer the simplest mechanism that genuinely enforces the requirement.

Do not add architecture merely because it might be useful later.

# Handling specification ambiguity

When you believe the specification is ambiguous:

1. reread the complete relevant sections;
2. check the mapped acceptance cases in `VERIFICATION.md`;
3. check `TECHNICAL-GUIDANCE.md` for nonbinding interpretation;
4. choose the narrowest implementation consistent with the north star when the choice is ordinary technical latitude;
5. ask only if two materially different product behaviors remain possible.

Never use an ambiguity as permission to silently inherit behavior from an old Python tool.

# Keep requirement traceability live

Use `requirements.json` and `VERIFICATION.md` as an implementation/qualification checklist.

For every ORC requirement, be able to identify:

* where it is enforced;
* the relevant public behavior;
* the positive test;
* the adversarial/negative test;
* its execution status.

Do not create a second competing product specification.

A small implementation-status/qualification artifact is acceptable if it is generated or maintained as execution evidence rather than redefining requirements.

# Completion criteria

Do not declare implementation complete until the first-version Definition of Done in `ORCHESTRATE-SPEC.md` is actually demonstrated.

At minimum:

* all core first-version requirements are implemented;
* all mandatory deterministic and integration machinery cases have executed and pass;
* deliberate guard mutations are caught by tests;
* the complete scripted valid chain passes;
* the deliberately faulty implementation is rejected;
* its corrected implementation can pass;
* the no-change chain can pass;
* restart/partial-publication tests pass;
* source contamination/baseline substitution tests pass;
* packaged CLI works outside the checkout;
* phase skills/guides are packaged correctly;
* native macOS required cases have actually run;
* claimed host surfaces have live smoke evidence;
* required semantic qualification has actually run on the declared profiles where available;
* Build remains explicitly external/deferred rather than half-implemented;
* no planned/skipped test is represented as passed.

If an external qualification dependency is genuinely unavailable, distinguish:

```text
IMPLEMENTATION COMPLETE
QUALIFICATION BLOCKED: <specific unavailable dependency>
```

from:

```text
FULLY QUALIFIED
```

Do not collapse those states.

# Final adversarial pass

Before finishing, perform a dedicated adversarial review of the finished implementation against the product specification.

Attack at least:

* peer Discovery contamination;
* wrong source baseline;
* wrong request/context revision;
* duplicate opinion aliases;
* rotating majorities;
* minority-only scope insertion;
* condition loss;
* user requirement loss;
* stale adoption;
* moving implementation target;
* forged/relabeled evidence;
* missing Audit coverage;
* provider failure/retry behavior;
* partial artifact publication;
* process interruption;
* same-run concurrent mutation;
* cross-effort collision;
* unsafe cleanup;
* installed-resource drift;
* forbidden crate dependencies.

Any material defect found during this pass must be corrected and the affected qualification rerun.

Do not treat the adversarial review itself as evidence that everything is correct. The executable tests remain authoritative for mechanically testable guarantees.

# Final report

When the work is complete, report concisely but specifically:

## Product delivered

State what now works through the four-stage chain.

## Architecture

Describe the actual crate/module boundaries and why they enforce the intended artifact boundaries.

## Public CLI

List the implemented human-facing commands and phase skills.

## Verification

Report separately:

* deterministic machinery tests;
* real CLI/integration/E2E tests;
* installed-host live smoke;
* semantic qualification;
* native macOS qualification.

Include exact commands and pass/fail counts.

Do not merge these into one score.

## Requirement coverage

State whether all core ORC requirements are implemented and tested. List any exceptions by requirement ID.

## Known limits

Explicitly state:

* Build remains external/deferred;
* any host/provider not qualified;
* any isolation mode that is cooperative rather than access-enforced;
* any platform not tested;
* any remaining blocked qualification.

## Evidence

Identify the retained qualification report/run artifacts needed to independently inspect the result.

Do not report "done" based solely on compilation, a successful demo, or a high test count.

The governing question at the end remains:

```text
Discovery
   │ finalized public artifact
   ▼
Consensus
   │ Agreement artifact
   ▼
Build
   │ exact implementation
   ▼
Audit
```

Can the delivered tool demonstrate that exact chain faithfully, mechanically, and inspectably?

If yes, finish.

If not, keep working on the requirement that prevents it.
