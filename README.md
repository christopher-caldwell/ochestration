# Orchestrate

Orchestrate is deterministic machinery for an interactive model-window workflow:

```text
Prepare → Discovery × N → Reconcile → explicit approval → external Build → Audit
```

Models investigate, ask material questions, reconcile evidence, and assess implementation. Rust
freezes the request and Git baseline, creates isolated Discovery workspaces, validates evidence and
lineage, publishes immutable artifacts, and derives Audit verdicts. It does not launch models or
decide engineering truth.

Install the CLI with `cargo install --path crates/cli --locked`, then copy these five checked-in
skills to your model host: `prep-discovery-ticket`, `prep-discovery-freeform`, `discovery`,
`reconcile`, and `audit`.

The normal experience is simply:

```text
$prep-discovery-ticket
$discovery
$discovery
$discovery
$reconcile
$audit
```

Run Discovery in as many independent model windows as you want. Reconcile receives the exact
finalized Discovery artifacts you choose; it never investigates the repository again. Its binding
requirements become Build and Audit authority. Its technical suggestions remain advisory unless
their underlying property is stated separately as a binding requirement.

See the [run guide](docs/guides/run.md), [architecture](docs/architecture.md), and
[installation guide](docs/guides/agent-installation.md).
