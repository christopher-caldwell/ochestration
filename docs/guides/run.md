# Run Orchestrate

This is the canonical day-to-day workflow.

You normally interact with Orchestrate through model skills, not raw CLI commands.

```text
Prepare → Discovery × N → Reconcile → approve → Build → Audit
```

## 1. Prepare the request

### Existing ticket

Create a small Markdown file containing any **extra context** you want Discovery to know.

Example:

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
Notes: /absolute/path/to/notes.md
Project: /absolute/path/to/target-repository
Effort: short-visible-name
```

The skill returns a prepared request file.

Open it, paste the original ticket verbatim into the ticket placeholder, save it, and review the
file once.

That prepared file is the input to every Discovery run.

### Freeform work

Put the request and context in a Markdown file, then invoke:

```text
$prep-discovery-freeform
```

Give it the request file, target repository, and a short effort name.

It returns the prepared request file directly.

For more detail, see [Request preparation](request-preparation.md).

---

## 2. Run independent Discoveries

Open several **fresh model windows**.

Three is a reasonable default. Use five or more when the change is important, ambiguous, or worth
extra independent investigation.

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
/path/to/discovery-4
/path/to/discovery-5
```

A `BLOCKED` Discovery is a valid result, but do not pass it to Reconcile.

For more detail, see [Discovery](discovery.md).

---

## 3. Reconcile the results

Open one fresh model window.

Pass every finalized Discovery directory you want included:

```text
$reconcile   "/path/to/discovery-1"   "/path/to/discovery-2"   "/path/to/discovery-3"   "/path/to/discovery-4"   "/path/to/discovery-5"
```

Reconcile analyzes only those selected Discovery outputs. It does not reopen the repository and
does not become another Discovery lane.

It produces one **Reconciled Discovery** with:

```text
Core result
Binding requirements
Advisory technical suggestions
Blocking issues, if any
```

The binding requirements answer: **what must Build actually accomplish?**

Technical suggestions are implementation guidance only.

Read the reconciled document. If the model asks whether you approve it for Build, approve it only
when the binding result is what you want implemented. Approval creates the adoption receipt.

Before closing the Reconcile window, keep:

```text
Effort ID
Reconciled document path
```

The effort ID is also visible in Orchestrate artifact paths under:

```text
.../efforts/<effort-id>/...
```

For more detail, see [Reconcile](reconcile.md).

---

## 4. Build

Build is intentionally outside Orchestrate.

Give your coding model the reconciled document and the target repository.

A useful handoff is:

> Implement the adopted Reconciled Discovery at
> `/absolute/path/to/reconciled-discovery.md`.
>
> Treat its binding requirements as the authority. Technical suggestions are useful starting
> points, not mandatory implementation choices unless the underlying property is itself a binding
> requirement.
>
> When the implementation is complete, run the relevant tests, commit the finished work, and
> register that exact commit with Orchestrate for effort `<effort-id>`.

Let the Build model run the registration command. You do not need to operate it yourself.

For reference, the underlying command is:

```sh
orchestrate implementation register --effort "<effort-id>"
```

---

## 5. Audit

Open a fresh model window and invoke:

```text
$audit
```

Then give it:

```text
Effort: <effort-id>
```

Audit finds the adopted Reconciled Discovery and registered implementation, inspects the immutable
implementation snapshot, runs relevant checks, and verifies every binding requirement.

It returns:

```text
PASS
CHANGES_REQUIRED
BLOCKED
```

- `PASS` — every binding requirement was verified.
- `CHANGES_REQUIRED` — at least one binding requirement failed.
- `BLOCKED` — Audit could not establish whether every requirement was satisfied.

Audit does not require Build to follow advisory technical suggestions.

For more detail, see [Build and Audit](build-and-audit.md).

---

# The version to remember

Once installed:

```text
1. $prep-discovery-ticket
   or $prep-discovery-freeform

2. $discovery "/path/request.prepared.md"
   Run the same file in 3–5 fresh model windows.

3. $reconcile "/path/discovery-1" "/path/discovery-2" ...

4. Approve the Reconciled Discovery.

5. Give reconciled-discovery.md to Build.
   Tell Build to commit and register when done.

6. $audit
   Effort: <effort-id>
```

Everything else in the docs is explanation or reference.
