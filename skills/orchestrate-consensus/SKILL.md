---
name: orchestrate-consensus
description: Run an interactive Orchestrate Consensus pass over the finalized Discovery results and publish the Agreement candidate.
disable-model-invocation: true
---

# Run Consensus

Use this skill only when explicitly invoked. It owns the normal lifecycle of one Consensus pass in this provider session.

## 1. Resolve the CLI and the inputs

- Resolve the installed executable with `command -v orchestrate`, falling back to `$CARGO_HOME/bin/orchestrate` or `~/.cargo/bin/orchestrate`. If it cannot be found, stop and tell the user.
- Take the effort ID from the user. Ask if it is missing; never guess it.
- If the user supplies a store root, include `--root "<root>"` on every command below. Otherwise omit `--root` so the CLI uses `~/.orchestration`.
- Pass `--host`, `--provider`, `--model`, and `--model-effort` only when the user supplies them.

## 2. Load the controlling instructions

Run `orchestrate guide consensus` (with `--root` when the user supplied one). That guide is authoritative for phase behavior; follow it throughout this session.

## 3. Resolve the exact inputs before any reconciliation

```sh
orchestrate consensus inputs --effort "<effort>"
```

Rust resolves the exact eligible finalized Discovery artifact for slots `a`, `b`, and `c` and prints each one with its ID and digest. Those three artifacts are the only inputs for this Consensus session: record the three IDs and use them unchanged from here on.

Do the resolution first. Never write `proposal.json`, and never begin reconciling, before the three exact artifacts are known.

If Rust reports a missing slot, or several candidates for one slot, stop and show the error with its candidate artifact IDs to the user. Ask which artifact that slot should use, then confirm the complete set with the same read-only command, which validates it and freezes it:

```sh
orchestrate consensus inputs --effort "<effort>" \
  --opinion "<a artifact id>" \
  --opinion "<b artifact id>" \
  --opinion "<c artifact id>"
```

Never guess, never silently select one of several candidates, and never reconcile against one set of Discovery artifacts and then finalize against another.

## 4. Read those exact three artifacts

Read only the frozen request and context plus the three resolved public Discovery specifications. Do not inspect the source repository, sibling run workspaces, or private chat history. Do not re-investigate.

To read a finalized artifact's files, locate its bundle directory under the store:

```sh
ls "<root>"/projects/*/efforts/"<effort>"/*/"<artifact-id>"/
```

Artifact IDs are unique within an effort, so this resolves exactly one directory. Read published bundles; never modify them.

## 5. Reconcile the Discovery results

Normalize equivalent conclusions, distinguish silence from contradiction, preserve material conditions, and retain meaningful dissent. Every mandatory consensus-derived requirement needs at least two supporting slots, and the whole mandatory package must share the same strict majority. Never synthesize rotating majorities.

## 6. Write the proposal

Write a `proposal.json` to an absolute path the user can see, outside any published artifact directory, containing:

- `requirements` — each with `requirement` (`id`, `text`, `acceptance`, `condition`, `governing: false`), `supporters` (slot names), and optional `source_refs` keyed by slot;
- `comparison_md` — the readable reconciliation;
- `selection_rationale` — why this package represents the shared result;
- `dissent` — meaningful remaining disagreement;
- `counterexample_blocks` — whether a concrete counterexample prevents an implementation-ready package.

Do not manufacture supporter sets to force a package through. Do not add unrelated scope and do not re-derive the request.

## 7. Finalize against those exact three artifacts

```sh
orchestrate consensus finalize --effort "<effort>" \
  --opinion "<a artifact id>" \
  --opinion "<b artifact id>" \
  --opinion "<c artifact id>" \
  --bundle "<absolute path to proposal.json>"
```

Always pass the three artifact IDs resolved in step 3. Automatic inference stays available for manual CLI use, but this session must not re-infer its parents after reconciliation: a Discovery artifact that becomes eligible in the meantime must not change what this proposal is finalized against. If finalization rejects the explicit artifacts, show the error to the user instead of substituting different inputs.

## 8. Report and stop

On `NO_CONSENSUS`, report it plainly, explain that no implementation-ready package survived, and stop; do not force a package forward.

On `ELIGIBLE_CANDIDATE`, report the Agreement artifact ID from `details.agreement`, show the Agreement (`agreement.md`) to the user so they can review exactly what would be implemented, and print the exact adopt command with actual values:

```sh
orchestrate agreement adopt --effort "<effort>" --authorization-label "<authorization label>"
```

Use the user's authorization label when they gave one, otherwise print a clearly marked placeholder. If more than one eligible Agreement exists, or the user chose a specific Agreement, print the explicit form instead:

```sh
orchestrate agreement adopt --effort "<effort>" --agreement "<agreement-id>" --authorization-label "<authorization label>"
```

Do not adopt the Agreement. Adoption is a separate, explicit human authority boundary. Stop at the Consensus boundary.
