# Orchestrate

Orchestrate is deterministic machinery for an interactive model-window workflow:

```text
Prepare → Discovery × N → Reconcile → STOP

Later, explicitly: Build → done
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

2. Open three fresh model windows and run the same prepared request in each. Run additional
   independent Discoveries when you deliberately want broader investigation or corroboration:
   $discovery "/absolute/path/request.prepared.md"

3. In a fresh model window, pass the finished Discovery directories to:
   $reconcile "/path/discovery-1" "/path/discovery-2" ...

4. Review the Reconciled Discovery. Reconcile stops here.

5. Later, use `$build` to discuss and prepare the effort and detailed implementation plan. It does
   not run the CLI or start/resume the driver. Separately authorize the exact Build CLI operation
   when ready; the Rust driver then owns work/review/correction, implementation registration, and
   final Audit until completion or a safe stop.
```

You normally interact through Orchestrate skills. A skill may load its current instructions from
the CLI when that specific operation is authorized. `$build` is deliberately non-executing: it
uses the checked-in Build guide and does not call even a guide command. Updating the CLI updates
its embedded guidance; reinstall skills when dispatcher behavior changes.

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

- **Prepare** reads the original ticket or freeform request and may ask focused questions about
  missing user intent before freezing the common input. It preserves the ticket verbatim and does
  not perform engineering investigation. Review the prepared request before Discovery.
- **Discovery** investigates the frozen repository and may ask you material questions.
- **Reconcile** analyzes only the Discovery outputs you explicitly give it and produces the
  authoritative answer to "what are we actually going to do?"
- **Build** is a checkpointed Rust-driven Work → Review → Audit gate runner with one bounded Unblock detour.
- **Audit** checks the exact registered implementation against the binding reconciled requirements;
  unattended Build invokes it automatically.

## How roles talk to providers

Build config uses schema 4 and each role selects one native adapter (`codex`, `claude`, or `cursor`),
with optional native `model` and opaque CLI `args`. The adapters preserve the launching environment
and only own provider transport, working-directory, and session flags. Build state and plan are
breaking schema versions; historical Build state is intentionally not migrated.

For deeper details, see the [documentation index](docs/README.md) and
[architecture](docs/architecture.md).
