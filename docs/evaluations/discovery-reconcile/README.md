# Discovery/Reconcile evaluation

This procedure tests whether models preserve meaning while authoring Discovery, reconciling
selected outputs, planning Build, and preparing Chat Discovery. Its original Reconcile exercise
tests synthesis without reopening source. These are manual model evaluations, not CLI admission
tests, production reconciliation, or Build requests. This directory contains no selected project
or expected product solution. Keep project-specific inputs, scope answers, case sheets, prompts,
transcripts, raw outputs, and access records outside this repository. The issue-29 effort is the
recording exception: its sanitized per-case status summary is checked in, while all raw and
project-specific records remain external.

## Select a Reconcile case

For the Reconcile exercise, explicitly select two or more published Discovery directories.
Record their exact paths and artifact IDs in an external evaluation directory, assigning labels
A, B, and so on. Keep the full specifications and public graphs available. Do not select substitute or newest artifacts
when an input is unavailable; record the evaluation as not run.

To exercise the original full synthesis rubric, choose outputs that contain a material scope
conflict, competing directions, compatibility evidence, an unresolved factual question, and verification limitations.
Before the run, record the expected evidence citations and any inapplicable rubric criteria in
an operator-only case sheet. Derive expectations from those outputs; do not invent missing
facts or require a particular product architecture.

## Acceptance-meaning cases

Use contrasting cases to test both a missed conflict and unnecessary rejection. Record each
case's role, exact input boundary, supported interpretations, and expected behavior in an
operator-only case sheet. Do not add the table, case sheet, diagnosis, or expected answer to a
model's prompt. Keep meaningful full context rather than selecting only favorable excerpts.

| Case | Behavior to assess |
| --- | --- |
| Historical selected outputs | Reconcile traces what a health predicate and a passing live suite each assert, preserves relevant exclusions, and gives a supported disposition or asks about a genuinely unresolved choice. Do not grade only one product outcome as correct or infer a cause, remedy, waiver, or impossibility from a reported failure. |
| Explicit hard live prerequisite with pending result | Retains the gate and its completion consequence without automatically rejecting semantic readiness or demanding fresh approval. |
| Explicit real-target response-handling acceptance | Allows correct handling of an unhealthy observation to pass while preserving other separately required live successes. Adds no healthy-target gate. |
| Ambiguous predicate plus stronger phase instruction | Planner detects the changed assertion and returns a material authority question instead of silently changing the contract. |
| Qualified exclusion | Preserves the condition, neither converting it into a permanent ban nor treating satisfaction of the condition as automatic permission for a remedy. |
| Routine dependency, test plan, or credential setup | Planner repairs the plan or arranges setup locally, without unnecessary Reconcile/Discovery or one task per Review. |
| Material missing authority or engineering fact | Returns a precise question through the existing upstream route; Reconcile stays within selected public evidence and Planner feedback supplies no engineering finding. |
| Published ready versus blocked | Ready gives the exact normal handoff; blocked names the unresolved issue and needed upstream action without advertising Build readiness. |

For reproducible variants, keep the behavioral facts fixed where possible and change the
explicit acceptance intent. Label synthetic variants as synthetic, leave published bundles
untouched, and never present them as historical records. If an original archive or other required
input is unavailable, record that case as **not run** rather than substituting the reviewer's
summary. A historical Reconcile replay receives only the originally selected public Discovery
outputs and permitted authority, not baseline source, phase reports, later incident analysis, or
this case sheet.

Production admission rules remain unchanged. If a historical case requires an analytical
exception for schema, integrity, or context differences, document the exact exception and its
limitations in the case sheet and model prompt before the run. Never silently bypass validation,
repair the inputs, or describe such an exercise as a successful production reconciliation.

## Procedure

1. In the external evaluation directory, record the selected inputs, model/provider and reasoning
   setting when known, evaluation date, and Orchestrate revision or binary version.
2. Capture `orchestrate reconcile guide` from the binary being evaluated. Use a binary built from
   the intended revision; editing an embedded guide does not update an already installed binary.
3. Prepare a copy of [model-prompt.md](model-prompt.md) with the selected paths and any explicitly
   scoped historical exception. Prepare a hypothetical answer using
   [scope-answer-template.txt](scope-answer-template.txt), selecting a direction already present
   in the outputs. Resolve all placeholders before the run. Keep the answer operator-only until
   the model asks a material scope question.
4. Start a fresh model session without previous review or Discovery conversation history. Supply
   the captured guide and completed prompt. Supply no other project evidence. Keep this README,
   the case sheet, rubric, previous reviews, and expected results out of the model's context.
