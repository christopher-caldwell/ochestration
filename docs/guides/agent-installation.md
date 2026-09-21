# Install Orchestrate

The easiest installation is agent-led. You should not need to manually decide which files belong
where.

## Recommended: let the model install it

Open the Orchestrate checkout in the model host you want to use — Codex, Claude Code, or Cursor —
and send:

> Install Orchestrate from this checkout for the model host I am using. Install the CLI and exactly
> these six checked-in skills: `prep-discovery-ticket`, `prep-discovery-freeform`,
> `discovery`, `reconcile`, `build`, and `audit`. Preserve unrelated existing skills and verify the
> installation when finished.

The model should follow this guide and perform the commands itself.

Afterward, verify that these skills are available:

```text
prep-discovery-ticket
prep-discovery-freeform
discovery
reconcile
audit
```

That is all you need for normal use.

## Manual fallback

Install the CLI from the checkout:

```sh
cargo install --path "/absolute/path/to/ochestration/crates/cli" --locked
```

Then copy exactly these six directories from `skills/` into the current host's personal skill
directory:

```text
prep-discovery-ticket
prep-discovery-freeform
discovery
reconcile
audit
build
```

Typical skill directories are:

```text
Codex       ~/.codex/skills
Claude Code ~/.claude/skills
Cursor      ~/.cursor/skills
```

Preserve unrelated skills. If an Orchestrate skill already exists and may have local edits, back it
up before replacing it.

## Verification

Installation is good when these commands succeed:

```sh
orchestrate --version
orchestrate discovery --help
orchestrate reconcile --help
orchestrate audit --help
orchestrate build --help
orchestrate guide discovery
orchestrate guide reconcile
orchestrate guide audit
```

Do not start a workflow as part of installation.

Next: [Run Orchestrate](run.md).
