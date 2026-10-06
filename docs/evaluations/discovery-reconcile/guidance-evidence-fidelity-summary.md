# Issue-29 guidance and evidence fidelity evaluation summary

## Scope and status

This is the sanitized, checked-in status summary for the eight bounded case families added for
issue 29. It is a record of authored cases and evaluation availability, not an evaluation result.
No old-versus-revised model session was executed. Every old and revised attempt below is
`not run`; detection and valid-case/false-positive outcomes are `unobserved`. No case is graded.

All case inputs are synthetic. The original improve-docs conversation was not available to this
Worker, so historical Chat Discovery replay was not run. The ticket identifies the private
original portable input by SHA-256
`c19925309b01d7f3700caf1428350954f1cacb996062acb5054c2d3d06c0f41c` and the exported effort by
SHA-256 `e9d7d3c04753ed1266aa30906c4269e2d02d78b197ee0cf89b027b8734d18f00`; their bytes and the
original conversation were not supplied in this Worker checkout. The issue summary is not a
substitute for either historical input.

## Common run provenance

- Baseline guidance revision: `8cbd94efaf19d80112810077d1fb13950b7d0da4`.
- Revised guidance revision: `3f15584487d16300cd9eae84aaca65b7e1dec52c` (the accepted foundation
  checkpoint; the evaluation-doc changes in this phase do not modify those role guides).
- Guide capture: no binary or standalone embedded-guide capture was made for model delivery.
  Work, Review, Final Audit, and public Audit source revisions are identified above, but the
  corresponding revision-built binary and captured guide text were not retained because no
  evaluation session was available. Chat Discovery source is the `docs/chat-discovery.md` file at
  each stated revision.
- Inputs and fixture identity: private external synthetic input catalog at
  `/Users/christophercaldwell/Library/Application Support/Orchestrate/evaluations/issue-29/synthetic-case-inputs.md`,
  SHA-256 `63c01684d944eab1ab74e04cf5c8b9909b5911a0c6334615ecdf5f8aff5f0c9d`. Section anchors
  below select the exact bounded input for each case; synthetic fixture contents are included in
  the catalog section, not in this repository.
- Model, provider, reasoning, and other session settings: `unknown`; no session was launched.
- Session/access transcript and model outputs: unavailable because no session occurred.
- Run dates: none. This worker turn authored the case catalog and summary but did not make an
  evaluation attempt.
- Setting differences: no observed old/revised settings to compare. No result can be attributed to
  guidance wording.
- Specific reason every attempt was not run: the Worker environment has no isolated fresh-session
  model evaluation runner or controls for matching model/provider/settings. This active Work
  conversation is not a fresh matched session and cannot serve as one. No attempt was made using a
  stale installed guide. The synthetic input catalog is available, but the required isolated
  sessions and revision-matched guide delivery were not.

## Per-case records

### R-17 — Manual tutorial, optional copy shortcut, withdrawn generator

- Input label and identity: synthetic; external catalog SHA above, `R-17` section. No fixture
  checkout is needed; the with-phase variant includes its exact phase excerpt in that section.
- Applicable roles and variants: Chat Discovery with no phases and with the named phase excerpt.
- Old guide: Chat Discovery document at baseline `8cbd94efaf19d80112810077d1fb13950b7d0da4`; no
  session delivery or transcript.
- Revised guide: Chat Discovery document at `3f15584487d16300cd9eae84aaca65b7e1dec52c`; no
  session delivery or transcript.
- Model/provider/settings: unknown; session identity and access record unavailable.
- Attempts: old no-phase `not run`; old with-phase `not run`; revised no-phase `not run`; revised
  with-phase `not run`.
- Specific not-run reason: no isolated matched model sessions or revision-matched guide delivery
  were available. Historical replay is additionally unavailable because the originating
  conversation was not supplied; this case is synthetic, not a substitute replay.
- Detection: unobserved. False-positive behavior: unobserved. No grading was performed.
- Setting differences: none observed; no sessions occurred. Limitation: the authored conversation
  and phase contrast define a case but do not show how a model would respond.

### R-18 — Later verifier rechecks an earlier invariant

- Input label and identity: synthetic; external catalog SHA above, `R-18` section. No fixture
  checkout is needed; with-phase variant contains the exact phase excerpt.