5. Save the initial response and tool-access transcript. If the model asks the scope question,
   provide the prepared hypothetical answer verbatim and save the subsequent response. Do not
   supply rubric feedback during the run. If it asks a different question, record that question
   and any answer separately; do not treat a scope answer as factual evidence.
6. Grade both responses with [operator-rubric.md](operator-rubric.md) and the case sheet. Use
   [result-template.md](result-template.md) for the record, citing actual passages or tool accesses.
   A justified clarification request is a successful initial result.

If the initial response chooses a materially unresolved scope without asking, retain and grade
that response. A later hypothetical answer may evaluate recovery, but does not erase the initial
failure. For repeat runs, use fresh sessions and the same selected inputs, guide, prompt, and
answer. Report every run and changes in settings; do not select only the best response.

## Follow-up with current Discovery

To evaluate publication guidance separately, run independent Discoveries from one unchanged
prepared request using the intended binary. Keep run-local answers separate and inspect the
published outputs against the Discovery guide's publication checks. Record observed successes
and failures; neither guide edits nor synthesis of older outputs proves current Discovery quality.

## Build planning and Chat Discovery checks

Run fresh, isolated sessions for these roles when their inputs are available. Give Planner the
exact Reconciled contract and Build guide, plus phase drafts only for a phase-consistency case;
do not give it private Discovery evidence or ask it to perform Reconcile. Give Chat Discovery its
own supplied conversation and evidence, Chat Discovery guide, and optional phase documents when
the case includes them. Exercise Chat Discovery both with phases and without phases. A no-phase
import still needs coherent binding acceptance; it does not need a fabricated phase plan. Grade
the outputs with the separate role checks in the rubric. Record phase repairs, upstream returns, blockers,
and launch/readiness wording actually observed. Do not use these sessions as production imports
or launches.

For a ready/blocked handoff case, supply a simulated successful finalization result with its
outcome and artifact path, then assess the Reconcile guide's response. Test each outcome and a
separate failed-finalization result. Label these as simulations; do not publish or imply that
they are historical artifacts.

For every role, record exact inputs, guide revision or compiled binary version, model/provider
and settings when known, response, access transcript, and pass/fail/unobserved results. Capture
embedded guides from a binary built at the intended revision; edited Markdown is absent from an
older installed binary. If isolated model sessions or their inputs are unavailable, mark those
cases **not run** and state the limitation. Manual walkthroughs, guide-string tests, and prior
incident reports are not executed model evaluations.

## Chat Discovery and role-level evidence cases

The eight bounded case families below extend the existing evaluation convention. The short
descriptions and expected distinctions are operator guidance, not model inputs. Keep the exact
case sheet, expected answers, rubric, prior attempts, and incident diagnosis out of each evaluated
session. An evaluated session receives only its selected guide and the case's bounded input
packet. For Chat Discovery, that packet is the specified synthetic conversation/evidence plus the
optional phase documents named by the variant. For Work, Review, Final Audit, public Audit, and
Unblock, it is the exact role packet or packet-equivalent handoff and fixture checkout listed for
that case. Do not give any role another role's report unless the case explicitly includes it.

Retain the exact input bytes and fixtures in a private external evaluation directory. Identify
each case by an input SHA-256 or exact external path; identify a fixture by its archive/tree hash
or exact path. Use synthetic inputs when the original conversation or private historical packet
is unavailable, label them synthetic, and do not imply historical replay. If an input needed for a
historical replay is unavailable, record that replay as not run; an incident summary is not a
substitute. Do not check in synthetic transcripts, fixtures, raw results, private archives, or
project-specific content.

