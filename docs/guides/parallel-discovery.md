# Run Discovery in parallel across providers

This is the first-class workflow for the sentence:

> Codex, Claude, Cursor, Provider X — run the same request for Discovery.

If you want the shorter skill-driven ticket path instead, see
[Ticket workflow](ticket-workflow.md).

Preparation happens once, in sequence. The actual provider agents then run concurrently in
isolated workspaces, and a single reconciler consumes every result afterwards.

## 0. Prerequisites

- `orchestrate` is installed and on your `PATH`.
- Each headless provider CLI you plan to use is installed (`codex`, `claude`).
- For interactive providers (Cursor), you have a way to open the workspace and invoke the `orchestrate-discovery` skill with the pre-prepared run ID and workspace.
- `jq` is available if you use the bundled launcher script.
- The store, provider plan, launch directory, prompts, and logs must all be outside the target repository. `orchestrate` never writes into the repository it is investigating, and it rejects a store root or explicit launch directory that is inside the target repository.

## 1. Declare the providers

Create `orchestrate.providers.toml` outside the target repository. Copy
[`docs/examples/providers.example.toml`](../examples/providers.example.toml) as a starting point.
Slot names are provider names and are frozen into the cohort. If the file is named
`orchestrate.providers.toml` in the directory where you run `orchestrate`, the `--providers`
argument can be omitted; otherwise pass its absolute path explicitly.

```toml
quorum = "majority"           # or an integer in 2..N

[[provider]]
name = "codex"
host = "codex"
provider = "deepseek"
model = "deepseek-v4-pro"
model_effort = "high"
interactive = false
command = [
  "codex", "exec", "--skip-git-repo-check",
  "--dangerously-bypass-approvals-and-sandbox",
  "-C", "{workspace}", "--output-last-message", "{log}/last-message.json",
  "{prompt}",
]

[[provider]]
name = "claude"
host = "claude-code"
provider = "anthropic"
model = "claude-sonnet-4-5"
interactive = false
command = ["claude", "-p", "{prompt}", "--add-dir", "{workspace}", "--output-format", "json"]

[[provider]]
name = "cursor"
host = "cursor"
interactive = true
command = ["cursor", "{workspace}"]
```

Codex uses `--dangerously-bypass-approvals-and-sandbox` because the generated prompt must run
`orchestrate discovery finalize`, which writes to the shared store outside the per-slot workspace.
Use it only for locally trusted parallel runs; isolation is still enforced by distinct workspaces,
logs, environment, and the prompt rule against reading sibling workspaces.

The Codex flags below are spelled for the installed Codex CLI in this repository. Verify the Claude
flags against your installed `claude --help`; the isolation and finalize semantics stay the same
even if a flag spelling differs.

Available command placeholders: `{slot}`, `{effort}`, `{run}`, `{root}`, `{workspace}`,
`{source}`, `{prompt}`, and `{log}`.

To add **Provider X**, add another `[[provider]]` block with its own `name`, `host`, `interactive`,
and `command` template.

## 2. Initialize the effort

```sh
orchestrate init \
  --project "/absolute/path/to/repository" \
  --effort "my-discovery" \
  --request "Investigate the requested behavior exactly." \
  --providers "./orchestrate.providers.toml"
```

If you already prepared a reviewed request file, combine the same `--providers` flag with
`--from-file` instead of repeating `--project`, `--effort`, or `--request`:

```sh
orchestrate init \
  --from-file "/absolute/path/to/request.prepared.md" \
  --providers "/absolute/path/to/orchestrate.providers.toml"
```

Copy the returned effort ID and export it:

```sh
export EFFORT_ID="PASTE-EFFORT-ID"
```

`quorum = "majority"` means `floor(N/2)+1` slots must support every mandatory Consensus
requirement and the package intersection. For three slots that is two; for four slots it is three.
Override it at init with `--quorum 2` (or another integer in `[2, N]`) when you want a different
strict threshold.

## 3. Prepare every slot in sequence

This is the only sequential step. It creates one isolated run workspace per provider and emits a
launch manifest. If `--launch-dir` is omitted, the manifest is written under the store's effort
directory (never into the target repository). If you pass it explicitly, choose a path outside the
target repository:

```sh
orchestrate discovery prepare-all \
  --effort "$EFFORT_ID" \
  --providers "./orchestrate.providers.toml" \
  --launch-dir "/absolute/path/to/launch"
```

The CLI verifies this launch directory is outside the target repository before it writes anything.

`prepare-all` prints the launch directory. Inside it you will find:

```text
launch.json
codex/prompt.md
claude/prompt.md
cursor/prompt.md
```

Each provider's `command` has already been expanded with that provider's own workspace, prompt,
and log directory.

## 4. Launch the providers in parallel

```sh
scripts/discovery-parallel.sh "/absolute/path/to/launch"
```

or, from the repository root:

```sh
just discovery-parallel "/absolute/path/to/launch"
```

The launcher starts every `interactive = false` provider concurrently, each with its own working
directory, environment (`ORCHESTRATE_SLOT`, `ORCHESTRATE_EFFORT`, `ORCHESTRATE_RUN`,
`ORCHESTRATE_WORKSPACE`), and `stdout.log`/`stderr.log`/`exit` files under its log directory.

For every `interactive = true` provider it prints the exact workspace, run ID, prompt path, and
command instead of spawning it. Open the provider in that workspace and invoke the
`orchestrate-discovery` skill with the printed effort ID, slot, run ID, workspace, and store root.
The skill will load `orchestrate guide discovery`, use the already-prepared run, and finalize
without running `orchestrate discovery prepare` again.

The launcher waits for the headless jobs, then runs `orchestrate status --effort` and prints which
slots have finalized. It never runs Consensus.

## 5. Confirm the results

```sh
orchestrate status --effort "$EFFORT_ID"
```

Every slot must show a finalized `Discovery` artifact with outcome `IMPLEMENTATION_READY`.
A `BLOCKED` Discovery is a valid artifact but is ineligible for Consensus.

Each run writes only inside its own workspace. Providers must not read sibling workspaces, parent
store records, or one another's outputs.

## 6. Reconcile once all results are in

Run the reconciler in one fresh session, after every slot has finalized:

```text
orchestrate-consensus  # or /orchestrate-consensus in Claude/Cursor
```

Manual equivalent:

```sh
orchestrate consensus inputs --effort "$EFFORT_ID"
orchestrate consensus finalize \
  --effort "$EFFORT_ID" \
  --opinion "$CODEX_ARTIFACT" \
  --opinion "$CLAUDE_ARTIFACT" \
  --opinion "$CURSOR_ARTIFACT" \
  --bundle "/absolute/path/to/proposal.json"
```

Consensus now resolves one eligible Discovery artifact per cohort slot and applies the cohort
quorum from initialization. Add one `--opinion` line per additional cohort slot; run
`orchestrate status --effort "$EFFORT_ID"` to see the exact slot names and artifact IDs.

## Non-interference guarantees

- `discovery prepare-all` prepares workspaces one at a time.
- Each provider receives a distinct workspace with its own `source/` checkout, `run.json`,
  `request.md`, and `context.json`.
- The launcher gives each headless provider its own working directory, environment, and log files.
- Concurrent `discovery finalize` calls serialize their journal appends so `journal.jsonl` stays
  line-framed.
- Providers are instructed in their generated prompt not to inspect sibling workspaces or other
  providers' outputs.
