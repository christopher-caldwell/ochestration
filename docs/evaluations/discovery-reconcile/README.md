# Discovery/Reconcile evaluation

This procedure tests whether models preserve meaning while authoring Discovery, reconciling
selected outputs, planning Build, and preparing Chat Discovery. Its original Reconcile exercise
tests synthesis without reopening source. These are manual model evaluations, not CLI admission
tests, production reconciliation, or Build requests. This directory contains no selected project
or expected product solution. Keep project-specific inputs, scope answers, and results outside
this repository.

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
