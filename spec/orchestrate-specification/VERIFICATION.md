---
artifact: acceptance-and-verification-plan
product: Orchestrate
specification_id: orchestrate-product-v0.1
status: planned-not-executed
date: "2026-09-17"
---

# Orchestrate — Verification and acceptance plan

**Authority:** `ORCHESTRATE-SPEC.md`. This document maps its 94 numbered requirements to 147 acceptance cases: the previous 136 enumerated cases, normalized to this specification, plus 11 explicit additional cases. The six detailed recipes elaborate those cases; they are not additional counted tests.

**Execution status:** Every application test and model trial is **NOT EXECUTED**. The existence of this file, a case mapping, or a document consistency check does not establish that Orchestrate works.

# 1. How to qualify the product

There are four complementary layers:

| Code | Layer | What must be real | What may be controlled |
|---|---|---|---|
| D | Deterministic rules, fixed vectors, generated state sequences | Production validation and public observations | Seeds, clocks, small model/state inputs |
| I | Integration and actual CLI E2E | Executable, Git, filesystem, serialization, subprocesses, publication, restart | Scripted investigator actions and provider protocol responses |
| H | Installed-host smoke | Installed skills, release binary, exact host/editor/CLI surface, guide, actual calls | Small synthetic task and low-strength profile |
| S | Semantic regression | Actual phase instructions and working profile | Curated original inputs with independent grading rubrics |

A combined layer such as D,I,S requires evidence at all relevant boundaries. A scripted majority matrix can prove counting rules; it cannot prove that a model extracted the correct positions from prose.

Report machinery correctness, host/provider compatibility, and semantic quality separately. Operational failure, unsupported capability, missing credential, safe refusal, and semantic task failure must remain visible. A successfully refused invalid request is a positive protection result. Refusing a valid solvable task is not successful task completion.

# 2. Fixture and oracle discipline

Each offline trial uses a fresh disposable root containing a synthetic committed source repo, isolated home/configuration, external orchestration store, run scratch, and captured observations. The actual user home, work repositories, installed skills, and credentials are outside the permitted effects. Requirement-to-case links are design traceability, not proof that every acceptance statement already has a sufficient assertion; the implemented tests must demonstrate both A1 and A2 for each requirement. Apply environment settings per child process rather than mutating global HOME/PATH in parallel tests.

Keep examiner-only expected results and known fixes outside all agent-visible material. Do not put the solution on another visible Git branch or in a shared cache. Verify that the good reference passes the independent evaluator and deliberately bad references fail the intended assertions before evaluating Orchestrate.

Use a tiny booking/group fixture with explicit user-driven group creation, many-to-many membership, and membership removal that preserves the booking and other memberships. It is synthetic software, not the user’s financial tracker. Allow any implementation meeting the behavior; do not grade architecture taste.

Additional fixtures cover no-change discovery, false ticket premises, missing authoritative intent, conditions, coherent 2–1 repair, rotating majorities, minority scope insertion, unavailable verification, an inconsistent Agreement, a known defect, its repair, and a repair regression.

The scripted Discovery host performs the same public recording/finalization operations as a real host. A scripted managed provider emits protocol responses through the real adapter parser. Neither may alter authoritative run files or publish accepted artifacts directly.

# 3. Fault injection and test sensitivity

For every consequential operation consider valid input, invalid input, boundary size, interruption, relevant concurrency, and unauthorized effects. Not every operation needs a separate test for each dimension, but omitted dimensions need a reason.

Exercise failures at run creation, source capture, attempt reservation, provider/check launch, result retention, output staging, finalization, and acknowledgement. Use deterministic handshakes to kill at a known boundary. Process-kill tests do not establish power-loss resilience.

Seed at least these faults in disposable builds: skip digest verification; accept an unpublished draft; use union instead of intersection for package support; treat silence as support; include peer output in a packet; re-resolve HEAD; omit a coverage obligation; relabel a receipt; publish before payloads are committed; delete an unowned cleanup path. The real test suite must fail. No destructive mutation may run against actual user resources.

# 4. Live policy and release gates

Live tests require explicit opt-in, finite calls/attempts/time/byte limits, and declared profiles. Run the selected small must-pass semantic cases five times in fresh contexts/profile, retaining every trial and allowed repair. Five trials are a diagnostic minimum, not a reliability probability.

Cheap models qualify the small installed-host path. Semantic qualification also uses the actual profiles intended for work. The original inputs and independent rubrics determine the answer; the model’s extraction or self-evaluation is not the oracle. An LLM grader may assist only after known-good/known-bad controls show that it can grade the distinctions.

Require all core machinery cases and applicable conditional cases at their declared boundaries. Qualify natively on macOS. Each additional platform or host surface is separately claimed/tested. A Cursor CLI test is not an editor test. Manually exercised graphical-host tests remain labeled manual. No planned, filtered-to-zero, skipped, or blocked lane is a pass.

For the predeclared small must-pass semantic corpus, require successful task outcomes and no observed critical integrity, claimed-isolation, unsupported-scope, or false-conformance acceptance failure. Larger exploratory workloads may use reported baselines, but cannot replace the must-pass corpus. Any observed failure remains in the record; fixes produce a newly identified qualification run rather than erasing earlier trials.

# 5. Requirement traceability

Each row below links to the product requirement. Test execution results belong in a separate report. Adding a test is not a reason to add a new product feature. A conditional mechanism’s omission is recorded as unsupported/out of scope, not silently counted as tested.

