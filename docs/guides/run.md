# Complete run guide

This is the shortest end-to-end human workflow for Orchestrate.

Use the focused guides when you need more detail:

- [Request preparation](request-preparation.md)
- [Discovery](discovery.md)
- [Consensus and Agreement](consensus.md)
- [Build and Audit](build-and-audit.md)

## 1. Prepare and review the request

Use `prep-discovery-ticket` for an existing ticket or `prep-discovery-freeform` for a freeform request.

The prep skill writes a reviewed Markdown input containing YAML frontmatter.

For tickets, paste the original ticket verbatim into the generated placeholder yourself.

Review the file, then run the exact command the prep skill returns:

```sh
orchestrate init --from-file "/absolute/path/to/request.prepared.md"
```

Copy the returned effort ID.

Set the store root to the `root` value from the prepared file:

```sh
export ORCH_ROOT="/absolute/root/from-prepared-file"
export EFFORT_ID="PASTE-EFFORT-ID"
```

## 2. Prepare A, B, and C

Prepare all three independent Discovery workspaces before starting the investigations.

```sh
orchestrate --root "$ORCH_ROOT" discovery prepare \
  --effort "$EFFORT_ID" --slot a \
  --host codex --provider openai --model "ACTUAL-MODEL" --model-effort high
```

```sh
orchestrate --root "$ORCH_ROOT" discovery prepare \
  --effort "$EFFORT_ID" --slot b \
  --host claude-code --provider anthropic --model "ACTUAL-MODEL" --model-effort high
```

```sh
orchestrate --root "$ORCH_ROOT" discovery prepare \
  --effort "$EFFORT_ID" --slot c \
  --host cursor --provider "ACTUAL-PROVIDER" --model "ACTUAL-MODEL" --model-effort high
```

Save each returned run ID and workspace path.

## 3. Run the three Discoveries independently

Open three fresh provider sessions.

Give each provider only its own workspace and invoke `orchestrate-discovery`.

Each run should:

- read its `run.json`, `request.md`, and `context.json`;
- investigate the frozen Git checkout under `source/`;
- use evidence, including Git history where relevant;
- ask you about material ambiguity instead of silently assuming;
- write `technical-spec.md` and its evidence graph;
- stop before implementation.

If one provider obtains a material user clarification, share the same clarification with the peer runs before finalization without sharing peer reasoning.

See [Discovery](discovery.md) for the detailed independence and clarification rules.

## 4. Validate and finalize A, B, and C

For each run:

```sh
orchestrate --root "$ORCH_ROOT" discovery validate \
  --effort "$EFFORT_ID" --run "RUN-ID"
```

Then:

```sh
orchestrate --root "$ORCH_ROOT" discovery finalize \
  --effort "$EFFORT_ID" --run "RUN-ID"
```

Save the three Discovery artifact IDs.

A blocked Discovery is a valid result but cannot proceed into implementation-ready Consensus.

## 5. Run Consensus

Use a fresh Consensus session and invoke `orchestrate-consensus`.

Give it the three finalized public Discovery results and frozen request/context. Do not give it private Discovery chats and do not have it re-investigate the repository.

Have it create a `proposal.json` matching `scripts/proposal-schema.json`.

Finalize:

```sh
orchestrate --root "$ORCH_ROOT" consensus finalize \
  --effort "$EFFORT_ID" \
  --opinion "A-DISCOVERY-ARTIFACT" \
  --opinion "B-DISCOVERY-ARTIFACT" \
  --opinion "C-DISCOVERY-ARTIFACT" \
  --bundle "/absolute/path/to/proposal.json" \
  --host "CONSENSUS-HOST" \
  --provider "CONSENSUS-PROVIDER" \
  --model "CONSENSUS-MODEL" \
  --model-effort high
```

If the result is `NO_CONSENSUS`, stop rather than forcing a package forward.

## 6. Review and adopt the Agreement

Read the Agreement candidate and verify that it is actually what you want implemented.

Then adopt it:

```sh
orchestrate --root "$ORCH_ROOT" agreement adopt \
  --effort "$EFFORT_ID" \
  --agreement "AGREEMENT-ARTIFACT" \
  --authorization-label "YOUR-NAME" \
  --host human
```

Save the adoption artifact ID.

## 7. Build externally

Give the adopted Agreement to your coding agent or implement it manually.

When the implementation is complete, review it and commit it.

```sh
git rev-parse HEAD
```

Save that exact commit.

## 8. Register the implementation

```sh
orchestrate --root "$ORCH_ROOT" implementation register \
  --effort "$EFFORT_ID" \
  --adoption "ADOPTION-ARTIFACT" \
  --project "/absolute/path/to/project" \
  --commit "IMPLEMENTATION-COMMIT" \
  --declaration "Implemented from adopted Agreement" \
  --status submitted
```

Save the returned implementation artifact ID.

## 9. Audit

Open a fresh Audit session and invoke `orchestrate-audit`.

Have the assessor evaluate the exact adopted Agreement against the exact registered implementation and create `assessment.json` matching `scripts/audit-schema.json`.

Finalize:

```sh
orchestrate --root "$ORCH_ROOT" audit finalize \
  --effort "$EFFORT_ID" \
  --bundle "/absolute/path/to/assessment.json" \
  --host "AUDIT-HOST" \
  --provider "AUDIT-PROVIDER" \
  --model "AUDIT-MODEL" \
  --model-effort high
```

The result is mechanically derived as `PASS`, `CHANGES_REQUIRED`, or `BLOCKED`.

## 10. Inspect the run

```sh
orchestrate --root "$ORCH_ROOT" status --effort "$EFFORT_ID"
```

```sh
orchestrate --root "$ORCH_ROOT" journal --effort "$EFFORT_ID"
```

```sh
orchestrate --root "$ORCH_ROOT" lineage \
  --effort "$EFFORT_ID" \
  --artifact "AUDIT-ARTIFACT"
```

The artifact lineage is authoritative. The journal is diagnostic.
