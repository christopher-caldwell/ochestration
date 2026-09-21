# Orchestrate

Orchestrate is deterministic machinery for an interactive model-window workflow:

```text
Prepare → Discovery × N → Reconcile → explicit approval → Build → done
```

The model does the engineering work. Orchestrate keeps the request, Discovery runs, reconciled
contract, implementation, and Audit tied to exact immutable inputs.

## Start here

If Orchestrate is already installed, use the [run guide](docs/guides/run.md). It is the canonical
day-to-day workflow.

The short version is:

```text
1. $prep-discovery-ticket
   or $prep-discovery-freeform

2. Open 3–5 fresh model windows and run the same prepared request in each:
   $discovery "/absolute/path/request.prepared.md"

3. In a fresh model window, pass the finished Discovery directories to:
   $reconcile "/path/discovery-1" "/path/discovery-2" ...

4. Review and approve the Reconciled Discovery.

5. In the model window, run `$build` with the approved detailed implementation plan.
   It prepares Build, drives work/review/correction, registers the implementation, and runs the
   final Audit until completion or a genuine external requirement.
```

You normally do **not** operate the Orchestrate CLI yourself. The `discovery`, `reconcile`,
`build`, and `audit` skills call it for you.

## Easiest installation

Open this checkout in Codex, Claude Code, or Cursor and tell the model:

> Install Orchestrate from this checkout for the model host I am using. Follow
> `docs/guides/agent-installation.md`, install the CLI and all six checked-in user-facing skills, and verify
> the installation.

The user-facing skills are:

```text
prep-discovery-ticket
prep-discovery-freeform
discovery
reconcile
audit
build
```

See the [installation guide](docs/guides/agent-installation.md) for the manual fallback.

## What each phase does

- **Discovery** investigates the frozen repository and may ask you material questions.
- **Reconcile** analyzes only the Discovery outputs you explicitly give it and produces the
  authoritative answer to "what are we actually going to do?"
- **Build** is an unattended Rust-driven worker/reviewer loop for an approved phased plan.
- **Audit** checks the exact registered implementation against the binding reconciled requirements;
  unattended Build invokes it automatically.

For deeper details, see the [documentation index](docs/README.md) and
[architecture](docs/architecture.md).