| ID | Case family | Applicable roles and variants | Bounded input supplied to the evaluated role | Operator-only distinction |
| --- | --- | --- | --- | --- |
| R-17 | Manual tutorial, optional copy shortcut, withdrawn generator | Chat Discovery, with phases and without phases | Synthetic settled conversation with the manual empty-directory tutorial decision, optional reuse of a ready example, and later withdrawal of only the generator; selected Chat Discovery guide; phase variant only when named | Preserve the manual construction and its starting conditions, keep copying optional, and do not add a generator or duplicate fixture. A copy-only canonical walkthrough fails once manual construction is binding. |
| R-18 | Later verifier rechecks an earlier invariant | Chat Discovery, with phases and without phases | Synthetic settled conversation and teaching examples about observing an earlier invariant at the current transition; selected Chat Discovery guide; phase variant only when named | Preserve the broader observation permission and teaching intent without inventing an arbitrary-earlier-verifier command or mandatory live HTTP integration. |
| R-19 | Existing-checkout success and fresh-checkout failure on an undocumented generated prerequisite | Work, Review, Final Audit, public Audit | One synthetic packet-equivalent contract; the role's exact packet; a fixture with an existing-build/copied-example success, a fresh-checkout failure on an undocumented generated prerequisite, documented setup facts, and peer UI/browser acceptance; selected role guide | Bound each observation to its starting state. Review connects the relevant failure to runnable onboarding; Audit requires evidence for the full claim rather than headings. Proper documented dependency setup is not presumed to fail. |
| R-20 | Documented setup succeeds or an unrelated optional check cannot run | Work, Review, Final Audit, public Audit | Synthetic packet-equivalent contract and fixture showing the documented prerequisite setup succeeds, plus a separate failed optional check unrelated to the claimed behavior; selected role guide | Accept the supported claim with bounded evidence, record the unrelated limitation, and do not impose a universal failed-command or dependency blocker. |
| R-21 | Meaningful current-phase result with explicitly later-phase proof | Work, Review | Synthetic Reconciled contract, current-phase packet and fixture showing valid phase acceptance while a named final-completion check remains assigned to a later phase; selected Work or Review guide | Accept the valid current phase, preserve and explicitly disposition the later obligation, and neither add a gate nor silently waive final completion. |
| R-22 | Overlapping guides | Review, Final Audit, public Audit | Synthetic documentation fixture with one concrete contradictory factual/procedural claim plus useful short repeated setup instructions; packet states the applicable consistency requirement; selected role guide | Correct the contradiction required by the contract and allow useful repetition. Do not invent a zero-duplication rule. |
| R-23 | Post-build discovery of omitted source intent | Chat Discovery, with phases and without phases | Synthetic post-build conversation and original binding contract that omit one newly reported intent; implementation evidence showing what the contract already requires; selected Chat Discovery guide; phase variant only when named | Classify existing-contract defects, lost/malformed authority, and new suggestions separately. Restore missing binding intent only through supported publication; do not smuggle it into unchanged-authority feedback or phase prose. |
| R-24 | Mixed governing and non-governing requirements | Final Audit, public Audit, Unblock | Synthetic Reconciled contract with governing and `governing: false` requirements, advisory technical suggestions, a conditional requirement, and an incomplete assessment/feedback packet; selected role guide | Require one evidence-supported row for every Reconciled requirement regardless of `governing`, no rows for technical suggestions, and preserve #28. Coverage is not substantive proof; `not_applicable` requires a false stated condition and justification. |

The role guide and product commit used for each comparison must be recorded separately. Work,
Review, and Final Audit are embedded strings: capture them from a binary built at the tested
revision. Public Audit is captured with that binary's `orchestrate audit guide`; Chat Discovery
uses the document at the tested revision. Never evaluate an old installed guide as if it were the
revised one. Keep the operator-only distinctions above out of the evaluated session.

Compare baseline `8cbd94efaf19d80112810077d1fb13950b7d0da4` with the revised implementation using
the same bounded input in fresh sessions and matched model/settings where available. Retain every
attempt, including initial failures and recovery, and assess both detection behavior and valid-case
false positives. Record setting differences instead of attributing unmatched results to the guide.
For each case, record an attempt status (`pass`, `fail`, `unobserved`, or `not run`), a specific
reason for every `not run`, observed or unobserved detection and false-positive results, and what
the comparison cannot establish. An authored case, keyword check, fixture-only result, routing
experiment, or not-run entry is not a behavioral model result. Execution and a particular score
are not completion gates; faithful guidance, retained cases, complete truthful records, and the
separate repository checks remain required.

## Issue-29 result recording

For this effort, check in one sanitized summary at
[`guidance-evidence-fidelity-summary.md`](guidance-evidence-fidelity-summary.md). For all eight
cases it records historical/synthetic status, exact external input identity or explicit
unavailability, old/revised guide provenance, roles and model settings (or `unknown`), each
attempt's actual status, specific not-run reasons, detection and false-positive status, differences,
and limitations. The summary records status; it does not authenticate inaccessible private runs.
Keep exact inputs, fixtures, prompts, transcripts, tool-access records, raw outputs, and
project-specific material external. Use the [result template](result-template.md) and apply the
[operator rubric](operator-rubric.md); never treat the expected distinctions in this README as
results.
