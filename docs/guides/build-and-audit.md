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

The builder may be Codex, Claude, Cursor, another tool, or a human engineer. Orchestrate does not schedule this work.

## 2. Review and commit the implementation

Review the implementation normally in the target repository, then create a Git commit representing the exact implementation you want audited:

```sh
git status
git diff

git add .
git commit -m "Implement adopted Orchestration agreement"
```

Audit is tied to this commit, not to later working-tree changes.

## 3. Register the implementation

```sh
orchestrate implementation register --effort "$EFFORT_ID"
```

Orchestrate already knows the canonical project, so there is no `--project` argument. It also infers the sole Adoption receipt for the effort when exactly one exists. Defaults are `--commit HEAD`, `--status submitted`, and `--declaration "external implementation"`.

Override them when reality differs:

```sh
orchestrate implementation register \
  --effort "$EFFORT_ID" \
  --adoption "ADOPTION-ARTIFACT" \
  --commit "abc123" \
  --status partial \
  --declaration "Partial implementation; blocked by..."
```

When several Adoption artifacts exist, registration fails and lists them; pass `--adoption "<artifact-id>"` explicitly.

Registration verifies that:

- the repository is the effort's canonical project;
- the commit descends from the Agreement baseline;
- the exact commit and tree can be resolved.

Orchestrate then retains an immutable implementation source snapshot.

Save the returned implementation artifact ID.

## 4. Run the Audit skill

Open a fresh model session and invoke `orchestrate-audit` with the effort ID (and the store root if it is not `~/.orchestration`).

The skill resolves the `orchestrate` executable, runs `orchestrate guide audit` as its controlling instructions, and locates the exact Agreement → Adoption → Implementation chain. It asks you only when several valid chains make the selection ambiguous.

Audit assesses:

```text
exact Agreement
exact adoption
exact registered implementation
```

It inspects the immutable implementation source retained for the registered commit — not a mutable later working tree — runs relevant tests, and writes `assessment.json`. There is exactly one coverage row per Agreement requirement:

```text
pass            needs evidence
fail            needs evidence and a bounded correction
unknown         conformance could not be established
not_applicable  needs a real rationale
```

Do not turn optional architectural preferences into contract failures.

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

`audit finalize` prints that verdict as `details.verdict` alongside the Audit artifact ID.

## 5. If Audit requires changes

A `CHANGES_REQUIRED` result does not change the adopted Agreement.

Correct the implementation, commit the new exact state, register the new implementation, and run Audit against that new registered implementation.

Do not silently rewrite the Agreement to make the implementation pass. If the Agreement itself is discovered to be wrong, treat that as an authority problem rather than an implementation correction: re-enter the workflow deliberately instead of mutating finalized artifacts.

## Advanced and manual operation

Finalize an Audit by hand when you produced the assessment yourself:

```sh
orchestrate audit finalize \
  --effort "$EFFORT_ID" \
  --bundle "/absolute/path/to/assessment.json" \
  --host "AUDIT-HOST" \
  --provider "AUDIT-PROVIDER" \
  --model "AUDIT-MODEL" \
  --model-effort high
```

`assessment.json` must reference the exact Agreement, Adoption, and Implementation artifacts — `orchestrate status` prints those references, including digests — along with the coverage rows and an `assessor_context` description.

To inspect the completed authority chain:

```sh
orchestrate status --effort "$EFFORT_ID"
orchestrate lineage --effort "$EFFORT_ID" --artifact "$AUDIT_ARTIFACT"
orchestrate journal --effort "$EFFORT_ID"
```

Remember the distinction:

```text
artifacts + lineage = authority
journal             = diagnostic history
```
