# Evaluation record

Complete raw records outside the repositories. This template is not an executed evaluation.
For issue-29, the sanitized per-case summary may be checked in; keep exact inputs, fixtures,
prompts, transcripts, raw outputs, and project-specific content external.

## Run identity

- Date:
- Model/provider and reasoning setting (unknown if unavailable):
- Orchestrate revision or binary version:
- Role and case variant (historical or synthetic):
- Captured guide path:
- Model prompt copy/path:
- Exact selected input labels, artifact IDs, and directories:
- Operator case sheet, evidence citations, and inapplicable criteria:
- Explicit evaluation exceptions and limitations (if any):
- Initial response and access transcript:
- Hypothetical scope-answer turn and subsequent response:
- Isolation/access visibility and limitations:

## Evidence-backed judgments

For each item, record pass/fail/unobserved, the response passage or tool access, and the reason.

1. Selected inputs and eligibility:
2. Supported explanation:
3. Initial scope conflict and clarification:
4. Coherent selected direction and acceptance criteria:
5. Alternatives:
6. Compatibility:
7. Factual uncertainty:
8. Verification limits:
9. Scope limitations:
10. Evidence/access boundary:
11. Acceptance assertion and completion consequence:
12. Conditional exclusions:
13. Published ready/blocked handoff (if applicable):

## Other role observations (if exercised)

- Discovery publication: acceptance meaning, known limitation, clarification, and evidence status:
- Planner: phase evidence, local repairs, capability setup, or precise upstream return:
- Chat Discovery with / without supplied phase documents: coherent authority, generated phase plan, phase consistency, blockers:
- Exact input boundary and any unobserved access for each role:

## Result

- Overall: not run / pass / fail / incomplete observation
- Any initial failure followed by recovery:
- Specific failure requiring a change, if demonstrated:
- What this run does not establish:

Do not claim a live-model result from guide inspection, Rust tests, or prior reviews.

## Issue-29 Chat Discovery and role-level case record

Create one record per case and retain every attempt. The issue-29 checked-in summary should
contain sanitized status and limitation fields for all eight cases; it must not contain raw
transcripts, private fixture contents, model outputs, or project-specific material.

### Case identity and input boundary

- Case ID and family (R-17 through R-24):
- Input label: synthetic / historical:
- Exact external input SHA-256 or path, with section/variant:
- Exact fixture archive/tree SHA-256 or path (or explicit unavailable):
- Role(s) and variant (include Chat Discovery with/without supplied phase documents where applicable):
- Exact bounded inputs supplied (conversation/evidence, selected guide, phase documents,
  packet-equivalent handoff, fixture checkout):
- Inputs withheld from the evaluated session (rubric, expected behavior, previous outputs,
  later incident diagnosis):
- Historical source identity, if applicable (hash/path); if unavailable, why no replay occurred:

### Revision and run settings

- Baseline product revision: `8cbd94efaf19d80112810077d1fb13950b7d0da4`
- Revised product commit:
- Old guide source and capture identity (binary/version or document revision):
- Revised guide source and capture identity (binary/version or document revision):
- Role guide actually delivered:
- Date/time:
- Model/provider/reasoning/settings (or `unknown`):
- Fresh isolated session identity or specific reason unavailable:
- Setting/provenance differences between old and revised attempt:

### Attempts and grading

| Attempt | Guide revision/capture | Status (`pass`/`fail`/`unobserved`/`not run`) | Output/access record path | Detection result and citation | Valid-case/false-positive result and citation | Specific not-run reason or unobserved limitation |
| --- | --- | --- | --- | --- | --- | --- |
| Old | | | | | | |
| Revised | | | | | | |
| Recovery/additional (repeat as needed) | | | | | | |

- Operator rubric citations and reasoning (external):
- Initial failure and recovery, if any:
- Differences that prevent attribution to guide wording alone:
- What this case does not establish:

Repeat the case record for all eight families. A case not attempted still needs its synthetic or
historical label, input identity or explicit unavailability, applicable roles, old/revised guide
provenance or why capture was unavailable, status, a specific not-run reason, detection and
false-positive status (`unobserved` if not assessed), and limitations. Never mark an unrun attempt
as graded. The summary may state that all model settings are unknown when no session occurred.

## Sanitized summary entry shape

When producing the issue-29 checked-in summary, retain these fields for each case: case ID and
family; synthetic/historical label; external input/fixture identity or explicit unavailability;
old and revised guide revisions/captures or capture limitation; applicable roles; model/provider/
settings or `unknown`; each attempt's status and specific not-run reason; detection and
false-positive results or `unobserved`; setting differences; and limitations, including what the
record does not establish. Do not include an evaluation score or imply that authored cases,
repository checks, not-run status, or private runs unauthenticated by Audit demonstrate model
improvement.
