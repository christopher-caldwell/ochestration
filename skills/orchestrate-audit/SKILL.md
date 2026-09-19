---
name: orchestrate-audit
description: Run an interactive Orchestrate Audit of the exact registered implementation against the adopted Agreement.
disable-model-invocation: true
---

# Run Audit

Use this skill only when explicitly invoked. It owns the normal lifecycle of one Audit pass in this provider session.

## 1. Resolve the CLI and the inputs

- Resolve the installed executable with `command -v orchestrate`, falling back to `$CARGO_HOME/bin/orchestrate` or `~/.cargo/bin/orchestrate`. If it cannot be found, stop and tell the user.
- Take the effort ID from the user. Ask if it is missing; never guess it.
- If the user supplies a store root, include `--root "<root>"` on every command below. Otherwise omit `--root` so the CLI uses `~/.orchestration`.
- Pass `--host`, `--provider`, `--model`, and `--model-effort` only when the user supplies them.

## 2. Load the controlling instructions

Run `orchestrate guide audit` (with `--root` when the user supplied one). That guide is authoritative for phase behavior; follow it throughout this session.

## 3. Locate the exact authority chain

```sh
orchestrate status --effort "<effort>"
```

Status lists every artifact with its kind, ID, and digest. Find the single chain Agreement → Adoption → Implementation: the Adoption's `adoption.json` names its Agreement, and the Implementation's `implementation.json` names its Adoption and Agreement. Each bundle directory is:

```sh
ls "<root>"/projects/*/efforts/"<effort>"/*/"<artifact-id>"/
```

If exactly one complete chain exists, use it. If several valid candidates exist, list them and ask the user which chain to audit. Never guess.

## 4. Evaluate the exact implementation

Read the exact Agreement, the Adoption receipt, the Implementation record, and the retained immutable implementation source at:

```sh
ls "<root>"/snapshots/"<target-commit>"/source/
```

Inspect code and run relevant tests with the available host tools, using that source rather than the live working tree. Do not edit source or authority, and do not introduce architectural preferences as contract failures.

Return exactly one coverage row for every Agreement requirement: `pass`, `fail`, `unknown`, or justified `not_applicable`, each with bounded evidence; failures also need a correction. Rust, not you, derives the verdict.

## 5. Write the assessment

Write `assessment.json` to an absolute path the user can see, outside any published artifact directory. Copy the three artifact references (`kind`, `artifact_id`, `digest`) verbatim from the `status` output, and include `coverage` and `assessor_context`:

```json
{
  "agreement": { "kind": "agreement", "artifact_id": "...", "digest": "..." },
  "adoption": { "kind": "adoption", "artifact_id": "...", "digest": "..." },
  "implementation": { "kind": "implementation", "artifact_id": "...", "digest": "..." },
  "coverage": [],
  "assessor_context": "what you inspected and how"
}
```

## 6. Finalize and report

```sh
orchestrate audit finalize --effort "<effort>" --bundle "<absolute path to assessment.json>"
```

Report the mechanically derived verdict from `details.verdict`: `PASS`, `CHANGES_REQUIRED`, or `BLOCKED`, together with the Audit artifact ID. If an unknown or missing row forces `BLOCKED`, say which requirement could not be assessed and what access or context would resolve it.

Stop after the Audit result. Do not start another phase.