- Applicable roles and variants: Chat Discovery with no phases and with the named phase excerpt.
- Old guide: Chat Discovery document at baseline `8cbd94efaf19d80112810077d1fb13950b7d0da4`; no
  session delivery or transcript.
- Revised guide: Chat Discovery document at `3f15584487d16300cd9eae84aaca65b7e1dec52c`; no
  session delivery or transcript.
- Model/provider/settings: unknown; session identity and access record unavailable.
- Attempts: old no-phase `not run`; old with-phase `not run`; revised no-phase `not run`; revised
  with-phase `not run`.
- Specific not-run reason: no isolated matched model sessions or revision-matched guide delivery
  were available. The case is synthetic and does not reproduce the unavailable originating chat.
- Detection: unobserved. False-positive behavior: unobserved. No grading was performed.
- Setting differences: none observed; no sessions occurred. Limitation: no observed artifact tests
  whether broader observation teaching survives without invented commands or live integration.

### R-19 — Existing-checkout success and fresh-checkout generated-prerequisite failure

- Input label and identity: synthetic; external catalog SHA above, `R-19` section, containing the
  packet-equivalent facts and fixture file contents. No separate fixture checkout was created.
- Applicable roles: Work, Review, Final Audit, and public Audit, each with its respective packet
  boundary and guide.
- Old guides: Work/Review/Final Audit sources at baseline `8cbd94efaf19d80112810077d1fb13950b7d0da4`;
  public Audit source at the same revision. No binary/capture or session delivery.
- Revised guides: Work/Review/Final Audit sources at
  `3f15584487d16300cd9eae84aaca65b7e1dec52c`; public Audit source at that revision. No
  revision-built binary/capture or session delivery.
- Model/provider/settings: unknown; session identity and access record unavailable.
- Attempts: old Work, Review, Final Audit, and public Audit `not run`; revised Work, Review, Final
  Audit, and public Audit `not run`.
- Specific not-run reason: no isolated matched model sessions, role packet delivery, or
  revision-matched embedded-guide captures were available. The catalog contains the synthetic
  fixture facts but no checkout from which to run the role's commands.
- Detection: unobserved. False-positive behavior: unobserved. No grading was performed.
- Setting differences: none observed; no sessions occurred. Limitation: the case does not
  establish whether models connect evidence to starting state or independently assess browser
  and clean-install claims.

### R-20 — Documented setup succeeds or an unrelated optional check cannot run

- Input label and identity: synthetic; external catalog SHA above, `R-20` section, containing the
  packet-equivalent setup result and unrelated runner limitation. No separate fixture checkout
  was created.
- Applicable roles: Work, Review, Final Audit, and public Audit.
- Old guides: Work/Review/Final Audit/public Audit sources at baseline
  `8cbd94efaf19d80112810077d1fb13950b7d0da4`; no binary/capture or session delivery.
- Revised guides: corresponding source at `3f15584487d16300cd9eae84aaca65b7e1dec52c`; no
  revision-built binary/capture or session delivery.
- Model/provider/settings: unknown; session identity and access record unavailable.
- Attempts: old Work, Review, Final Audit, and public Audit `not run`; revised Work, Review, Final
  Audit, and public Audit `not run`.
- Specific not-run reason: no isolated matched model sessions, role packet delivery, or
  revision-matched embedded-guide captures were available. The recorded success is synthetic case
  content, not an executed setup in this Worker turn.
- Detection: unobserved. False-positive behavior: unobserved. No grading was performed.
- Setting differences: none observed; no sessions occurred. Limitation: the case does not show
  whether a model accepts documented successful setup while recording an unrelated optional
  failure proportionately.

### R-21 — Meaningful current-phase result with explicitly later-phase proof

- Input label and identity: synthetic; external catalog SHA above, `R-21` section, containing the
  packet-equivalent phase contract and fixture observations. No separate fixture checkout was
  created.
- Applicable roles: Work and Review.
- Old guides: Work/Review sources at baseline `8cbd94efaf19d80112810077d1fb13950b7d0da4`; no
  binary/capture or session delivery.
- Revised guides: Work/Review sources at `3f15584487d16300cd9eae84aaca65b7e1dec52c`; no
  revision-built binary/capture or session delivery.
