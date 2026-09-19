---
name: orchestrate-discovery
description: Run one interactive Orchestrate Discovery investigation in this session, from preparation through finalization.
disable-model-invocation: true
---

# Run one Discovery

Use this skill only when explicitly invoked. It owns the normal lifecycle of a single interactive Discovery run inside this provider session. One session handles one slot; other slots are run independently by other sessions.

## 1. Resolve the CLI and the inputs

- Resolve the installed executable with `command -v orchestrate`, falling back to `$CARGO_HOME/bin/orchestrate` or `~/.cargo/bin/orchestrate`. If it cannot be found, stop and tell the user.
- Take the effort ID and the slot (`a`, `b`, or `c`) from the user. Ask for whichever value is missing; never guess either one.
- If the user supplies a store root, include `--root "<root>"` on every command below. Otherwise omit `--root` so the CLI uses `~/.orchestration`.
- Pass `--host`, `--provider`, `--model`, and `--model-effort` only when the user supplies them. Never invent provider or model values; report when provenance fell back to CLI defaults.

## 2. Load the controlling instructions

Run `orchestrate guide discovery` (with `--root` when the user supplied one). That guide is authoritative for phase behavior; follow it throughout this session.

## 3. Prepare the run

```sh
orchestrate discovery prepare --effort "<effort>" --slot "<slot>"
```

`prepare` prints one JSON line. Capture `details.run` (the run ID) and `details.workspace` (the workspace path).

## 4. Read the workspace

Read `run.json`, `request.md`, and `context.json` in the workspace. All investigation happens inside `source/`, the clean detached Git checkout of the cohort baseline.

Do not inspect sibling Discovery workspaces, parent store records, the live target repository, or unrelated paths. Do not edit the target repository or implement the feature.

## 5. Investigate interactively

- Investigate the request against the frozen source: relevant source, tests, `git log`, `git blame`, `git show`, authoritative external documentation, and authorized experiments where they matter.
- Ask the user directly in this session whenever material ambiguity, conflicting evidence, or a missing product or engineering decision could change what should be built. Share only user clarifications, never another slot's reasoning.
- Write the evidence graph as `graph/*.md` nodes with valid frontmatter and dependencies, and write `technical-spec.md` as the standalone public result. The guide defines required content, node kinds, and status meanings.

## 6. Check the workspace when useful

`discovery finalize` validates before publication, so validation is not required in the normal path. Run it whenever an intermediate check helps:

```sh
orchestrate discovery validate --effort "<effort>" --run "<run>"
```

The outcome is derived mechanically: any blocked question makes the run `BLOCKED`; otherwise readiness validation must pass. Fix reported problems and continue.

## 7. Finalize

When the phase is genuinely complete, finalize without a separate approval step:

```sh
orchestrate discovery finalize --effort "<effort>" --run "<run>"
```

If finalization fails structural validation, read the error, fix the workspace, and retry. A blocked investigation is a valid final result: leave the blocked question in place and finalize it as `BLOCKED`.

## 8. Report and stop

Report the run ID, the finalized artifact ID from `details.artifact`, the outcome from `details.outcome` (`IMPLEMENTATION_READY` or `BLOCKED`), every blocked question when the outcome is `BLOCKED` (read them from the graph, or run `discovery validate`, which prints them), and any defaulted provenance.

Stop at the Discovery boundary. Do not start Consensus, adopt an Agreement, or begin implementation.
