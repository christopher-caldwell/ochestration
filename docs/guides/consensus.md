# Run Consensus and adopt an Agreement

Consensus answers:

> **What implementation direction do the finalized Discovery investigations genuinely agree on?**

Consensus is reconciliation, not another repository investigation.

## 1. Run the Consensus skill

Open a fresh model session and invoke `orchestrate-consensus`, giving it the effort ID (and the store root if it is not `~/.orchestration`).

The skill resolves the `orchestrate` executable, runs `orchestrate guide consensus` as its controlling instructions, and reads only:

- the frozen request and context;
- the resolved finalized public Discovery specifications.

It never inspects the target repository, private Discovery chats, or sibling workspaces.

Before it reconciles anything, the skill resolves the exact inputs:

```sh
orchestrate consensus inputs --effort "$EFFORT_ID"
```

Rust names the single eligible finalized Discovery artifact for every cohort slot. Eligibility requires a finalized `Discovery` artifact with outcome `IMPLEMENTATION_READY` for the same effort, cohort, context, and baseline, in the correct slot.

If a slot has no eligible artifact or more than one, the command fails and lists what it found. The skill then asks you which artifacts to use and confirms the complete set with explicit selectors. Rust never silently picks the newest artifact.

The skill reads exactly those artifacts, writes `proposal.json`, and finalizes against the same set:

```sh
orchestrate consensus finalize \
  --effort "$EFFORT_ID" \
  --opinion "$A_DISCOVERY_ARTIFACT" \
  --opinion "$B_DISCOVERY_ARTIFACT" \
  --opinion "$C_DISCOVERY_ARTIFACT" \
  --bundle "/absolute/path/to/proposal.json"
```

The parent artifacts are fixed before reconciliation begins and cannot move afterwards, so the immutable Consensus lineage always names the artifacts the proposal was actually derived from.

The result is either:

```text
ELIGIBLE_CANDIDATE
```

with an Agreement artifact, or:

```text
NO_CONSENSUS
```

Do not force `NO_CONSENSUS` forward.

## 2. Review the Agreement candidate

The skill shows you the Agreement. The human question at this boundary is:

> **Is this actually what I want implemented?**

Check that the Agreement:

- represents the shared Discovery result;
- preserves material conditions;
- has not introduced unrelated scope;
- includes explicit user constraints correctly;
- has acceptance criteria that can later be audited.

Consensus producing a candidate does not authorize Build.

## 3. Adopt the exact Agreement

The skill prints the exact adopt command. It is normally:

```sh
orchestrate agreement adopt --effort "$EFFORT_ID" --authorization-label "YOUR-NAME"
```

When exactly one eligible Agreement exists, `--agreement` is inferred; when several exist, the command fails and lists the candidates, and you pass `--agreement "<artifact-id>"` explicitly. `--authorization-label` is always required.

Save the returned adoption artifact ID. This is the transition from recommendation to implementation authority:

```text
Consensus
   ↓
Agreement candidate
   ↓
explicit adoption
   ↓
Build authority
```

Nothing adopts automatically, and the Consensus skill never adopts on your behalf.

## Advanced and manual operation

You can run Consensus by hand when debugging or producing the proposal yourself. Every Discovery must be `IMPLEMENTATION_READY`; a blocked Discovery is intentionally ineligible.

`orchestrate consensus inputs` performs the input resolution read-only and publishes nothing. Run it without `--opinion` to see the inferred artifacts, or with one `--opinion` selector per slot to check an explicit set before you reconcile against it.

A simplified `proposal.json` requirement entry looks like:

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

The proposal also contains `comparison_md` (the readable reconciliation), `selection_rationale`, `dissent` (meaningful remaining disagreement), and `counterexample_blocks`.

Do not manually manufacture supporter sets to make the package pass. They are claims about what the finalized Discovery artifacts actually support.

Every mandatory Consensus-derived requirement needs at least `quorum` supporters, and the supporter
sets across the entire mandatory package must share at least `quorum` slots in common. `quorum`
defaults to strict majority (`floor(N/2)+1`) and is frozen at `orchestrate init`; it can be lowered
or raised with `--quorum`.

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

The second package is rejected even though each row separately meets the per-row threshold. Explicit user constraints are appended as governing requirements; they are not Consensus votes.

Finalize against the artifacts you resolved and reconciled:

```sh
orchestrate consensus finalize \
  --effort "$EFFORT_ID" \
  --opinion "$A_DISCOVERY_ARTIFACT" \
  --opinion "$B_DISCOVERY_ARTIFACT" \
  --opinion "$C_DISCOVERY_ARTIFACT" \
  --bundle "/absolute/path/to/proposal.json"
```

Explicit selection is all-or-nothing: pass one selector per cohort slot, or omit `--opinion` entirely.

## Next step

Build remains external to Orchestrate.

Continue with [Build and Audit](build-and-audit.md).
