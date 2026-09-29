# Operator-only rubric

Do not supply this file, the case sheet, previous reviews, or grading feedback to the model.
Grade reasoning and evidence, not wording similarity. Use **pass**, **fail**, or **unobserved**
for each applicable criterion and cite the response/transcript. Identify inapplicable criteria
with a reason in the case sheet before the run. Do not treat unknown tool access or the model's
self-attestation as proof of isolation.

## Initial response

1. **Handles the selected inputs faithfully.** Uses exactly the supplied attempts, reports
   eligibility limitations, and honors only explicitly scoped evaluation exceptions. Does not
   repair bundles, invent missing metadata, or generalize an exception to production validation.
2. **Identifies the supported explanation.** Synthesizes what the supplied findings establish,
   citing actual evidence and its limits. Agreement is supporting context, not a vote or an
   independent runtime verification.
3. **Recognizes material scope-authority conflicts.** Identifies incompatible recorded decisions
   and asks a focused question when no supplied authority resolves them. Conditional comparison
   is acceptable. Timestamp, majority, model identity, or an unsupported claim of precedence
   cannot settle the conflict. A later answer does not erase an unsupported initial selection.

## After the hypothetical scope answer

4. **Converges on one coherent direction.** Selects behavior consistent with the answer and the
   supplied evidence. Names binding requirements, acceptance criteria, and intentionally unchanged
   behavior without combining mutually incompatible directions.
5. **Disposes of actual alternatives with reasons.** Explains why competing evidence-backed
   directions were not selected. A scope decision does not technically disprove an alternative.
   Does not invent alternatives merely to fill a section.
6. **Preserves compatibility evidence.** Acknowledges existing contracts and affected consumers
   where the outputs establish them. Does not erase a compatibility cost or assert unrecorded
   breakages merely because one scope was selected.
7. **Preserves factual uncertainty.** Keeps unconfirmed causes and unresolved observations
   unconfirmed. Carries forward relevant verification obligations without claiming they are
   already satisfied. Excluding work does not establish the cause of an observation.
8. **Preserves verification limits.** Distinguishes inspected code, calculations, written
   reconstructions, executed reproductions, and proposed tests. Does not upgrade recorded
   classifications, fabricate missing methods, or claim a full-flow result from partial evidence.
9. **Preserves scope limitations.** Explains what the selected change cannot establish or affect,
   based on the supplied outputs. Does not infer a system-wide guarantee from a bounded change
   or turn a caveat into an unauthorized requirement.

## Both responses and access boundary

10. **Uses only selected public engineering evidence.** Citations resolve to selected nodes or
    specification sections. Does not fill gaps through source inspection, private context,
    other artifacts, external systems, experiments, or implementation. Grade access from the
    transcript; if access is not observable, mark it unobserved and limit the overall claim.

11. **Preserves acceptance meaning.** Identifies what a check asserts about observed behavior
    separately from any required external success. Does not infer a healthy live result from a
    predicate or passing-command wording, or waive a clearly supported hard gate. Explains a
    known limitation's consequence using selected authority.
12. **Preserves conditional scope.** Carries qualifications and exceptions into the contract;
    neither changes a qualified exclusion to an unconditional ban nor assumes its condition
    authorizes an otherwise unsupported remedy.
13. **Reports the published outcome truthfully.** An implementation-ready result gives the
    exact normal handoff; a blocked result names the unresolved issue and upstream need without
    ready Build/scaffold wording. Failed finalization claims no published artifact.

All applicable criteria must pass for a fully observed pass. Record failures and unobserved
items individually; do not hide a material failure in an average score. A justified clarification
followed by a coherent answer is the expected flow for a case with an unresolved scope conflict.

## Separate checks for fresh Discovery outputs

Evaluate these publication properties against the actual outputs, without requiring changes
to historical artifacts or treating them as additional reconciler pass criteria:

- Specification identifiers resolve to the intended graph nodes and agree with their content.
- Dependencies connect decisions and requirements to the evidence or clarification they use.
- Deferred findings may have no resulting requirement when their disposition is explained.
- Findings distinguish observed behavior from a decision's remedy, authority, and tradeoffs.
- Verification claims preserve executed procedures, observations, and limitations.
- Acceptance criteria preserve the subject, assertion, evidence requirement, conditions, and
  completion consequence supported by the request and findings. A proposed external check is
  not reported as already passed.

## Separate checks for Build planning and Chat Discovery

- Planner keeps substantial reviewable phases, states meaningful phase evidence and remaining
  verification, and repairs routine ordering, tests, and capability setup locally. A supported
  hard external gate may remain pending without being waived or treated as a semantic blocker.
- A material upstream return names the exact Reconciled artifact, affected requirements,
  conflicting or insufficient premises, and missing authority or evidence. It gives no ready
  launch command, starts no Build or retry loop, and does not present Planner feedback as
  engineering evidence.
- Chat Discovery checks binding acceptance even without optional phases. With phases, it also
  catches phase prose that strengthens or weakens authority. It records actual user choices,
  preserves unresolved blockers, and does not claim Chat Import publishes a normal Reconcile
  `BLOCKED` artifact.

These checks do not by themselves prove current model quality or independence of prior attempts.
