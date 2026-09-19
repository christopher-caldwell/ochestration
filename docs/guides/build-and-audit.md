# Build and Audit

Build is intentionally external to Orchestrate.

The adopted Agreement tells your implementation process what to build. Orchestrate becomes involved again when you register the exact resulting Git commit and ask Audit to evaluate it.

```text
adopted Agreement
      ↓
external Build
      ↓
Git commit
      ↓
implementation registration
      ↓
Audit
```

## 1. Build from the adopted Agreement

Give your builder the exact adopted Agreement.

A useful instruction is:

```text
Implement the adopted Agreement exactly.

Treat the Agreement as the implementation contract. Do not re-derive the ticket from scratch and do not add unrelated scope.

Run the relevant project checks and leave the completed implementation ready for review.
```

The builder may be Codex, Claude, Cursor, another tool, or a human engineer.

Orchestrate does not schedule this work.

## 2. Review and commit the implementation

Review the implementation normally in the target repository.

Then create a Git commit representing the exact implementation you want audited:

```sh
git status
git diff

git add .
git commit -m "Implement adopted Orchestration agreement"
```

Capture the commit:

```sh
git rev-parse HEAD
```

Save it as:

```text
IMPLEMENTATION_COMMIT
```

Audit is tied to this commit, not to later working-tree changes.

## 3. Register the implementation

You need:

```text
EFFORT_ID
ADOPTION_ARTIFACT
TARGET_REPO
IMPLEMENTATION_COMMIT
```

Run:

```sh
orchestrate --root "$ORCH_ROOT" implementation register \
  --effort "$EFFORT_ID" \
  --adoption "$ADOPTION_ARTIFACT" \
  --project "$TARGET_REPO" \
  --commit "$IMPLEMENTATION_COMMIT" \
  --declaration "Implemented from adopted Agreement" \
  --status submitted \
  --host "BUILDER-HOST" \
  --provider "BUILDER-PROVIDER" \
  --model "BUILDER-MODEL"
```

If the implementation was manual, use truthful metadata rather than inventing a model identity.

Registration verifies that:

- the repository matches the effort project;
- the commit descends from the Agreement baseline;
- the exact commit and tree can be resolved.

Orchestrate then retains an immutable implementation source snapshot.

Save the returned implementation artifact ID:

```text
IMPLEMENTATION_ARTIFACT
```

## 4. Start a fresh Audit session

Open a fresh model session and invoke `orchestrate-audit`.

The model-facing rules are available from:

```sh
orchestrate guide audit
```

Audit must assess:

```text
exact Agreement
exact adoption
exact registered implementation
```

It should inspect the immutable implementation source retained for the registered commit, not a mutable later working tree.

## 5. Produce the Audit assessment

For the manual path, have the Audit model write `assessment.json` matching `scripts/audit-schema.json`.

There is exactly one coverage row per Agreement requirement.

Example pass:

```json
{
  "requirement_id": "R1",
  "state": "pass",
  "rationale": "The implementation satisfies the required behavior.",
  "evidence": ["src/example.rs:40-65", "cargo test relevant_test"],
  "correction": ""
}
```

Example failure:

```json
{
  "requirement_id": "R2",
  "state": "fail",
  "rationale": "The implementation changes behavior that the Agreement requires preserving.",
  "evidence": ["src/example.rs:80-94"],
  "correction": "Restore the preserved behavior while keeping R1 intact."
}
```

Allowed states are:

```text
pass
fail
unknown
not_applicable
```

A pass needs evidence.

A fail needs evidence and a bounded correction.

`unknown` means conformance could not be established.

`not_applicable` requires a real rationale.

Do not turn optional architectural preferences into contract failures.

## 6. Finalize Audit

Run:

```sh
orchestrate --root "$ORCH_ROOT" audit finalize \
  --effort "$EFFORT_ID" \
  --bundle "/absolute/path/to/assessment.json" \
  --host "AUDIT-HOST" \
  --provider "AUDIT-PROVIDER" \
  --model "AUDIT-MODEL" \
  --model-effort high
```

Rust derives the final verdict from the coverage rows and implementation status:

```text
any fail
    → CHANGES_REQUIRED

otherwise any unknown or missing row
    → BLOCKED

otherwise
    → PASS
```

A partial or blocked implementation cannot pass.

Save the Audit artifact ID.

## 7. Inspect the completed authority chain

Use status for a broad view:

```sh
orchestrate --root "$ORCH_ROOT" status --effort "$EFFORT_ID"
```

Use lineage to reconstruct artifact authority:

```sh
orchestrate --root "$ORCH_ROOT" lineage \
  --effort "$EFFORT_ID" \
  --artifact "$AUDIT_ARTIFACT"
```

Use the journal for operational history:

```sh
orchestrate --root "$ORCH_ROOT" journal --effort "$EFFORT_ID"
```

Remember the distinction:

```text
artifacts + lineage = authority
journal             = diagnostic history
```

## 8. If Audit requires changes

A `CHANGES_REQUIRED` result does not change the adopted Agreement.

Correct the implementation, commit the new exact state, register the new implementation, and run Audit against that new registered implementation.

Do not silently rewrite the Agreement to make the implementation pass.

If the Agreement itself is discovered to be wrong, treat that as an authority problem rather than an implementation correction. Re-enter the workflow deliberately instead of mutating finalized artifacts.
