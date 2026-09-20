---
name: discovery
description: Run one Orchestrate Discovery pass from a prepared request Markdown file, then return the finalized output directory.
disable-model-invocation: true
---

# Run Discovery from a prepared request

Use this skill when the user provides a prepared request file and asks to run Discovery in this
provider session. The user should not have to type Orchestrate CLI commands; you run them.

## 1. Get the input and slot

- Require one absolute path to a prepared `*.prepared.md` or otherwise frontmatter-bearing request
  file.
- If the user supplied a slot after the path, use it. Otherwise, if `ORCHESTRATE_SLOT` is set, use
  that value. Otherwise, if you can reliably identify this session's provider (for example Codex,
  Claude Code, or Cursor) and that name matches one of the frontmatter `slots`, use that slot.
  Otherwise ask one question: **"Which Discovery slot should this session own?"** and list the
  `slots` from the file's frontmatter. Never guess the slot.
- Read the frontmatter. Record `root`, `project`, `effort`, `request_kind`, `slots`, and `quorum`.
  If required fields are missing, stop and tell the user to prepare the file first.

## 2. Resolve the CLI

Resolve `orchestrate` with `command -v orchestrate`, then fall back to `$CARGO_HOME/bin/orchestrate`
or `~/.cargo/bin/orchestrate`. If it is not found, stop and tell the user.

## 3. Initialize or load the shared effort

Run:

```sh
orchestrate init --from-file "<absolute prepared request path>"
```

The command is idempotent for the same prepared request: if another provider already created this
effort, it returns the existing effort instead of failing. Capture `details.effort` from the JSON
output.

## 4. Prepare this session's run

Always pass the store root from the frontmatter in the global position:

```sh
orchestrate --root "<root>" discovery prepare --effort "<effort>" --slot "<slot>"
```

Capture `details.run` and `details.workspace` from the JSON output.

## 5. Load the controlling instructions

```sh
orchestrate --root "<root>" guide discovery
```

Follow that guide for the rest of the phase.

## 6. Read the workspace and investigate

Read `run.json`, `request.md`, and `context.json` in the workspace. Investigate only the frozen
`source/` checkout. Ask the user directly in this session about material ambiguity, conflicting
evidence, or missing product/engineering decisions. Do not inspect sibling Discovery workspaces,
other providers' outputs, or the live target repository. Do not edit the target repository.

## 7. Write the public result

Write:

```text
technical-spec.md
graph/*.md
```

Use the evidence graph and content rules from `orchestrate guide discovery`. A material unknown
that cannot be resolved must be a blocked Question, not an assumption.

## 8. Validate when useful

```sh
orchestrate --root "<root>" discovery validate --effort "<effort>" --run "<run>"
```

`finalize` validates anyway, so this is optional.

## 9. Finalize

```sh
orchestrate --root "<root>" discovery finalize --effort "<effort>" --run "<run>"
```

If finalization fails, read the error, fix the workspace, and retry. A blocked Discovery is a valid
final result; leave the blocked question in place and finalize it as `BLOCKED`.

## 10. Report the output directory

From the finalize JSON, capture `details.artifact.artifact_id` and `details.outcome`. Locate the
published bundle directory:

```sh
ls -d "<root>"/projects/*/efforts/"<effort>"/discovery/"<artifact-id>"
```

Report to the user:

- the output directory path above;
- the run ID;
- the Discovery artifact ID;
- the outcome (`IMPLEMENTATION_READY` or `BLOCKED`);
- any blocked questions when the outcome is `BLOCKED`.

Do not start Consensus, adopt an Agreement, or begin implementation.