- Model/provider/settings: unknown; session identity and access record unavailable.
- Attempts: old Work and Review `not run`; revised Work and Review `not run`.
- Specific not-run reason: no isolated matched model sessions, role packet delivery, or
  revision-matched embedded-guide captures were available.
- Detection: unobserved. False-positive behavior: unobserved. No grading was performed.
- Setting differences: none observed; no sessions occurred. Limitation: no output was observed to
  test preservation of phase acceptance and the later final-completion obligation.

### R-22 — Overlapping guides

- Input label and identity: synthetic; external catalog SHA above, `R-22` section, containing
  exact excerpt text as the documentation fixture. No separate checkout was created.
- Applicable roles: Review, Final Audit, and public Audit.
- Old guides: Review/Final Audit/public Audit sources at baseline
  `8cbd94efaf19d80112810077d1fb13950b7d0da4`; no binary/capture or session delivery.
- Revised guides: corresponding source at `3f15584487d16300cd9eae84aaca65b7e1dec52c`; no
  revision-built binary/capture or session delivery.
- Model/provider/settings: unknown; session identity and access record unavailable.
- Attempts: old Review, Final Audit, and public Audit `not run`; revised Review, Final Audit, and
  public Audit `not run`.
- Specific not-run reason: no isolated matched model sessions, role packet delivery, or
  revision-matched embedded-guide captures were available.
- Detection: unobserved. False-positive behavior: unobserved. No grading was performed.
- Setting differences: none observed; no sessions occurred. Limitation: no output was observed to
  distinguish a material factual contradiction from useful repeated guidance.

### R-23 — Post-build discovery of omitted source intent

- Input label and identity: synthetic; external catalog SHA above, `R-23` section. It explicitly
  does not replay the original conversation. No fixture checkout is needed; with-phase variant
  contains the exact phase excerpt.
- Applicable roles and variants: Chat Discovery with no phases and with the named phase excerpt.
- Old guide: Chat Discovery document at baseline `8cbd94efaf19d80112810077d1fb13950b7d0da4`; no
  session delivery or transcript.
- Revised guide: Chat Discovery document at `3f15584487d16300cd9eae84aaca65b7e1dec52c`; no
  session delivery or transcript.
- Model/provider/settings: unknown; session identity and access record unavailable.
- Attempts: old no-phase `not run`; old with-phase `not run`; revised no-phase `not run`; revised
  with-phase `not run`.
- Specific not-run reason: no isolated matched model sessions or revision-matched guide delivery
  were available. The exact originating conversation is unavailable, so historical replay could
  not be substituted for this separately labeled synthetic case.
- Detection: unobserved. False-positive behavior: unobserved. No grading was performed.
- Setting differences: none observed; no sessions occurred. Limitation: the synthetic post-build
  report does not authenticate historical intent or show whether a model routes authority repair
  separately from an existing-contract defect.

### R-24 — Mixed governing and non-governing requirements

- Input label and identity: synthetic; external catalog SHA above, `R-24` section, including the
  exact Reconciled packet, fixture facts, incomplete assessment, and Unblock feedback. No
  separate checkout was created.
- Applicable roles: Final Audit, public Audit, and Unblock.
- Old guides: Final Audit/public Audit/Unblock sources at baseline
  `8cbd94efaf19d80112810077d1fb13950b7d0da4`; no binary/capture or session delivery.
- Revised guides: corresponding sources at `3f15584487d16300cd9eae84aaca65b7e1dec52c`; no
  revision-built binary/capture or session delivery.
- Model/provider/settings: unknown; session identity and access record unavailable.
- Attempts: old Final Audit, public Audit, and Unblock `not run`; revised Final Audit, public Audit,
  and Unblock `not run`.
- Specific not-run reason: no isolated matched model sessions, role packet delivery, or
  revision-matched embedded-guide captures were available.
- Detection: unobserved. False-positive behavior: unobserved. No grading was performed.
- Setting differences: none observed; no sessions occurred. Limitation: the case does not test
  model preservation of #28's all-requirements coverage or whether Audit distinguishes complete
  row coverage from substantive evidence.

## What this summary does not establish

The authored catalog, guide text, repository tests, and this not-run summary establish no model
behavioral improvement, no reduction in false positives, and no claim that the revised guidance
would prevent the improve-docs incident or guarantee correct judgment. Audit can inspect this
summary's presence, completeness, and internal consistency; it cannot independently authenticate
private external executions that did not occur here.
