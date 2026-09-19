# Complete run guide

This is the normal end-to-end human workflow for Orchestrate. The installed phase skills own the routine mechanics, so you work in provider sessions and only type the decisions.

```text
prepared request → init → three Discovery skills → Consensus skill
→ explicit adoption → external Build → register → Audit skill
```

Use the focused guides when you need more detail:

- [Request preparation](request-preparation.md)
- [Discovery](discovery.md)
- [Consensus and Agreement](consensus.md)
- [Build and Audit](build-and-audit.md)

Raw command sequences for manual or debugging use are in [Advanced and manual operation](#advanced-and-manual-operation) at the end of this guide.

## 1. Prepare and review the request

Use `prep-discovery-ticket` for an existing ticket or `prep-discovery-freeform` for a freeform request.

The prep skill writes a reviewed Markdown input containing YAML frontmatter. For tickets, paste the original ticket verbatim into the generated placeholder yourself.

Review the file, then run the exact command the prep skill returns:

```sh
orchestrate init --from-file "/absolute/path/to/request.prepared.md"
```

Copy the returned effort ID. If the prepared file's `root` is not `~/.orchestration`, tell every phase skill and command that root; otherwise the default store applies.

```sh
export EFFORT_ID="PASTE-EFFORT-ID"
```

## 2. Run three independent Discovery sessions

Open three fresh provider sessions, one per slot:

```text
Codex  → $orchestrate-discovery  → slot a
Claude → /orchestrate-discovery  → slot b
Cursor → /orchestrate-discovery  → slot c
```

Give each session only:

- the effort ID;
- its slot (`a`, `b`, or `c`);
- the store root, if it is not `~/.orchestration`.

The skill runs `orchestrate guide discovery`, prepares its own run workspace, investigates the frozen baseline interactively, asks you about material ambiguity, writes `technical-spec.md` and the evidence graph, and finalizes the run. It then reports the run ID, the finalized Discovery artifact ID, and the outcome (`IMPLEMENTATION_READY` or `BLOCKED`, with any blocked questions).

You do not need to type `prepare`, `validate`, or `finalize` yourself.

If one session raises a material question, answer it there and share the same clarification with the peer sessions before they finalize. Share only the user's clarification, never another provider's reasoning or conclusion.

A blocked Discovery is a valid result but cannot proceed into Consensus. Do not force it to implementation-ready.

## 3. Run Consensus

In a fresh session, invoke `orchestrate-consensus` with the effort ID (and the store root if it is not `~/.orchestration`).

The skill reads the frozen request and the three finalized public Discovery specifications, reconciles them, writes `proposal.json`, and finalizes. Rust infers the single eligible finalized Discovery artifact for each slot; the skill asks you only if a slot has no candidate or more than one, and re-runs with explicit selectors.

The skill then reports either `NO_CONSENSUS` or the Agreement candidate. If a candidate exists, review the Agreement it shows you — that is exactly what adoption would authorize.

## 4. Adopt the Agreement

Adoption is the explicit transition from recommendation to implementation authority. Nothing adopts automatically.

The Consensus skill prints the exact command to run, which is normally:

```sh
orchestrate agreement adopt --effort "$EFFORT_ID" --authorization-label "YOUR-NAME"
```

When exactly one eligible Agreement exists, `--agreement` is inferred. If several exist, the command fails and lists the candidates; pass `--agreement "<artifact-id>"` explicitly.

Save the returned adoption artifact ID.

## 5. Build externally

Give the adopted Agreement to your coding agent or implement it manually. Orchestrate does not schedule or run Build.

Review the implementation normally, then commit it:

```sh
git status
git diff
git add .
git commit -m "Implement adopted Orchestration agreement"
```

## 6. Register the implementation

Registration records the exact commit and tree and retains an immutable implementation snapshot:

```sh
orchestrate implementation register --effort "$EFFORT_ID"
```

Defaults and inference:

- the effort's stored canonical project (there is no `--project` argument);
- `--commit HEAD`;
- `--status submitted`;
- `--declaration "external implementation"`;
- the sole Adoption receipt for the effort, when exactly one exists.

Override any of these when reality differs:

```sh
orchestrate implementation register \
  --effort "$EFFORT_ID" \
  --adoption "ADOPTION-ARTIFACT" \
  --commit "abc123" \
  --status partial \
  --declaration "Partial implementation; blocked by..."
```

Repository identity, baseline ancestry, the exact commit and tree, the immutable snapshot, and lineage are still verified mechanically.

Save the returned implementation artifact ID.

## 7. Run Audit

In a fresh session, invoke `orchestrate-audit` with the effort ID (and the store root if it is not `~/.orchestration`).

The skill locates the Agreement → Adoption → Implementation chain, inspects the retained immutable implementation source, runs relevant tests, writes `assessment.json` with one coverage row per Agreement requirement, and finalizes the Audit. It asks you only when several valid chains make the selection ambiguous.

The verdict is derived mechanically as `PASS`, `CHANGES_REQUIRED`, or `BLOCKED`.

## 8. Inspect the run

```sh
orchestrate status --effort "$EFFORT_ID"
orchestrate journal --effort "$EFFORT_ID"
orchestrate lineage --effort "$EFFORT_ID" --artifact "AUDIT-ARTIFACT"
```

The artifact lineage is authoritative. The journal is diagnostic.

## Advanced and manual operation

The underlying commands remain available for debugging, manual inspection, and scripted use. They are the same deterministic boundaries the skills use.

`orchestrate` does not launch model providers. The removed `discovery run`, `consensus run`, and `audit run` commands had no way to support an interactive conversation, so phase work happens in provider sessions (or by hand) instead.

### Discovery

```sh
orchestrate discovery prepare --effort "$EFFORT_ID" --slot a \
  --host codex --provider openai --model "ACTUAL-MODEL" --model-effort high

orchestrate discovery validate --effort "$EFFORT_ID" --run "RUN-ID"

orchestrate discovery finalize --effort "$EFFORT_ID" --run "RUN-ID"
```

`prepare` prints the run ID and workspace path. Write `technical-spec.md` and `graph/*.md` in that workspace, then finalize. `validate` is optional at any point; `finalize` validates before it publishes anything. The outcome is derived from the evidence graph and cannot be overridden: any blocked question produces `BLOCKED`.

### Consensus

```sh
orchestrate consensus finalize \
  --effort "$EFFORT_ID" \
  --bundle "/absolute/path/to/proposal.json"
```

Omit `--opinion` to infer the single eligible Discovery artifact per slot. When a slot has no candidate or several candidates, the command stops and lists them; re-run with all three explicit selectors:

```sh
orchestrate consensus finalize \
  --effort "$EFFORT_ID" \
  --opinion "$A_DISCOVERY_ARTIFACT" \
  --opinion "$B_DISCOVERY_ARTIFACT" \
  --opinion "$C_DISCOVERY_ARTIFACT" \
  --bundle "/absolute/path/to/proposal.json"
```

The result is `ELIGIBLE_CANDIDATE` with an Agreement artifact, or `NO_CONSENSUS`. Do not force `NO_CONSENSUS` forward.

### Agreement adoption

```sh
orchestrate agreement adopt \
  --effort "$EFFORT_ID" \
  --authorization-label "YOUR-NAME"
```

When several eligible Agreements exist, pass `--agreement "<artifact-id>"` explicitly. `--authorization-label` is always required.

### Implementation registration and Audit

```sh
orchestrate implementation register --effort "$EFFORT_ID"

orchestrate audit finalize \
  --effort "$EFFORT_ID" \
  --bundle "/absolute/path/to/assessment.json"
```

The Audit assessment must reference the exact Agreement, Adoption, and Implementation artifacts and cover every Agreement requirement. `orchestrate status` prints the artifact references, including digests, that belong in `assessment.json`.

### Provenance metadata

`prepare`, `finalize`, `consensus finalize`, `agreement adopt`, `implementation register`, and `audit finalize` accept `--host`, `--provider`, `--model`, `--model-effort`, and `--independence`. Set them truthfully when you know them; leave them unset rather than guessing.
