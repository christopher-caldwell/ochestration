# Run Orchestrate

This is the canonical day-to-day workflow.

You normally interact with Orchestrate through model skills, not raw CLI commands. Each skill loads
its current instructions from `orchestrate <action> guide`.

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

## 4. Build and final Audit

Later, explicitly invoke `$build` with the exact effort, Reconciled Discovery, and detailed implementation plan. That invocation authorizes Build and creates or reuses Adoption. It prepares the
contained Build files; you do not hand-author the controller JSON/TOML.

The process runs every delivery phase, independent reviews, corrections, implementation
registration, and fresh final Audits until it reaches a published Audit PASS or cannot safely
continue automatically. An external user or infrastructure need is one common blocker. There are
no normal `next`, `accept`, `continue`, or Audit-window steps. Technical suggestions remain advisory.

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

5. Later, explicitly run $build with the detailed implementation plan.

```

Everything else in the docs is explanation or reference.
