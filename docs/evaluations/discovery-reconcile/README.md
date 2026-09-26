# Discovery/Reconcile evaluation

This procedure tests whether a model can reconcile substantive but conflicting Discovery
outputs without reopening source. It is a manual model evaluation, not a CLI admission test,
production reconciliation, or Build request. It contains no selected project or expected
product solution. Keep project-specific inputs, scope answers, and results outside this repository.

## Select a case

Explicitly select two or more published Discovery directories. Record their exact paths and
artifact IDs in an external evaluation directory, assigning labels A, B, and so on. Keep the
full specifications and public graphs available. Do not select substitute or newest artifacts
when an input is unavailable; record the evaluation as not run.

To exercise the full rubric, choose outputs that contain a material scope conflict, competing
directions, compatibility evidence, an unresolved factual question, and verification limitations.
Before the run, record the expected evidence citations and any inapplicable rubric criteria in
an operator-only case sheet. Derive expectations from those outputs; do not invent missing
facts or require a particular product architecture.

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
