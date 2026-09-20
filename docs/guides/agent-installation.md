# Agent-led installation

Install only when the user explicitly asks to install or update Orchestrate.

Install the CLI from this checkout:

```sh
cargo install --path "/actual/checkout/crates/cli" --locked
```

Copy exactly these five folders from `<checkout>/skills/` into the current host's personal skill
directory, preserving their provider metadata:

```text
prep-discovery-ticket
prep-discovery-freeform
discovery
reconcile
audit
```

Typical locations are `~/.codex/skills`, `~/.claude/skills`, and `~/.cursor/skills`. Preserve
unrelated skills and avoid overwriting a locally modified Orchestrate skill without a backup or
user direction.

Verify without starting a workflow:

```sh
orchestrate --version
orchestrate init --help
orchestrate discovery --help
orchestrate reconcile --help
orchestrate implementation register --help
orchestrate audit --help
orchestrate guide discovery
orchestrate guide reconcile
orchestrate guide audit
```