| Requirement | Product behavior | Cases | Status |
|---|---|---|---|
| [ORC-001](ORCHESTRATE-SPEC.md#orc-001) | Deliver the four-stage product | E2E-01, E2E-02, BLD-06 | NOT EXECUTED |
| [ORC-002](ORCHESTRATE-SPEC.md#orc-002) | Select operations explicitly | CLI-02, CLI-04, CLI-11, ID-02 | NOT EXECUTED |
| [ORC-003](ORCHESTRATE-SPEC.md#orc-003) | Stop at consequential boundaries | CLI-03, AUD-09, E2E-01 | NOT EXECUTED |
| [ORC-004](ORCHESTRATE-SPEC.md#orc-004) | Preserve user authority above model agreement | CON-10, DISC-08, ACX-01 | NOT EXECUTED |
| [ORC-005](ORCHESTRATE-SPEC.md#orc-005) | Keep the new product independent of the prototypes | CLI-05, ART-02, ARCH-05, BLD-06 | NOT EXECUTED |
| [ORC-006](ORCHESTRATE-SPEC.md#orc-006) | Distinguish completion from correctness and approval | META-04, CON-11, CON-16, BLD-05 | NOT EXECUTED |
| [ORC-007](ORCHESTRATE-SPEC.md#orc-007) | Keep routine operation usable | CLI-01, DISC-01, DISC-09, ACX-02 | NOT EXECUTED |
| [ORC-008](ORCHESTRATE-SPEC.md#orc-008) | Keep orchestration state outside the target repository | ID-05, ID-06, ID-09, ISO-01 | NOT EXECUTED |
| [ORC-009](ORCHESTRATE-SPEC.md#orc-009) | Separate display labels from identity | ID-01, ID-03, ID-04, ID-08 | NOT EXECUTED |
| [ORC-010](ORCHESTRATE-SPEC.md#orc-010) | Preserve exact request and governing-context revisions | SRC-03, SRC-04, CON-10, ACX-01 | NOT EXECUTED |
| [ORC-011](ORCHESTRATE-SPEC.md#orc-011) | Represent separate efforts and comparison cohorts | ID-02, ID-07, CON-08, CON-09, E2E-06, ACX-03 | NOT EXECUTED |
| [ORC-012](ORCHESTRATE-SPEC.md#orc-012) | Resolve the source baseline once per cohort | SRC-01, SRC-03, SRC-05, E2E-03 | NOT EXECUTED |
| [ORC-013](ORCHESTRATE-SPEC.md#orc-013) | Supply the committed source, not a dirty working copy | SRC-02, SRC-05, SRC-06, ID-05 | NOT EXECUTED |
| [ORC-014](ORCHESTRATE-SPEC.md#orc-014) | Make external context and source completeness explicit | SRC-07, SRC-08, ENV-03, DISC-03 | NOT EXECUTED |
| [ORC-015](ORCHESTRATE-SPEC.md#orc-015) | Do not retarget completed work | BLD-04, ART-05, ART-06, ART-09, AUD-05 | NOT EXECUTED |
| [ORC-016](ORCHESTRATE-SPEC.md#orc-016) | Exclude peer outputs from supplied investigator context | ISO-01, ISO-02, ISO-08, SRC-08, CON-15 | NOT EXECUTED |
| [ORC-017](ORCHESTRATE-SPEC.md#orc-017) | Report the actual independence boundary | ISO-03, ISO-04, ISO-08, META-02, ACX-03 | NOT EXECUTED |
| [ORC-018](ORCHESTRATE-SPEC.md#orc-018) | Use fresh semantic contexts for independent work | DISC-09, DISC-10, ISO-08, META-01, ACX-03 | NOT EXECUTED |
| [ORC-019](ORCHESTRATE-SPEC.md#orc-019) | Keep experiments and checks in owned disposable environments | ISO-05, ISO-06, DISC-07, AUD-12, PRO-08, ACX-04 | NOT EXECUTED |
| [ORC-020](ORCHESTRATE-SPEC.md#orc-020) | Bound permissions and disclose enforcement limits | PRO-08, PRO-09, ISO-03, ISO-04, ACX-04 | NOT EXECUTED |
| [ORC-021](ORCHESTRATE-SPEC.md#orc-021) | Clean up only owned disposable resources | ISO-07, ID-08, REC-07 | NOT EXECUTED |
| [ORC-022](ORCHESTRATE-SPEC.md#orc-022) | One investigator-owned run is one opinion | DISC-01, DISC-10, CON-08 | NOT EXECUTED |
| [ORC-023](ORCHESTRATE-SPEC.md#orc-023) | Cover mandatory research without arbitrary ceremony | DISC-02, DISC-03, DISC-08 | NOT EXECUTED |
| [ORC-024](ORCHESTRATE-SPEC.md#orc-024) | Separate product blockers from delegated technical choices | DISC-03, DISC-08, CON-10 | NOT EXECUTED |
| [ORC-025](ORCHESTRATE-SPEC.md#orc-025) | Keep facts, interpretations, proposals, and observations distinct | DISC-07, PRO-06, AUD-03, META-02 | NOT EXECUTED |
| [ORC-026](ORCHESTRATE-SPEC.md#orc-026) | Permit useful no-change conclusions | DISC-04, ACX-05 | NOT EXECUTED |
| [ORC-027](ORCHESTRATE-SPEC.md#orc-027) | Preserve counterevidence and invalidate dependent conclusions | DISC-05, DISC-06, ART-01 | NOT EXECUTED |
| [ORC-028](ORCHESTRATE-SPEC.md#orc-028) | Investigate and prototype without implementing the target feature | DISC-07, ISO-05, ID-05, CLI-03 | NOT EXECUTED |
| [ORC-029](ORCHESTRATE-SPEC.md#orc-029) | Deliver a substantive public Discovery artifact | DISC-01, CON-15, ART-11 | NOT EXECUTED |
| [ORC-030](ORCHESTRATE-SPEC.md#orc-030) | Gate finalization on a current evidence case | DISC-02, DISC-03, DISC-05, CON-05 | NOT EXECUTED |
| [ORC-031](ORCHESTRATE-SPEC.md#orc-031) | Recover useful investigation work honestly | DISC-09, CLI-04, REC-01 | NOT EXECUTED |
| [ORC-032](ORCHESTRATE-SPEC.md#orc-032) | Explain confidence without procedural numerology | DISC-03, DISC-06, META-04 | NOT EXECUTED |
| [ORC-033](ORCHESTRATE-SPEC.md#orc-033) | Consume finalized public opinions and shared governing context | CON-14, CON-15, ART-11, CLI-04 | NOT EXECUTED |
| [ORC-034](ORCHESTRATE-SPEC.md#orc-034) | Require comparable, distinct cohort membership | CON-08, SRC-03, CON-14, CLI-04 | NOT EXECUTED |
| [ORC-035](ORCHESTRATE-SPEC.md#orc-035) | Handle incomplete participation without false consensus | CON-09, CON-11, ACX-03 | NOT EXECUTED |
| [ORC-036](ORCHESTRATE-SPEC.md#orc-036) | Normalize meaning while preserving conditions | CON-01, CON-03, CON-05, CON-12 | NOT EXECUTED |
| [ORC-037](ORCHESTRATE-SPEC.md#orc-037) | Validate a common majority for the mandatory package | CON-02, CON-04, CON-07, CON-10, ACX-06 | NOT EXECUTED |
| [ORC-038](ORCHESTRATE-SPEC.md#orc-038) | Prefer useful agreement over an empty unanimous generality | CON-02, CON-04, CON-11 | NOT EXECUTED |
| [ORC-039](ORCHESTRATE-SPEC.md#orc-039) | Keep dissent and counterexamples consequential | CON-02, CON-13, ACX-06 | NOT EXECUTED |
| [ORC-040](ORCHESTRATE-SPEC.md#orc-040) | Bind all normative output to validated adopted records | CON-06, CON-07, CON-10, ART-01, ACX-01 | NOT EXECUTED |
| [ORC-041](ORCHESTRATE-SPEC.md#orc-041) | Preserve opinion identity through any internal extraction | CON-08, CON-15, PRO-10 | NOT EXECUTED |
| [ORC-042](ORCHESTRATE-SPEC.md#orc-042) | Report scoped consensus confidence separately from execution | CON-03, CON-09, CON-11, META-04 | NOT EXECUTED |
| [ORC-043](ORCHESTRATE-SPEC.md#orc-043) | Do not use retries to manufacture agreement | PRO-02, PRO-10, CLI-03, CON-11 | NOT EXECUTED |
| [ORC-044](ORCHESTRATE-SPEC.md#orc-044) | Make the Agreement complete and self-contained | CON-06, CON-10, ART-11, ACX-01 | NOT EXECUTED |
| [ORC-045](ORCHESTRATE-SPEC.md#orc-045) | Separate finalization from user adoption | CON-16, ACX-07 | NOT EXECUTED |
| [ORC-046](ORCHESTRATE-SPEC.md#orc-046) | Do not let adoption conceal missing consensus or authority | CON-09, CON-13, CON-16, ACX-07, ACX-08 | NOT EXECUTED |
| [ORC-047](ORCHESTRATE-SPEC.md#orc-047) | Keep Agreement revisions immutable and traceable | ART-09, BLD-04, AUD-09, ACX-07 | NOT EXECUTED |
| [ORC-048](ORCHESTRATE-SPEC.md#orc-048) | Bind every implementation attempt to exact authority and starting source | BLD-01, BLD-03, BLD-06, ACX-09 | NOT EXECUTED |
| [ORC-049](ORCHESTRATE-SPEC.md#orc-049) | Register the exact implementation rather than a moving checkout | BLD-02, BLD-03, AUD-05, ID-08 | NOT EXECUTED |
| [ORC-050](ORCHESTRATE-SPEC.md#orc-050) | Separate implementation claims from observed verification | BLD-01, BLD-05, META-01, ACX-09 | NOT EXECUTED |
| [ORC-051](ORCHESTRATE-SPEC.md#orc-051) | Do not let Build reinterpret the Agreement | BLD-03, AUD-02, AUD-09, ACX-01 | NOT EXECUTED |
| [ORC-052](ORCHESTRATE-SPEC.md#orc-052) | Support no-change and correction handoffs | BLD-04, AUD-10, AUD-11, E2E-02, ACX-05 | NOT EXECUTED |
| [ORC-053](ORCHESTRATE-SPEC.md#orc-053) | Do not advertise an unimplemented builder | CLI-08, BLD-06, ARCH-05 | NOT EXECUTED |
| [ORC-054](ORCHESTRATE-SPEC.md#orc-054) | Freeze the audit authority, target, and permitted evidence | AUD-05, AUD-06, ART-01, E2E-03 | NOT EXECUTED |
| [ORC-055](ORCHESTRATE-SPEC.md#orc-055) | Use a fresh assessment independent of the builder | AUD-02, AUD-08, ISO-02, ACX-10 | NOT EXECUTED |
| [ORC-056](ORCHESTRATE-SPEC.md#orc-056) | Account for the complete Agreement | AUD-01, AUD-03, AUD-07, ACX-08, ACX-10 | NOT EXECUTED |
| [ORC-057](ORCHESTRATE-SPEC.md#orc-057) | Obtain evidence that can support the required claim | AUD-02, AUD-03, AUD-04, PRO-06, ACX-04 | NOT EXECUTED |
| [ORC-058](ORCHESTRATE-SPEC.md#orc-058) | Reuse evidence only with established applicability | AUD-06, AUD-10, AUD-11, ART-09 | NOT EXECUTED |
| [ORC-059](ORCHESTRATE-SPEC.md#orc-059) | Treat counterexamples as evidence, not minority votes | AUD-02, AUD-07, AUD-08, AUD-10 | NOT EXECUTED |
| [ORC-060](ORCHESTRATE-SPEC.md#orc-060) | Distinguish implementation defects, evidence gaps, and authority defects | AUD-03, AUD-04, AUD-09, CLI-03 | NOT EXECUTED |
| [ORC-061](ORCHESTRATE-SPEC.md#orc-061) | Derive a conservative but achievable conformance verdict | AUD-01, AUD-02, AUD-03, META-04, ACX-10 | NOT EXECUTED |
| [ORC-062](ORCHESTRATE-SPEC.md#orc-062) | Close corrections without expanding the contract | AUD-10, AUD-11, E2E-02 | NOT EXECUTED |
| [ORC-063](ORCHESTRATE-SPEC.md#orc-063) | Keep Audit read-only with respect to authority and product | AUD-12, ID-05, ISO-05, ACX-04 | NOT EXECUTED |
| [ORC-064](ORCHESTRATE-SPEC.md#orc-064) | Publish minimal self-describing public contracts | ART-02, ART-10, ART-11, ARCH-03 | NOT EXECUTED |
| [ORC-065](ORCHESTRATE-SPEC.md#orc-065) | Make finalization a committed boundary | ART-03, ART-07, ART-08, REC-02, REC-03, REC-04 | NOT EXECUTED |
| [ORC-066](ORCHESTRATE-SPEC.md#orc-066) | Preserve finalized bytes and truthful invalidation history | ART-01, ART-04, ART-08, ACX-03 | NOT EXECUTED |
| [ORC-067](ORCHESTRATE-SPEC.md#orc-067) | Derive cross-phase lineage from authoritative records | ART-05, ART-06, ID-02, E2E-06 | NOT EXECUTED |
| [ORC-068](ORCHESTRATE-SPEC.md#orc-068) | Bind consumption to retained bytes, not mutable locations | CON-14, ART-01, AUD-05, SRC-01 | NOT EXECUTED |
| [ORC-069](ORCHESTRATE-SPEC.md#orc-069) | Make artifacts portable enough for retained inspection | ID-08, ART-05, ART-11 | NOT EXECUTED |
| [ORC-070](ORCHESTRATE-SPEC.md#orc-070) | Validate paths and ownership before effects | ID-03, ID-04, ISO-07, PRO-07 | NOT EXECUTED |
| [ORC-071](ORCHESTRATE-SPEC.md#orc-071) | Handle duplicate operations without duplicating effects | REC-04, REC-08, ART-08, PRO-03 | NOT EXECUTED |
| [ORC-072](ORCHESTRATE-SPEC.md#orc-072) | Recover unknown external outcomes conservatively | REC-04, REC-05, REC-10, PRO-04 | NOT EXECUTED |
| [ORC-073](ORCHESTRATE-SPEC.md#orc-073) | Cancel with bounded, attributable cleanup | REC-06, REC-07, REC-09, PRO-05 | NOT EXECUTED |
| [ORC-074](ORCHESTRATE-SPEC.md#orc-074) | Keep concurrent local operations safe without requiring a scheduler | ID-07, ISO-06, ART-07, ART-08, REC-11, E2E-06 | NOT EXECUTED |
| [ORC-075](ORCHESTRATE-SPEC.md#orc-075) | Keep inspection read-only and unambiguous | CLI-01, ART-04, ART-05, ID-02 | NOT EXECUTED |
| [ORC-076](ORCHESTRATE-SPEC.md#orc-076) | Use explicit provider identity and capability selection | PRO-03, PRO-04, META-01, META-02 | NOT EXECUTED |
| [ORC-077](ORCHESTRATE-SPEC.md#orc-077) | Bound execution without silently discarding meaning | PRO-02, PRO-05, ENV-03, META-06 | NOT EXECUTED |
| [ORC-078](ORCHESTRATE-SPEC.md#orc-078) | Retain attempts and enforce the declared repair policy | PRO-01, PRO-02, PRO-03, PRO-10, CON-11 | NOT EXECUTED |
| [ORC-079](ORCHESTRATE-SPEC.md#orc-079) | Capture useful environment identity automatically and honestly | META-01, META-02, META-03, CLI-12 | NOT EXECUTED |
| [ORC-080](ORCHESTRATE-SPEC.md#orc-080) | Protect credentials and local private material | PRO-09, ISO-02, ENV-05 | NOT EXECUTED |
| [ORC-081](ORCHESTRATE-SPEC.md#orc-081) | Pin the operating contract of resumable work | CLI-12, ART-02, META-03 | NOT EXECUTED |
| [ORC-082](ORCHESTRATE-SPEC.md#orc-082) | Present decision-oriented results with evidence limits | META-04, META-05, META-06, DISC-01 | NOT EXECUTED |
| [ORC-083](ORCHESTRATE-SPEC.md#orc-083) | Install thin, explicit phase skills | CLI-08, CLI-09, CLI-10, CLI-11 | NOT EXECUTED |
| [ORC-084](ORCHESTRATE-SPEC.md#orc-084) | Make guide a read-only bundled instruction reader | CLI-01, CLI-05, CLI-07, ARCH-05 | NOT EXECUTED |
| [ORC-085](ORCHESTRATE-SPEC.md#orc-085) | Separate operator instructions from managed-role prompts | CLI-11, CON-15, META-01 | NOT EXECUTED |
| [ORC-086](ORCHESTRATE-SPEC.md#orc-086) | Install and update without damaging the user’s environment | CLI-05, CLI-06, CLI-07, ENV-01 | NOT EXECUTED |
| [ORC-087](ORCHESTRATE-SPEC.md#orc-087) | Use public contracts instead of phase internals | ARCH-01, ARCH-02, ARCH-03, ART-11 | NOT EXECUTED |
| [ORC-088](ORCHESTRATE-SPEC.md#orc-088) | Share mechanics only when the behavior is genuinely common | ARCH-02, ARCH-04, ISO-04 | NOT EXECUTED |
| [ORC-089](ORCHESTRATE-SPEC.md#orc-089) | Test machinery through real public boundaries | PRO-01, E2E-01, E2E-02, BLD-06, E2E-04 | NOT EXECUTED |
| [ORC-090](ORCHESTRATE-SPEC.md#orc-090) | Give every critical guarantee an adversarial test | REC-12, ARCH-04, META-05, META-06, ACX-11 | NOT EXECUTED |
| [ORC-091](ORCHESTRATE-SPEC.md#orc-091) | Isolate test infrastructure from real work and answer keys | ISO-02, SRC-08, ENV-05, PRO-09, ACX-11 | NOT EXECUTED |
| [ORC-092](ORCHESTRATE-SPEC.md#orc-092) | Qualify installed hosts, not just prompt text | CLI-09, CLI-10, E2E-05, META-06 | NOT EXECUTED |
| [ORC-093](ORCHESTRATE-SPEC.md#orc-093) | Test semantic fidelity against independent rubrics | CON-01, CON-02, CON-05, CON-12, AUD-01, AUD-02, E2E-05, ACX-11 | NOT EXECUTED |
| [ORC-094](ORCHESTRATE-SPEC.md#orc-094) | Report qualification honestly on real target environments | ENV-01, ENV-02, ENV-03, ENV-04, META-06, ACX-11 | NOT EXECUTED |

# 6. Acceptance case catalog

All cases are initially NOT EXECUTED. A case is a behavior obligation, not necessarily one test function. Parameterization is appropriate. Conditional access-enforcement, extra platforms, and optional multiple assessors require qualification only if claimed.

## Explicit operation selection, skills, and installation

<a id="cli-01"></a>

### CLI-01 — I

**Setup/attack:** Invoke help, version, guide, inspect, or status on a fresh test installation without credentials.

**Required observation:** The documented read-only operation works or reports a missing local artifact clearly; no model launch, run creation, repo edit, or credential requirement is introduced.

**Requirements:** ORC-007, ORC-075, ORC-084. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="cli-02"></a>

### CLI-02 — D,I

**Setup/attack:** Supply an unknown phase or omit a mandatory explicit phase.

**Required observation:** A specific input error; no inferred phase, model invocation, or partial mutation.

**Requirements:** ORC-002. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="cli-03"></a>

### CLI-03 — I

**Setup/attack:** Complete Discovery, then leave the process idle; repeat for Consensus and Audit.

**Required observation:** No successor phase, extra model session, or implementation action is launched. Compare actual invocation logs and directories, not the final prose.

**Requirements:** ORC-003, ORC-028, ORC-043, ORC-060. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="cli-04"></a>

### CLI-04 — D,I

**Setup/attack:** Try a phase with a parent of the wrong type, a draft parent, or a different effort's parent.

**Required observation:** The contract mismatch is rejected before semantic execution; no child is falsely finalized.

**Requirements:** ORC-002, ORC-031, ORC-033, ORC-034. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="cli-05"></a>

### CLI-05 — I

**Setup/attack:** Install packaged binary and skills into a disposable prefix; remove access to the source checkout.

**Required observation:** All supported guides and necessary resources load from the installation. No source-tree or old Python/uv fallback is required.

**Requirements:** ORC-005, ORC-084, ORC-086. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="cli-06"></a>

### CLI-06 — I

**Setup/attack:** Install twice, update, and remove the test installation beside an unrelated sentinel skill.

**Required observation:** Orchestrate's managed files behave idempotently under the installer policy; unrelated skills/configuration remain unchanged. Installed modifications are not silently destroyed.

**Requirements:** ORC-086. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="cli-07"></a>

### CLI-07 — I

**Setup/attack:** Put a conflicting executable earlier on PATH or make the intended runtime absent.

**Required observation:** The wrapper identifies the selected executable/version or stops with a useful mismatch diagnostic; it does not operate a remembered workflow against the wrong program.

**Requirements:** ORC-084, ORC-086. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="cli-08"></a>

### CLI-08 — D,I

**Setup/attack:** Inspect each generated phase wrapper and host metadata; include Build before Build is implemented.

**Required observation:** Fixed phase-to-guide mapping with no classifier. Canonical semantic bodies are not independently duplicated. Discovery, Consensus, and Audit skills are installed; internal Build execution/skill is not advertised. External implementation registration remains available.

**Requirements:** ORC-053, ORC-083. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="cli-09"></a>

### CLI-09 — H

**Setup/attack:** In a fresh actual Codex/Claude/Cursor session, explicitly invoke an installed phase skill.

**Required observation:** The host discovers that skill, loads the matching guide, and uses the intended executable. Record host version and evidence; pasting the skill into a prompt is not this test.

**Requirements:** ORC-083, ORC-092. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="cli-10"></a>

### CLI-10 — H

**Setup/attack:** Send a vague request without invoking an explicit-only phase skill, then explicitly invoke it with missing inputs.

**Required observation:** The implicit-invocation policy behaves as configured; the explicit invocation produces a prerequisite diagnostic rather than choosing another phase. Grade host invocation separately from general chat behavior.

**Requirements:** ORC-083, ORC-092. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="cli-11"></a>

### CLI-11 — H,S

**Setup/attack:** Invoke Consensus explicitly but include user text requesting investigation or code edits outside Consensus.

**Required observation:** The active workflow does not silently become Discovery or Build. Necessary disagreement is surfaced without running the other phase.

**Requirements:** ORC-002, ORC-083, ORC-085. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="cli-12"></a>

### CLI-12 — I,H

**Setup/attack:** Begin a resumable run, change installed guide/runtime identity, and resume in a fresh session.

**Required observation:** Record runtime/guide/policy identities. An incompatible resume stops without data mutation; a specifically supported compatible continuation is explicit and recorded. Never silently load current rules as though they governed the older run.

**Requirements:** ORC-079, ORC-081. **Applicability:** core. **Status:** NOT EXECUTED.


## Project/effort identity and external storage

<a id="id-01"></a>

### ID-01 — I

**Setup/attack:** Two unrelated repositories share the basename `server`; both use `TIX-1234`.

**Required observation:** Their records cannot collide or cross-link. Display names are not the sole identity.

**Requirements:** ORC-009. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="id-02"></a>

### ID-02 — I

**Setup/attack:** One repository has two requests with similar slugs and several runs per phase.

**Required observation:** Explicit effort/run selection targets the intended records; “latest directory” never silently chooses unrelated authority.

**Requirements:** ORC-002, ORC-011, ORC-067, ORC-075. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="id-03"></a>

### ID-03 — D,I

**Setup/attack:** Use spaces, Unicode, separators, `..`, absolute path fragments, and unusually long slugs.

**Required observation:** Valid labels round-trip; invalid labels receive a clear error; no escape from the configured storage root or accidental overwrite. Test actual supported filesystem rules.

**Requirements:** ORC-009, ORC-070. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="id-04"></a>

### ID-04 — I

**Setup/attack:** Refer to the same repository through a symlink and canonical path; test macOS `/var` and `/private/var`.

**Required observation:** The documented alias policy is consistent; path spelling alone does not merge unrelated repos or cause false containment failures.

**Requirements:** ORC-009, ORC-070. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="id-05"></a>

### ID-05 — I

**Setup/attack:** Prepopulate tracked/untracked files, `.gitignore`, and local Git exclude/configuration; run Discovery, Consensus, and Audit.

**Required observation:** Product-file bytes and configuration remain unchanged. Any claimed zero-repository-write guarantee includes relevant Git administration effects, not only a clean diff.

**Requirements:** ORC-008, ORC-013, ORC-028, ORC-063. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="id-06"></a>

### ID-06 — I

**Setup/attack:** Use an explicit test orchestration root and invoke from a different working directory.

**Required observation:** Paths resolve according to the documented CLI contract; no writes leak into the real home, the caller's cwd, or target repository.

**Requirements:** ORC-008. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="id-07"></a>

### ID-07 — I

**Setup/attack:** Two processes create runs simultaneously for the same effort.

**Required observation:** Unique run identities; no shared output collision; each result remains attributable. Parallel model execution is optional, but concurrent local commands cannot corrupt records.

**Requirements:** ORC-011, ORC-074. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="id-08"></a>

### ID-08 — I

**Setup/attack:** Move an artifact bundle to another allowed location, or make the original source path unavailable.

**Required observation:** Self-contained public content and retained lineage remain inspectable; unavailable external references are reported, not guessed. Relocating labels does not create another opinion.

**Requirements:** ORC-009, ORC-021, ORC-049, ORC-069. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="id-09"></a>

### ID-09 — I

**Setup/attack:** Make the root unwritable or simulate allocation/write failure.

**Required observation:** A normal diagnostic and no fabricated finalized artifact. The application does not invent a different storage root to bypass the failure.

**Requirements:** ORC-008. **Applicability:** core. **Status:** NOT EXECUTED.


## Committed source and request baselines

<a id="src-01"></a>

### SRC-01 — I

**Setup/attack:** Create commit A, initialize a comparison cohort, then advance the original checkout to B before the second Discovery.

**Required observation:** All cohort investigations use A unless a new baseline/cohort is explicitly selected. The manifest records the material actually supplied.

**Requirements:** ORC-012, ORC-068. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="src-02"></a>

### SRC-02 — I

**Setup/attack:** At A, add dirty tracked edits and untracked repro files, then start a committed-baseline investigation.

**Required observation:** The investigation sees committed A, not the dirty version. Original dirty/untracked work is neither cleaned nor overwritten.

**Requirements:** ORC-013. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="src-03"></a>

### SRC-03 — D,I

**Setup/attack:** Combine finalized opinions from different source commits, requests, or incorporated constraints.

**Required observation:** The mismatch is detected before claiming direct comparability. No automatic mixing as three equivalent votes. Any future comparison-across-baselines mode needs its own explicit contract.

**Requirements:** ORC-010, ORC-012, ORC-034. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="src-04"></a>

### SRC-04 — I

**Setup/attack:** Change an upstream request after one run, retaining the same human slug.

**Required observation:** Request identity changes are visible; older opinions do not silently become opinions about the new request.

**Requirements:** ORC-010. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="src-05"></a>

### SRC-05 — I

**Setup/attack:** Change the source during snapshot preparation.

**Required observation:** The supplied committed snapshot is consistent with its recorded identity, or preparation fails. No mixed snapshot labeled as one commit.

**Requirements:** ORC-012, ORC-013. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="src-06"></a>

### SRC-06 — I

**Setup/attack:** Investigate a repo without a commit, or a target that is not supported by the committed-snapshot implementation.

**Required observation:** A clear unsupported/prerequisite result. The program does not invent a commit, make an initial commit, or silently switch to dirty-tree semantics.

**Requirements:** ORC-013. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="src-07"></a>

### SRC-07 — I

**Setup/attack:** Include executable modes, supported symlinks, binary files, and unsupported submodule/LFS arrangements.

**Required observation:** The declared snapshot policy preserves relevant supported inputs and explicitly handles unsupported ones; no silent omission of required source material.

**Requirements:** ORC-014. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="src-08"></a>

### SRC-08 — I

**Setup/attack:** Place a known solution on an extra branch or in a cache not authorized for the investigator.

**Required observation:** That solution is not reachable through the provided research boundary. Check refs/config/caches actually exposed; do not equate a separate directory with isolation.

**Requirements:** ORC-014, ORC-016, ORC-091. **Applicability:** core. **Status:** NOT EXECUTED.


## Contamination, visibility, and experiment ownership

<a id="iso-01"></a>

### ISO-01 — I

**Setup/attack:** Run A creates a distinctive repro, notes, and a false recommendation. Start B from the same cohort.

**Required observation:** A's generated material is absent from the original source and B's supplied workspace/packet. B receives its own run state only.

**Requirements:** ORC-008, ORC-016. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="iso-02"></a>

### ISO-02 — I

**Setup/attack:** Plant different sentinels in peer outputs, examiner keys, original dirty files, and unrelated home files; inspect actual outgoing packets/tool results.

**Required observation:** No forbidden sentinel is supplied to the semantic role. Separately assert useful authorized source remains available; “empty input” is not a passing isolation solution.

**Requirements:** ORC-016, ORC-055, ORC-080, ORC-091. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="iso-03"></a>

### ISO-03 — I,H

**Setup/attack:** **Conditional: enforced peer access.** An adversarial probe attempts a direct read of the known peer path using the investigator's actual capabilities.

**Required observation:** Access is denied by the declared boundary. If only cooperative policy exists, this capability is NOT QUALIFIED; an unquoted sentinel or a verbal refusal is insufficient.

**Requirements:** ORC-017, ORC-020. **Applicability:** conditional. **Status:** NOT EXECUTED.

<a id="iso-04"></a>

### ISO-04 — I,H

**Setup/attack:** **Conditional: enforced peer access.** Repeat peer access through symlink/relative traversal, process execution, relevant Git metadata, and allowed retrieval tools.

**Required observation:** No alternate authorized tool bypasses the claimed restriction. A filesystem-only result does not qualify browser/connectors or an unrestricted shell.

**Requirements:** ORC-017, ORC-020, ORC-088. **Applicability:** conditional. **Status:** NOT EXECUTED.

<a id="iso-05"></a>

### ISO-05 — I

**Setup/attack:** Run a successful, failing, and interrupted experiment producing files and temporary service data.

**Required observation:** Application-owned check/reproduction effects use only declared run-owned disposable resources; target repository is unchanged; receipts distinguish experiment patches from the original target. Stronger arbitrary-process confinement is claimed only under a qualified boundary.

**Requirements:** ORC-019, ORC-028, ORC-063. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="iso-06"></a>

### ISO-06 — I

**Setup/attack:** Start two run-owned experiments with similarly named files, ports, and output paths.

**Required observation:** Separate resource identities; no shared writable caches/data cause cross-run evidence or result contamination. If parallel execution is unsupported, reject safely without corruption.

**Requirements:** ORC-019, ORC-074. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="iso-07"></a>

### ISO-07 — I

**Setup/attack:** Attempt cleanup with a sibling-run path, a symlink to the target repo, and a stale ownership marker.

**Required observation:** Cleanup refuses unowned/unsafe resources; it does not delete the source, peer evidence, or unrelated test sentinels.

**Requirements:** ORC-021, ORC-070. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="iso-08"></a>

### ISO-08 — H,S

**Setup/attack:** Run normal low-strength Discovery with misleading sibling artifacts present outside its permitted input.

**Required observation:** No incorporation or citation of peer material; record observed behavior separately from enforcement qualification. Use an independent trace/input audit, not only text matching.

**Requirements:** ORC-016, ORC-017, ORC-018. **Applicability:** core. **Status:** NOT EXECUTED.


## Discovery

<a id="disc-01"></a>

### DISC-01 — I,S

**Setup/attack:** Give a small solvable request with explicit repository evidence and acceptance behavior.

**Required observation:** The scripted path can finalize through public operations; live Discovery produces an actionable opinion rather than only a completed ledger.

**Requirements:** ORC-007, ORC-022, ORC-029, ORC-082. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="disc-02"></a>

### DISC-02 — D,I

**Setup/attack:** Omit a required research/check item; try to finalize.

**Required observation:** The missing obligation remains visible and finalization follows the declared gate. The model cannot clear it by renaming an unrelated record.

**Requirements:** ORC-023, ORC-030. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="disc-03"></a>

### DISC-03 — D,I,S

**Setup/attack:** Record a required source as unavailable, a nonapplicable check with a reason, and an unresolved product decision.

**Required observation:** These are distinct from successful verification. Blockers are preserved; legitimate scope-limited outcomes remain possible where policy allows them.

**Requirements:** ORC-014, ORC-023, ORC-024, ORC-030, ORC-032. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="disc-04"></a>

### DISC-04 — D,I,S

**Setup/attack:** research establishes that no implementation change is necessary.

**Required observation:** Research can be dispositioned without a fabricated requirement/task. A justified preservation requirement is still allowed when genuinely needed.

**Requirements:** ORC-026. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="disc-05"></a>

### DISC-05 — D,I

**Setup/attack:** Revise a premise or invalidate an assumption after the draft was reviewed.

**Required observation:** Dependent conclusions/review status become stale as specified; unrelated valid evidence remains reusable. No silent finalization on old support.

**Requirements:** ORC-027, ORC-030. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="disc-06"></a>

### DISC-06 — D,I,S

**Setup/attack:** Add contrary evidence to a previously supported conclusion.

**Required observation:** The evidence remains represented and the affected decision cannot silently keep an unqualified old status. Live assessment handles its meaning instead of counting supporters.

**Requirements:** ORC-027, ORC-032. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="disc-07"></a>

### DISC-07 — I,S

**Setup/attack:** A reproduction demonstrates a defect, but no repair has been executed; another command exits zero without meaningful assertions.

**Required observation:** Receipts report observations, not proof of a repair. The final opinion distinguishes reproduced failure, proposed solution, and actual validation.

**Requirements:** ORC-019, ORC-025, ORC-028. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="disc-08"></a>

### DISC-08 — D,I,S

**Setup/attack:** The ticket incorrectly describes existing code; a separate product rule genuinely is unavailable.

**Required observation:** Discovery can correct the mistaken premise while asking for the missing product authority; it must not ask the user to choose an ordinary delegated technical detail.

**Requirements:** ORC-004, ORC-023, ORC-024. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="disc-09"></a>

### DISC-09 — I

**Setup/attack:** Interrupt a run with saved findings, then resume from a fresh caller/session.

**Required observation:** Saved work is recovered; identities and unresolved state remain honest. No duplicate replacement run merely to bypass a blocker.

**Requirements:** ORC-007, ORC-018, ORC-031. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="disc-10"></a>

### DISC-10 — D,I

**Setup/attack:** one investigator-owned run emits several files/copies and changes sessions.

**Required observation:** One investigator-owned run remains one logical opinion across sessions/files/exports. No nested investigator committee is required by the initial product.

**Requirements:** ORC-018, ORC-022. **Applicability:** core. **Status:** NOT EXECUTED.


## Consensus

<a id="con-01"></a>

### CON-01 — D,I,S

**Setup/attack:** Three opinions endorse the same actionable behavior in different wording.

**Required observation:** The Agreement preserves the behavior and correct support. Terminology changes alone do not create disagreement.

**Requirements:** ORC-036, ORC-093. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-02"></a>

### CON-02 — D,I,S

**Setup/attack:** Two opinions endorse a repair; the third explicitly disagrees.

**Required observation:** A coherent majority result is possible under the chosen policy, with dissent visible. Do not collapse every 2–1 result into an inconclusive shrug.

**Requirements:** ORC-037, ORC-038, ORC-039, ORC-093. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-03"></a>

### CON-03 — D,I,S

**Setup/attack:** Two endorse X; the third is silent. Compare with a third that contradicts X.

**Required observation:** Support/contradiction/not-observed remain distinct; silence is not a negative vote or an endorsement.

**Requirements:** ORC-036, ORC-042. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-04"></a>

### CON-04 — D,I,S

**Setup/attack:** X is supported by A/B; Y by B/C; the requested package requires both.

**Required observation:** Per-claim majorities remain visible but no common-majority endorsement of X+Y is asserted. Independently calculate supporter-set intersection.

**Requirements:** ORC-037, ORC-038. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-05"></a>

### CON-05 — D,I,S

**Setup/attack:** All agree on “X only when P”; the candidate omits P.

**Required observation:** Structural reference/condition rules reject a detected omission; live grading checks that meaning was not broadened. Do not claim the validator understands arbitrary prose.

**Requirements:** ORC-030, ORC-036, ORC-093. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-06"></a>

### CON-06 — D,I,S

**Setup/attack:** One opinion adds attractive feature Z; model places Z in an extra recommendation paragraph outside adopted claims.

**Required observation:** Output binding disallows unaccounted mandatory instructions; semantic grading checks that paraphrases do not sneak Z into the Agreement.

**Requirements:** ORC-040, ORC-044. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-07"></a>

### CON-07 — D,I,S

**Setup/attack:** A majority rejects “replace interface”; model marks that rejected proposal decisive.

**Required observation:** The adopted direction is represented as the supported scoped alternative, such as retain the interface, rather than a rejected positive claim.

**Requirements:** ORC-037, ORC-040. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-08"></a>

### CON-08 — D,I

**Setup/attack:** Supply the same run through two aliases/copies; or alter its metadata but retain the same original run identity.

**Required observation:** One logical opinion is counted. Conflicting copies of one identity are reported, not counted as independent voters. Equal text alone is not proof that two genuinely distinct runs are aliases.

**Requirements:** ORC-011, ORC-022, ORC-034, ORC-041. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-09"></a>

### CON-09 — D,I

**Setup/attack:** One of the expected three opinions fails or is unavailable.

**Required observation:** Expected participation stays 3. A user-requested partial comparison may report findings, but no Agreement is adoptable until three eligible slots are complete. Replacement is explicit and retained.

**Requirements:** ORC-011, ORC-035, ORC-042, ORC-046. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-10"></a>

### CON-10 — D,I,S

**Setup/attack:** All three opinions overlook an explicit user requirement.

**Required observation:** The request remains bound to the effort; the Agreement does not gain authority to delete the requirement by omission. Semantic grading checks preservation; automated claims are limited to representable constraints.

**Requirements:** ORC-004, ORC-010, ORC-024, ORC-037, ORC-040, ORC-044. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-11"></a>

### CON-11 — D,I,S

**Setup/attack:** No actionable package obtains required support, but useful shared findings exist.

**Required observation:** Publish an honest comparison with the missing decision; no fabricated build-ready Agreement and no empty diagnostic pretending comparison never happened.

**Requirements:** ORC-006, ORC-035, ORC-038, ORC-042, ORC-043, ORC-078. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-12"></a>

### CON-12 — D,I,S

**Setup/attack:** Permute input order and change neutral provider labels without changing the opinions.

**Required observation:** Mechanical counts/eligibility are invariant. Live quality checks prohibit provider prestige, ordering, and verbosity from manufacturing support.

**Requirements:** ORC-036, ORC-093. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-13"></a>

### CON-13 — D,I,S

**Setup/attack:** One report has a concrete counterexample to a majority premise.

**Required observation:** Dissent is retained and qualified rather than erased. Consensus still reports endorsement counts honestly; it does not invent additional research or new votes.

**Requirements:** ORC-039, ORC-046. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-14"></a>

### CON-14 — I

**Setup/attack:** Complete reconciliation, then mutate an input before publication/consumption; delete an unneeded upstream private database.

**Required observation:** Verify public input identities at intake and verify retained authoritative bytes before publication/consumption. Tampering with those bytes fails. A live original path changing after correct capture does not retarget the frozen input. Private upstream databases are unnecessary.

**Requirements:** ORC-033, ORC-034, ORC-068. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-15"></a>

### CON-15 — I,S

**Setup/attack:** Include internal ledgers and summaries in retained storage but exclude them from the declared consensus input projection.

**Required observation:** Models receive the substantive finalized opinion and approved context, not an accidental recursive dump of the entire run. Retention is not automatic model visibility.

**Requirements:** ORC-016, ORC-029, ORC-033, ORC-041, ORC-085. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="con-16"></a>

### CON-16 — D,I

**Setup/attack:** Finalize an Agreement candidate without the adoption action required by the chosen approval policy.

**Required observation:** Candidate finalization and adoption are distinct. Build authority requires a separate adoption receipt naming the exact eligible Agreement identity/digest; the receipt may be recorded by an explicitly authorized combined user command.

**Requirements:** ORC-006, ORC-045, ORC-046. **Applicability:** core. **Status:** NOT EXECUTED.


## External Build handoff

<a id="bld-01"></a>

### BLD-01 — D,I

**Setup/attack:** Register an externally prepared implementation against one exact Agreement and starting baseline.

**Required observation:** The handoff names that authority and actual resulting commit/tree. It does not claim Orchestrate performed the build.

**Requirements:** ORC-048, ORC-050. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="bld-02"></a>

### BLD-02 — D,I

**Setup/attack:** Present a dirty checkout while identifying only HEAD as the exact implementation.

**Required observation:** Register only the selected committed target and state prominently that dirty/untracked changes are excluded. Do not clean, commit, or certify those changes. A missing or unsupported committed target is refused.

**Requirements:** ORC-049. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="bld-03"></a>

### BLD-03 — D,I

**Setup/attack:** Supply a nonexistent commit, another repo's identity, or an implementation built from a mismatched authority.

**Required observation:** Nonexistent commit, wrong registered project, wrong authority, or contradicted source identity is rejected. Unobserved external build history is an attributed declaration, not a mechanically proven fact. Content alone cannot prove historical intent.

**Requirements:** ORC-048, ORC-049, ORC-051. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="bld-04"></a>

### BLD-04 — I

**Setup/attack:** Publish Agreement v2 after registering a build against v1.

**Required observation:** The historical build remains bound to v1 and its applicability to v2 is not assumed. No rewrite of prior records.

**Requirements:** ORC-015, ORC-047, ORC-052. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="bld-05"></a>

### BLD-05 — D,I

**Setup/attack:** Worker prose says complete while required handoff/evidence fields are absent or the operation is partial.

**Required observation:** It remains a claim/partial outcome, not successful build finalization or Audit acceptance.

**Requirements:** ORC-006, ORC-050. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="bld-06"></a>

### BLD-06 — I

**Setup/attack:** Exercise the boundary without Taskledger, Flow, task routing, checkpoints, or a scheduler installed.

**Required observation:** Discovery/Consensus/Audit and external target registration remain usable. Real Build execution is reported as deferred.

**Requirements:** ORC-001, ORC-005, ORC-048, ORC-053, ORC-089. **Applicability:** core. **Status:** NOT EXECUTED.


## Audit

<a id="aud-01"></a>

### AUD-01 — I,S

**Setup/attack:** Audit the known-good fixture against its matching Agreement with required checks available.

**Required observation:** The scripted path and live evaluator can return supported conformance. An always-blocking system fails this case.

**Requirements:** ORC-056, ORC-061, ORC-093. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="aud-02"></a>

### AUD-02 — I,S

**Setup/attack:** Audit a known required-behavior violation with a reproducible check.

**Required observation:** Identify the relevant requirement, concrete evidence, consequence and bounded correction; do not certify the implementation.

**Requirements:** ORC-051, ORC-055, ORC-057, ORC-059, ORC-061, ORC-093. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="aud-03"></a>

### AUD-03 — I,S

**Setup/attack:** A required runtime observation is missing.

**Required observation:** Report unverified behavior, not fabricated success and not an unsupported claim that the code is defective.

**Requirements:** ORC-025, ORC-056, ORC-057, ORC-060, ORC-061. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="aud-04"></a>

### AUD-04 — I,S

**Setup/attack:** A required check cannot run because the fixture environment is unavailable or misconfigured.

**Required observation:** The infrastructure limitation is distinguished from an assertion failure demonstrating nonconformance. The harness itself is checked before blaming the implementation.

**Requirements:** ORC-057, ORC-060. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="aud-05"></a>

### AUD-05 — I

**Setup/attack:** Request audit of commit A, then move HEAD to B during the run.

**Required observation:** The actual inspected/tested target stays A; final record and receipts identify A, not whatever HEAD means later.

**Requirements:** ORC-015, ORC-049, ORC-054, ORC-068. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="aud-06"></a>

### AUD-06 — D,I

**Setup/attack:** Supply a legitimate-looking passing receipt from another commit, environment, command, or Agreement.

**Required observation:** It cannot satisfy the current required observation without explicit established applicability. No automatic retargeting.

**Requirements:** ORC-054, ORC-058. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="aud-07"></a>

### AUD-07 — I,S

**Setup/attack:** Required checks pass, but an additional focused probe demonstrates a violation.

**Required observation:** The violation is retained; the number of passing checks does not override it.

**Requirements:** ORC-056, ORC-059. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="aud-08"></a>

### AUD-08 — D,I,S

**Setup/attack:** **Conditional: multiple assessments.** One assessor demonstrates a violation; others report no issue.

**Required observation:** Aggregation does not suppress the counterexample by majority vote. No mandatory committee is introduced by this case.

**Requirements:** ORC-055, ORC-059. **Applicability:** conditional. **Status:** NOT EXECUTED.

<a id="aud-09"></a>

### AUD-09 — I,S

**Setup/attack:** Audit discovers a contradiction or consequential ambiguity in the Agreement.

**Required observation:** It records an authority issue; does not silently rewrite the Agreement, choose a new product rule, or start upstream work.

**Requirements:** ORC-003, ORC-047, ORC-051, ORC-060. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="aud-10"></a>

### AUD-10 — I,S

**Setup/attack:** Correct the known defect; retain an optional hardening suggestion unrelated to required behavior.

**Required observation:** The correction can close and conformance can pass. Optional advice does not become an endless acceptance blocker.

**Requirements:** ORC-052, ORC-058, ORC-059, ORC-062. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="aud-11"></a>

### AUD-11 — I,S

**Setup/attack:** Correct one defect but introduce another required-behavior regression.

**Required observation:** The new target does not inherit the old pass or close solely because the original finding was fixed.

**Requirements:** ORC-052, ORC-058, ORC-062. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="aud-12"></a>

### AUD-12 — I

**Setup/attack:** Run checks that create caches/temp files; attempt an edit to product code or the Agreement.

**Required observation:** Checks run in owned scratch; the target/authority remain unchanged. Forbidden writes are denied where claimed, otherwise detected and the affected verdict cannot be trusted.

**Requirements:** ORC-019, ORC-063. **Applicability:** core. **Status:** NOT EXECUTED.


## Artifacts and lineage

<a id="art-01"></a>

### ART-01 — D,I

**Setup/attack:** Edit one byte of each finalized artifact component separately, including a condition or parent reference.

**Required observation:** Required integrity verification detects the modification; no stale success is reused. Independently compute expected digests from retained bytes.

**Requirements:** ORC-027, ORC-040, ORC-054, ORC-066, ORC-068. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="art-02"></a>

### ART-02 — D,I

**Setup/attack:** Remove a component, truncate a manifest, change its schema version, or add duplicate required keys.

**Required observation:** A clear structural/integrity error; no silent defaulting of missing authoritative fields. Unknown future formats are not treated as current ones.

**Requirements:** ORC-005, ORC-064, ORC-081. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="art-03"></a>

### ART-03 — D,I

**Setup/attack:** Supply a final-looking Markdown file without a successfully published finalization record.

**Required observation:** Mere file existence does not establish finalized public output.

**Requirements:** ORC-065. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="art-04"></a>

### ART-04 — I

**Setup/attack:** Inspect, list lineage, and rerun a read-only check repeatedly.

**Required observation:** Finalized bytes/identity remain unchanged; no hidden model work or reinterpretation.

**Requirements:** ORC-066, ORC-075. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="art-05"></a>

### ART-05 — I

**Setup/attack:** Delete or corrupt the optional global index, then rebuild it.

**Required observation:** The same complete retained lineage can be reconstructed from authoritative manifests. Missing parents remain missing, not invented.

**Requirements:** ORC-015, ORC-067, ORC-069, ORC-075. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="art-06"></a>

### ART-06 — D,I

**Setup/attack:** Inject cyclic, cross-effort, dangling, or conflicting parent references.

**Required observation:** The lineage validator reports the actual structural problem; traversal terminates and does not select a plausible alternative.

**Requirements:** ORC-015, ORC-067. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="art-07"></a>

### ART-07 — I

**Setup/attack:** Read a bundle while another process finalizes it.

**Required observation:** Reader sees an incomplete state or a complete valid final artifact, never a mixture advertised as complete.

**Requirements:** ORC-065, ORC-074. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="art-08"></a>

### ART-08 — I

**Setup/attack:** Race two finalizers for one logical output.

**Required observation:** No torn or conflicting successful publications. Identical retries follow declared idempotency; different payloads cannot silently overwrite each other.

**Requirements:** ORC-065, ORC-066, ORC-071, ORC-074. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="art-09"></a>

### ART-09 — I

**Setup/attack:** Produce v2 after a build/audit referenced v1.

**Required observation:** The earlier records remain historically true; status does not relabel them as having used v2.

**Requirements:** ORC-015, ORC-047, ORC-058. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="art-10"></a>

### ART-10 — D,I

**Setup/attack:** Same bytes are serialized/reordered under supported encoding rules; use known hash vectors.

**Required observation:** Identity follows the declared byte/canonicalization contract consistently. Test fixed vectors as well as round-trips, so producer and consumer cannot share a hidden encoding bug.

**Requirements:** ORC-064. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="art-11"></a>

### ART-11 — I

**Setup/attack:** Keep only finalized public artifacts and required exact-target material; remove upstream run internals.

**Required observation:** Downstream phases still operate through the intended contract. Missing material genuinely required by the public contract is reported precisely.

**Requirements:** ORC-029, ORC-033, ORC-044, ORC-064, ORC-069, ORC-087. **Applicability:** core. **Status:** NOT EXECUTED.


## Recovery and concurrency

<a id="rec-01"></a>

### REC-01 — I

**Setup/attack:** Kill the CLI during run creation or initial request capture.

**Required observation:** Restart reports incomplete/failed state or a valid committed operation; no phantom completed run.

**Requirements:** ORC-031. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="rec-02"></a>

### REC-02 — I

**Setup/attack:** Fail a temporary artifact write or rename immediately before final publication.

**Required observation:** No consumer sees a successful incomplete artifact; prior valid state remains inspectable.

**Requirements:** ORC-065. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="rec-03"></a>

### REC-03 — I

**Setup/attack:** Kill after all output bytes are written but before the finalization boundary.

**Required observation:** Files alone do not authorize downstream consumption. Recovery follows the documented incomplete-state path.

**Requirements:** ORC-065. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="rec-04"></a>

### REC-04 — I

**Setup/attack:** Kill after finalization commits but before the caller receives success.

**Required observation:** Retry/recovery discovers the committed result; no duplicate conflicting publication or extra semantic work merely because acknowledgement was lost.

**Requirements:** ORC-065, ORC-071, ORC-072. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="rec-05"></a>

### REC-05 — I

**Setup/attack:** Provider/check process starts; controller dies before recording its outcome.

**Required observation:** External outcome is unknown unless independently recovered. No automatic replay that assumes the effect never occurred.

**Requirements:** ORC-072. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="rec-06"></a>

### REC-06 — I

**Setup/attack:** Cancel a long-running operation normally; then force-kill a separate trial.

**Required observation:** Terminal/cancellation records accurately distinguish the paths; confirmed evidence remains retained; ordinary graceful cleanup is not mistaken for crash qualification.

**Requirements:** ORC-073. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="rec-07"></a>

### REC-07 — I

**Setup/attack:** Child hangs, ignores normal termination, or leaves a descendant running.

**Required observation:** Enforce the declared supported termination/cleanup contract with bounded waits. Any unproven descendant cleanup is surfaced, not claimed complete; unrelated processes are never targeted.

**Requirements:** ORC-021, ORC-073. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="rec-08"></a>

### REC-08 — D,I

**Setup/attack:** Retry the same logical mutation with identical payload, then with a different payload and reused identity.

**Required observation:** Identical retry does not duplicate effects; conflicting reuse receives a clear error under the selected operation policy.

**Requirements:** ORC-071. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="rec-09"></a>

### REC-09 — I

**Setup/attack:** A stale result arrives after cancellation, expiry, or a newer attempt.

**Required observation:** It cannot silently finalize the current attempt or overwrite newer authority. Preserve the late result as diagnostic material where appropriate.

**Requirements:** ORC-073. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="rec-10"></a>

### REC-10 — I

**Setup/attack:** Simulate write failure, permissions failure, or unavailable artifact storage during recovery itself.

**Required observation:** The second failure is retained honestly; no false successful recovery or loss of the last trustworthy artifact.

**Requirements:** ORC-072. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="rec-11"></a>

### REC-11 — I

**Setup/attack:** Race operations in independent runs and independent efforts; attempt concurrent operations on the same active run.

**Required observation:** Independent local operations cannot corrupt shared records. Same-run conflicting writes serialize or reject. Parallel model dispatch is optional, but any unsupported request for it is rejected before launch and before scratch sharing.

**Requirements:** ORC-074. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="rec-12"></a>

### REC-12 — D,I

**Setup/attack:** Generate sequences of valid and invalid lifecycle actions and restart points.

**Required observation:** Public observations agree with a separately written minimal reference state model. Persist/shrink failures; add separate barriers-based tests for actual concurrent execution.

**Requirements:** ORC-090. **Applicability:** core. **Status:** NOT EXECUTED.


## Provider transport, evidence execution, and limits

<a id="pro-01"></a>

### PRO-01 — I

**Setup/attack:** Correct provider response, emitted through the real subprocess/protocol parser.

**Required observation:** Only validated data reaches the normal finalizer. The emulator does not write the final bundle or bypass production operations.

**Requirements:** ORC-078, ORC-089. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="pro-02"></a>

### PRO-02 — D,I

**Setup/attack:** Empty, truncated, malformed, oversized, wrong-schema, or unexpected-tool response.

**Required observation:** Bounded diagnostic and no falsely accepted result. Any allowed repair attempt is explicit in policy and counts.

**Requirements:** ORC-043, ORC-077, ORC-078. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="pro-03"></a>

### PRO-03 — I

**Setup/attack:** Emit events fragmented across reads, duplicated, reordered where invalid, or attributed to another invocation.

**Required observation:** Parser/state association is correct; duplicate output is not another vote/completion. Invalid ordering fails safely without a hang.

**Requirements:** ORC-071, ORC-076, ORC-078. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="pro-04"></a>

### PRO-04 — I

**Setup/attack:** Authentication failure, unavailable provider executable, capability mismatch, nonzero exit, or early EOF.

**Required observation:** Errors are classified; no silent provider/model substitution and no model-confidence report pretending the task completed.

**Requirements:** ORC-072, ORC-076. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="pro-05"></a>

### PRO-05 — D,I

**Setup/attack:** Exceed configured calls, output bytes, diagnostic size, or time limit.

**Required observation:** The declared bound is enforced at its actual control point. A soft admission budget is not advertised as a hard cap on already-running provider usage.

**Requirements:** ORC-073, ORC-077. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="pro-06"></a>

### PRO-06 — I

**Setup/attack:** Provider/check prints success while returning a failure; returns zero with no result; emits relevant errors to stderr.

**Required observation:** The engine retains all relevant observations and uses documented completion rules, not reassuring text or exit zero alone. Semantic proof still requires appropriate evidence.

**Requirements:** ORC-025, ORC-057. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="pro-07"></a>

### PRO-07 — I

**Setup/attack:** Process input includes shell metacharacters, quotes, spaces, and Unicode.

**Required observation:** Values are passed as intended arguments/data; no shell interpolation or altered target identity.

**Requirements:** ORC-070. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="pro-08"></a>

### PRO-08 — D,I

**Setup/attack:** Attempt to run an unapproved/destructive/live-service check described in model prose.

**Required observation:** The approved execution boundary is enforced. A prose suggestion is not authority to mutate production or run arbitrary extra commands.

**Requirements:** ORC-019, ORC-020. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="pro-09"></a>

### PRO-09 — I

**Setup/attack:** Provide credentials/config for the live adapter and inject secret sentinels into environment and stderr.

**Required observation:** Authorized authentication works without credentials leaking into prompts/artifacts/normal diagnostics; retained sensitive evidence follows explicit policy.

**Requirements:** ORC-020, ORC-080, ORC-091. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="pro-10"></a>

### PRO-10 — D,I

**Setup/attack:** Configure a three-attempt validation-repair allowance; reject two payloads and then return a valid payload. Repeat with allowance exhausted. Also test the one-attempt default.

**Required observation:** The predeclared allowance is enforced, attempts and feedback are retained, no valid semantic answer is retried to improve confidence, and exhausted attempts cannot fabricate finalization.

**Requirements:** ORC-041, ORC-043, ORC-078. **Applicability:** core. **Status:** NOT EXECUTED.


## Provenance and reports

<a id="meta-01"></a>

### META-01 — D,I,H

**Setup/attack:** A run is operated in Cursor while a configured managed role executes on a different provider/model.

**Required observation:** Host, execution provider, model and effort are distinct, attributable fields. The human-facing source label does not falsely imply execution identity.

**Requirements:** ORC-018, ORC-050, ORC-076, ORC-079, ORC-085. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="meta-02"></a>

### META-02 — D,I,H

**Setup/attack:** Host does not expose the actual model or reasoning setting; the operator supplies a value instead.

**Required observation:** Unknown actual metadata remains unknown; declared/reported settings are not promoted to observed facts.

**Requirements:** ORC-017, ORC-025, ORC-076, ORC-079. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="meta-03"></a>

### META-03 — I

**Setup/attack:** Compare manifest and generated frontmatter for host/model/effort, baseline, run ID, timestamps, and guide identity.

**Required observation:** They agree because presentation derives from recorded provenance. No need for manual frontmatter repair and no generic “run 1” as the only identifying information.

**Requirements:** ORC-079, ORC-081. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="meta-04"></a>

### META-04 — D,I

**Setup/attack:** Return valid inconclusive consensus, blocked verification, malformed output, and transport failure.

**Required observation:** Reports distinguish semantic result from operational state. “Command completed” does not mean Agreement adopted or implementation conforming.

**Requirements:** ORC-006, ORC-032, ORC-042, ORC-061, ORC-082. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="meta-05"></a>

### META-05 — I

**Setup/attack:** Fail halfway through a trial and inspect the test report.

**Required observation:** Retain fixture identity, actual commands/inputs, relevant outputs, observed state, and failure classification. Do not store only “assertion failed.”

**Requirements:** ORC-082, ORC-090. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="meta-06"></a>

### META-06 — D,I

**Setup/attack:** Test report contains missing/filtered/skipped lanes, unknown usage, or deferred Build.

**Required observation:** No claim of complete qualification. Unknown token/cost fields are not zero; cached/input/output/effort are not casually added into a fabricated billing number.

**Requirements:** ORC-077, ORC-082, ORC-090, ORC-092, ORC-094. **Applicability:** core. **Status:** NOT EXECUTED.


## Crate boundaries

<a id="arch-01"></a>

### ARCH-01 — D,I

**Setup/attack:** A fixture consumer tries to import a producer's private application/storage internals.

**Required observation:** Compilation fails for the prohibited access. Public artifact consumers still compile and work.

**Requirements:** ORC-087. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="arch-02"></a>

### ARCH-02 — D

**Setup/attack:** Add a forbidden dependency, alias, feature-gated edge, or re-export between phase implementations.

**Required observation:** The dependency-policy check fails. Crates alone do not prevent someone editing Cargo.toml to create a new dependency.

**Requirements:** ORC-087, ORC-088. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="arch-03"></a>

### ARCH-03 — D,I

**Setup/attack:** Exercise public contract parsing/validation without producer runtime or database initialized.

**Required observation:** Contract code does not need live producer internals or implicit state mutation.

**Requirements:** ORC-064, ORC-087. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="arch-04"></a>

### ARCH-04 — D,I

**Setup/attack:** Deliberately corrupt shared encoding/path helpers; run consumer tests using fixed external vectors.

**Required observation:** Independent expected values catch common-mode failures that shared round-trip helpers could conceal.

**Requirements:** ORC-088, ORC-090. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="arch-05"></a>

### ARCH-05 — I

**Setup/attack:** Build the release artifact, install elsewhere, and test guide/phase resources.

**Required observation:** Embedded/bundled resources match the actual product version; missing source files or different cwd do not break installed execution.

**Requirements:** ORC-005, ORC-053, ORC-084. **Applicability:** core. **Status:** NOT EXECUTED.


## Complete journeys

<a id="e2e-01"></a>

### E2E-01 — I

**Setup/attack:** Full valid chain: three scripted investigator runs -> Consensus -> Agreement -> external known-good implementation -> Audit.

**Required observation:** All production boundaries execute; lineage joins exact inputs/outputs; the result is useful and conforming; no unauthorized phase continuation.

**Requirements:** ORC-001, ORC-003, ORC-089. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="e2e-02"></a>

### E2E-02 — I,S

**Setup/attack:** Same chain, but external Build intentionally leaves one Agreement requirement broken, then supplies a correction.

**Required observation:** First audit identifies the violation; second can pass; both exact targets and their history remain. This tests rejection and successful closure.

**Requirements:** ORC-001, ORC-052, ORC-062, ORC-089. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="e2e-03"></a>

### E2E-03 — I

**Setup/attack:** Finalize against one baseline; attempt substitution of a newer source, different request, or different Agreement at each handoff.

**Required observation:** Each affected boundary catches the mismatch. A valid-looking later artifact does not repair an invalid earlier link automatically.

**Requirements:** ORC-012, ORC-054. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="e2e-04"></a>

### E2E-04 — I

**Setup/attack:** Interrupt/restart at publication and external-effect checkpoints throughout the journey.

**Required observation:** No partial success, fabricated evidence, duplicate vote, or silently repeated uncertain operation; independently reconstructable history.

**Requirements:** ORC-089. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="e2e-05"></a>

### E2E-05 — H,S

**Setup/attack:** Actual installed low-strength host performs the small Discovery task; real Consensus/Audit roles consume their permitted artifacts.

**Required observation:** Real guide/skill/provider flow succeeds with honest provenance. The external Build step remains explicitly external; repeat in each claimed host/surface.

**Requirements:** ORC-092, ORC-093. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="e2e-06"></a>

### E2E-06 — I

**Setup/attack:** Repeat E2E-01 in two efforts with identical labels and overlapping timings; delete the optional index.

**Required observation:** No cross-effort contamination or retargeting; rebuilt status/lineage identifies both chains accurately.

**Requirements:** ORC-011, ORC-067, ORC-074. **Applicability:** core. **Status:** NOT EXECUTED.


## Platform and size qualification

<a id="env-01"></a>

### ENV-01 — I

**Setup/attack:** Run packaged-path/process/recovery cases natively on the user's macOS environment.

**Required observation:** No `/var` spelling workaround or assumed Linux equivalence; record OS, filesystem and tool versions actually tested.

**Requirements:** ORC-086, ORC-094. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="env-02"></a>

### ENV-02 — I

**Setup/attack:** **Conditional: additional platforms.** Run the same declared contract on Linux/Windows when supported.

**Required observation:** Real execution rather than cross-compilation alone qualifies behavior. Unsupported cases remain visible and are not counted as passes.

**Requirements:** ORC-094. **Applicability:** conditional. **Status:** NOT EXECUTED.

<a id="env-03"></a>

### ENV-03 — D,I

**Setup/attack:** Empty/minimal, typical, at-limit, and over-limit artifacts, logs, file counts, and path sizes.

**Required observation:** Useful processing within declared limits; explicit bounded errors beyond them; no silent truncation of mandatory content.

**Requirements:** ORC-014, ORC-077, ORC-094. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="env-04"></a>

### ENV-04 — I

**Setup/attack:** Many retained runs, rebuildable index absent, and multiple simultaneous readers.

**Required observation:** Correct status/lineage without unacceptable unbounded growth or inconsistent snapshots. Record actual resource use; choose thresholds from intended personal workload rather than arbitrary large-system targets.

**Requirements:** ORC-094. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="env-05"></a>

### ENV-05 — I

**Setup/attack:** Disposable fixture has no network, unexpected ambient host configuration, or missing external test service.

**Required observation:** Offline lanes remain offline. Dependencies are explicit; environment failures are visible and do not become false product verdicts.

**Requirements:** ORC-080, ORC-091. **Applicability:** core. **Status:** NOT EXECUTED.


## Additional consolidated-policy cases

<a id="acx-01"></a>

### ACX-01 — D,I,S

**Setup/attack:** All opinions omit one original user rule; then try hiding an additional single-opinion feature in narrative or latitude.

**Required observation:** The governing rule is accounted for with source authority, not fabricated votes. Omitted required behavior or an unbound extra prevents readiness. Build/Audit cannot use an inferior plan to override the Agreement.

**Requirements:** ORC-004, ORC-010, ORC-040, ORC-044, ORC-051. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="acx-02"></a>

### ACX-02 — I,H

**Setup/attack:** Run the ordinary supported user journey with only request, project, chosen phase, selectors and approval.

**Required observation:** The user does not create UUIDs, manipulate database rows, or hand-author JSON; generated IDs and focused errors remain available to the operator.

**Requirements:** ORC-007. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="acx-03"></a>

### ACX-03 — D,I,H

**Setup/attack:** Mark an opinion known contaminated after publication, attempt to count it, replace its cohort slot, and attempt to count both runs.

**Required observation:** A separate invalidation notice preserves history. Known-compromised input is not eligible as a clean slot. Explicit replacement does not change the fixed denominator or create an extra vote; actual isolation remains honestly labeled.

**Requirements:** ORC-011, ORC-017, ORC-018, ORC-035, ORC-066. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="acx-04"></a>

### ACX-04 — D,I

**Setup/attack:** A check script edits a candidate implementation or requests an outside/live-service operation; compare its passing result with the untouched target.

**Required observation:** The permitted runner enforces its boundary, or the affected safety claim is unqualified. Probe-modified code is never evidence that the original target passes. No production data or target repository is changed.

**Requirements:** ORC-019, ORC-020, ORC-057, ORC-063. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="acx-05"></a>

### ACX-05 — I,S

**Setup/attack:** Complete Discovery -> Consensus -> adoption -> external registration -> Audit for a request already satisfied at the baseline.

**Required observation:** The same original commit is a valid target. No artificial requirement, code edit, or new product commit is necessary. The audit still checks the preservation/no-change contract and can pass.

**Requirements:** ORC-026, ORC-052. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="acx-06"></a>

### ACX-06 — D,I,S

**Setup/attack:** Supply a majority-backed proposal with an unresolved concrete counterexample, then a similar proposal with only non-material dissent.

**Required observation:** Both endorsement counts are accurate. The first is non-adoptable until narrowed/resolved from permitted evidence; the second can yield an eligible contested-majority package. No new research is silently run.

**Requirements:** ORC-037, ORC-039. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="acx-07"></a>

### ACX-07 — D,I

**Setup/attack:** Finalize G1, attempt downstream authority use before adoption, adopt G1, alter bytes, and attempt to reuse adoption for G2.

**Required observation:** Unadopted, ineligible, altered, or wrong-version authority is refused. Adoption is a separate exact-reference receipt; no forced consensus override exists.

**Requirements:** ORC-045, ORC-046, ORC-047. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="acx-08"></a>

### ACX-08 — D,I,S

**Setup/attack:** Adopt a conditional Agreement, then try to mark its prerequisite satisfied or not applicable without evidence.

**Required observation:** The condition remains attached to the affected obligations. Adoption is not evidence of condition satisfaction. No dependent completion or Audit PASS is granted without appropriate support or genuine non-applicability.

**Requirements:** ORC-046, ORC-056. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="acx-09"></a>

### ACX-09 — D,I

**Setup/attack:** Register externally authored code with claimed model, baseline, and test history that the application did not observe.

**Required observation:** Exact target and declared authority are checked, but historical authorship/production claims remain declared. No misleading statement that Orchestrate built or observed the preparation appears.

**Requirements:** ORC-048, ORC-050. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="acx-10"></a>

### ACX-10 — D,I,S

**Setup/attack:** Return a reassuring audit statement with one missing coverage row; use a fresh assessor and repeat with a substantiated defect plus an authority blocker.

**Required observation:** The missing row prevents PASS. The fresh assessment cannot be replaced by worker self-review. A proven defect yields CHANGES_REQUIRED with authority/evidence blockers retained; a failed invocation has no verdict.

**Requirements:** ORC-055, ORC-056, ORC-061. **Applicability:** core. **Status:** NOT EXECUTED.

<a id="acx-11"></a>

### ACX-11 — D,I,H,S

**Setup/attack:** Deliberately break a critical guard, filter a required test to zero executions, and run known-good/known-bad grader controls.

**Required observation:** Independent controls catch the fault. Zero/filtered/skipped work cannot qualify a lane. All trial outcomes, supported surfaces, deferred capabilities, and NOT_EXECUTED results remain visible.

**Requirements:** ORC-090, ORC-091, ORC-093, ORC-094. **Applicability:** core. **Status:** NOT EXECUTED.


# 7. Detailed scenario recipes

These six retained fixture recipes elaborate the cases above. Where a recipe references a policy, use the resolved product requirements—not the earlier proposal’s open alternatives.


### Recipe 1 — Reproduce the poisoned Discovery failure

**Cases:** ID-05, SRC-02, ISO-01 through ISO-04, ISO-08.

Create committed source A and record independent checksums of the protected target files/configuration. Add an original untracked file containing a sentinel; it must remain intact but must not enter a committed-baseline investigation. Initialize a request and pin A once.

Run investigator A through public operations. Have its experiment create `repro.rs` and a plausible but intentionally wrong recommendation containing another sentinel. These belong to A's scratch/evidence, not source. Start investigator B from A's source baseline without copying A's outputs or conversation. Check B's actual workspace, provided packet, exposed refs, and authorized tool responses.

The structural pass condition is unchanged original source/configuration, correct pinned source in B, and no inclusion of A's generated material. Separately issue a deliberate read of A's absolute path through the actual isolation boundary. It must fail before claiming enforced peer isolation.

Finally run an ordinary live B investigator in that setup and grade whether peer material appears or influences the recommendation. Absence of the exact sentinel alone is not proof of no influence. Preserve the distinction between excluded inputs, denied access, and merely observed non-use. A failing isolation attempt is a design gap to fix or honestly scope, not a reason to make the grader less strict.

### Recipe 2 — Coherent majority without accidental extra scope

**Cases:** CON-01 through CON-07, CON-11, CON-12.

Use explicit propositions:

- R1: Creating a booking never automatically creates a group.
- R2: A booking can belong to multiple groups.
- P: Preserve the current public operation names while changing membership storage.
- Z: Add automatic group suggestions in the interface.

R1/R2 are in the user's request and cannot be deleted by voting. Opinion A supports R1, R2 and P. Opinion B supports R1, R2 and P in different wording. Opinion C supports R1/R2 but recommends a different public naming scheme. Only C proposes Z, which is outside the requested scope.

Expected: preserve R1/R2; a P-based package can be labeled 2/3-supported under the approved consensus policy; C's disagreement remains visible; Z is not mandatory. Test explicit matrices mechanically and original prose semantically. Test the model rendering boundary with a valid adopted set plus a sneaked-in paragraph requiring Z.

Create a separate rotating-majority variant: A/B support X, B/C support Y, but only B supports X+Y. Require no fabricated common-majority label for X+Y. The expected result may retain point-level findings or a narrower adequate supported package; do not arbitrarily demand a specific losing alternative.

### Recipe 3 — A success string is not implementation evidence

**Cases:** DISC-07, AUD-02 through AUD-07, PRO-06.

Provide a target with the automatic-group-creation defect. Include a script/check that exits zero and prints “all tests passed” without exercising group creation. Provide a real independent behavioral probe that demonstrates group creation from a booking operation. Run both through the declared evidence executor.

Mechanical expectations: receipts identify the actual command, target, exit status, outputs and origin. The success text must not directly authorize conformance. Semantic expectation: the auditor explains the demonstrated R1 violation rather than accepting the superficial green message.

Variant: make the behavioral probe unavailable because its fixture environment cannot start. The correct result is then a verification limitation, not a fabricated counterexample or pass. Verify the grader distinguishes those outcomes.

### Recipe 4 — Crash on each side of finalization

**Cases:** ART-03, ART-07, ART-08, REC-01 through REC-05.

Use test-only barriers to stop the real CLI at each publication checkpoint. Have the parent harness kill it, then launch a fresh process to inspect/recover. Independently read retained files and identity records.

Before the finalization boundary, downstream consumption must not treat partial files as valid final output. After the boundary but before caller acknowledgement, inspection must find the committed result. Recovery must not make a second contradictory publication.

For an external effect, use a helper that records a nonce when the effect occurs, then withholds acknowledgement. Kill the controller in the gap. Recovery cannot assert “never executed.” Do not infer exactly-once guarantees from an in-process mocked error.

### Recipe 5 — Agreement revisions do not rewrite audit history

**Cases:** BLD-04, ART-09, AUD-05, AUD-06.

Publish Agreement G1, register implementation I1, and audit that exact pair. Publish G2 with one changed requirement. Move the working checkout HEAD to I2. Then inspect the old audit and attempt to use it as conformance evidence for G2/I2.

Expected: old audit remains bound to G1/I1. Current status may show it is not applicable to G2/I2; it must not change its historical inputs. Receipts remain attached to the original checks. A new audit may reuse evidence only with explicit applicability, not by relabeling the old invocation as new.

### Recipe 6 — Repair closes rather than expanding into a new project

**Cases:** AUD-01, AUD-09 through AUD-11, E2E-02.

Audit a faulty implementation and record a bounded required correction. Provide a correct repair and a separate optional refactoring suggestion. The next audit should close the finding and may pass. Then test a second repair variant that breaks a different original requirement; that variant must not pass.

Include a separate inconsistent Agreement variant. The auditor must surface an authority problem instead of choosing new product meaning or demanding unrelated implementation work. This tests the feedback destination without authorizing automatic phase progression.


# 8. Additional no-change journey

Create a target that already satisfies the explicit requested behavior. Produce three finalized justified no-change opinions, form and adopt the preservation Agreement, register the existing commit as the external implementation, and Audit it with adequate verification. It must be possible to reach PASS without generating a fake task, product patch, or new commit. Then break a required preservation behavior in another exact target and verify that Audit refuses conformance. This is ACX-05 and exercises positive completion, not merely refusal safety.

# 9. Required execution-report fields

Keep execution reports separate from the specification. Record product build/revision, spec version/digest, fixture identities, selected cases and actual count executed, layers exercised, exact platform/host surface, model configuration and observed identity, input inventories, retry/limit policy, all trial outcomes, actual effects/receipts, and unresolved limitations.

Use explicit result categories: PASS, FAIL, BLOCKED, NOT_EXECUTED, UNSUPPORTED, and DEFERRED. Do not present the internal Build engine as qualified by external registration tests. Do not claim enforced peer access from merely observing that a model did not quote a peer document.

`requirements.json` contains the same requirement/case mapping for machine use. It is generated from this package, not runtime product state. An implementation test manifest should reference these IDs and append actual execution evidence; it should not reword the acceptance requirement to match whatever code currently does.
