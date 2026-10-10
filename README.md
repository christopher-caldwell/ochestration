# Orchestrate

Orchestrate is deterministic machinery for an interactive model-window workflow:

```text
Normal:
Prepare → Discovery × N → Reconcile ─┐
                                     ├→ Reconciled Discovery → STOP
Chat:
Conversation → import as 1 Discovery ┘

Later, explicitly:
Reconciled Discovery → Adoption → Build → Implementation → Audit
```

The model does the engineering work. Orchestrate keeps the request, Discovery runs, reconciled
contract, implementation, and Audit tied to exact immutable inputs.

For a collaborative single-conversation alternative to independent Discovery runs, use the
[Chat Discovery guide](docs/chat-discovery.md). Importing its ZIP prepares the same Reconciled
Discovery and Build handoff, then stops. Normal Reconcile still requires at least two distinct
implementation-ready Discoveries.

## Start here

If Orchestrate is already installed, use the [run guide](docs/guides/run.md). It is the canonical
day-to-day workflow.

For a bounded synthetic bug, the [Discovery walkthrough](docs/guides/discovery-walkthrough.md)
shows the lifecycle, independent information flow, and replay instructions using a disposable
target repository.

The short version is:

```text
1. $prep-discovery-ticket
   or $prep-discovery-freeform

2. Open three fresh model windows and run the same prepared request in each. Run additional
   independent Discoveries when you deliberately want broader investigation or corroboration:
   $discovery "/absolute/path/request.prepared.md"

3. In a fresh model window, pass the finished Discovery directories to:
   $reconcile "/path/discovery-1" "/path/discovery-2" ...

4. Review the Reconciled Discovery and its outcome. Reconcile stops here; a `BLOCKED` result
   reports the upstream decision or evidence needed before Build.

5. For `IMPLEMENTATION_READY`, use `$build` to prepare the ordered phases and their implementation documents. Then
   explicitly authorize the Build launch. Rust runs Work ↔ Review across the phases, registers the
   exact Implementation, and invokes independent Audit automatically.
```

You normally interact through Orchestrate skills. A skill may load its current instructions from
the CLI when that specific operation is authorized. `$build` is deliberately non-executing: it
uses the checked-in Build guide and does not call even a guide command. Updating the CLI updates
its embedded guidance; reinstall skills when dispatcher behavior changes.

## Easiest installation

Open this checkout in Codex, Claude Code, or Cursor and tell the model:

> Install Orchestrate from this checkout for the model host I am using. Follow
> `docs/guides/agent-installation.md`, install the CLI and all seven checked-in user-facing skills, and verify
> the installation.

The user-facing skills are:

```text
prep-discovery-ticket
prep-discovery-freeform
discovery
reconcile
audit
build
investigate
```

See the [installation guide](docs/guides/agent-installation.md) for the manual fallback.

With `just` installed, run `just install-cli` from this checkout to install the CLI and seven skills
for Codex. Run `just update-cli` to refresh both from the current checkout, including CLI changes
that keep the same version number. Use `just install-only-cli` or `just update-only-cli` for the
binary alone, and `just install-skills` or `just update-skills` for the skills alone. Skill recipes
default to `~/.agents/skills`; pass an absolute destination to a skill recipe or set
`ORCHESTRATE_SKILLS_DIR=/absolute/path/to/host/skills` for a combined recipe.

## What each stage does

- **Prepare** reads the original ticket or freeform request and may ask focused questions about
  missing user intent before freezing the common input. It preserves the ticket verbatim and does
  not perform engineering investigation. Review the prepared request before Discovery.
- **Discovery** investigates the frozen repository and may ask you material questions.
- **Reconcile** analyzes only the Discovery outputs you explicitly give it and produces the
  authoritative answer to "what are we actually going to do?"
- **Adoption** records the exact Reconciled Discovery that Build will follow and Audit will assess
  against.
- **Build** runs checkpointed Work ↔ Review across ordered phases and produces an exact Implementation candidate.
- **Audit** independently checks that exact registered Implementation against the complete binding
  Reconciled Discovery. The unattended Rust driver invokes it automatically under the Build launch
  authorization, with one bounded Unblock detour available. Audit also supports eligible registered
  Implementations from manual or external work through its public workflow, without running Build.

## How roles talk to providers

Build config uses schema 5 and each role selects one native adapter (`codex`, `claude`, or `cursor`),
with optional native `model`, neutral `effort`, and opaque CLI `args`. When `effort` is omitted,
Orchestrate adds no first-class effort setting and chooses no default. Codex and Claude translate
exact lowercase provider-level effort values; Cursor uses provider-native effort-bearing model
strings instead. Build does not check effort support for individual models. Accepted opaque `args`
are appended unchanged after any first-class effort arguments; they may independently set effort,
which Build does not inspect, normalize, reconcile, or prevent. Build state and plan are breaking
schema versions; historical Build state is intentionally not migrated.

For deeper details, see the [documentation index](docs/README.md) and
[architecture](docs/architecture.md).

## Opt-in multi-lane investigations

Run `orchestrate investigate guide` to prepare concurrent independent reviews in consensus or wide
mode. [The investigation guide](docs/guides/investigate.md) describes pinned inputs, evidence graphs,
minority objections and retained local artifacts. This workflow preserves existing Audit and Build
authority and is never inserted into Build automatically.
