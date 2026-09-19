# Agent-led installation

Install Orchestrate from an agent session when you want the CLI and the five checked-in skills available to the provider you are currently using.

Installation does not prepare a request or start a workflow.

## User prompt

Open the Orchestrate checkout in Codex, Claude Code, or Cursor and say:

```text
Install the Orchestration CLI and all five skills from this checkout for the provider I am using now. Follow docs/guides/agent-installation.md. Verify the installation without preparing a request or starting an investigation.
```

## Instructions for the installing agent

### 1. Resolve the checkout

Use the Orchestrate checkout the user opened or explicitly supplied.

Confirm that `cargo` and `git` are available. Do not substitute an unrelated registry package or similarly named executable.

### 2. Install or refresh the CLI

Locate any existing `orchestrate` executable and identify whether it belongs to this project.

Install from the checkout:

```sh
cargo install --path "/actual/checkout/crates/cli" --locked
```

For an explicit refresh of a confirmed existing installation from this project, add `--force` when needed.

Do not overwrite a different executable merely because it has the same name.

### 3. Install the five checked-in skills

Copy every folder under:

```text
<checkout>/skills/
```

into the current provider's personal skill directory.

Default locations:

```text
Codex       ~/.agents/skills/
Claude Code ~/.claude/skills/
Cursor      ~/.cursor/skills/
```

The five skills are:

```text
prep-discovery-ticket
prep-discovery-freeform
orchestrate-discovery
orchestrate-consensus
orchestrate-audit
```

Retain each skill folder's files, including `SKILL.md` and any provider metadata.

Inspect the destination before replacing anything. Preserve unrelated skills. If an existing Orchestrate skill has local modifications, back it up outside skill-discovery directories or ask before overwriting it.

An ordinary install affects only the current provider. Do not synchronize other providers unless the user explicitly asks.

### 4. Verify without starting a run

From outside the checkout, resolve the installed executable and run:

```sh
orchestrate --version
orchestrate init --help
orchestrate guide discovery
orchestrate guide consensus
orchestrate guide audit
```

Confirm that `init --help` includes `--from-file`.

Confirm that all five skill directories are present in the selected provider's personal skill location.

Do not run `orchestrate init` merely to test installation.

### 5. Report the result

Tell the user:

- the installed `orchestrate` executable path;
- the skill directory used;
- which five skills were installed or refreshed;
- whether CLI verification passed;
- that a fresh provider session may be necessary before new skills appear.

Do not claim that a provider UI has discovered a skill unless you can actually observe that UI.
