# Run Orchestrate

This is the canonical day-to-day workflow.

You normally interact with Orchestrate through model skills. Skills load current guidance from the
installed CLI when their operation calls for it. `$build` is the exception: it never invokes any
Orchestrate CLI command by itself. A human must explicitly authorize each Build CLI operation; the
read-only help command is `orchestrate build prepare`.

```text
Prepare → Discovery × N → Reconcile → STOP

Later, explicitly: Build → done
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

Read the reconciled document. Reconcile reports the result and stops; it does not ask for Build
approval or create Adoption.

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

Later, use `$build` to discuss and prepare the exact effort, Reconciled Discovery, and phase
implementation documents. Invoking `$build` does not run a CLI command, write Build files, start or
continue the driver, or dispatch provider work. Explicitly authorize each CLI operation. Scaffold the
Build directory, bind the exact Reconciled artifact and ordered phase directory names in plan
schema 4, and configure the worker/reviewer (and optional unblocker) in config schema 4. Each phase
has a required `phase.md` and optional immediate Markdown task/context files. Only `plan.json`
bytes are frozen by digest at initialization. Phase Markdown may be refined after a semantic stop.
Each explicit Build launch validates the plan and documents and loads config once.

An explicit `orchestrate build --effort "<effort>"` launch authorizes Rust to run the Work ↔ Review
phase loop, register the exact Implementation candidate, and invoke the independent Audit stage
automatically. No additional confirmation or orchestration command is needed before Audit. Work
commits at the product checkpoint. Review and Audit use disposable detached checkouts. A phase
Review passes to the next phase or, after the last phase, Implementation registration; corrections
return to Work with the complete report. Rust derives the Audit verdict from exact requirement
coverage: `PASS` permits reviewed completion; `CHANGES_REQUIRED` routes to final-scope Build Work,
then a new registered Implementation and another Audit; unknown or missing coverage routes through
one bounded Unblock detour. Technical suggestions remain advisory.

Audit also remains independently callable for eligible registered Implementations from manual or
external work: Reconciled Discovery → Adoption → implementation → Implementation registration →
Audit. Use the public [Audit workflow](../../crates/guides/resources/guides/audit.md); no Build
controller run or Build state is required.

`orchestrate build status --effort "<effort>"` reports durable state only. Provider failure,
malformed output, Git invariant failure, or interrupted execution stops for explicit reset.
`orchestrate build reset` restores the saved commit, removes ordinary untracked files while
preserving ignored files, and requeues the same gate. Unblock `blocked` or a repeated gate block
stops cleanly as `stopped / blocked`; once addressed, explicitly launch `orchestrate build` again. That launch preserves a
clean operator repair and continues the loop. Stderr carries compact transitions; stdout carries one
JSON result. Provider sessions are optional process-local conveniences; a new launch starts fresh
conversations from complete action packets.

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

4. Review the Reconciled Discovery; Reconcile stops.

5. Later, use $build to prepare. Separately authorize the exact CLI launch when ready.

```

Everything else in the docs is explanation or reference.
