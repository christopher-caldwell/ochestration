# Run Consensus and adopt an Agreement

Consensus answers:

> **What implementation direction do the three finalized Discovery investigations genuinely agree on?**

Consensus is reconciliation, not another repository investigation.

It consumes exactly one eligible Discovery artifact from each of slots A, B, and C in the same cohort.

## 1. Prerequisites

You need:

```text
EFFORT_ID
A_DISCOVERY_ARTIFACT
B_DISCOVERY_ARTIFACT
C_DISCOVERY_ARTIFACT
```

All three Discoveries must be `IMPLEMENTATION_READY`.

A blocked Discovery is intentionally ineligible.

Set the store root if it is not already set:

```sh
export ORCH_ROOT="/absolute/root/from-the-prepared-file"
```

## 2. Use a fresh Consensus session

Open a fresh model session and invoke `orchestrate-consensus`.

The model-facing rules are available from:

```sh
orchestrate guide consensus
```

The Consensus model should receive only:

- the frozen request/context;
- the three finalized public Discovery specifications.

It should not inspect the target repository or private Discovery chats.

Its job is to normalize equivalent conclusions, preserve material conditions, distinguish silence from disagreement, and identify which slots support each mandatory requirement.

## 3. Prepare a Consensus proposal

For the manual path, have the Consensus model write a `proposal.json` file matching the repository's `scripts/proposal-schema.json` contract.

A simplified requirement entry looks like:

```json
{
  "requirement": {
    "id": "R1",
    "text": "Required behavior",
    "acceptance": "Observable proof",
    "condition": null,
    "governing": false
  },
  "supporters": ["a", "b"],
  "source_refs": {}
}
```

The proposal also contains:

- `comparison_md` — the readable reconciliation;
- `selection_rationale` — why this package represents the shared result;
- `dissent` — meaningful remaining disagreement;
- `counterexample_blocks` — whether a concrete counterexample prevents the package from being implementation-ready.

Do not manually manufacture supporter sets to make the package pass. They are claims about what the finalized Discovery artifacts actually support.

## 4. The majority rule

Every mandatory Consensus-derived requirement needs at least two supporters.

In addition, the supporter sets across the **entire mandatory package** must share at least two slots in common.

Valid example:

```text
R1 = {A, B}
R2 = {A, B, C}
R3 = {A, B}

common = {A, B}
```

Invalid rotating-majority example:

```text
R1 = {A, B}
R2 = {B, C}

common = {B}
```

The second package is rejected even though each row separately has two supporters.

Explicit user constraints are appended as governing requirements. They are not Consensus votes.

## 5. Finalize Consensus

Run:

```sh
orchestrate --root "$ORCH_ROOT" consensus finalize \
  --effort "$EFFORT_ID" \
  --opinion "$A_DISCOVERY_ARTIFACT" \
  --opinion "$B_DISCOVERY_ARTIFACT" \
  --opinion "$C_DISCOVERY_ARTIFACT" \
  --bundle "/absolute/path/to/proposal.json" \
  --host "CONSENSUS-HOST" \
  --provider "CONSENSUS-PROVIDER" \
  --model "CONSENSUS-MODEL" \
  --model-effort high
```

The result is either:

```text
ELIGIBLE_CANDIDATE
```

with an Agreement artifact, or:

```text
NO_CONSENSUS
```

Do not force `NO_CONSENSUS` forward.

Save the Agreement artifact ID when one is produced.

## 6. Review the Agreement

Locate and read the finalized Agreement before adoption.

The human question at this boundary is:

> **Is this actually what I want implemented?**

Check that the Agreement:

- represents the shared Discovery result;
- preserves material conditions;
- has not introduced unrelated scope;
- includes explicit user constraints correctly;
- has acceptance criteria that can later be audited.

Consensus producing a candidate does not authorize Build.

## 7. Adopt the exact Agreement

When you approve it, run:

```sh
orchestrate --root "$ORCH_ROOT" agreement adopt \
  --effort "$EFFORT_ID" \
  --agreement "$AGREEMENT_ARTIFACT" \
  --authorization-label "YOUR-NAME" \
  --host human
```

Save the returned adoption artifact ID.

```text
ADOPTION_ARTIFACT
```

This is the transition from recommendation to implementation authority:

```text
Consensus
   ↓
Agreement candidate
   ↓
explicit adoption
   ↓
Build authority
```

## Next step

Build remains external to Orchestrate.

Continue with [Build and Audit](build-and-audit.md).
