# Discovery/Reconcile evaluation: AGE-351

This case tests whether a model can reconcile substantive but conflicting Discovery outputs
without reopening source. It uses three actual historical public bundles, unchanged. It is a
manual model evaluation, not a CLI admission test, production reconciliation, or Build request.

The bundles are external inputs, not checked-in fixtures. Do not copy their application or
patient-linked contents into this repository. Their schema, prior hash failures, and differing
context identities are deliberately excluded from the evaluation's admission criteria. That
exception applies only to this historical evaluation; production validation remains unchanged.

## Selected inputs

Locate the existing published directories by these exact artifact IDs:

- A: `discovery-c96cfdfdf38709ea`
- B: `discovery-3f34cea1a85a9c6d`
- C: `discovery-6bd6a1dfeb752983`

Use only those three. Do not select the newest artifacts, discover substitutes, repair manifests,
add verification classifications, or re-finalize the bundles. Keep all graph nodes and the full
specifications available; do not replace them with the earlier audit's summary. If the files are
unavailable, record the evaluation as not run rather than grading substitute inputs.

## Procedure

1. Create an evaluation directory outside the application and Orchestrate repositories. Record
   the exact input paths, model/provider and reasoning setting when known, evaluation date, and
   Orchestrate revision or binary version. Keep the transcript and results there.
2. Capture `orchestrate reconcile guide` from the binary being evaluated into that directory.
   Use a binary built from the intended revision; editing the embedded guide does not update an
   already installed binary. Preserve this exact guide snapshot with the results.
3. Start a fresh high-strength model session without previous review/discovery conversation
   history. Supply the captured guide and [model-prompt.md](model-prompt.md), replacing its three
   path placeholders. Its explicit historical-evaluation exception overrides the guide's CLI
   admission, finalization, and output-format instructions for this exercise only. Supply no
   other application evidence. Keep this README, the operator rubric, previous reviews, and
   expected results out of the model's context.
4. Save the initial response and any tool-access transcript. If the model asks a scope question,
   reply with [scope-agents-only.txt](scope-agents-only.txt), verbatim. This is an explicitly
   hypothetical evaluation answer, not a new instruction for the real project. Save the
   resulting synthesis as a second response. Do not supply rubric feedback during the run.
5. Grade both responses with [operator-rubric.md](operator-rubric.md). Use
   [result-template.md](result-template.md) for the record. Cite actual response passages or
   tool accesses for each judgment. A correct clarification request is a successful initial
   result, not a reason to invalidate the attempts or abandon the evaluation.

If the initial response chooses a scope without asking, retain that response and grade it; do
not erase the failure by restarting. The hypothetical answer may still be supplied afterward
to evaluate recovery, but distinguish recovery from a successful initial response.

For repeat runs, use fresh sessions and the same input set, guide snapshot, prompt, and scope
answer. Report each run; do not select only the best response. Record changes in model/settings
or guide separately. Do not infer current Discovery quality from performance on these older
outputs.

## Follow-up with current Discovery

The historical case evaluates synthesis. To evaluate the updated publication guidance, run
fresh independent Discoveries from one unchanged prepared request using the intended current
binary. Keep run-local answers separate and inspect their published outputs for the four
publication checks in the embedded Discovery guide. Record observed successes and failures;
do not claim improvement merely because the guide text changed. This follow-up is a separate
evaluation, not a prerequisite for accepting the historical attempts above.
