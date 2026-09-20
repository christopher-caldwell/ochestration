# Orchestrate

Orchestrate is deterministic machinery for an interactive model-window workflow:

```text
Prepare → Discovery × N → Reconcile → explicit approval → external Build → Audit
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

5. Give the reconciled document to your Build model.
   Have that model commit and register the implementation.

6. In a fresh model window:
   $audit
   Effort: <effort-id>
```

You normally do **not** operate the Orchestrate CLI yourself. The `discovery`, `reconcile`, and
`audit` skills call it for you.

## Easiest installation

Open this checkout in Codex, Claude Code, or Cursor and tell the model:

> Install Orchestrate from this checkout for the model host I am using. Follow
> `docs/guides/agent-installation.md`, install the CLI and all five checked-in skills, and verify
> the installation.

The five user-facing skills are:

```text
prep-discovery-ticket
prep-discovery-freeform
discovery
reconcile
audit
```

See the [installation guide](docs/guides/agent-installation.md) for the manual fallback.

## What each phase does

- **Discovery** investigates the frozen repository and may ask you material questions.
- **Reconcile** analyzes only the Discovery outputs you explicitly give it and produces the
  authoritative answer to "what are we actually going to do?"
- **Build** is external and decides how to implement that reconciled result.
- **Audit** checks the exact registered implementation against the binding reconciled requirements.

For deeper details, see the [documentation index](docs/README.md) and
[architecture](docs/architecture.md).
