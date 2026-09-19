# Agent-led installation

Use this procedure only for an explicit request to install or update Orchestration for the provider currently in use. It installs the CLI and all five checked-in skills; it does not prepare a request, initialize an effort, or start an investigation.

## Procedure

1. Resolve the Orchestration checkout the user opened. Confirm that `cargo` and `git` are available. Use that checkout, not an unrelated registry package or similarly named executable.
2. Locate any existing `orchestrate` executable and identify whether it is this project. Install the checkout using Cargo:

   ```sh
   cargo install --path "/actual/checkout/crates/cli" --locked
   ```

   An explicit refresh of a confirmed installation from this project may add `--force`. Do not overwrite a different executable merely because it has the same name.
3. Install the five folders from `<checkout>/skills/` into the current provider's personal skill directory, retaining each `SKILL.md` and `agents/openai.yaml`:

   - Codex: `~/.agents/skills/`
   - Claude Code: `~/.claude/skills/`
   - Cursor: `~/.cursor/skills/`

   First inspect the provider's existing direct copies and any established custom location. Preserve unrelated skills and user modifications. Refresh a clearly identified, unmodified Orchestration skill when appropriate. Back up a modified Orchestration copy outside a skill-discovery directory, or ask before overwriting it. Do not synchronize other providers or modify plugin caches.
4. From outside the checkout, resolve the executable that was installed and verify:

   ```sh
   orchestrate --version
   orchestrate init --help
   orchestrate guide discovery
   orchestrate guide consensus
   orchestrate guide audit
   ```

   Confirm that `init --help` includes `--from-file`; check that all five copied folders and their metadata are present. Do not run `init` to verify installation.
5. Report the installed binary path, skill paths, and native invocation names: `prep-discovery-ticket`, `prep-discovery-freeform`, `orchestrate-discovery`, `orchestrate-consensus`, and `orchestrate-audit`. Mention that a fresh provider session may be required, and distinguish copied files from observing them in the provider UI.

## User prompt

Users can ask their current provider:

```text
Install the Orchestration CLI and all five skills from this checkout for the
provider I am using now. Follow docs/guides/agent-installation.md. Verify the
installation without preparing a request or starting an investigation.
```
