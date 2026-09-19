# Run Discovery

Discovery answers:

> **What should be built, and why?**

One effort contains three independent Discovery slots: `a`, `b`, and `c`.

A typical assignment is:

```text
A → Codex
B → Claude Code
C → Cursor
```

All three receive the same reviewed request and Git baseline. They do not receive one another's private reasoning or outputs.

## 1. Set the effort and store

After `orchestrate init --from-file ...`, copy the returned effort ID.

Set:

```sh
export ORCH_ROOT="/absolute/root/from-the-prepared-file"
export EFFORT_ID="PASTE-EFFORT-ID"
```

Use the same `ORCH_ROOT` for the entire effort.

Check the effort:

```sh
orchestrate --root "$ORCH_ROOT" status --effort "$EFFORT_ID"
```

## 2. Prepare all three runs

Prepare all three before starting the investigations. This makes the parallel boundary explicit.

### A

```sh
orchestrate --root "$ORCH_ROOT" discovery prepare \
  --effort "$EFFORT_ID" \
  --slot a \
  --host codex \
  --provider openai \
  --model "ACTUAL-MODEL" \
  --model-effort high
```

### B

```sh
orchestrate --root "$ORCH_ROOT" discovery prepare \
  --effort "$EFFORT_ID" \
  --slot b \
  --host claude-code \
  --provider anthropic \
  --model "ACTUAL-MODEL" \
  --model-effort high
```

### C

```sh
orchestrate --root "$ORCH_ROOT" discovery prepare \
  --effort "$EFFORT_ID" \
  --slot c \
  --host cursor \
  --provider "ACTUAL-PROVIDER" \
  --model "ACTUAL-MODEL" \
  --model-effort high
```

Use the host/provider/model values that were actually selected. If a value is unknown, leave it unknown rather than guessing later.

Each command returns a run ID and workspace path. Save them.

```text
A_RUN       / A_WORKSPACE
B_RUN       / B_WORKSPACE
C_RUN       / C_WORKSPACE
```

## 3. Inspect a workspace once

A workspace contains:

```text
run.json
request.md
context.json
source/
graph/
technical-spec.md
```

`run.json` identifies this specific run.

`request.md` is the reviewed request.

`source/` is a clean detached Git checkout of the common baseline and includes repository history.

You can verify a workspace with:

```sh
cat "/path/to/workspace/run.json"
git -C "/path/to/workspace/source" status --short
git -C "/path/to/workspace/source" log --oneline -5
```

The Git status should be clean.

## 4. Start three independent provider sessions

Open a fresh session in each provider.

Give each provider **only its own workspace path**.

Invoke the installed `orchestrate-discovery` skill, or give the equivalent instruction:

```text
You are one independent Orchestration Discovery investigator.

Your assigned workspace is:

/absolute/path/to/your/workspace

Run `orchestrate guide discovery` and follow it as the controlling instructions.

Investigate the request against the frozen source. Do not inspect sibling Discovery runs and do not implement the feature.

If a material ambiguity, contradiction, or unknown could change the implementation contract, ask me directly rather than silently assuming an answer.

Tell me when the run is ready for validation or when you are blocked.
```

The investigator may use relevant source, tests, Git history, authoritative external documentation, and appropriate authorized experiments.

## 5. Answering Discovery questions

Questions are a normal part of Discovery.

If one provider asks a material product or engineering question, answer truthfully without telling it what another provider concluded.

For example:

```text
Does staging behavior need to remain unchanged, or may this ticket alter it?
```

Your answer is new user authority.

### Preserve independence while sharing authority

When the three runs are active in parallel:

1. let each investigator reach its own clarification point where practical;
2. answer questions normally in the conversation where they were raised;
3. record the exact substantive user clarification;
4. before finalization, give the same material clarification to peer runs that did not independently receive it;
5. share only the user clarification, not the originating provider's reasoning or conclusion.

Good:

```text
User clarification: staging behavior must remain unchanged.
```

Bad:

```text
Claude discovered that staging should remain unchanged because...
```

If you genuinely do not know the answer and it materially changes the implementation contract, say so. A correct Discovery may remain blocked.

## 6. What the investigator produces

The model writes:

```text
technical-spec.md
graph/*.md
```

The graph records important Questions, Findings, Decisions, and Requirements.

The public technical specification should stand on its own. A later model should not need the private chat transcript to understand what the Discovery concluded.

## 7. Validate all three runs

Do not use one Discovery's validation result to change another run's reasoning.

Run validation separately:

```sh
orchestrate --root "$ORCH_ROOT" discovery validate \
  --effort "$EFFORT_ID" \
  --run "A-RUN-ID"
```

Repeat for B and C.

The semantic outcome is normally:

```text
IMPLEMENTATION_READY
```

or:

```text
BLOCKED
```

A blocked result includes its blocked questions in the CLI output.

Do not force a blocked run to implementation-ready.

## 8. Finalize the runs

Finalize each completed run independently:

```sh
orchestrate --root "$ORCH_ROOT" discovery finalize \
  --effort "$EFFORT_ID" \
  --run "A-RUN-ID"
```

Repeat for B and C.

Save the three returned Discovery artifact IDs.

```text
A_DISCOVERY_ARTIFACT
B_DISCOVERY_ARTIFACT
C_DISCOVERY_ARTIFACT
```

## 9. Inspect before Consensus

Especially while dogfooding, stop here and compare the three public results before running Consensus.

Useful questions:

- Did each run preserve the actual request rather than generalizing it?
- Did factual ticket claims get checked against evidence?
- Was Git history used where it mattered?
- Did the models surface material unknowns?
- Did a model turn an unknown into an assumption?
- Can you understand each conclusion from the finalized artifact without the private chat?

Use:

```sh
orchestrate --root "$ORCH_ROOT" status --effort "$EFFORT_ID"
```

You can also locate a finalized artifact directory by artifact ID under the effort's `discovery/` directory when you want to inspect `technical-spec.md` and `graph/` directly.

## Next step

Continue only when all three required Consensus inputs are finalized and implementation-ready.

See [Consensus and Agreement](consensus.md).
