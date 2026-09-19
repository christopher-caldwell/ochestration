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

## 1. Give each slot its inputs

After `orchestrate init --from-file ...`, copy the returned effort ID.

```sh
export EFFORT_ID="PASTE-EFFORT-ID"
```

Open three fresh provider sessions and invoke `orchestrate-discovery` in each. Give each session:

- the effort ID;
- its own slot (`a`, `b`, or `c`);
- the store root, if it is not `~/.orchestration`;
- host/provider/model metadata, if you want it recorded truthfully in the artifact.

Do not give a session another slot's workspace, reasoning, or conclusions.

## 2. Let the skill run the phase

The skill resolves the `orchestrate` executable and runs `orchestrate guide discovery` as its controlling instructions. It then:

1. runs `orchestrate discovery prepare` for its own slot;
2. captures the run ID and workspace path from the output;
3. reads `run.json`, `request.md`, and `context.json`;
4. investigates the frozen `source/` checkout interactively;
5. asks you about material ambiguity, conflicting evidence, or missing decisions;
6. writes `technical-spec.md` and the `graph/*.md` evidence nodes;
7. optionally runs `discovery validate` as an intermediate check;
8. runs `discovery finalize` and reports the run ID, artifact ID, outcome, and blocked questions.

You do not need to type `prepare`, `validate`, or `finalize` yourself, and there is no non-interactive `discovery run` command. A fresh provider session per slot keeps the runs independent.

## 3. Answer questions without breaking independence

Questions are a normal part of Discovery. If one provider asks a material product or engineering question, answer truthfully without telling it what another provider concluded.

For example:

```text
Does staging behavior need to remain unchanged, or may this ticket alter it?
```

Your answer is new user authority. When the three runs are active in parallel:

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

## 4. What the investigator produces

The model writes:

```text
technical-spec.md
graph/*.md
```

The graph records important Questions, Findings, Decisions, and Requirements. The public technical specification should stand on its own: a later model should not need the private chat transcript to understand what the Discovery concluded.

The outcome is derived mechanically from that graph. Any blocked question produces a `BLOCKED` result, which is a valid final artifact but is ineligible for Consensus. Otherwise the run must be implementation-ready: every required question `answered` or `no_change`, and every mandatory requirement traceable to accepted evidence and not stale.

## 5. Inspect before Consensus

Especially while dogfooding, stop here and compare the three public results before running Consensus.

Useful questions:

- Did each run preserve the actual request rather than generalizing it?
- Did factual ticket claims get checked against evidence?
- Was Git history used where it mattered?
- Did the models surface material unknowns?
- Did a model turn an unknown into an assumption?
- Can you understand each conclusion from the finalized artifact without the private chat?

Use `orchestrate status --effort "$EFFORT_ID"` to list the finalized artifacts, then read a bundle's `technical-spec.md` and `graph/` directly from its artifact directory under the effort's `discovery/` directory.

## Advanced and manual operation

You can run Discovery by hand when debugging, inspecting a workspace, or working manually with a model.

Prepare all three runs first so the parallel boundary is explicit:

```sh
orchestrate discovery prepare --effort "$EFFORT_ID" --slot a \
  --host codex --provider openai --model "ACTUAL-MODEL" --model-effort high

orchestrate discovery prepare --effort "$EFFORT_ID" --slot b \
  --host claude-code --provider anthropic --model "ACTUAL-MODEL" --model-effort high

orchestrate discovery prepare --effort "$EFFORT_ID" --slot c \
  --host cursor --provider "ACTUAL-PROVIDER" --model "ACTUAL-MODEL" --model-effort high
```

Each command returns a run ID and workspace path. A workspace contains:

```text
run.json
request.md
context.json
source/
graph/
technical-spec.md
```

`run.json` identifies this specific run, `request.md` is the reviewed request, and `source/` is a clean detached Git checkout of the common baseline including repository history. Verify it with:

```sh
cat "/path/to/workspace/run.json"
git -C "/path/to/workspace/source" status --short
git -C "/path/to/workspace/source" log --oneline -5
```

The Git status should be clean. Give a manual investigator only its own workspace path, plus `orchestrate guide discovery` as the controlling instructions.

Validate whenever an intermediate check is useful:

```sh
orchestrate discovery validate --effort "$EFFORT_ID" --run "RUN-ID"
```

Validation is not required before finalization; `discovery finalize` validates the workspace before it publishes anything. Do not use one Discovery's validation result to change another run's reasoning, and do not force a blocked run to implementation-ready.

Finalize each completed run independently:

```sh
orchestrate discovery finalize --effort "$EFFORT_ID" --run "RUN-ID"
```

The command prints the finalized artifact ID and the derived outcome (`details.artifact` and `details.outcome`). Save the three Discovery artifact IDs.

## Next step

Continue only when all three required Consensus inputs are finalized and implementation-ready.

See [Consensus and Agreement](consensus.md).
