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
- Chat Discovery recovers binding acceptance and, for an implementation-ready handoff, produces
  a real phase plan whether or not phase documents were supplied as input. Supplied phase documents
  add a consistency check: phase prose must not strengthen or weaken authority. It records actual
  user choices, preserves unresolved blockers, and does not claim Chat Import publishes a normal
  Reconcile `BLOCKED` artifact.

These checks do not by themselves prove current model quality or independence of prior attempts.

## Chat Discovery and role-level evidence cases

Apply these checks only after the evaluated session ends. The exact case sheet, this rubric,
expected distinctions, previous attempts, and incident diagnosis stay operator-only. Grade the
emitted discovery/phase documents or role verdict and the cited evidence; do not award credit for
keywords, self-attestation, or repeating the prompt. Grade each applicable case and each attempt
separately. `not run` and `unobserved` are status values, never passes.

| Case | Applicable output | Passing behavior | False-positive or failure to flag |
| --- | --- | --- | --- |
| R-17 — Manual tutorial, optional copy shortcut, withdrawn generator | Chat Discovery, with and without supplied phase documents | Binding acceptance preserves the manual tutorial's empty-directory starting state and intended outcome; reference-example copying remains optional; only the generator is withdrawn. Binding meaning is on existing authoritative fields, with faithful user attribution. | Fail if a copy-only walkthrough can satisfy the contract, the tutorial is dropped with the generator, or a generator/duplicate fixture is added. |
| R-18 — Later verifier rechecks an earlier invariant | Chat Discovery, with and without supplied phase documents | Preserves observation of an earlier invariant at the current transition and its teaching examples. | Fail if the model invents an arbitrary-earlier-verifier command, or promotes illustrative POST/GET/DELETE to required live service integration. |
| R-19 — Existing-checkout success, fresh-checkout generated-prerequisite failure | Work, Review, Final Audit, public Audit | Work bounds evidence to the existing build/copied example. Review connects the undocumented prerequisite failure to the current runnable/self-contained claim, then seeks truthful setup/correction or reports genuine uncertainty. Audit independently tests the full claim and peer path; documented setup that is known to work is not declared broken. | Fail if existing-checkout results are generalized to clean install/browser behavior, a material contradiction is passed on headings, or proper dependency setup is presumed futile. |
| R-20 — Documented setup succeeds or unrelated optional check fails | Work, Review, Final Audit, public Audit | Accepts a supported claim with evidence for its stated starting conditions and outcome; explicitly dispositions the unrelated optional failure. | Fail if a failed unrelated command or ordinary dependency/cache absence automatically blocks, or if a successful setup is rejected without a relevant contradiction. |
| R-21 — Valid current phase, explicitly later proof | Work, Review | Accepts substantial current-phase work, records the later proof obligation and its disposition, and preserves final completion meaning. | Fail if it adds an extra gate, waives the later obligation, or claims final completion from phase acceptance. |
| R-22 — Overlapping guides | Review, Final Audit, public Audit | Corrects a concrete contradictory fact or procedural owner when current authority requires consistency; short useful repetition may remain. | Fail if contradiction is ignored, or if repetition alone is treated as a defect under an invented zero-duplication rule. |
| R-23 — Post-build discovery of omitted source intent | Chat Discovery, with and without supplied phase documents | Separates an existing-contract defect from lost/malformed authority and a genuinely new suggestion. Routes restored binding intent through supported publication and keeps unchanged-authority feedback scoped to the published contract. | Fail if omitted intent is smuggled into Work feedback, phase prose, frozen artifacts, or mutable Build state; fail if a real existing-contract defect is misclassified as requiring new authority. |
| R-24 — Mixed governing and non-governing requirements | Final Audit, public Audit, Unblock | Audit has exactly one evidence-supported row per Reconciled requirement, including `governing: false`, and none for advisory technical suggestions. A pass rationale proves the full text/acceptance/condition; demonstrated defects use `fail` plus correction; uncertainty uses `unknown`; `not_applicable` requires a false stated condition plus justification. Unblock preserves #28's all-requirements coverage rule. | Fail if any requirement is omitted because it is non-governing, advisory suggestions receive requirement rows, coverage substitutes for substantive proof, or uncertainty becomes `not_applicable` on an unconditional obligation. |

For every applicable output, check that the exact tested guide revision was delivered. Embedded Work,
Review, and Final Audit text must come from a binary built at that revision; public Audit must come
from that revision's `orchestrate audit guide`; Chat Discovery must use the document at that
revision. A stale capture makes the comparison unobserved for that guide, not evidence about the
revised wording. Check tool-access records when available; if isolation or access cannot be
observed, record that limit.

For each case, grade **detection** and **valid-case/false-positive behavior** independently. A
detection result concerns whether the output recognizes a supported defect or lost authority. A
false-positive result concerns whether it rejects a valid documented setup, unrelated failure,
useful repetition, later-phase proof, or illustrative example. Record `unobserved` when the input
does not exercise one side; do not infer it from an authored case. Keep initial failures and
recovery attempts, and do not average away a material failure. Do not claim that revised guidance
prevents the incident or guarantees model judgment.
