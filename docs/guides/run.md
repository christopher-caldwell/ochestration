# Run Orchestrate

This is the canonical day-to-day workflow.

You normally interact with Orchestrate through model skills. Skills load current guidance from the
installed CLI when their operation calls for it. `$build` is the exception: it never invokes any
Orchestrate CLI command by itself. A human must explicitly authorize each Build CLI operation; the
read-only help command is `orchestrate build prepare`.

```text
Prepare → Discovery × N → Reconcile → STOP

Later, explicitly authorized:

Build → Implementation → Audit → reviewed final product
```

## 1. Prepare the request

### Existing ticket

Provide the original ticket as a Markdown file. You may also provide **extra context**.

Example extra context:

```markdown
This appears to happen only in production.

I am not sure whether staging behavior is expected to remain unchanged.
```

Then invoke:

```text
$prep-discovery-ticket
```

Give the model:

```text
Ticket: /absolute/path/to/ticket.md
Optional context: /absolute/path/to/notes.md
Project: /absolute/path/to/target-repository
Effort: short-visible-name
```

Prepare may ask focused questions about missing user decisions that would otherwise make the
independent Discoveries assume different things. It does not investigate technical facts. The
skill returns a prepared request file containing the original ticket verbatim. Review it before
Discovery.

That prepared file is the input to every Discovery run.

### Freeform work

Put the request and context in a Markdown file, then invoke:

```text
$prep-discovery-freeform
```

Give it the request file, target repository, and a short effort slug.

Prepare may ask the same kind of focused intent questions, then returns the prepared request.
Review it before Discovery.

For more detail, see [Request preparation](request-preparation.md).

---

## 2. Run independent Discoveries

Open several **fresh model windows**.

Three independent Discoveries are the normal default. Run additional independent Discoveries when
you deliberately want broader investigation or additional corroboration; no count has special
quorum or confidence meaning.

In every window, run the **same** prepared request:

```text
$discovery "/absolute/path/to/request.prepared.md"
```

Then let the model investigate.

If a Discovery reaches a material decision it cannot safely make, it may ask you a question in that
window. Answer normally. Do not tell one Discovery what another Discovery concluded.

When a Discovery finishes, it reports:

```text
run ID
human source label
artifact ID
outcome
published Discovery directory
```

The only value you need for the next phase is the **published Discovery directory**.

Collect the directories from the successful `IMPLEMENTATION_READY` runs.

Example:

```text
/path/to/discovery-1
/path/to/discovery-2
/path/to/discovery-3
```

A `BLOCKED` Discovery is a valid result, but do not pass it to Reconcile.

For more detail, see [Discovery](discovery.md).

---

## 3. Reconcile the results

Open one fresh model window.

Pass every finalized Discovery directory you want included:

```text
$reconcile   "/path/to/discovery-1"   "/path/to/discovery-2"   "/path/to/discovery-3"
```

Reconcile's only engineering evidence is those selected Discovery outputs. Frozen explicit
constraints remain direct user authority and the frozen request/context remain available for goal,
identity, and lineage; neither permits another engineering investigation. It does not reopen the
repository or become another Discovery lane.

It produces one rich **Reconciled Discovery** with:

```text
Selected direction, changed and unchanged behavior
Binding requirements and precise acceptance criteria
Evidence mapping, verification methods, and limitations
Disagreements, rejected alternatives, risks, compatibility, and caveats
Advisory technical suggestions and blocking issues, if any
```

The binding requirements answer: **what must Build actually accomplish?**

Technical suggestions are implementation guidance only.

Read the reconciled document. Reconcile reports the published outcome and stops; it does not ask
for Build approval or create Adoption. If the outcome is `BLOCKED`, resolve the reported issue
through the appropriate upstream workflow before planning Build. Only `IMPLEMENTATION_READY`
receives the normal Build handoff.

Before closing the Reconcile window, keep:

```text
Effort ID
Reconciled document path
```

The effort slug is also visible in human-readable Orchestrate artifact paths under:

```text
.../projects/<project-name>/efforts/<effort-slug>/...
```

For more detail, see [Reconcile](reconcile.md).

---

## 4. Build, then independent Audit

After `IMPLEMENTATION_READY`, use `$build` to prepare the ordered phases and their implementation documents for the exact
Reconciled Discovery. Preparation does not run a CLI command, start the driver, or dispatch provider
work. See [Build and Audit](build-and-audit.md) for setup details. When ready, explicitly authorize
the Build launch:

```sh
orchestrate build --effort "<effort>"
```

When Build begins, Rust creates or reuses Adoption for that exact Reconciled Discovery. Each phase
runs through Work and Review; Review corrections return to Work, and the phase advances after Review
passes. After every phase passes, Rust registers the exact Implementation and invokes independent
Audit automatically; no separate process or approval is needed before Audit. Audit checks that
Implementation against the complete adopted Reconciled Discovery. If Audit requires changes, the
same driver sends them to final-scope Build Work, registers a new Implementation, and audits it
again.

Audit can also run independently against any eligible registered Implementation, including manual
or external work. Use the public [Audit workflow](../../crates/guides/resources/guides/audit.md) to
select one exact Reconciled Discovery → Adoption → Implementation chain.

If an operation is blocked, Rust allows one bounded Unblock detour. If that cannot resolve the
blocker, Rust stops cleanly; address it and explicitly launch Build again. If provider failure,
malformed output, Git invariant failure, or interruption makes execution uncertain, explicitly
reset to the saved checkpoint before continuing. `orchestrate build status --effort <effort>` reports
state without dispatching work, and `orchestrate build reset --effort <effort>` restores the
checkpoint and requeues the same gate.

For more detail, see [Build and Audit](build-and-audit.md).

---

# The version to remember

Once installed:

```text
1. $prep-discovery-ticket
   or $prep-discovery-freeform

2. $discovery "/path/request.prepared.md"
   Run the same file in three fresh model windows; add more deliberately when useful.

3. $reconcile "/path/discovery-1" "/path/discovery-2" ...

4. Review the Reconciled Discovery and its outcome; Reconcile stops. Resolve a `BLOCKED` result
   upstream before Build planning.

5. For `IMPLEMENTATION_READY`, use $build to prepare the phase implementation documents. Explicitly authorize the Build
   launch when ready; Rust runs Work ↔ Review across phases, registers the exact Implementation,
   and invokes independent Audit.
```

Everything else in the docs is explanation or reference.
