# Install Orchestrate

The easiest installation is agent-led. You should not need to manually decide which files belong
where.

Installed skills are dispatchers. They ask the current `orchestrate` binary for instructions.
Updating the CLI updates all embedded Orchestrate guides and templates. Reinstall skills only when
the dispatcher command itself changes.

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
build
audit
```

That is all you need for normal use. Work, review, final Audit, and unblock instructions are not
skills. The Build driver loads them from the installed CLI.

## Manual fallback

Install the CLI from the checkout:

```sh
cargo install --path "/absolute/path/to/ochestration/crates/cli" --locked
```

Then run the checked-in installer with the current host's personal skill directory:

```sh
./scripts/install-skills.sh "/absolute/path/to/host/skills"
```

It replaces exactly these six directories from `skills/`:

```text
prep-discovery-ticket
prep-discovery-freeform
discovery
reconcile
build
audit
```

Typical skill directories are:

```text
Codex       ~/.agents/skills
Claude Code ~/.claude/skills
Cursor      ~/.cursor/skills
```

The installer preserves unrelated skills, replaces existing Orchestrate dispatchers, verifies all
six, and does not create backup skill directories.

## Verification

Installation is good when these commands succeed from any directory:

```sh
orchestrate --version
orchestrate discovery --help
orchestrate reconcile --help
orchestrate audit --help
orchestrate build --help
orchestrate prep-discovery-ticket guide
orchestrate prep-discovery-freeform guide
orchestrate discovery guide
orchestrate reconcile guide
orchestrate build guide
orchestrate audit guide
```

Do not start a workflow as part of installation.

Next: [Run Orchestrate](run.md).
