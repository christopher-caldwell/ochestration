---
artifact: product-specification
product: Orchestrate
specification_id: orchestrate-product-v0.1
version: "0.1"
date: "2026-09-17"
status: consolidated-proposed-specification
implementation_language: Rust
implementation_qualification: not-executed
---

# Orchestrate
## Product specification and acceptance contract

**Purpose:** one local engineering workflow that turns independently investigated requirements into a coherent Agreement, an exact implementation, and an evidence-backed conformance assessment.

**Status:** This is a consolidated specification for a **new** tool. It is not a report that the tool exists or that any application test has passed. User decisions are preserved below. Where earlier discussion left policy open, this draft makes explicit first-version choices; those choices are identified rather than attributed retroactively to the user.

---

## Contents

[1. The north star](#section-1)  
[2. Decisions, scope, and document authority](#section-2)  
[3. What the first useful delivery includes](#section-3)  
[4. Product model](#section-4)  
[5. Ownership and enforcement](#section-5)  
[6. Required behavior and acceptance criteria](#section-6)  
[7. Public artifact contents and handoff rules](#section-7)  
[8. Lifecycle, readiness, and failure behavior](#section-8)  
[9. Consensus decision examples](#section-9)  
[10. CLI and installed skill experience](#section-10)  
[11. Verification and acceptance of the delivered tool](#section-11)  
[12. What does not carry forward from the reference tools](#section-12)  
[13. Technical freedom and boundaries still deliberately open](#section-13)  
[14. Definition of done](#section-14)  
[15. Source and decision register](#section-15)  

<a id="section-1"></a>

# 1. The north star

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

This chain is the product. The CLI, skills, manifests, crates, checks, and test harness exist to make this chain useful and trustworthy. They are not separate product goals.

**Discovery** answers: “What should be built, and what supports that recommendation?” It must investigate the request against the actual selected source, correct false premises, expose real missing decisions, and deliver a specification someone can use.

**Consensus** answers: “What coherent implementation direction do these independent investigations genuinely agree on?” It must preserve scope and conditions, distinguish silence from disagreement, avoid combining attractive minority ideas into invented consensus, and produce one Agreement.

**Build** answers: “What exact implementation was produced against that exact Agreement?” Its internal strategy is intentionally deferred. The boundary must be usable with an external human or tool before Orchestrate has its own builder.

**Audit** answers: “Does this exact implementation satisfy this exact Agreement, and what evidence supports that answer?” It must identify proven deviations, missing verification, and contract defects without becoming an unbounded redesign exercise.

A useful success is not a folder full of valid JSON. A useful success is an actionable opinion, an accurately synthesized and adopted Agreement, a precisely identified implementation, and a conformance result the user can inspect without reconstructing prior chats.

The tool also succeeds by stopping honestly when the chain cannot support the next claim. But a tool that rejects everything is not correct: solvable requests and correct implementations must be able to finish.

## 1.1 What must never be confused

| These are different | Why the distinction matters |
|---|---|
| A final-looking file and a finalized public artifact | Incomplete or uncommitted output cannot authorize another phase. |
| Three files and three independent opinions | Copies, sessions, and exports do not create votes. |
| Agreement and factual truth | Independent models can still share a mistaken premise. |
| Agreement candidate and user adoption | Producing a recommendation does not authorize building it. |
| A recorded implementation and a conforming implementation | The worker’s claim remains subject to independent Audit. |
| A completed command and a successful engineering outcome | Valid comparison can find no consensus; a failed model call yields no engineering verdict. |
| External storage and access isolation | Moving files out of the repo does not stop an unrestricted host reading them. |
| A compiler boundary and a runtime permission boundary | Rust visibility cannot confine a filesystem-capable agent. |

<a id="section-2"></a>

# 2. Decisions, scope, and document authority

## 2.1 Confirmed foundations from the conversation

Orchestrate is one product, repository, and CLI, implemented fresh in Rust. There is no obligation to port old code, preserve old behavior, read old databases, maintain old commands, or make old tests pass unchanged. Discovery, Independent Consensus Audit, Build Skills, Flow, and Taskledger are evidence of lessons learned, not specifications for the new product.

The user chooses phases explicitly. Several thin phase skills are preferred to a generic skill that guesses a phase. Each loads instructions from the installed `orchestrate guide <phase>` command. Canonical semantic instructions are maintained once; host packaging may differ.

Pipeline artifacts live outside the target repository, under `~/.orchestration/<project>/<request-slug>/<phase>/<run-id>/`. Investigations must not contaminate the source repository with reproductions or orchestration files. Each run identifies its host/source, model, and effort setting where known. The default investigation baseline is the latest local committed source, frozen once for the cohort. Parallel model runs are desirable but not necessary for initial usefulness.

The Build implementation remains undecided. Real machinery tests are required, including end-to-end testing with low-strength live models where appropriate.

These decisions are reflected in the earlier reference disposition review and testing design, which explicitly distinguish user requirements from proposed simplifications. [D1, D2]

## 2.2 Explicit resolutions in this draft

These resolve gaps in prior proposals. They are **specification choices proposed here**, not assertions that the user separately approved each one.

| Topic | First-version resolution | Reason |
|---|---|---|
| Comparison population | Three fixed Discovery slots; all three required for an eligible Agreement. Partial comparisons remain useful but not build-ready. | Preserve the intended three-investigation workflow without silently weakening its denominator. |
| Discovery inside one run | One investigator-owned opinion, with substantive self-challenge, not a nested committee. | Consensus already owns reconciliation between independent investigations. |
| Adoption | A separate exact-reference adoption receipt before Build authority; it can be captured with one deliberate downstream action. | Prevent model output from authorizing itself without adding repeated approval ceremony. |
| Confidence | Scoped descriptive confidence and exact support counts; no inherited 0–100 formula. | Explain support without making a score a gate or implying calibration. |
| Source support | Committed local Git baseline; no automatic dirty-tree, unborn-repo, or old-format fallback. | Preserve source identity and the user’s work. |
| Isolation | Required input exclusion and contamination prevention; stronger access-enforced isolation is a separately qualified capability. | Do not promise confinement a host cannot provide. |
| Managed retries | One attempt by default; a bounded validation-repair allowance can be authorized before a run. | Keep failures visible and prevent hidden reruns for preferred answers. |
| Audit execution | Bounded checks through a permitted runner in disposable scratch; not a universal no-tools auditor. | Runtime claims need an available path to real evidence. |
| Initial delivery | Discovery, Consensus, adoption, external implementation registration, Audit, skills, inspection, and their tests. | Prove the pipeline without prematurely choosing a builder. |
| Platforms | Native macOS qualification first; additional platforms supported only when actually exercised. | Match the user’s environment while avoiding an untested cross-platform promise. |

## 2.3 Authority within this specification package

This file controls product behavior. `VERIFICATION.md` expands the acceptance tests and traceability; it does not create product scope by itself. `TECHNICAL-GUIDANCE.md` recommends mechanisms; different mechanisms are acceptable when they satisfy this specification and its acceptance criteria. `requirements.json` is a generated index of the requirements in this document, not another independently maintained source of truth.

“SHALL” identifies required behavior **within this draft’s declared scope**. Conditional capabilities become mandatory only when shipped or claimed. Deferred Build execution is not a passed or missing implementation of a first-version requirement; it is outside that delivery profile.

The specification as a whole remains proposed until adopted for the implementation effort. There are no hidden policy-pending requirements in the core: changes to the resolutions above require an explicit specification revision.

<a id="section-3"></a>

# 3. What the first useful delivery includes

The first delivery must support this complete journey:

1. Preserve a request and create its three-slot cohort against one committed source baseline.
2. Operate three independent Discovery runs using the user’s selected hosts, each with its own source copy and run state.
3. Compare the finalized public opinions into an eligible Agreement candidate, or an honest no-consensus/partial result.
4. Record deliberate user adoption of the exact eligible Agreement.
5. Register an externally prepared exact implementation and its attributable evidence against that Agreement.
6. Audit the exact pair; record a useful pass, required changes, or blocked verification/authority, and stop.

The numbered journey describes behavior, not a requirement for six separate human confirmations. Routine actions within an authorized phase are performed by the tool and its operator skill.

**Included:** local persistent records; safe independent run preparation; documented skills for Codex, Claude Code, and Cursor; one initially qualified managed provider adapter; controlled evidence execution; source and artifact integrity; recovery from process interruption; readable reports; mechanical status/lineage; a real test suite.

**Not included:** an autonomous end-to-end agent; a universal workflow engine; hosted/cloud orchestration; multi-user administration; legacy migration; generalized arbitrary-report reconciliation; a task scheduler; multiple automatic build strategies; a mandatory reviewer committee; a dashboard/TUI; numerical confidence calibration; universally enforced OS isolation; automatic source commits, pushes, or merges outside a future specified Build capability.

The number of internal crates, tables, files, model calls, and helper objects is not a success metric. No mechanism is retained solely because a prototype used it.

<a id="section-4"></a>

# 4. Product model

| Term | Meaning |
|---|---|
| Project | An externally registered source-repository identity with human-friendly labels and verified local locators. |
| Effort/request | One intended change or engineering question, such as TIX-1234 or add-group-trips. Multiple efforts may coexist per project. |
| Context revision | Exact request text, attributed corrections, and incorporated governing constraints for a particular comparison. |
| Source baseline | The exact selected committed source and material inventory actually supplied for investigation. |
| Cohort | The request/context, source baseline, and three intended opinion slots being compared. |
| Run/attempt | Attributable execution of one phase; retries/sessions do not create independent opinions. |
| Public artifact | Finalized self-describing output that another phase can consume without private producer state. |
| Opinion | One investigator’s justified, scoped Discovery recommendation. |
| Agreement candidate | A finalized eligible Consensus contract not yet necessarily adopted by the user. |
| Adoption | An immutable record that the user chose one exact eligible Agreement as Build authority. |
| Implementation | An exact committed target and evidence/declaration record associated with an adopted Agreement. |
| Receipt | An attributable record of an observation or execution, with source, environment, and outcome identity. |
| Audit | An independent conformance assessment of one exact Agreement/implementation pair. |

The storage layout is a readable organization of these objects, not their identity system. On a label collision, use a stable disambiguator. No hidden selection of the newest matching folder is permitted.

```text
~/.orchestration/
  <project-label-or-disambiguated-label>/
    <request-slug>/
      <effort/context/cohort records>
      discovery/<run-id>/
        <run state, frozen public output, own scratch/evidence>
      consensus/<run-id>/
        <comparison, Agreement candidate, public manifest>
      build/<run-or-registration-id>/
        <external declaration, exact implementation, evidence>
      audit/<run-id>/
        <coverage, findings, receipts, public report>
```

This is conceptual layout guidance. It does not prescribe every filename or a global database. Adoption and invalidation notices can be separate immutable records beside the relevant runs. The store is not automatically part of any investigator’s visible workspace.

<a id="section-5"></a>

# 5. Ownership and enforcement

| Responsibility | Owner |
|---|---|
| Choose requested phase, approve product decisions and the Agreement | User, with an attributable operator action when delegated |
| Preserve identities, exact bytes, allowed transitions, publication, limits, and lineage | Orchestrate machinery |
| Interpret a request, evaluate evidence relevance, recommend a design | Discovery investigator |
| Determine semantic equivalence, positions, and a coherent selected package | Consensus reconciler |
| Count supporters, reject impossible references, bind the rendered Agreement | Orchestrate machinery |
| Implement within the adopted behavioral contract | External builder initially; later specified Build mechanism |
| Judge whether evidence establishes conformance, a violation, or uncertainty | Fresh Audit assessor |
| Refuse PASS with incomplete coverage, invalid inputs, or supported blocking findings | Orchestrate machinery |

There is no proposed automatic proof engine for arbitrary engineering meaning. The machinery enforces what is structurally checkable and makes judgments attributable, reviewable, and testable. The semantic tests check what structural validation cannot establish.

One crucial limitation: a human or agent with unrestricted control of the machine can manually rewrite files or bypass a CLI. This personal tool is not an authentication or hostile-owner security system. It must accurately enforce its own public operations and never inflate a cooperative host policy into a stronger claim.

<a id="section-6"></a>

# 6. Required behavior and acceptance criteria

Each requirement below specifies the behavior, the relevant enforcement boundary, and observable acceptance. `D` tests are deterministic rules; `I` tests exercise the actual CLI/filesystem/Git/processes; `H` tests exercise installed hosts; `S` tests evaluate live semantic quality. The full case definitions and mappings are in `VERIFICATION.md`.

Every listed application test is initially **NOT EXECUTED**. Cross-reference checks on this document package are not application qualification.


## 6.1. Purpose, scope, and authority

<a id="orc-001"></a>

### ORC-001 — Deliver the four-stage product

Orchestrate SHALL support Discovery producing a finalized public opinion; Consensus producing an Agreement from the selected opinions; an implementation handoff bound to that Agreement; and Audit assessing the exact implementation against it. Every mechanism must serve one of these outcomes or the integrity of their handoffs. Existing Python tools are references, not compatibility targets.

**Enforcement:** Each phase has a typed entry contract and a distinct output artifact. The initial delivery uses an external implementation handoff, not an invented internal builder.

**Acceptance:**
- **ORC-001-A1:** A complete known-good fixture travels through all public boundaries and receives a supported Audit result.
- **ORC-001-A2:** A working phase cannot declare the overall pipeline complete when a required downstream artifact or assessment is absent.

**Verification cases:** [E2E-01](VERIFICATION.md#e2e-01), [E2E-02](VERIFICATION.md#e2e-02), [BLD-06](VERIFICATION.md#bld-06). **Applicability:** core.

<a id="orc-002"></a>

### ORC-002 — Select operations explicitly

The user selects the phase through an explicit CLI operation or a fixed phase skill. Orchestrate SHALL NOT classify free-form intent to select or switch phases. Missing or ambiguous phase, effort, parent, or target selection produces a specific diagnostic before phase work.

**Enforcement:** Command parsing fixes the operation; entry validation checks selected identities, types, and prerequisites. A model cannot satisfy a failed entry check by explaining that another phase is probably intended.

**Acceptance:**
- **ORC-002-A1:** Explicit Consensus loads the Consensus operation and guide, even when the supplied text asks for source edits.
- **ORC-002-A2:** An unknown phase or ambiguous effort starts no model invocation and creates no successful run.

**Verification cases:** [CLI-02](VERIFICATION.md#cli-02), [CLI-04](VERIFICATION.md#cli-04), [CLI-11](VERIFICATION.md#cli-11), [ID-02](VERIFICATION.md#id-02). **Applicability:** core.

<a id="orc-003"></a>

### ORC-003 — Stop at consequential boundaries

Completing Discovery, Consensus, implementation registration, or Audit SHALL NOT start the next phase. Within an explicitly authorized phase, routine evidence capture, checking, and bounded model calls may continue to that phase’s outcome. Readiness is not authorization.

**Enforcement:** Phase completion returns a result and legal next-operation information. No default pipeline loop, automatic repair loop, or model scheduler advances the four-stage workflow.

**Acceptance:**
- **ORC-003-A1:** After each phase finishes and the process is left idle, invocation records show no successor phase.
- **ORC-003-A2:** A failed Audit names the correction destination without modifying code, changing the Agreement, or starting Discovery.

**Verification cases:** [CLI-03](VERIFICATION.md#cli-03), [AUD-09](VERIFICATION.md#aud-09), [E2E-01](VERIFICATION.md#e2e-01). **Applicability:** core.

<a id="orc-004"></a>

### ORC-004 — Preserve user authority above model agreement

Preserved user requirements and explicitly incorporated project constraints govern the effort. Consensus SHALL NOT delete, weaken, or add product requirements by vote. Statements in a ticket about current behavior remain claims to investigate, not unquestionable facts. A missing consequential product decision must be surfaced rather than invented.

**Enforcement:** Keep immutable request/context revisions and a traceable register of governing constraints. Agreement coverage must account for every registered constraint, with source references. Structural completeness is checked mechanically; semantic fidelity to the original wording is evaluated separately.

**Acceptance:**
- **ORC-004-A1:** Three opinions overlooking an explicit requirement cannot produce an adoptable Agreement that silently omits it.
- **ORC-004-A2:** Discovery can refute the ticket’s factual diagnosis without treating that refutation as permission to change the requested outcome.

**Verification cases:** [CON-10](VERIFICATION.md#con-10), [DISC-08](VERIFICATION.md#disc-08), [ACX-01](VERIFICATION.md#acx-01). **Applicability:** core.

<a id="orc-005"></a>

### ORC-005 — Keep the new product independent of the prototypes

Ship one Rust product, repository, release, and executable named orchestrate. No old command alias, database migration, importer, scoring formula, Python runtime, or old skill is required. Persisted Orchestrate artifact formats have explicit versions independent of the product version.

**Enforcement:** The packaged release and its public contracts stand alone. Unknown artifact versions are refused without mutation rather than interpreted as a current format.

**Acceptance:**
- **ORC-005-A1:** A fresh installation operates without any Discovery, Consensus Audit, Flow, Taskledger, or Build Skills installation.
- **ORC-005-A2:** Unsupported artifact schemas produce a clear compatibility diagnostic and leave bytes untouched.

**Verification cases:** [CLI-05](VERIFICATION.md#cli-05), [ART-02](VERIFICATION.md#art-02), [ARCH-05](VERIFICATION.md#arch-05), [BLD-06](VERIFICATION.md#bld-06). **Applicability:** core.

<a id="orc-006"></a>

### ORC-006 — Distinguish completion from correctness and approval

Operation completion, semantic outcome, publication, and authorization SHALL be distinct. A valid no-consensus comparison is a completed operation. A finalized Agreement candidate is not yet permission to build. A recorded implementation is not a passing Audit. A provider failure is not a low-confidence engineering conclusion.

**Enforcement:** Reports expose separate operation status, phase outcome, publication state, and readiness/authorization information. No numerical score or transport exit code implicitly changes those fields.

**Acceptance:**
- **ORC-006-A1:** Inconclusive Consensus, blocked verification, malformed output, and transport failure are visibly different outcomes.
- **ORC-006-A2:** Only the exact authorized Agreement/implementation pair can receive the corresponding conformance result.

**Verification cases:** [META-04](VERIFICATION.md#meta-04), [CON-11](VERIFICATION.md#con-11), [CON-16](VERIFICATION.md#con-16), [BLD-05](VERIFICATION.md#bld-05). **Applicability:** core.

<a id="orc-007"></a>

### ORC-007 — Keep routine operation usable

A human supplies the request, project, chosen phase, selected inputs, and consequential approvals—not bookkeeping UUIDs, relational records, or manually authored JSON. Skills operate the public interface on the user’s behalf. The CLI SHALL provide ordinary help and focused diagnostics; identifiers may be returned for later selection.

**Enforcement:** Allocate run and operation identities in the application. Accept request files or captured text without requiring the human to convert them into an internal schema. Keep machine output separate from readable results.

**Acceptance:**
- **ORC-007-A1:** The normal supported journey can be performed without the user generating UUIDs or editing manifests.
- **ORC-007-A2:** An input error names the missing prerequisite and leaves existing work intact rather than asking the user to repair internal state.

**Verification cases:** [CLI-01](VERIFICATION.md#cli-01), [DISC-01](VERIFICATION.md#disc-01), [DISC-09](VERIFICATION.md#disc-09), [ACX-02](VERIFICATION.md#acx-02). **Applicability:** core.

## 6.2. Project, request, cohort, and source identity

<a id="orc-008"></a>

### ORC-008 — Keep orchestration state outside the target repository

Default storage SHALL be ~/.orchestration/<project-label>/<request-slug>/<phase>/<run-id>/. An explicit alternative root supports testing and local preferences. Discovery, Consensus, Audit, and orchestration setup SHALL NOT write reports, reproductions, configuration, Git exclude entries, or skill installations into the target repository.

**Enforcement:** Resolve all application-owned destinations under the external root. Create source snapshots and scratch there or in explicitly owned external temporary space. Never use the target checkout as scratch or change its Git administration to support investigation.

**Acceptance:**
- **ORC-008-A1:** Tracked files, dirty files, untracked files, .gitignore, Git exclude/configuration, index, and refs retain their pre-existing state across non-Build phases.
- **ORC-008-A2:** An unwritable root produces a failure; no fallback directory is silently created in the repo or another home.

**Verification cases:** [ID-05](VERIFICATION.md#id-05), [ID-06](VERIFICATION.md#id-06), [ID-09](VERIFICATION.md#id-09), [ISO-01](VERIFICATION.md#iso-01). **Applicability:** core.

<a id="orc-009"></a>

### ORC-009 — Separate display labels from identity

Projects, requests, cohorts, runs, and artifacts SHALL have stable identities. A project basename, request slug, host name, or directory path is a human label or locator, not sufficient identity. Two unrelated repositories named server with request TIX-1234 must not collide.

**Enforcement:** Canonicalize supported local aliases, keep an external project binding, and disambiguate label collisions. Renaming/moving a locator requires explicit verified rebinding; neither a remote URL nor matching content alone silently merges projects.

**Acceptance:**
- **ORC-009-A1:** Two same-named repositories create separate, correctly linked efforts.
- **ORC-009-A2:** Supported symlink and macOS path aliases resolve consistently, while a conflicting directory identity is rejected rather than overwritten.

**Verification cases:** [ID-01](VERIFICATION.md#id-01), [ID-03](VERIFICATION.md#id-03), [ID-04](VERIFICATION.md#id-04), [ID-08](VERIFICATION.md#id-08). **Applicability:** core.

<a id="orc-010"></a>

### ORC-010 — Preserve exact request and governing-context revisions

Capture the original request and attributed user corrections without rewriting their meaning. A request slug groups work; it does not freeze its meaning. Changed required behavior or authoritative clarification creates a new context revision, with exact retained bytes and a digest.

**Enforcement:** Cohorts and phase outputs bind the request/context revision. Source locators accompany extracted governing constraints. A new clarification cannot silently be injected into only one opinion and then compared as if all opinions had the same instructions.

**Acceptance:**
- **ORC-010-A1:** Editing a request under the same slug changes its recorded context identity.
- **ORC-010-A2:** Opinions based on different governing revisions cannot produce an unqualified common-baseline Agreement.

**Verification cases:** [SRC-03](VERIFICATION.md#src-03), [SRC-04](VERIFICATION.md#src-04), [CON-10](VERIFICATION.md#con-10), [ACX-01](VERIFICATION.md#acx-01). **Applicability:** core.

<a id="orc-011"></a>

### ORC-011 — Represent separate efforts and comparison cohorts

A repository may contain multiple independent request efforts. Within an effort, a comparison cohort identifies the shared request/context, frozen source, and three intended independent Discovery slots for the initial product. New investigations, corrected context, or a deliberately changed comparison produce an explicit cohort revision rather than rewriting history.

**Enforcement:** Use cohort identity, slot identity, and selected finalized run identities. Replacements are attributed to a slot and prior attempts remain visible. There is no global “current stage” that conflates different requests.

**Acceptance:**
- **ORC-011-A1:** Two efforts can be inspected independently even with overlapping work.
- **ORC-011-A2:** Replacing an interrupted or contaminated slot does not create a fourth vote or silently erase the prior attempt.

**Verification cases:** [ID-02](VERIFICATION.md#id-02), [ID-07](VERIFICATION.md#id-07), [CON-08](VERIFICATION.md#con-08), [CON-09](VERIFICATION.md#con-09), [E2E-06](VERIFICATION.md#e2e-06), [ACX-03](VERIFICATION.md#acx-03). **Applicability:** core.

<a id="orc-012"></a>

### ORC-012 — Resolve the source baseline once per cohort

The default baseline is the latest local committed HEAD at cohort creation, resolved once to immutable Git commit/tree identity. Each Discovery in that cohort receives that same subject. This does not mean latest remote, automatic fetch, or whatever HEAD happens to be when a later run starts.

**Enforcement:** Record the resolved source identity in the cohort and every run. Construct later workspaces from that frozen identity, never by rereading a moving checkout as the investigation source.

**Acceptance:**
- **ORC-012-A1:** After the checkout advances from A to B, all remaining cohort investigators still receive A.
- **ORC-012-A2:** A user-selected new baseline creates a new explicit cohort; previous results remain about A.

**Verification cases:** [SRC-01](VERIFICATION.md#src-01), [SRC-03](VERIFICATION.md#src-03), [SRC-05](VERIFICATION.md#src-05), [E2E-03](VERIFICATION.md#e2e-03). **Applicability:** core.

<a id="orc-013"></a>

### ORC-013 — Supply the committed source, not a dirty working copy

Discovery SHALL inspect a materialized copy of the selected committed source. Pre-existing dirty edits, untracked reproductions, and generated local files are neither included implicitly nor removed. Merely recording HEAD while reading different working-tree bytes is invalid provenance.

**Enforcement:** Read committed objects and verify the delivered snapshot against its material manifest. Preserve the original checkout; no clean, reset, stash, stage, initial commit, or source worktree registration is an investigation setup step.

**Acceptance:**
- **ORC-013-A1:** A fixture with conflicting dirty and untracked code yields a snapshot matching the committed version only.
- **ORC-013-A2:** An unborn/non-Git target produces a supported-prerequisite diagnostic instead of fabricating a baseline.

**Verification cases:** [SRC-02](VERIFICATION.md#src-02), [SRC-05](VERIFICATION.md#src-05), [SRC-06](VERIFICATION.md#src-06), [ID-05](VERIFICATION.md#id-05). **Applicability:** core.

<a id="orc-014"></a>

### ORC-014 — Make external context and source completeness explicit

Relevant external documents, runtime observations, or authoritative answers may be gathered during Discovery, but must identify their origin, capture time, and scope. Core snapshot support SHALL define handling of file modes, binary files, symlinks, and nested dependencies. Required unavailable material must not silently disappear.

**Enforcement:** Retain a source inventory and explicit exclusions. Safely preserve supported internal links; reject unsafe path escapes. Unsupported submodule/LFS or live-service needs become precise limitations/blockers rather than ordinary complete-source claims.

**Acceptance:**
- **ORC-014-A1:** Supported source constructs remain faithful to their declared baseline; unsupported required constructs are reported before claiming complete investigation.
- **ORC-014-A2:** Different captured external observations stay attributable and are reconciled as such, not presented as one shared frozen fact.

**Verification cases:** [SRC-07](VERIFICATION.md#src-07), [SRC-08](VERIFICATION.md#src-08), [ENV-03](VERIFICATION.md#env-03), [DISC-03](VERIFICATION.md#disc-03). **Applicability:** core.

<a id="orc-015"></a>

### ORC-015 — Do not retarget completed work

A new request, source baseline, Agreement, or implementation SHALL NOT rewrite the target identity of an old run. Historical results remain inspectable. Applicability to a new target must be established in a new attributed artifact or assessment, not inferred from the same slug or filename.

**Enforcement:** Parent identities are immutable. Status identifies the exact selected chain and any newer revision separately; it does not search for a convenient replacement parent.

**Acceptance:**
- **ORC-015-A1:** Publishing Agreement v2 or moving HEAD leaves prior build/audit references unchanged.
- **ORC-015-A2:** A missing parent is shown as missing rather than replaced with the newest similarly named artifact.

**Verification cases:** [BLD-04](VERIFICATION.md#bld-04), [ART-05](VERIFICATION.md#art-05), [ART-06](VERIFICATION.md#art-06), [ART-09](VERIFICATION.md#art-09), [AUD-05](VERIFICATION.md#aud-05). **Applicability:** core.

## 6.3. Contamination prevention and safe evidence work

<a id="orc-016"></a>

### ORC-016 — Exclude peer outputs from supplied investigator context

Each Discovery receives the selected request/context, its frozen source, approved research access, and only its own run state. Orchestrate SHALL NOT supply sibling reports, scratch, extracted claims, partial consensus, or prior conclusions to it. Apply the same peer-exclusion rule to independent initial audit assessments when used.

**Enforcement:** Construct explicit input inventories and run-owned workspaces. Do not recursively expose the central orchestration tree. Exclude peer material from file retrieval, provided repositories, caches, and built-in packet assembly.

**Acceptance:**
- **ORC-016-A1:** Run A’s distinctive reproduction and recommendation never appear in B’s supplied files or captured packets.
- **ORC-016-A2:** Authorized source remains usable; providing an empty packet is not a passing isolation solution.

**Verification cases:** [ISO-01](VERIFICATION.md#iso-01), [ISO-02](VERIFICATION.md#iso-02), [ISO-08](VERIFICATION.md#iso-08), [SRC-08](VERIFICATION.md#src-08), [CON-15](VERIFICATION.md#con-15). **Applicability:** core.

<a id="orc-017"></a>

### ORC-017 — Report the actual independence boundary

External storage and separate skill names SHALL NOT be described as filesystem confinement or proof of independent judgment. Every run records whether input exclusion was controlled, broader access was constrained, fresh context was used, and independence was user-attested, observed within a declared boundary, unknown, or known compromised.

**Enforcement:** Distinguish input_excluded from access_enforced and historical independence. Access-enforced claims require a qualified host/tool boundary. A host with unrestricted filesystem or connector access remains cooperative even when it is instructed not to inspect peers.

**Acceptance:**
- **ORC-017-A1:** A direct peer-path read that succeeds prevents an access-enforced qualification claim.
- **ORC-017-A2:** Known peer exposure is recorded and that run cannot silently serve as a clean independent slot; replacement is a new run.

**Verification cases:** [ISO-03](VERIFICATION.md#iso-03), [ISO-04](VERIFICATION.md#iso-04), [ISO-08](VERIFICATION.md#iso-08), [META-02](VERIFICATION.md#meta-02), [ACX-03](VERIFICATION.md#acx-03). **Applicability:** core; access-enforced qualification is conditional.

<a id="orc-018"></a>

### ORC-018 — Use fresh semantic contexts for independent work

Different independent opinions must not be continuations of one conversation that already contains another opinion. A new session continuing the same investigator’s work remains one opinion. Shared user instructions may be supplied consistently, but peer reasoning and grading keys must not be inherited.

**Enforcement:** Managed calls receive fresh contexts and explicit packets. Host-operated runs record the stated session/context provenance and refuse to claim stronger separation than the host can establish.

**Acceptance:**
- **ORC-018-A1:** Three same-thread rephrasings cannot be reported as three independent investigations.
- **ORC-018-A2:** A legitimate interrupted-run continuation retains one run/slot identity and preserves its own saved findings.

**Verification cases:** [DISC-09](VERIFICATION.md#disc-09), [DISC-10](VERIFICATION.md#disc-10), [ISO-08](VERIFICATION.md#iso-08), [META-01](VERIFICATION.md#meta-01), [ACX-03](VERIFICATION.md#acx-03). **Applicability:** core.

<a id="orc-019"></a>

### ORC-019 — Keep experiments and checks in owned disposable environments

Reproductions, prototypes, temporary services, check outputs, and mutable build caches SHALL belong to the active run’s external scratch. The immutable baseline and target repository are not experiment workspaces. Retain required evidence before cleanup; a successful reproduction is not proof that a proposed fix works.

**Enforcement:** Prepare fresh scratch from the frozen source/target, allocate run-local resources, capture exact commands and observations, and verify relevant protected inputs. Use only authorized disposable/synthetic or sanitized data.

**Acceptance:**
- **ORC-019-A1:** Success, assertion failure, crash, and interruption leave honest receipts and no target-repository modifications.
- **ORC-019-A2:** A probe patch remains experimental and cannot become the code later identified as the audited target.

**Verification cases:** [ISO-05](VERIFICATION.md#iso-05), [ISO-06](VERIFICATION.md#iso-06), [DISC-07](VERIFICATION.md#disc-07), [AUD-12](VERIFICATION.md#aud-12), [PRO-08](VERIFICATION.md#pro-08), [ACX-04](VERIFICATION.md#acx-04). **Applicability:** core.

<a id="orc-020"></a>

### ORC-020 — Bound permissions and disclose enforcement limits

Source material and model prose are untrusted input, not permission to expand tool access. No phase may autonomously mutate production services, expose credentials, inspect peer conclusions, or widen its allowed sources. A working directory alone is not a confinement guarantee.

**Enforcement:** Use explicit permitted-source and check-execution policies. When a required restriction cannot be enforced on a host, record the limitation before work and block the affected safety claim or operation. Do not add a universal security-sandbox project to satisfy unrelated read-only work.

**Acceptance:**
- **ORC-020-A1:** A model-suggested destructive or live-service command cannot execute without a valid permitted operation.
- **ORC-020-A2:** An unsafe alternate filesystem path or unexpected tool request is rejected by the claimed boundary, or the capability is visibly unqualified.

**Verification cases:** [PRO-08](VERIFICATION.md#pro-08), [PRO-09](VERIFICATION.md#pro-09), [ISO-03](VERIFICATION.md#iso-03), [ISO-04](VERIFICATION.md#iso-04), [ACX-04](VERIFICATION.md#acx-04). **Applicability:** core.

<a id="orc-021"></a>

### ORC-021 — Clean up only owned disposable resources

Cleanup SHALL remove only resources whose ownership and resolved location are established for that run. It must not delete sibling evidence, finalized outputs, source repositories, or user work. No cleanup is required to make a report look successful.

**Enforcement:** Validate ownership and resolved containment at deletion time; refuse symlink escapes and stale ownership. Final artifacts are retained by default. Cancellation records any cleanup uncertainty.

**Acceptance:**
- **ORC-021-A1:** An attempted cleanup targeting a sibling, source path, or escaped symlink is refused.
- **ORC-021-A2:** Ordinary run scratch can be safely removed after evidence retention without breaking finalized public artifacts.

**Verification cases:** [ISO-07](VERIFICATION.md#iso-07), [ID-08](VERIFICATION.md#id-08), [REC-07](VERIFICATION.md#rec-07). **Applicability:** core.

## 6.4. Discovery: produce a justified implementation-ready opinion

<a id="orc-022"></a>

### ORC-022 — One investigator-owned run is one opinion

The first product version SHALL implement one investigator-owned Discovery run, not a built-in committee or nested consensus workflow. The investigator can perform many research actions and adversarial checks. Multiple files, sessions, exports, and retries do not become independent votes.

**Enforcement:** Publish one opinion identity per finalized run revision, bound to its slot and provenance. No internal replica scheduler or quorum is needed for Discovery.

**Acceptance:**
- **ORC-022-A1:** Repeated exports or resumed sessions of one run still count once downstream.
- **ORC-022-A2:** The investigator can complete a substantial evidence-backed opinion without a subagent committee installed.

**Verification cases:** [DISC-01](VERIFICATION.md#disc-01), [DISC-10](VERIFICATION.md#disc-10), [CON-08](VERIFICATION.md#con-08). **Applicability:** core.

<a id="orc-023"></a>

### ORC-023 — Cover mandatory research without arbitrary ceremony

Discovery SHALL assess intent/authority, relevant current behavior, affected contracts and dependencies, meaningful failure/edge cases, existing tests and required verification, and consequential risks. Each category requires a disposition; deeper checks scale with the request. The investigator may add request-specific obligations but may not silently remove mandatory ones.

**Enforcement:** Initialize a small versioned checklist; require evidence/reason links for completed, not-applicable, unavailable, or inaccessible dispositions. Pending obligations prevent finalization. Check naming or quantity does not substitute for meaning.

**Acceptance:**
- **ORC-023-A1:** Omitting a mandatory category prevents finalization.
- **ORC-023-A2:** A genuinely irrelevant risk can be marked not applicable with a reason, without producing fake experiments or redundant reports.

**Verification cases:** [DISC-02](VERIFICATION.md#disc-02), [DISC-03](VERIFICATION.md#disc-03), [DISC-08](VERIFICATION.md#disc-08). **Applicability:** core.

<a id="orc-024"></a>

### ORC-024 — Separate product blockers from delegated technical choices

An unknown product rule, permission, incompatible requirement, or consequential business definition blocks dependent conclusions until authoritative resolution. Ordinary implementation choices delegated to Discovery are researched and recommended, not sent back to the user as design homework. Safe assumptions remain explicit and scoped.

**Enforcement:** Record questions with impact, required authority/evidence, affected conclusions, and status. Blocking decisions cannot be assumed away; assumptions include why they are safe and what invalidates them.

**Acceptance:**
- **ORC-024-A1:** A missing tenant-visibility rule remains a blocker; choosing an existing storage abstraction is ordinary investigation.
- **ORC-024-A2:** Useful independent research may continue while a blocker exists, but its dependent design cannot be finalized as ready.

**Verification cases:** [DISC-03](VERIFICATION.md#disc-03), [DISC-08](VERIFICATION.md#disc-08), [CON-10](VERIFICATION.md#con-10). **Applicability:** core.

<a id="orc-025"></a>

### ORC-025 — Keep facts, interpretations, proposals, and observations distinct

Important findings SHALL identify the assertion, inspected source or observed receipt, concise evidentiary rationale, qualifications, and contradictory material. User assertions, imported testimony, source inspection, and actual execution observations must not be relabeled as one another. The tool needs an auditable evidence case, not a transcript of private reasoning.

**Enforcement:** Require typed provenance and resolvable references for material adopted findings. Record who supplied an observation and whether Orchestrate observed the action. Review relevance semantically; file existence alone is not proof.

**Acceptance:**
- **ORC-025-A1:** A reported “tests passed” statement without a receipt remains testimony.
- **ORC-025-A2:** A zero-exit command with no meaningful assertions cannot automatically satisfy a behavioral proof obligation.

**Verification cases:** [DISC-07](VERIFICATION.md#disc-07), [PRO-06](VERIFICATION.md#pro-06), [AUD-03](VERIFICATION.md#aud-03), [META-02](VERIFICATION.md#meta-02). **Applicability:** core.

<a id="orc-026"></a>

### ORC-026 — Permit useful no-change conclusions

Discovery may conclude that existing behavior already satisfies the request, or that the proposed change is unnecessary. Every research question requires an honest disposition; not every research question requires an implementation task or new requirement.

**Enforcement:** Separate research-obligation completion from adopted-behavior coverage. Allow a finalized, justified no-change opinion with explicit preservation/verification expectations where relevant.

**Acceptance:**
- **ORC-026-A1:** A solved investigation can finalize with no proposed code change.
- **ORC-026-A2:** The model is not forced to manufacture a requirement merely to close a research lane; the eventual no-change chain can be audited against the existing commit.

**Verification cases:** [DISC-04](VERIFICATION.md#disc-04), [ACX-05](VERIFICATION.md#acx-05). **Applicability:** core.

<a id="orc-027"></a>

### ORC-027 — Preserve counterevidence and invalidate dependent conclusions

New material counterevidence, changed assumptions, or revised claims SHALL make dependent decisions, drafts, and reviews visibly stale. Unrelated valid evidence may remain reusable. Support counts do not erase a consequential evidenced objection.

**Enforcement:** Track explicit support/dependency references or a conservative equivalent. Require a new assessment and current final-review binding after material revisions. No requirement to recreate the prototype’s entire phase-revision graph.

**Acceptance:**
- **ORC-027-A1:** A changed premise blocks publication using the old review.
- **ORC-027-A2:** A challenged claim is narrowed, rejected, or explicitly unresolved with reasons; its contrary evidence remains inspectable.

**Verification cases:** [DISC-05](VERIFICATION.md#disc-05), [DISC-06](VERIFICATION.md#disc-06), [ART-01](VERIFICATION.md#art-01). **Applicability:** core.

<a id="orc-028"></a>

### ORC-028 — Investigate and prototype without implementing the target feature

Discovery ends with an opinion, not product changes. Experiments may test candidate designs in scratch, but their outcomes must distinguish diagnosis reproduced, candidate demonstrated, and proposed but untested behavior. They never merge or commit changes into the user’s source repository.

**Enforcement:** Limit the phase’s owned write surfaces, bind experiment receipts to scratch source identity, and forbid Build/integration transitions from Discovery operations.

**Acceptance:**
- **ORC-028-A1:** A diagnostic prototype may support the specification while the target repository stays unchanged.
- **ORC-028-A2:** An untested repair is labeled as proposed, not verified by a test of the original defect.

**Verification cases:** [DISC-07](VERIFICATION.md#disc-07), [ISO-05](VERIFICATION.md#iso-05), [ID-05](VERIFICATION.md#id-05), [CLI-03](VERIFICATION.md#cli-03). **Applicability:** core.

<a id="orc-029"></a>

### ORC-029 — Deliver a substantive public Discovery artifact

The finalized public opinion SHALL be understandable without the prior chat or a private working database. It contains the recommendation, interpreted intent, relevant observed behavior, selected direction and reasons, changed/unchanged scope, conditions, alternatives/dissent, concrete acceptance expectations, verification performed/planned, and remaining limitations.

**Enforcement:** Publish readable opinion content plus minimal structured identity, findings/decisions/requirements, conditions, references, and provenance. A short pointer summary is not the opinion. Do not require the prototype’s four-file bundle.

**Acceptance:**
- **ORC-029-A1:** A downstream reader can determine what to build and how to judge it using only the public contract.
- **ORC-029-A2:** Removing the private Discovery database does not stop Consensus consuming an otherwise complete public opinion.

**Verification cases:** [DISC-01](VERIFICATION.md#disc-01), [CON-15](VERIFICATION.md#con-15), [ART-11](VERIFICATION.md#art-11). **Applicability:** core.

<a id="orc-030"></a>

### ORC-030 — Gate finalization on a current evidence case

A finalized Discovery opinion requires disposition of mandatory checks, current referenced support for adopted material conclusions, no unresolved blocking product decision, and an adversarial assessment of the current proposal. Non-blocking conditions may remain if carried clearly into the proposed contract.

**Enforcement:** Validate required fields, referenced artifacts, check dispositions, active blockers, and draft/review identity before committing final output. Semantic adequacy remains the investigator’s responsibility and a live-evaluation target.

**Acceptance:**
- **ORC-030-A1:** A missing check, stale review, or unresolved blocking decision prevents final opinion publication.
- **ORC-030-A2:** A justified conditional recommendation can finalize without falsely asserting that its condition is already satisfied.

**Verification cases:** [DISC-02](VERIFICATION.md#disc-02), [DISC-03](VERIFICATION.md#disc-03), [DISC-05](VERIFICATION.md#disc-05), [CON-05](VERIFICATION.md#con-05). **Applicability:** core.

<a id="orc-031"></a>

### ORC-031 — Recover useful investigation work honestly

An interrupted or blocked Discovery SHALL retain findings, evidence, questions, assumptions, and the last trustworthy draft for explicit resumption. Interim delivery must be clearly non-final. A new chat may continue the run, but may not discard blockers by creating a replacement without recording that choice.

**Enforcement:** Return a bounded recovery view and exact run identity. Preserve active workflow state; allocate new session provenance when needed. Export interim reports without making them eligible finalized Consensus inputs.

**Acceptance:**
- **ORC-031-A1:** A fresh session resumes recorded work rather than repeating it from memory.
- **ORC-031-A2:** An interim report remains visible and useful but fails the finalized-opinion entry check.

**Verification cases:** [DISC-09](VERIFICATION.md#disc-09), [CLI-04](VERIFICATION.md#cli-04), [REC-01](VERIFICATION.md#rec-01). **Applicability:** core.

<a id="orc-032"></a>

### ORC-032 — Explain confidence without procedural numerology

Discovery SHALL describe which conclusions are supported, conditional, refuted, or unresolved and explain material limitations. It SHALL NOT present completed check counts, a procedural percentage, or a model’s confidence number as probability of correctness or permission to skip evidence.

**Enforcement:** Use scoped status and evidence summaries. No inherited procedural-assurance formula or scoring lifecycle is required.

**Acceptance:**
- **ORC-032-A1:** A report with all structural checks completed still exposes an unresolved semantic limitation.
- **ORC-032-A2:** Adding low-risk completed checks cannot erase a critical unresolved dependency.

**Verification cases:** [DISC-03](VERIFICATION.md#disc-03), [DISC-06](VERIFICATION.md#disc-06), [META-04](VERIFICATION.md#meta-04). **Applicability:** core.

## 6.5. Consensus: derive a coherent Agreement, not a new investigation

<a id="orc-033"></a>

### ORC-033 — Consume finalized public opinions and shared governing context

Consensus SHALL accept the selected cohort’s finalized public Discovery artifacts and the same preserved request/governing context. It does not inspect private investigation state, conduct fresh source research, follow arbitrary evidence links, or change its task because input packaging differs.

**Enforcement:** Resolve public artifacts by identity and digest; construct an allowlisted input projection. Keep underlying evidence available for separately authorized inspection, not automatically model-visible. Reject old-format or arbitrary-document imports in the initial product.

**Acceptance:**
- **ORC-033-A1:** Consensus still works when private upstream databases are absent.
- **ORC-033-A2:** Retained logs, summaries, scratch, and ledgers are not accidentally embedded in the model packet.

**Verification cases:** [CON-14](VERIFICATION.md#con-14), [CON-15](VERIFICATION.md#con-15), [ART-11](VERIFICATION.md#art-11), [CLI-04](VERIFICATION.md#cli-04). **Applicability:** core.

<a id="orc-034"></a>

### ORC-034 — Require comparable, distinct cohort membership

Each voting opinion must be finalized, intact, associated with one selected cohort slot, and bound to the same request/context and source baseline. A copied artifact, resumed session, renamed directory, or alternate export is not another opinion. Conflicting copies of the same identity are integrity errors. Identical text from independently identified runs is not automatically deduplicated.

**Enforcement:** Check slot/run/artifact identity and content digests independently. Freeze the selected input identities before reasoning; never count files, extractor passes, or provider labels as voters.

**Acceptance:**
- **ORC-034-A1:** Aliases of one run count once and cannot fill multiple slots.
- **ORC-034-A2:** Different baseline/context or conflicting same-ID content fails before an Agreement can be published.

**Verification cases:** [CON-08](VERIFICATION.md#con-08), [SRC-03](VERIFICATION.md#src-03), [CON-14](VERIFICATION.md#con-14), [CLI-04](VERIFICATION.md#cli-04). **Applicability:** core.

<a id="orc-035"></a>

### ORC-035 — Handle incomplete participation without false consensus

The initial workflow expects three Discovery slots. A user may explicitly request a partial comparison of available eligible opinions, but the result SHALL retain the expected denominator and missing slots and SHALL NOT publish a build-ready Agreement until all three slots contain eligible finalized opinions. A failed or compromised slot may be explicitly replaced.

**Enforcement:** Separate complete-cohort admission from optional partial-comparison output. One available opinion is not a consensus. A missing result is not silence, opposition, or a reason to relabel 2/3 participation as 2/2 unanimity.

**Acceptance:**
- **ORC-035-A1:** Two received opinions can produce a clearly partial comparison with shared findings, but no adoptable Agreement.
- **ORC-035-A2:** After an explicit replacement completes the third slot, a new comparison can finalize without changing the old partial result.

**Verification cases:** [CON-09](VERIFICATION.md#con-09), [CON-11](VERIFICATION.md#con-11), [ACX-03](VERIFICATION.md#acx-03). **Applicability:** core.

<a id="orc-036"></a>

### ORC-036 — Normalize meaning while preserving conditions

The reconciler SHALL compare propositions in their stated scope, including prerequisites, exclusions, timing, and behavioral consequences. Each selected opinion’s position is supports, contradicts, related, or not observed. Missing participation is tracked separately. Silence is neither endorsement nor disagreement.

**Enforcement:** Require one source-linked position per selected opinion for every material canonical proposition. The machine validates completeness and identifiers; semantic evaluations judge whether those positions faithfully represent the original opinions.

**Acceptance:**
- **ORC-036-A1:** Equivalent designs with different vocabulary are recognized as agreement.
- **ORC-036-A2:** “X only when P” is not treated as support for unconditional X; related discussion is not counted as endorsement.

**Verification cases:** [CON-01](VERIFICATION.md#con-01), [CON-03](VERIFICATION.md#con-03), [CON-05](VERIFICATION.md#con-05), [CON-12](VERIFICATION.md#con-12). **Applicability:** core.

<a id="orc-037"></a>

### ORC-037 — Validate a common majority for the mandatory package

Every mandatory consensus-derived proposition in the adopted implementation package SHALL be supported by one common strict majority of the original three eligible opinions. For an adopted proposition set P, supporters are the intersection of the original-opinion supporter sets for every p in P. The required count is two. Empty packages do not acquire unanimous support.

**Enforcement:** Compute package support deterministically from the validated position matrix, independently of model-provided counts. Governing user constraints retain their own source authority and are not fabricated opinion votes. Conditions indispensable to the package are included in its support accounting.

**Acceptance:**
- **ORC-037-A1:** A/B support X and B/C support Y: the package X+Y has one supporter, not two.
- **ORC-037-A2:** The exact same common pair supporting every required part permits a majority package with dissent retained.

**Verification cases:** [CON-02](VERIFICATION.md#con-02), [CON-04](VERIFICATION.md#con-04), [CON-07](VERIFICATION.md#con-07), [CON-10](VERIFICATION.md#con-10), [ACX-06](VERIFICATION.md#acx-06). **Applicability:** core.

<a id="orc-038"></a>

### ORC-038 — Prefer useful agreement over an empty unanimous generality

Consensus SHALL select a sufficiently specific coherent answer, not the longest union of suggestions or a vague statement chosen to maximize apparent agreement. Use unanimous material when sufficient; use an eligible majority direction when unanimity only establishes the problem. Necessary conditions stay attached.

**Enforcement:** Require a selection rationale, complete adopted package, and coverage of the requested decision. If no adequate jointly supported package exists or equally supported incompatible alternatives cannot be resolved from the opinions, publish a no-consensus result naming the remaining choice.

**Acceptance:**
- **ORC-038-A1:** A clear 2–1 repair recommendation is not reduced to “there is a problem.”
- **ORC-038-A2:** A no-consensus report preserves real common ground without fabricating a build-ready direction.

**Verification cases:** [CON-02](VERIFICATION.md#con-02), [CON-04](VERIFICATION.md#con-04), [CON-11](VERIFICATION.md#con-11). **Applicability:** core.

<a id="orc-039"></a>

### ORC-039 — Keep dissent and counterexamples consequential

Minority objections and relevant contrary evidence SHALL remain visible. A credible unresolved objection that undermines an essential premise makes that package non-adoptable until resolved or properly narrowed using the supplied opinions. A mere alternative preference does not automatically veto a coherent majority.

**Enforcement:** Link material objections to affected package propositions and require a recorded disposition. Report endorsement counts even when an objection blocks readiness; do not invent new research or new votes to settle it.

**Acceptance:**
- **ORC-039-A1:** A concrete minority counterexample is not erased by two supporting opinions.
- **ORC-039-A2:** Non-material dissent remains reported without forcing every majority decision to become inconclusive.

**Verification cases:** [CON-02](VERIFICATION.md#con-02), [CON-13](VERIFICATION.md#con-13), [ACX-06](VERIFICATION.md#acx-06). **Applicability:** core.

<a id="orc-040"></a>

### ORC-040 — Bind all normative output to validated adopted records

The Agreement’s mandatory behavior, exclusions, compatibility choices, and essential conditions SHALL be represented in its adopted requirement/decision records or directly sourced governing constraints. Free-form recommendation fields, summaries, and “next steps” cannot add mandatory scope outside that set.

**Enforcement:** Render normative sections from the validated ordered records. Separate rationale, optional observations, rejected alternatives, and unresolved choices. Require traceable source positions for consensus-derived obligations and direct source references for governing constraints.

**Acceptance:**
- **ORC-040-A1:** A single-report feature injected into an extra prose paragraph cannot become a mandatory Agreement instruction.
- **ORC-040-A2:** Every normative paragraph has an identifiable validated source record; semantic review checks paraphrase fidelity as well as structural linkage.

**Verification cases:** [CON-06](VERIFICATION.md#con-06), [CON-07](VERIFICATION.md#con-07), [CON-10](VERIFICATION.md#con-10), [ART-01](VERIFICATION.md#art-01), [ACX-01](VERIFICATION.md#acx-01). **Applicability:** core.

<a id="orc-041"></a>

### ORC-041 — Preserve opinion identity through any internal extraction

Separate extractor calls are not a product requirement. Consensus may reason directly over public opinions. If extraction is used, originals remain available within the permitted opinion boundary, every derived item is source-linked, and extraction/repair outputs never count as new independent opinions.

**Enforcement:** Keep original-slot identity separate from model invocation identity. Evaluate extraction against the original documents, not against only its own derived records.

**Acceptance:**
- **ORC-041-A1:** Adding or removing an implementation-internal extraction pass does not change the cohort denominator.
- **ORC-041-A2:** An extraction omission is visible to semantic evaluation; retained originals are not replaced by an unverifiable summary.

**Verification cases:** [CON-08](VERIFICATION.md#con-08), [CON-15](VERIFICATION.md#con-15), [PRO-10](VERIFICATION.md#pro-10). **Applicability:** core.

<a id="orc-042"></a>

### ORC-042 — Report scoped consensus confidence separately from execution

Consensus SHALL state whole-package supporters, explicit opponents, silent/related positions, participation, conditions, and independence limitations. The first product uses descriptive confidence—unanimous, majority-supported, contested, or no adequate consensus—with explanations. It does not inherit the old numeric tiers, rescore command, or probability claims.

**Enforcement:** Calculate counts from the position matrix and display them beside, not instead of, readiness and operational status. Unknown independence remains unknown. A coherent contested majority can be a valid completed comparison.

**Acceptance:**
- **ORC-042-A1:** A successful contested-majority result is not labeled a transport failure because confidence is moderate.
- **ORC-042-A2:** Optional minority ideas do not reduce support for a different adopted package merely by existing.

**Verification cases:** [CON-03](VERIFICATION.md#con-03), [CON-09](VERIFICATION.md#con-09), [CON-11](VERIFICATION.md#con-11), [META-04](VERIFICATION.md#meta-04). **Applicability:** core.

<a id="orc-043"></a>

### ORC-043 — Do not use retries to manufacture agreement

Consensus may use only the bounded intra-phase execution/validation attempts authorized at run start. Corrections to malformed responses may identify a structural error, but SHALL NOT instruct a model to raise confidence, add supporters, hide dissent, change the denominator, or reinvestigate the project.

**Enforcement:** Retain each attempt, error, input identity, and correction feedback. Once a valid comparison completes, new semantic work requires a new explicit request rather than an automatic rerun for a preferable answer.

**Acceptance:**
- **ORC-043-A1:** A malformed payload is retried only under the declared allowance and every attempt is counted.
- **ORC-043-A2:** A valid low-support answer ends the phase; no hidden extra reviewer or source investigation begins.

**Verification cases:** [PRO-02](VERIFICATION.md#pro-02), [PRO-10](VERIFICATION.md#pro-10), [CLI-03](VERIFICATION.md#cli-03), [CON-11](VERIFICATION.md#con-11). **Applicability:** core.

## 6.6. Agreement: the exact authorized contract for Build and Audit

<a id="orc-044"></a>

### ORC-044 — Make the Agreement complete and self-contained

The Agreement SHALL define the goal, scope, required behavior with stable IDs, preserved behavior, exclusions, essential conditions, verification expectations, implementation latitude, and provenance of adopted decisions. Material dissent and limitations remain distinct from requirements. It must be usable without reading all Discovery runs.

**Enforcement:** Require structured governing-constraint coverage and acceptance criteria for each adopted requirement. The readable Agreement is generated from the same normative records consumed by Build and Audit.

**Acceptance:**
- **ORC-044-A1:** A builder and auditor can determine required outcomes, prohibited expansion, and acceptance expectations from the Agreement alone.
- **ORC-044-A2:** An unresolved externally meaningful decision is not hidden under “implementation latitude.”

**Verification cases:** [CON-06](VERIFICATION.md#con-06), [CON-10](VERIFICATION.md#con-10), [ART-11](VERIFICATION.md#art-11), [ACX-01](VERIFICATION.md#acx-01). **Applicability:** core.

<a id="orc-045"></a>

### ORC-045 — Separate finalization from user adoption

Consensus may finalize an eligible Agreement candidate, but it becomes Build authority only through an explicit user adoption action naming the exact artifact/revision and digest. Adoption can be captured together with a deliberate downstream command; it need not introduce repeated ceremonial approvals.

**Enforcement:** Store adoption as a separate immutable receipt referencing the candidate. Do not mutate the Agreement to add approval or claim that model confidence is approval. The personal tool records user authorization; it does not claim hostile-user-resistant authentication.

**Acceptance:**
- **ORC-045-A1:** A finalized but unadopted candidate cannot authorize a managed build or ordinary adopted-contract Audit.
- **ORC-045-A2:** An adoption for G1 does not authorize different bytes or G2; a delegated skill cannot invent approval.

**Verification cases:** [CON-16](VERIFICATION.md#con-16), [ACX-07](VERIFICATION.md#acx-07). **Applicability:** core.

<a id="orc-046"></a>

### ORC-046 — Do not let adoption conceal missing consensus or authority

Adoption SHALL NOT override corrupt inputs, incomplete participation, absent common-majority support, unaccounted governing constraints, or unresolved blocking contract decisions. Selecting a different product direction requires explicit upstream revision; this first version has no hidden “force consensus” override.

**Enforcement:** Check eligibility at adoption and downstream use. Report why a candidate is not ready and the exact upstream issue. Non-blocking conditions may remain only as explicit contract conditions with a verification/discharge requirement before dependent completion claims.

**Acceptance:**
- **ORC-046-A1:** Approving a partial or structurally invalid comparison cannot turn it into an eligible Agreement.
- **ORC-046-A2:** An agreed condition can remain in the contract, but Build/Audit cannot treat it as satisfied merely because it was approved.

**Verification cases:** [CON-09](VERIFICATION.md#con-09), [CON-13](VERIFICATION.md#con-13), [CON-16](VERIFICATION.md#con-16), [ACX-07](VERIFICATION.md#acx-07), [ACX-08](VERIFICATION.md#acx-08). **Applicability:** core.

<a id="orc-047"></a>

### ORC-047 — Keep Agreement revisions immutable and traceable

Any change to required behavior, essential conditions, or verification expectations produces a new Agreement revision and new adoption. Earlier builds, evidence, and audits remain attached to the earlier authority. A discovered authority defect is an attributed issue, not permission to edit finalized bytes.

**Enforcement:** Reference prior revisions and change reasons in new records. Invalidate applicability to the new contract without retroactively changing historical findings.

**Acceptance:**
- **ORC-047-A1:** G1/I1/A1 remains intact after G2 is adopted.
- **ORC-047-A2:** Current status cannot label A1 as a pass for G2 or transfer adoption by matching a display name.

**Verification cases:** [ART-09](VERIFICATION.md#art-09), [BLD-04](VERIFICATION.md#bld-04), [AUD-09](VERIFICATION.md#aud-09), [ACX-07](VERIFICATION.md#acx-07). **Applicability:** core.

## 6.7. Build: a stable handoff, with execution strategy deliberately deferred

<a id="orc-048"></a>

### ORC-048 — Bind every implementation attempt to exact authority and starting source

The Build boundary SHALL consume an adopted Agreement identity/digest and the approved source baseline. A future build strategy may plan how to implement it, but cannot change what it requires. The initial product accepts externally produced implementations without requiring any specific builder or scheduler.

**Enforcement:** Record a build/implementation declaration with authority, project, starting source, production method, and later exact target. Reject structural mismatches; distinguish mechanically verified source identity from an external actor’s claim about how code was produced.

**Acceptance:**
- **ORC-048-A1:** An external implementation can be registered with no Flow or Taskledger installed.
- **ORC-048-A2:** The record never says Orchestrate built the code when it only registered a user-supplied result.

**Verification cases:** [BLD-01](VERIFICATION.md#bld-01), [BLD-03](VERIFICATION.md#bld-03), [BLD-06](VERIFICATION.md#bld-06), [ACX-09](VERIFICATION.md#acx-09). **Applicability:** core.

<a id="orc-049"></a>

### ORC-049 — Register the exact implementation rather than a moving checkout

An implementation handoff SHALL identify a real Git commit/tree and retain enough exact source material to make the recorded target inspectable. Registration concerns committed content only. Dirty or untracked edits are excluded and explicitly reported; they cannot silently be included under a HEAD label.

**Enforcement:** Resolve and snapshot the selected commit at registration, check project/baseline association, and freeze its manifest. Preserve the user’s working copy. A missing commit or unsupported required material is a registration error.

**Acceptance:**
- **ORC-049-A1:** Moving HEAD or deleting the original checkout after capture does not change the retained audit target.
- **ORC-049-A2:** A dirty checkout is reported as “committed target only”; its uncommitted fix cannot satisfy the registered implementation.

**Verification cases:** [BLD-02](VERIFICATION.md#bld-02), [BLD-03](VERIFICATION.md#bld-03), [AUD-05](VERIFICATION.md#aud-05), [ID-08](VERIFICATION.md#id-08). **Applicability:** core.

<a id="orc-050"></a>

### ORC-050 — Separate implementation claims from observed verification

The handoff SHALL identify the method/actor, declared scope and completion state, actual target, evidence references, deviations, and blockers. External preparation details not observed by Orchestrate remain declarations. Worker prose, successful compilation, and registration are not conformance acceptance.

**Enforcement:** Validate required handoff fields and provenance classifications. Partial, blocked, or unknown execution records remain distinct from a submitted exact target and from Audit results.

**Acceptance:**
- **ORC-050-A1:** A “complete” message missing target or authority cannot finalize a valid implementation handoff.
- **ORC-050-A2:** Claimed starting-baseline or test history is marked externally declared unless independently substantiated.

**Verification cases:** [BLD-01](VERIFICATION.md#bld-01), [BLD-05](VERIFICATION.md#bld-05), [META-01](VERIFICATION.md#meta-01), [ACX-09](VERIFICATION.md#acx-09). **Applicability:** core.

<a id="orc-051"></a>

### ORC-051 — Do not let Build reinterpret the Agreement

Ordinary internal design decisions are permitted only within the Agreement’s implementation latitude. A discovered contradiction, material unmet prerequisite, or necessary scope change must be surfaced. A plan may locate work and checks but SHALL NOT weaken the Agreement or add new product obligations.

**Enforcement:** Freeze authority in the handoff, record deviations rather than incorporating them silently, and require a new adopted Agreement for behavioral change. Audit judges the Agreement, not the builder’s self-created plan.

**Acceptance:**
- **ORC-051-A1:** A plan that omits a required edge case cannot make its implementation pass Audit.
- **ORC-051-A2:** A justified proposal to change the contract becomes an upstream issue, not an undocumented implementation decision.

**Verification cases:** [BLD-03](VERIFICATION.md#bld-03), [AUD-02](VERIFICATION.md#aud-02), [AUD-09](VERIFICATION.md#aud-09), [ACX-01](VERIFICATION.md#acx-01). **Applicability:** core.

<a id="orc-052"></a>

### ORC-052 — Support no-change and correction handoffs

A no-change Agreement can register the existing baseline as its exact implementation; no artificial code commit is required. A correction registers a new exact target against the same Agreement, or a new adopted Agreement when behavior changed. Prior records remain retained.

**Enforcement:** Use the same target/authority validation for unchanged and corrected implementations. Link correction ancestry explicitly; do not infer it from the newest directory.

**Acceptance:**
- **ORC-052-A1:** A justified no-change Discovery/Consensus outcome can complete through Audit without fabricated work.
- **ORC-052-A2:** Both faulty and repaired targets remain separately auditable and inspectable.

**Verification cases:** [BLD-04](VERIFICATION.md#bld-04), [AUD-10](VERIFICATION.md#aud-10), [AUD-11](VERIFICATION.md#aud-11), [E2E-02](VERIFICATION.md#e2e-02), [ACX-05](VERIFICATION.md#acx-05). **Applicability:** core.

<a id="orc-053"></a>

### ORC-053 — Do not advertise an unimplemented builder

Initial Orchestrate delivery SHALL expose external implementation registration and the Build artifact contract, not a working automatic Build phase. It must not install an apparently functional orchestrate-build skill or claim task scheduling, worktree integration, routing, or recovery for a builder that has not been specified.

**Enforcement:** Capability reporting and installed resources match shipped operations. Reserve future Build implementation behind the same contract without importing prototype machinery into shared infrastructure.

**Acceptance:**
- **ORC-053-A1:** External registration and Audit work while actual Build execution is explicitly unavailable.
- **ORC-053-A2:** Packaging tests find no misleading functional Build skill or hidden Taskledger dependency.

**Verification cases:** [CLI-08](VERIFICATION.md#cli-08), [BLD-06](VERIFICATION.md#bld-06), [ARCH-05](VERIFICATION.md#arch-05). **Applicability:** core for initial delivery; internal Build execution deferred.

## 6.8. Audit: establish conformance of the exact target to the exact Agreement

<a id="orc-054"></a>

### ORC-054 — Freeze the audit authority, target, and permitted evidence

Audit SHALL consume the exact adopted Agreement, exact registered implementation, and explicitly permitted evidence. It evaluates that pair even when working HEAD or the preferred Agreement later changes. It does not replace an input with a current version mid-run.

**Enforcement:** Verify and capture authority/target identities before assessment and execution. Every receipt and report names that pair. The audit packet includes governing constraints needed to detect upstream omission, not arbitrary private Discovery state.

**Acceptance:**
- **ORC-054-A1:** Moving HEAD during a test leaves the audit and receipts on the originally selected commit.
- **ORC-054-A2:** Passing a receipt or artifact from another Agreement/target cannot satisfy the current check by filename alone.

**Verification cases:** [AUD-05](VERIFICATION.md#aud-05), [AUD-06](VERIFICATION.md#aud-06), [ART-01](VERIFICATION.md#art-01), [E2E-03](VERIFICATION.md#e2e-03). **Applicability:** core.

<a id="orc-055"></a>

### ORC-055 — Use a fresh assessment independent of the builder

The acceptance auditor SHALL not simply be the worker grading its own conversation. Start a fresh semantic context with the Agreement, target, and attributable evidence. Builder statements may help locate work but remain claims. No fixed multi-reviewer committee is required in the initial product.

**Enforcement:** Separate role invocations and packets. Initial independent assessors, when more than one is deliberately requested, do not receive peer conclusions. Re-audit may receive prior findings as explicitly labeled correction context.

**Acceptance:**
- **ORC-055-A1:** A builder’s completion prose cannot substitute for a current independent assessment.
- **ORC-055-A2:** Adding optional multiple assessors does not suppress a demonstrated violation because only one found it.

**Verification cases:** [AUD-02](VERIFICATION.md#aud-02), [AUD-08](VERIFICATION.md#aud-08), [ISO-02](VERIFICATION.md#iso-02), [ACX-10](VERIFICATION.md#acx-10). **Applicability:** core.

<a id="orc-056"></a>

### ORC-056 — Account for the complete Agreement

Audit SHALL classify each required behavior, preserved constraint, and material condition as supported, violated, unresolved, or not applicable with justification. Coverage includes negative cases and exclusions where consequential. A missing row is a verification gap, not an implied pass.

**Enforcement:** Seed the coverage register from the immutable Agreement IDs. Reject missing, duplicate, unknown, or improperly exempted coverage records. Assess the sufficiency and relevance of evidence semantically.

**Acceptance:**
- **ORC-056-A1:** An audit omitting one required behavior cannot produce PASS.
- **ORC-056-A2:** A condition that truly does not apply has a source-linked justification; a required unmet prerequisite cannot be bypassed as not applicable.

**Verification cases:** [AUD-01](VERIFICATION.md#aud-01), [AUD-03](VERIFICATION.md#aud-03), [AUD-07](VERIFICATION.md#aud-07), [ACX-08](VERIFICATION.md#acx-08), [ACX-10](VERIFICATION.md#acx-10). **Applicability:** core.

<a id="orc-057"></a>

### ORC-057 — Obtain evidence that can support the required claim

Source inspection can establish source facts; runtime guarantees require appropriate observed checks when inspection alone is insufficient. Audit SHALL be able to request bounded verification through an authorized runner on disposable copies. Missing capabilities produce unresolved verification, not manufactured runtime evidence or an unsupported defect allegation.

**Enforcement:** Record exact command, source/target, relevant environment, timing, exit status, assertions/results, and bounded output. Distinguish assertion failure, infrastructure failure, cancellation, and unknown outcome. Running a check is not proof it meaningfully tests the requirement.

**Acceptance:**
- **ORC-057-A1:** A known defect’s focused probe produces an attributable violation.
- **ORC-057-A2:** A missing database or zero tests executed is not treated as passing the intended verification.

**Verification cases:** [AUD-02](VERIFICATION.md#aud-02), [AUD-03](VERIFICATION.md#aud-03), [AUD-04](VERIFICATION.md#aud-04), [PRO-06](VERIFICATION.md#pro-06), [ACX-04](VERIFICATION.md#acx-04). **Applicability:** core.

<a id="orc-058"></a>

### ORC-058 — Reuse evidence only with established applicability

Prior receipts may be reused only when their source/target, command, relevant environment, and required observation match the current claim and an assessor records why reuse is valid. Builder-run evidence does not become a new independently executed audit check. For changed implementation targets, rerun affected runtime checks unless a bounded non-runtime applicability argument suffices.

**Enforcement:** Keep receipt identity immutable and link reuse rather than rewriting its origin. Require current independent verification where the Agreement calls for it. A cache hit never overrides a new defect or missing prerequisite.

**Acceptance:**
- **ORC-058-A1:** An old passing receipt cannot be relabeled as execution on a new commit.
- **ORC-058-A2:** Valid same-target evidence may be referenced without rerunning unrelated work, while correction regressions still receive current checks.

**Verification cases:** [AUD-06](VERIFICATION.md#aud-06), [AUD-10](VERIFICATION.md#aud-10), [AUD-11](VERIFICATION.md#aud-11), [ART-09](VERIFICATION.md#art-09). **Applicability:** core.

<a id="orc-059"></a>

### ORC-059 — Treat counterexamples as evidence, not minority votes

One substantiated required-behavior violation defeats a conformance claim within its scope. Many passing checks or reviewers who missed it do not outvote it. Conversely, speculative risks, aesthetic preferences, and optional hardening are not demonstrated violations.

**Enforcement:** Require each mandatory finding to identify the violated Agreement/constraint ID, evidence, consequence, and bounded correction. Aggregate findings by their support and applicability, not simple reviewer popularity.

**Acceptance:**
- **ORC-059-A1:** A failing required probe blocks PASS despite other passing tests.
- **ORC-059-A2:** An unsupported improvement suggestion remains optional and does not prevent closure.

**Verification cases:** [AUD-02](VERIFICATION.md#aud-02), [AUD-07](VERIFICATION.md#aud-07), [AUD-08](VERIFICATION.md#aud-08), [AUD-10](VERIFICATION.md#aud-10). **Applicability:** core.

<a id="orc-060"></a>

### ORC-060 — Distinguish implementation defects, evidence gaps, and authority defects

Audit SHALL report separately: a proven implementation deviation; behavior not yet verified; and a contradiction, omission, or consequential ambiguity in the governing contract. It may identify the appropriate destination—Build, further authorized verification, or upstream Discovery/Consensus—but may not perform those phases automatically.

**Enforcement:** Finding categories and affected IDs are explicit. Authority issues retain exact source wording/references and do not mutate the Agreement. Several categories may coexist in one assessment.

**Acceptance:**
- **ORC-060-A1:** An inconsistent Agreement yields an authority issue, not a guessed new rule.
- **ORC-060-A2:** A missing runtime observation is not mislabeled as proof that the implementation is defective.

**Verification cases:** [AUD-03](VERIFICATION.md#aud-03), [AUD-04](VERIFICATION.md#aud-04), [AUD-09](VERIFICATION.md#aud-09), [CLI-03](VERIFICATION.md#cli-03). **Applicability:** core.

<a id="orc-061"></a>

### ORC-061 — Derive a conservative but achievable conformance verdict

PASS requires complete justified coverage, all required observations supported or legitimately not applicable, no demonstrated required violation, no unresolved material authority issue, and valid evidence/target identity. CHANGES_REQUIRED applies when any required violation is substantiated. Otherwise unresolved mandatory verification or authority yields BLOCKED. Operational failure yields no conformance verdict.

**Enforcement:** The application derives the verdict from validated coverage/findings and run integrity, not free-form “looks good” text. Preserve coexisting blockers even when CHANGES_REQUIRED is the primary verdict.

**Acceptance:**
- **ORC-061-A1:** The known-good target with sufficient evidence can pass; an always-blocking implementation fails acceptance.
- **ORC-061-A2:** A known-bad target, missing coverage, or fabricated receipt cannot pass; a transport crash is not a conformance outcome.

**Verification cases:** [AUD-01](VERIFICATION.md#aud-01), [AUD-02](VERIFICATION.md#aud-02), [AUD-03](VERIFICATION.md#aud-03), [META-04](VERIFICATION.md#meta-04), [ACX-10](VERIFICATION.md#acx-10). **Applicability:** core.

<a id="orc-062"></a>

### ORC-062 — Close corrections without expanding the contract

A re-audit SHALL address previous required findings, validate the new exact target, and check plausible correction regressions. It may withdraw a prior mistaken finding with a reason. It SHALL NOT invent extra product requirements or repeat the entire investigation merely to prolong review.

**Enforcement:** Publish a new immutable assessment linked to prior findings and targets. Required and optional observations remain separate. A new required finding must be a supported existing-contract violation or a correction regression.

**Acceptance:**
- **ORC-062-A1:** A correct repair closes the original finding and can pass with optional advice left non-blocking.
- **ORC-062-A2:** A repair introducing another required-behavior defect receives CHANGES_REQUIRED rather than inheriting closure.

**Verification cases:** [AUD-10](VERIFICATION.md#aud-10), [AUD-11](VERIFICATION.md#aud-11), [E2E-02](VERIFICATION.md#e2e-02). **Applicability:** core.

<a id="orc-063"></a>

### ORC-063 — Keep Audit read-only with respect to authority and product

Audit SHALL not fix product code, edit the Agreement, merge changes, or alter the registered target. Checks may create outputs in run-owned scratch. Any experimental patch must be identified as probe material and cannot be represented as the original target passing.

**Enforcement:** Separate immutable target material from mutable check workspaces and retained evidence. Validate protected input identity and write ownership; a detected change invalidates the affected claim.

**Acceptance:**
- **ORC-063-A1:** A check can generate normal temporary outputs without touching the source repository.
- **ORC-063-A2:** An auditor fixing the defect in scratch cannot claim that the unfixed registered target conforms.

**Verification cases:** [AUD-12](VERIFICATION.md#aud-12), [ID-05](VERIFICATION.md#id-05), [ISO-05](VERIFICATION.md#iso-05), [ACX-04](VERIFICATION.md#acx-04). **Applicability:** core.

## 6.9. Artifacts, integrity, lineage, and durable operation

<a id="orc-064"></a>

### ORC-064 — Publish minimal self-describing public contracts

Every finalized public artifact SHALL identify its type/schema, run, project/effort/cohort where applicable, producer version, input identities/digests, output inventory/digests, provenance, and semantic outcome. Downstream phases consume these contracts without the producer’s private runtime or working database.

**Enforcement:** Version envelope and payload schemas explicitly. Define exact byte/canonicalization rules and reject duplicate authoritative keys, missing required fields, and unsupported formats. Avoid self-referential digest definitions.

**Acceptance:**
- **ORC-064-A1:** Cold contract consumers work with only required public artifacts.
- **ORC-064-A2:** Malformed, truncated, missing-component, and wrong-schema artifacts fail precisely instead of filling authoritative defaults.

**Verification cases:** [ART-02](VERIFICATION.md#art-02), [ART-10](VERIFICATION.md#art-10), [ART-11](VERIFICATION.md#art-11), [ARCH-03](VERIFICATION.md#arch-03). **Applicability:** core.

<a id="orc-065"></a>

### ORC-065 — Make finalization a committed boundary

Files that look finished are not finalized output. A consumer SHALL see either an incomplete operation or a complete validated public artifact. Required payloads and finalization metadata must form one consistent committed result, including under concurrent readers and process interruption.

**Enforcement:** Stage and validate output, then publish through a recoverable commit boundary. Prevent conflicting finalizers and verify required bytes on consumption. Finalization commits do not start downstream phases.

**Acceptance:**
- **ORC-065-A1:** Killing before commit leaves no consumable partial result.
- **ORC-065-A2:** Killing after commit but before acknowledgement leaves one recoverable valid result, not a second publication on retry.

**Verification cases:** [ART-03](VERIFICATION.md#art-03), [ART-07](VERIFICATION.md#art-07), [ART-08](VERIFICATION.md#art-08), [REC-02](VERIFICATION.md#rec-02), [REC-03](VERIFICATION.md#rec-03), [REC-04](VERIFICATION.md#rec-04). **Applicability:** core.

<a id="orc-066"></a>

### ORC-066 — Preserve finalized bytes and truthful invalidation history

A finalized payload and its declared parents SHALL not be edited by inspection, a later run, a correction, or a discovered problem. Later invalidation or contamination notices are separate attributed records. Checksums provide byte integrity, not proof of truth or authentication against a user who can replace the entire store.

**Enforcement:** Use immutable publication semantics and per-component verification. Refuse same-identity conflicting content. Apply known invalidation notices when evaluating current eligibility without rewriting the original artifact.

**Acceptance:**
- **ORC-066-A1:** One-byte payload, condition, or parent-reference edits are detected.
- **ORC-066-A2:** A later known-contamination notice keeps the original evidence inspectable while preventing it silently serving as eligible current input.

**Verification cases:** [ART-01](VERIFICATION.md#art-01), [ART-04](VERIFICATION.md#art-04), [ART-08](VERIFICATION.md#art-08), [ACX-03](VERIFICATION.md#acx-03). **Applicability:** core.

<a id="orc-067"></a>

### ORC-067 — Derive cross-phase lineage from authoritative records

Agreement, adoption, implementation, and audit records SHALL name their exact parents. Status and lineage reconstruct these relationships mechanically, without asking an LLM. Any global index is rebuildable convenience; it cannot override artifact authority or silently pick the newest result.

**Enforcement:** Traverse validated manifests with cycle/conflict detection. Keep active-run state local to its operation; loss of an index must not erase durable runs or invent success. Missing retained parents remain explicit gaps.

**Acceptance:**
- **ORC-067-A1:** Deleting the optional index preserves reconstructable finalized lineage and discoverable active-run state.
- **ORC-067-A2:** Cyclic, dangling, and cross-effort references terminate with a diagnostic rather than retargeting the chain.

**Verification cases:** [ART-05](VERIFICATION.md#art-05), [ART-06](VERIFICATION.md#art-06), [ID-02](VERIFICATION.md#id-02), [E2E-06](VERIFICATION.md#e2e-06). **Applicability:** core.

<a id="orc-068"></a>

### ORC-068 — Bind consumption to retained bytes, not mutable locations

Before using a parent, Orchestrate SHALL verify the selected public identity and retain or resolve immutable required bytes. Subsequent changes at an original source path do not retarget the captured input. Changes to the actual retained authoritative bytes invalidate their use. Source locators are not source identity.

**Enforcement:** Verify at intake and at relevant publication/consumption boundaries. For frozen source snapshots, live checkout drift may be reported but does not silently replace or invalidate the unchanged captured snapshot.

**Acceptance:**
- **ORC-068-A1:** Mutating a consumed artifact’s authoritative bytes fails integrity verification.
- **ORC-068-A2:** Moving HEAD or an original document after valid capture leaves the run on its recorded retained version rather than silently adopting the new one.

**Verification cases:** [CON-14](VERIFICATION.md#con-14), [ART-01](VERIFICATION.md#art-01), [AUD-05](VERIFICATION.md#aud-05), [SRC-01](VERIFICATION.md#src-01). **Applicability:** core.

<a id="orc-069"></a>

### ORC-069 — Make artifacts portable enough for retained inspection

Required public content and retained target material SHALL be inspectable if the original checkout or private working state is unavailable. Internal references must resolve within retained bundles or identify external dependencies honestly. Relocation changes locators, not identity or vote count.

**Enforcement:** Use artifact-relative logical paths and stable IDs. Retain exact public parents/material required for the selected chain, without copying credentials or entire unrelated homes. Private evidence not required downstream can remain in the originating run.

**Acceptance:**
- **ORC-069-A1:** Move a valid bundle tree to a test location and reconstruct its supported lineage.
- **ORC-069-A2:** A truly missing external receipt is reported unavailable, not synthesized or replaced by another run’s receipt.

**Verification cases:** [ID-08](VERIFICATION.md#id-08), [ART-05](VERIFICATION.md#art-05), [ART-11](VERIFICATION.md#art-11). **Applicability:** core.

<a id="orc-070"></a>

### ORC-070 — Validate paths and ownership before effects

Labels and artifact references SHALL not escape the configured storage root or target another run through traversal, absolute-path injection, aliases, or unsafe links. Supported spaces and Unicode must not corrupt identity or command arguments.

**Enforcement:** Validate logical paths separately from OS paths, resolve canonical containment where relevant, and avoid shell interpolation for data arguments. Input discovery must not recursively collect every similarly named file as a run.

**Acceptance:**
- **ORC-070-A1:** Traversal and unsafe absolute references are rejected without outside writes.
- **ORC-070-A2:** Supported non-ASCII names and spaces round-trip through invocation, storage, and inspection.

**Verification cases:** [ID-03](VERIFICATION.md#id-03), [ID-04](VERIFICATION.md#id-04), [ISO-07](VERIFICATION.md#iso-07), [PRO-07](VERIFICATION.md#pro-07). **Applicability:** core.

<a id="orc-071"></a>

### ORC-071 — Handle duplicate operations without duplicating effects

A retried local mutation with the same operation identity and payload SHALL recover its known result or continue its incomplete local work safely. Reusing an operation identity for different content is a conflict. Generated operation identifiers are machinery, not user bookkeeping.

**Enforcement:** Retain operation intent/result at the chosen durable boundary. Prevent double publication and duplicate vote/receipt creation. Do not extend this promise to exactly-once remote inference or arbitrary external side effects.

**Acceptance:**
- **ORC-071-A1:** An identical acknowledged or lost-acknowledgement retry returns the same committed result.
- **ORC-071-A2:** A different payload under the same identity fails without overwrite or a second external effect.

**Verification cases:** [REC-04](VERIFICATION.md#rec-04), [REC-08](VERIFICATION.md#rec-08), [ART-08](VERIFICATION.md#art-08), [PRO-03](VERIFICATION.md#pro-03). **Applicability:** core.

<a id="orc-072"></a>

### ORC-072 — Recover unknown external outcomes conservatively

If a model call or check may have executed before interruption, its outcome SHALL remain unknown until independently recovered. Restart must not assume it never ran, fabricate success, or silently re-execute an uncertain effect. Saved trustworthy evidence remains available.

**Enforcement:** Reserve attempt identity before dispatch, record available provider/check identity, and retain results before finalization. Resume can recover a known outcome or request an explicit new attempt under the authorization policy.

**Acceptance:**
- **ORC-072-A1:** A helper performs an effect and withholds acknowledgement; after killing the controller, recovery reports unknown rather than repeating it.
- **ORC-072-A2:** A committed result discovered after restart is reused without another model call.

**Verification cases:** [REC-04](VERIFICATION.md#rec-04), [REC-05](VERIFICATION.md#rec-05), [REC-10](VERIFICATION.md#rec-10), [PRO-04](VERIFICATION.md#pro-04). **Applicability:** core.

<a id="orc-073"></a>

### ORC-073 — Cancel with bounded, attributable cleanup

Cancellation SHALL stop admission of new work, attempt bounded termination of owned processes, preserve known results, and report any unresolved running descendants or external outcome. The tool must not signal unrelated processes or claim complete cleanup it cannot establish.

**Enforcement:** Track process ownership, enforce deadlines, reject late/stale results from canceled or superseded attempts, and separate graceful-cancel evidence from force-kill recovery. Platform-specific process behavior remains an adapter concern.

**Acceptance:**
- **ORC-073-A1:** A hung or termination-resistant child cannot cause an unbounded wait.
- **ORC-073-A2:** Late output cannot finalize a newer attempt; unrelated sentinel processes remain alive.

**Verification cases:** [REC-06](VERIFICATION.md#rec-06), [REC-07](VERIFICATION.md#rec-07), [REC-09](VERIFICATION.md#rec-09), [PRO-05](VERIFICATION.md#pro-05). **Applicability:** core.

<a id="orc-074"></a>

### ORC-074 — Keep concurrent local operations safe without requiring a scheduler

Separate efforts/runs may coexist. Concurrent creation, publication, or readers SHALL not corrupt records. Conflicting writes within one run must serialize or reject safely. Parallel model execution is desirable but optional; unsupported parallel dispatch fails before launch rather than sharing mutable scratch.

**Enforcement:** Use run-local ownership and atomic/locked publication as appropriate. Avoid global mutable test or runtime configuration for per-run context. No task scheduler is required solely to support concurrent safe storage.

**Acceptance:**
- **ORC-074-A1:** Two simultaneous run creations have unique identities and isolated outputs.
- **ORC-074-A2:** Two finalizers cannot both publish conflicting successful results for one artifact.

**Verification cases:** [ID-07](VERIFICATION.md#id-07), [ISO-06](VERIFICATION.md#iso-06), [ART-07](VERIFICATION.md#art-07), [ART-08](VERIFICATION.md#art-08), [REC-11](VERIFICATION.md#rec-11), [E2E-06](VERIFICATION.md#e2e-06). **Applicability:** core.

<a id="orc-075"></a>

### ORC-075 — Keep inspection read-only and unambiguous

Help, version, guide, inspect, status, and lineage SHALL require no model call or provider credentials and shall not create runs, revise artifacts, adopt Agreements, clean resources, or edit the target. Inspection identifies the selected effort/chain and actual limitations rather than guessing a current stage.

**Enforcement:** Use read-only data paths; make index rebuild or repair a separate explicit operation. Return missing/ambiguous selectors as diagnostics, not reasons for semantic work.

**Acceptance:**
- **ORC-075-A1:** Fresh-install help/guide works offline.
- **ORC-075-A2:** Repeated inspection preserves finalized bytes and has zero recorded provider invocations.

**Verification cases:** [CLI-01](VERIFICATION.md#cli-01), [ART-04](VERIFICATION.md#art-04), [ART-05](VERIFICATION.md#art-05), [ID-02](VERIFICATION.md#id-02). **Applicability:** core.

## 6.10. Execution limits, provenance, and truthful reporting

<a id="orc-076"></a>

### ORC-076 — Use explicit provider identity and capability selection

Managed semantic calls SHALL use the selected configured provider/model/effort and permitted capabilities. The host operating a skill is not necessarily the execution provider. Unavailable models, authentication, or required boundaries cause a clear failure; there is no silent substitution.

**Enforcement:** Resolve and record effective configuration before dispatch, validate required capabilities, and bind returned protocol events to the owning attempt. Ship only adapters that are actually implemented and qualified.

**Acceptance:**
- **ORC-076-A1:** Cursor as operator with a separately configured managed provider records both identities correctly.
- **ORC-076-A2:** Missing authentication or a model mismatch does not fall back to a different model or produce an apparent engineering conclusion.

**Verification cases:** [PRO-03](VERIFICATION.md#pro-03), [PRO-04](VERIFICATION.md#pro-04), [META-01](VERIFICATION.md#meta-01), [META-02](VERIFICATION.md#meta-02). **Applicability:** core.

<a id="orc-077"></a>

### ORC-077 — Bound execution without silently discarding meaning

Runs SHALL have recorded finite limits for managed calls/attempts, elapsed time, input/output bytes, and diagnostic retention. At-limit inputs must remain complete; oversize required material causes a clear limitation/error rather than silent truncation or omission. Token budgets are described as admission or hard limits according to actual enforcement.

**Enforcement:** Validate source packets before invocation, bound process capture, terminate on enforced runtime limits, and separate truncated diagnostics from complete required artifacts. Effective limits and all consumed attempts remain visible.

**Acceptance:**
- **ORC-077-A1:** Over-limit material does not yield a misleading complete Agreement/Audit.
- **ORC-077-A2:** A call admitted under a token budget cannot be advertised as unable to exceed that budget when the provider does not enforce such a cap.

**Verification cases:** [PRO-02](VERIFICATION.md#pro-02), [PRO-05](VERIFICATION.md#pro-05), [ENV-03](VERIFICATION.md#env-03), [META-06](VERIFICATION.md#meta-06). **Applicability:** core.

<a id="orc-078"></a>

### ORC-078 — Retain attempts and enforce the declared repair policy

The default managed-call policy is one attempt; a bounded validation-repair allowance may be explicitly configured before the run. Every attempt counts toward the total allowance and preserves sufficient input/result/error provenance. Valid semantic results are not retried automatically for a different answer.

**Enforcement:** Separate transport failures, schema rejection, semantic inconclusiveness, and retry decisions. Unexpected tools, malformed output, duplicate events, and wrong invocation identity cannot directly publish final state.

**Acceptance:**
- **ORC-078-A1:** A three-attempt test policy records two rejections and the third accepted response accurately.
- **ORC-078-A2:** Exhaustion leaves an honest failed/incomplete outcome, not a fabricated final artifact or hidden extra call.

**Verification cases:** [PRO-01](VERIFICATION.md#pro-01), [PRO-02](VERIFICATION.md#pro-02), [PRO-03](VERIFICATION.md#pro-03), [PRO-10](VERIFICATION.md#pro-10), [CON-11](VERIFICATION.md#con-11). **Applicability:** core.

<a id="orc-079"></a>

### ORC-079 — Capture useful environment identity automatically and honestly

Run outputs/frontmatter SHALL include run/phase, host/source label, execution provider when applicable, requested and actually observed model/effort, product/guide identity, request/source baseline, and timestamps. Unknown observed identity remains unknown; operator-reported configuration is labeled as such.

**Enforcement:** Generate frontmatter from recorded provenance, not independent hand-edited text. Record additional OS/tool/runtime details when they materially affect a check or qualification. Do not infer the model from the host name.

**Acceptance:**
- **ORC-079-A1:** Codex, Claude, and Cursor runs are distinguishable without renaming “run 1.”
- **ORC-079-A2:** Missing host-reported effort remains unknown even when the requested profile says high; manifest and Markdown agree.

**Verification cases:** [META-01](VERIFICATION.md#meta-01), [META-02](VERIFICATION.md#meta-02), [META-03](VERIFICATION.md#meta-03), [CLI-12](VERIFICATION.md#cli-12). **Applicability:** core.

<a id="orc-080"></a>

### ORC-080 — Protect credentials and local private material

Orchestrate SHALL not place authentication tokens, environment secrets, or unrelated private files into model packets or ordinary logs. Local-first storage does not imply offline model execution: only selected bounded material may be sent through the explicitly selected provider. No unrelated telemetry or cloud synchronization is required.

**Enforcement:** Use scoped environment/config access, minimal permitted source lists, known-secret redaction for logs, and restrictive local storage where supported. Record redaction/omission when it affects evidence; do not claim arbitrary secret detection is perfect.

**Acceptance:**
- **ORC-080-A1:** Secret sentinels in known credential fields do not appear in prompts, manifests, or normal diagnostics while authorized authentication still works.
- **ORC-080-A2:** Read-only and offline tests use no network or real credentials.

**Verification cases:** [PRO-09](VERIFICATION.md#pro-09), [ISO-02](VERIFICATION.md#iso-02), [ENV-05](VERIFICATION.md#env-05). **Applicability:** core.

<a id="orc-081"></a>

### ORC-081 — Pin the operating contract of resumable work

A run SHALL record the runtime, guide, role prompt, policy, and public schema identities used. Resume must not silently change these semantics after an update. Exact-compatible continuation may be supported explicitly; otherwise stop and require an explicit replacement/new run while preserving old data.

**Enforcement:** Retain required guide/prompt content or stable digests with accessible release identity and compare compatibility at resume. No obligation exists to migrate prototype runs or indefinitely support old Orchestrate runtimes.

**Acceptance:**
- **ORC-081-A1:** An incompatible installed update cannot continue an old run under an unrecorded new guide.
- **ORC-081-A2:** Finalized artifacts remain inspectable under supported schemas without re-running their models.

**Verification cases:** [CLI-12](VERIFICATION.md#cli-12), [ART-02](VERIFICATION.md#art-02), [META-03](VERIFICATION.md#meta-03). **Applicability:** core.

<a id="orc-082"></a>

### ORC-082 — Present decision-oriented results with evidence limits

Each phase SHALL lead with its actual answer, scope, readiness, blocking issue if any, and artifact location. Detailed provenance stays available without burying the recommendation. Reports must distinguish observed success, actor claims, unverified behavior, operational failure, and deferred capabilities.

**Enforcement:** Render readable views from validated records and keep required unresolved information visible. No single confidence/coverage score summarizes machinery, host compatibility, and semantic quality.

**Acceptance:**
- **ORC-082-A1:** A completed Consensus returns its useful decision and dissent rather than only hashes.
- **ORC-082-A2:** An interrupted trial or unsupported host remains visible in reports instead of disappearing from a green total.

**Verification cases:** [META-04](VERIFICATION.md#meta-04), [META-05](VERIFICATION.md#meta-05), [META-06](VERIFICATION.md#meta-06), [DISC-01](VERIFICATION.md#disc-01). **Applicability:** core.

## 6.11. Skills, guides, packaging, and phase implementation boundaries

<a id="orc-083"></a>

### ORC-083 — Install thin, explicit phase skills

Provide orchestrate-discovery, orchestrate-consensus, and orchestrate-audit as separate explicit entrypoints into one product. Each has a fixed phase-to-guide mapping and no intent classifier. Host-specific metadata may differ; canonical semantic instruction bodies are maintained once. Build skill installation waits for real Build execution.

**Enforcement:** Generate/copy wrappers from the release’s canonical skill sources, using supported explicit-invocation policies for each host. Verify installation discovery in each supported host; do not equate a skill file with a permission boundary.

**Acceptance:**
- **ORC-083-A1:** Explicit skill invocation loads only the corresponding operator guide.
- **ORC-083-A2:** Vague requests do not implicitly activate an explicit-only skill in qualified hosts; absent inputs cause a diagnostic, not another phase.

**Verification cases:** [CLI-08](VERIFICATION.md#cli-08), [CLI-09](VERIFICATION.md#cli-09), [CLI-10](VERIFICATION.md#cli-10), [CLI-11](VERIFICATION.md#cli-11). **Applicability:** core.

<a id="orc-084"></a>

### ORC-084 — Make guide a read-only bundled instruction reader

orchestrate guide <phase> SHALL return the installed phase’s operator instructions, version/digest identity, prerequisites, permitted operations, output expectations, and stop conditions. It does not create a run, call a model, or mutate state. With no phase, it lists available guides.

**Enforcement:** Bundle or embed phase guides with the executable. Unsupported phases fail clearly. No stale fallback guide from another installation or source checkout is used when the installed guide is missing.

**Acceptance:**
- **ORC-084-A1:** The packaged guide works without the source checkout, credentials, or existing project.
- **ORC-084-A2:** A missing/incompatible runtime cannot be replaced by remembered workflow instructions.

**Verification cases:** [CLI-01](VERIFICATION.md#cli-01), [CLI-05](VERIFICATION.md#cli-05), [CLI-07](VERIFICATION.md#cli-07), [ARCH-05](VERIFICATION.md#arch-05). **Applicability:** core.

<a id="orc-085"></a>

### ORC-085 — Separate operator instructions from managed-role prompts

The Discovery host may perform the investigation through public commands. Consensus/Audit operator guides tell the host how to prepare, invoke, inspect, and present their managed phase; internal semantic-role prompts are not an invitation for the host to duplicate or replace that phase’s result.

**Enforcement:** Keep phase-owned operator guides and role prompts separately versioned within the same release. Provide only relevant instructions to each role. Preserve returned artifacts rather than rewriting a failed reconciliation in the parent conversation.

**Acceptance:**
- **ORC-085-A1:** A host invoking Consensus does not also supply its preferred diagnosis or perform an unrecorded replacement synthesis.
- **ORC-085-A2:** A role receives its required semantic instructions without unrelated phase workflows or peer private state.

**Verification cases:** [CLI-11](VERIFICATION.md#cli-11), [CON-15](VERIFICATION.md#con-15), [META-01](VERIFICATION.md#meta-01). **Applicability:** core.

<a id="orc-086"></a>

### ORC-086 — Install and update without damaging the user’s environment

One release SHALL install the executable and selected host skill collection without modifying target repositories, unrelated skills, or global host preferences. Reinstallation is idempotent for owned unchanged files; modified/conflicting files receive a diagnostic or explicit replacement choice. Resolve command identity before use.

**Enforcement:** Track installation-owned resources and runtime identity; detect collisions and stale wrappers. Test from a separate installation prefix with source access removed. No Python/uv or marketplace dependency is required for the core executable.

**Acceptance:**
- **ORC-086-A1:** Repeated install/update/remove preserves an unrelated sentinel skill and user modifications.
- **ORC-086-A2:** An earlier conflicting executable on PATH cannot silently receive Orchestrate operations.

**Verification cases:** [CLI-05](VERIFICATION.md#cli-05), [CLI-06](VERIFICATION.md#cli-06), [CLI-07](VERIFICATION.md#cli-07), [ENV-01](VERIFICATION.md#env-01). **Applicability:** core.

<a id="orc-087"></a>

### ORC-087 — Use public contracts instead of phase internals

Phase implementations SHALL consume upstream finalized public contracts, not another phase’s private database, application services, or unfinished state. Rust crate boundaries are the preferred enforcement mechanism. Contract consumers must work without initializing the producer runtime.

**Enforcement:** Use explicit allowed dependencies, private implementation modules, narrow contract surfaces, and dependency-policy tests. A contract module is not permission to expose the entire producer domain model.

**Acceptance:**
- **ORC-087-A1:** A forbidden internal import fails; the public contract consumer still compiles and functions.
- **ORC-087-A2:** Removing an upstream working database does not break downstream work when required public artifacts remain.

**Verification cases:** [ARCH-01](VERIFICATION.md#arch-01), [ARCH-02](VERIFICATION.md#arch-02), [ARCH-03](VERIFICATION.md#arch-03), [ART-11](VERIFICATION.md#art-11). **Applicability:** core.

<a id="orc-088"></a>

### ORC-088 — Share mechanics only when the behavior is genuinely common

Hashing, file publication, identity, bounded execution, and provider transport may be shared. Discovery judgment, consensus endorsement, and Audit conformance SHALL not be forced into a universal semantic graph or shared scoring engine. Crate organization does not by itself prevent runtime access or an unauthorized dependency change.

**Enforcement:** Test dependency declarations, aliases, features, and re-exports; independently validate shared encoding and path behavior against fixed vectors. Keep phase policies and permission checks at their real boundaries.

**Acceptance:**
- **ORC-088-A1:** A forbidden Cargo dependency edge is detected even if someone makes the formerly forbidden import public.
- **ORC-088-A2:** An intentionally broken shared digest/path helper fails independent consumer vectors instead of passing shared round-trip tests.

**Verification cases:** [ARCH-02](VERIFICATION.md#arch-02), [ARCH-04](VERIFICATION.md#arch-04), [ISO-04](VERIFICATION.md#iso-04). **Applicability:** core.

## 6.12. Verification and delivery acceptance

<a id="orc-089"></a>

### ORC-089 — Test machinery through real public boundaries

The suite SHALL combine deterministic rule tests with real-CLI integration/E2E using actual files, Git data, subprocesses, parsing, persistence, finalization, and restart. Scripted agents/providers control behavior but may not write accepted artifacts or bypass production validators/finalizers.

**Enforcement:** Use both focused controlled adapters and a protocol-level subprocess emulator. The scripted Discovery investigator performs the same public operations as a host. The initial complete journey uses external Build registration.

**Acceptance:**
- **ORC-089-A1:** A known-good scripted journey crosses all real stage boundaries.
- **ORC-089-A2:** A malformed or malicious scripted result fails the real parser/publication path rather than being pre-rejected solely inside a mock.

**Verification cases:** [PRO-01](VERIFICATION.md#pro-01), [E2E-01](VERIFICATION.md#e2e-01), [E2E-02](VERIFICATION.md#e2e-02), [BLD-06](VERIFICATION.md#bld-06), [E2E-04](VERIFICATION.md#e2e-04). **Applicability:** core.

<a id="orc-090"></a>

### ORC-090 — Give every critical guarantee an adversarial test

Acceptance SHALL trace each required behavior to positive and negative cases at its enforcement boundary, including meaningful interruption, concurrency, or boundary cases. High line coverage or an impressive test count is not a substitute.

**Enforcement:** Maintain requirement-to-case mapping and execution status. Use independent expected values, fault injection, generated state sequences, and selected mutations of critical guards. No mandatory case may disappear by filtering or silently skipping it.

**Acceptance:**
- **ORC-090-A1:** Disabling artifact verification, accepting drafts, unioning supporters, or using moving HEAD causes a test failure.
- **ORC-090-A2:** The report identifies planned, passed, failed, blocked, unsupported, and not-run cases separately.

**Verification cases:** [REC-12](VERIFICATION.md#rec-12), [ARCH-04](VERIFICATION.md#arch-04), [META-05](VERIFICATION.md#meta-05), [META-06](VERIFICATION.md#meta-06), [ACX-11](VERIFICATION.md#acx-11). **Applicability:** core.

<a id="orc-091"></a>

### ORC-091 — Isolate test infrastructure from real work and answer keys

Automated trials SHALL use disposable roots, synthetic repositories/data, child-local environment settings, and examiner-only answer keys. Offline tests have no model credentials/network. Live tests are explicitly authorized, bounded, and use only the selected provider’s required credentials.

**Enforcement:** The harness cannot write the real orchestration store, source projects, or installed user skills. Validate known-good and faulty reference implementations independently before using them as grading oracles.

**Acceptance:**
- **ORC-091-A1:** Offline lanes fail loudly if a provider invocation or external request is attempted.
- **ORC-091-A2:** The agent cannot receive hidden solutions through source history, sibling artifacts, test configuration, or grading keys within its declared boundary.

**Verification cases:** [ISO-02](VERIFICATION.md#iso-02), [SRC-08](VERIFICATION.md#src-08), [ENV-05](VERIFICATION.md#env-05), [PRO-09](VERIFICATION.md#pro-09), [ACX-11](VERIFICATION.md#acx-11). **Applicability:** core.

<a id="orc-092"></a>

### ORC-092 — Qualify installed hosts, not just prompt text

Low-strength live E2E smoke tests SHALL exercise the actual installed skill and executable in each host/surface claimed supported. Record the host version, requested/observed model profile, guide loading, actual public operations, result, and stopping behavior. Manual graphical-host checks are labeled manual.

**Enforcement:** Use fresh actual host sessions and explicit skills. Pasting SKILL.md into a model prompt, or testing a CLI instead of an editor surface, does not qualify the untested installation path.

**Acceptance:**
- **ORC-092-A1:** The supported Codex, Claude Code, and Cursor surfaces actually discover the phase skills and load the correct guide.
- **ORC-092-A2:** A skipped live test or unavailable credential is reported blocked, not counted as host compatibility.

**Verification cases:** [CLI-09](VERIFICATION.md#cli-09), [CLI-10](VERIFICATION.md#cli-10), [E2E-05](VERIFICATION.md#e2e-05), [META-06](VERIFICATION.md#meta-06). **Applicability:** core.

<a id="orc-093"></a>

### ORC-093 — Test semantic fidelity against independent rubrics

Live semantic evaluations SHALL judge Discovery usefulness, Consensus fidelity, and Audit correctness using original inputs and independently authored expected behaviors. Cheap-model smoke success does not qualify another profile’s reasoning. Retain every trial and allowed retry; do not rerun until a preferred result appears.

**Enforcement:** Use curated positive/negative fixtures, permitted alternatives, and predeclared repetitions. The initial small qualification corpus runs five fresh trials per selected scenario/profile; this is a diagnostic minimum, not a statistical reliability claim. LLM graders may assist only after their own controls are validated.

**Acceptance:**
- **ORC-093-A1:** Known-good and known-bad semantic outputs are discriminated by the grading rubric.
- **ORC-093-A2:** Order/wording/condition perturbations cannot be excused by accepting the model’s own extracted account as the answer key.

**Verification cases:** [CON-01](VERIFICATION.md#con-01), [CON-02](VERIFICATION.md#con-02), [CON-05](VERIFICATION.md#con-05), [CON-12](VERIFICATION.md#con-12), [AUD-01](VERIFICATION.md#aud-01), [AUD-02](VERIFICATION.md#aud-02), [E2E-05](VERIFICATION.md#e2e-05), [ACX-11](VERIFICATION.md#acx-11). **Applicability:** core.

<a id="orc-094"></a>

### ORC-094 — Report qualification honestly on real target environments

Initial delivery SHALL be qualified natively on the user’s macOS environment. Linux or Windows support, enforced access isolation, multi-assessor Audit, and parallel model dispatch are qualified only if shipped and claimed. Rust compilation or Linux CI alone is not evidence of native Mac behavior.

**Enforcement:** Publish a support/capability matrix with actual execution records and limits. All core machinery cases and selected must-pass semantic fixtures must run; no observed critical false acceptance, integrity failure, or claimed-isolation breach may remain in the accepted qualification set.

**Acceptance:**
- **ORC-094-A1:** Native path/process/cancellation cases pass without a hidden platform workaround.
- **ORC-094-A2:** Unexecuted hosts, optional capabilities, and deferred internal Build appear explicitly outside the qualified result.

**Verification cases:** [ENV-01](VERIFICATION.md#env-01), [ENV-02](VERIFICATION.md#env-02), [ENV-03](VERIFICATION.md#env-03), [ENV-04](VERIFICATION.md#env-04), [META-06](VERIFICATION.md#meta-06), [ACX-11](VERIFICATION.md#acx-11). **Applicability:** core.


<a id="section-7"></a>

# 7. Public artifact contents and handoff rules

The following are semantic contracts, not fixed filenames or a complete serialization schema. A technical implementation must choose and version a wire format that contains these meanings. Complete the wire schemas before implementing consumers, and test with independent valid/invalid fixtures.

## 7.1 Common envelope

A finalized public envelope identifies artifact kind/version; artifact and run ID; project/effort/context/cohort IDs where applicable; producing product version; operation outcome; creation/finalization timestamps; exact consumed parent identities and digests; payload inventory with byte digests; and relevant actor/model/guide provenance. Its finalization proof is externally verifiable under the chosen publication contract.

Use separate identities for a request effort and model reasoning effort. A frontmatter key called `source` may display the host name, but it must not replace the separately structured repository/source-baseline identity.

A digest authenticates neither the model’s interpretation nor the actor. Avoid a digest field that recursively includes its own value. Canonical record encoding and raw payload-byte digests are distinct and must be specified and tested as such.

## 7.2 Discovery public opinion

The substantive recommendation is retained in full, not reduced to a pointer summary. The public contract carries its scope, adopted decisions and behavioral requirements, key findings, qualifications/assumptions, meaningful alternatives, acceptance criteria, planned/performed verification, unresolved limitations, and compact evidence/source references. Proposed, accepted, and rejected ideas are distinguished.

The raw evidence collection can remain private to the run. Consensus receives the stated opinion and enough public context to interpret it. It does not receive every log or have to reconstruct the investigation graph.

## 7.3 Consensus comparison and Agreement

A comparison records all intended slots, eligible/received opinion identities, position mappings, selected mandatory package, common supporters, explicit opponents, useful point-level agreement, dissent, limitations, and selection rationale.

A build-ready candidate additionally contains the complete governing constraints and adopted contract, stable requirement IDs, acceptance/verification expectations, all essential conditions, exclusions, and bounded implementation latitude. The contract distinguishes what comes from explicit user authority from what has opinion consensus. No source gains extra votes by providing more files.

Readable normative sections are rendered from the adopted records. Optional observations and rationale may be prose, but are clearly non-normative and cannot contain an untracked instruction that Build is expected to obey. The model must still write clear complete adopted statements; IDs and a rigid renderer do not make ambiguous language precise.

## 7.4 Adoption record

Adoption records the exact eligible Agreement ID, revision and digest, the attributable user authorization, its time, and any user-declared execution scope. It is separate from the candidate so final content does not change after being hashed. Conditions remain conditions; approval does not claim they were verified.

No overriding arbitrary minority recommendation, bypassing missing participation, or resolving unknown product intent through a “force” flag is part of this version. A user change is captured upstream and yields an explicitly new contract.

## 7.5 Implementation handoff

The initial handoff records external production explicitly. It identifies the adopted Agreement/digest, project, selected baseline, actual resulting commit/tree/material inventory, declared producer/process, submitted/partial/blocked state, attributable evidence, known deviations, and unresolved questions.

The application verifies the exact target exists and was captured; it does not pretend that inspecting code proves which conversation or agreement historically produced it. An external builder’s history is declared unless supported by independent records. Audit establishes present conformance, not historical obedience.

A no-change implementation may use the original baseline commit. For a future integrated builder, record authority before work and still publish the same boundary when work is submitted. No task database, routing system, or worktree strategy is prescribed here.

## 7.6 Audit artifact

The report identifies the exact Agreement/adoption/implementation pair, assessor context and profile, coverage row for every required ID, observed/reused evidence, findings, authority issues, limitations, primary verdict, and any prior audit/target relationship.

A required finding includes its contract ID, concrete supporting evidence, consequence, and bounded correction. An observation about taste or optional hardening is not a required finding. The report explicitly separates “not verified” from “demonstrated incorrect.”

A retained test receipt identifies its original target, relevant environment, command and working source, timestamps/duration, outcome classification, exit status where applicable, and bounded output/assertion references. New use of the receipt links it rather than rewriting it.

<a id="section-8"></a>

# 8. Lifecycle, readiness, and failure behavior

## 8.1 Local execution is not a single linear global status

An effort may have several Discovery runs, comparison revisions, implementation targets, and audits. Its state is the collection of exact attributed records. A display may summarize a deliberately selected chain but must identify that selection.

Before finalization, a run’s working state may change. After finalization, its public output is immutable. Working scratch and diagnostics do not acquire artifact authority merely by living beside it. A later invalidation notice changes current eligibility, not historical bytes.

| Boundary | Preconditions | Possible useful outcome | What cannot follow automatically |
|---|---|---|---|
| Begin Discovery | Selected effort/cohort, captured request, prepared source, declared capabilities | Active investigation | No sibling discovery or automatic Consensus |
| Finalize Discovery | Current complete evidence case and no blocking intent gap | Final opinion, including justified no-change/conditional direction | No automatic selection for Build |
| Begin Consensus | Explicit selected public inputs, matching context/source, verified identities | Complete or deliberately partial comparison | No new source investigation |
| Publish Agreement candidate | Three eligible slots; coherent majority package; governing coverage; no blocking objection | Finalized candidate | No user adoption inferred |
| Adopt Agreement | Explicit user action; exact eligible candidate and digest | Adoption receipt | No automatic Build |
| Register implementation | Adopted contract and exact target/material identity | External implementation handoff | No self-certified conformance |
| Audit | Exact adopted contract and registered target; bounded verification policy | PASS, CHANGES_REQUIRED, or BLOCKED | No code edits or upstream redesign |

Publication, semantic outcome, and authorization are different axes. A fully completed comparison may have no eligible package. A failed provider invocation has no semantic verdict. A blocked audit is a valid assessment when it correctly identifies missing required evidence.

## 8.2 What happens after a failure

**Implementation deviation:** retain the Agreement, give the bounded finding to the chosen builder, register a new implementation, and explicitly request a new Audit.

**Verification gap:** obtain the missing authorized evidence or correct the disposable environment, then request/restart the affected verification and a new assessment. Do not demand unrelated product changes solely because an environment was missing.

**Authority defect:** retain the target and Agreement as historical records, revise the request/Discovery/Consensus work as needed, produce and adopt a new Agreement, and reassess implementation applicability. Audit itself chooses no new product meaning.

**Operational failure:** preserve the last trustworthy state, relevant diagnostics, and any unknown external outcome. A normal restart may resume known local work, but uncertain external effects require explicit handling; they are not silently replayed.

**Corruption/known contamination:** refuse the affected current authority or independent-opinion claim. Keep an attributed notice and history, and use a new explicit replacement/revision. Do not delete the inconvenient evidence to make a clean report.

<a id="section-9"></a>

# 9. Consensus decision examples

The examples below are product acceptance examples, not implementation algorithms for understanding prose.

## 9.1 Rotating majorities

| Mandatory proposition | A | B | C |
|---|---|---|---|
| X | Supports | Supports | Not observed |
| Y | Not observed | Supports | Supports |

X has two supporters and Y has two supporters. The whole X+Y package has only B. The reconciler may report both local majorities, select a narrower package if it actually answers the request, or report that the required complete direction has no common majority. It may not label X+Y a majority-endorsed implementation package.

## 9.2 Conditions are part of the recommendation

Three opinions support “apply X only to active accounts.” They do not thereby support applying X to every account. The active-account condition belongs in the adopted records, normative text, implementation obligations, and Audit coverage.

A condition that is a branch in specified behavior is different from an unresolved prerequisite. “Only active accounts may access X” is behavior to implement. “Proceed only after product confirms policy P” may be a blocker rather than implementation-ready behavior. The model must make that distinction and the records must preserve it.

## 9.3 User requirements are not voted away

The request says a booking may belong to several groups. All opinions propose some compatible change but fail to address multiplicity. Consensus must not infer that the request now means one group per booking. It must retain the governing rule and identify whether the adopted direction actually covers it. A complete coverage table without a faithful interpretation is still a semantic failure.

## 9.4 Minority objections are not all alike

A minority preference for a different library can remain dissent without blocking an otherwise supported contract. A supplied concrete counterexample that invalidates an essential premise cannot be ignored as merely a losing vote. The latter blocks adoption unless the permitted source material resolves it or a properly scoped package avoids it.

## 9.5 No change is a real answer

All three opinions conclude the requested guarantee already holds and identify how to verify it. Consensus can produce an Agreement to retain that behavior. The user adopts it, registers the existing commit, and Audit may pass without new code. Orchestrate does not require activity merely to fill every box with a code change.

<a id="section-10"></a>

# 10. CLI and installed skill experience

The canonical executable is `orchestrate`. Required top-level phase names are `discovery`, `consensus`, and `audit`; `build` is reserved for the later specified builder. Required supporting capabilities include effort/cohort selection, adoption, external implementation registration, status, inspection, lineage, and safe skill installation.

The following is **illustrative command organization**, not a requirement to implement an extra command for every line:

```text
orchestrate init ...
orchestrate discovery ...
orchestrate consensus ...
orchestrate agreement adopt ...
orchestrate implementation register ...
orchestrate audit ...
orchestrate status ...
orchestrate inspect ...
orchestrate lineage ...
orchestrate guide <phase>
```

The technical CLI contract must freeze actual flags, selectors, JSON envelopes, exit codes, and operation behavior before implementation. Human-facing use must not require raw JSON. Existing examples from the prototypes do not define this contract.

For shell automation, distinguish invocation/transport validity from semantic verdict. A successfully written nonconformance report is not an invocation crash. Machine output must expose the semantic outcome explicitly so a script cannot equate exit success with conformance. Invalid inputs and operational failure must be discoverable without parsing prose. Read-only commands require no provider session.

## 10.1 Fixed entrypoints

```text
orchestrate-discovery  →  orchestrate guide discovery
orchestrate-consensus  →  orchestrate guide consensus
orchestrate-audit      →  orchestrate guide audit
```

Hosts may use `$skill-name` or `/skill-name`; packaging adapts without changing phase meaning. The wrappers hard-code their phase. They do not inspect the prompt and decide which workflow to start.

A thin wrapper resolves the installed runtime, loads the corresponding operator guide, follows its permitted operations, presents the retained result, and stops. Missing prerequisites produce focused diagnostics. It never replaces a failed managed phase with a parent-chat answer.

`guide` is a read-only instruction reader, not a run command. Its content is packaged with the executable. Loading new instructions does not clear an already contaminated conversation; independent work requires fresh context and appropriately bounded inputs. A host opened in the original dirty checkout is not automatically a qualified investigation environment. Preparation must direct the investigator to the frozen run-owned source and prevent supplied editor/retrieval context from substituting original or peer content. If the host cannot establish the required supplied-context boundary, it must restart in an appropriate workspace or report the limitation before claiming a valid committed-baseline investigation.

## 10.2 Three hosts do not imply three managed provider adapters

Codex, Claude Code, and Cursor can be operator/investigator hosts. Managed Consensus/Audit execution may initially use one explicitly selected and qualified provider adapter. Host provenance and actual execution provenance are separate. No silently invented model identity, local-model requirement, or new provider integration follows merely from the word “local.”

The installation is one release containing multiple thin skills. A plugin/marketplace can be a future packaging choice; it is not required for the local tool to function. Inspectors and guides still work when model authentication is unavailable.

<a id="section-11"></a>

# 11. Verification and acceptance of the delivered tool

The suite tests three separately reported claims: machinery correctness, actual host/provider compatibility, and semantic quality. None can be averaged away by another.

**Deterministic rules and generated sequences** check identity, eligibility, coverage arithmetic, common-majority support, state transitions, and invalidation against an independent reference model.

**Offline actual-CLI integration/E2E** use real Git/source material, paths, subprocesses, protocol parsing, persistence, finalization, restart, and inspection. Scripted agents drive the actual public operations. A fake does not write the final artifact or change the production state behind its API.

**Installed-host live smoke** use low-strength available models and fresh real host sessions. These qualify the actual skill/runtime path, not just a pasted prompt. Every host/editor/CLI surface claimed supported needs its own evidence or an explicit unsupported status.

**Semantic regression evaluation** runs the intended working profiles against original opinions, deliberately faulty/good implementations, conditional directions, real blockers, and known scope traps. Predeclared trials and independent rubrics prevent cherry-picking. Five fresh trials per selected small scenario/profile is the initial diagnostic minimum, not a statistical guarantee.

## 11.1 Required complete journeys

The detailed suite contains the valid complete chain; faulty implementation followed by correction; source/request/Agreement substitution at each boundary; interruption around publication and external effects; actual low-strength installed-host execution; concurrent separate efforts with index reconstruction; and a no-change chain.

Every critical protection must be challenged: leak peer output, omit a user rule, broaden a condition, replace package intersection with union, accept a draft, retarget HEAD, forge a receipt, skip a coverage row, interrupt publication, reuse an adoption for changed bytes, and attempt unsafe cleanup. The suite must catch these deliberate errors.

## 11.2 Release gates

A first delivery is accepted only when all core requirements have implemented positive/negative cases at the relevant boundaries; all mandatory machinery cases execute successfully; the packaged binary/resources work outside the source checkout; native macOS path/process/recovery tests execute; every claimed host path has actual smoke evidence; and the selected must-pass semantic corpus completes successfully on its declared profiles without observed critical false acceptance, unreported scope drift, or a breach of a claimed isolation boundary.

For small known-answer must-pass fixtures, a safely rejected valid task is still a task failure, not a semantic pass. An unavailable provider is a recorded blocked/failed qualification, not a removed trial. More exploratory larger workloads may establish a baseline rather than a release gate, but must be labeled separately.

All results identify the exact spec, product build, fixtures, model/host settings, platform, test selection, limits, and repetitions. Reports distinguish passed, failed, blocked, unsupported, not executed, and deferred. A command selecting zero tests cannot qualify a lane.

Process-kill tests establish process-crash behavior, not power-loss resilience. A case catalog is a plan, not executed evidence. The `VERIFICATION.md` catalog and generated mapping supply the concrete cases; qualification results belong in a separate execution report.

<a id="section-12"></a>

# 12. What does not carry forward from the reference tools

| Reference mechanism | New-product disposition | Safeguard retained |
|---|---|---|
| Nested Discovery committees, leases, internal consensus | Omit initially | Independent opinion identity and real adversarial investigation |
| Full research bookkeeping protocol and mandatory traversal graph | Redesign around consequential actions | Mandatory-check coverage, blockers, stale dependencies, current evidence |
| Every research need requiring an implementation requirement | Remove | Honest research dispositions and acceptance criteria for genuinely adopted behavior |
| Four-file Discovery export required by name | Do not inherit | Complete public opinion and minimal structured provenance |
| Generic audit/research dispatch inside Consensus | Remove | Explicit reconciliation of finalized opinions |
| Mandatory per-opinion extraction calls | Not required | Original-source fidelity and one opinion per selected slot |
| Prototype numeric assurance/confidence/rescore lifecycle | Do not inherit | Scoped support, exact counts, dissent, and limitations |
| Universal tool prohibition for Audit | Replace with permitted bounded evidence execution | Fresh assessor, exact target, attributable receipts |
| Taskledger scheduling/routing/checkpoints or Flow’s whole lifecycle | Defer | Exact Agreement-to-implementation boundary and honest submission status |
| Repo-local runtime state and skill installation | Replace with external store/host installation | Local durable records without source contamination |
| Historical Python encoding/migration behavior | No compatibility commitment | Explicit new schema/encoding and immutable history |
| Giant common semantic graph | Do not require | Narrow shared mechanics and explicit phase contracts |

The tests may reuse synthetic versions of historical failure cases. They do not require installing or operating any prototype. Future proposals can reintroduce a capability only by showing how it serves an approved need; omission here is not a claim that every old feature was a mistake. [D1]

<a id="section-13"></a>

# 13. Technical freedom and boundaries still deliberately open

The behavior above is specified; these implementation choices remain open: exact crate count; SQLite versus files for an active run; manifest wire schemas and filenames; public command subverbs/envelopes; provider protocol details; platform-specific containment adapters; the check runner’s library choice; and internal Build execution strategy.

These are not permission to omit a behavioral requirement. The technical contract must choose implementable publication, path, and process policies and define finite supported limits before qualifying them. Do not promise that a hash, crate, worktree, or skill alone establishes a stronger guarantee than it actually enforces.

`TECHNICAL-GUIDANCE.md` gives a starting implementation direction and verified references. It is deliberately subordinate to this product specification.

<a id="section-14"></a>

# 14. Definition of done

The delivered tool must make the following statements true for a qualified selected chain:

> These are the exact independent-opinion inputs received, with the actual limits of their independence recorded.
>
> This Agreement contains the coherent adopted package and preserved governing requirements—not a new untracked synthesis of optional ideas.
>
> The user adopted these exact Agreement bytes.
>
> This is the exact implementation submitted against that authority; observed and declared production history are distinguishable.
>
> This Audit evaluated that exact pair using attributable evidence, and every required behavior has an honest disposition.
>
> The chain remains inspectable after process restarts without asking a model to reconstruct it from chat history.

That is the acceptance target. Additional ceremony is not evidence of additional rigor.

<a id="section-15"></a>

# 15. Source and decision register

**D1 — Project reference review:** `orchestrate-reference-disposition-review.md`, September 17, 2026. Prior source-informed proposal separating retained safeguards, omitted mechanisms, and deferred Build. It was a source/artifact review, not new runtime qualification.

**D2 — Project testing design:** `01-testing-strategy.md` and `02-case-catalog.md`, September 17, 2026. Prior testing proposals. This package resolves their core policy alternatives, maps their enumerated cases to requirements, and adds explicit missing-boundary cases. The six scenario recipes elaborate cases; they are not extra test executions.

**D3 — Controlling user discussion:** September 17, 2026 conversation on one product/CLI; explicit phase skills; external storage after a poisoned Discovery run; frozen committed source; host/model/effort provenance; a new Rust product with no legacy obligations; Build deferral; and real machinery/E2E testing. The user-provided four-stage pipeline controls the design.

External documentation informs implementation mechanisms and current host packaging, not product authority. Verified references and precise limitations are recorded in `TECHNICAL-GUIDANCE.md`. No external source is evidence that Orchestrate has already satisfied this specification.
