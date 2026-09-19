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

## 3. Inspect the effort state

```sh
orchestrate status --effort "<effort>"
```

The status output lists every artifact with its kind, ID, and digest. Rust selects the eligible finalized Discovery artifacts for slots `a`, `b`, and `c`; do not select them yourself unless the user must disambiguate.

To read a finalized artifact's files, locate its bundle directory under the store:

```sh
ls "<root>"/projects/*/efforts/"<effort>"/*/"<artifact-id>"/
```

Artifact IDs are unique within an effort, so this resolves exactly one directory. Read published bundles; never modify them.

## 4. Reconcile the Discovery results

Consensus answers what the three finalized Discovery opinions genuinely agree on. Read only the frozen request and context plus the three selected public Discovery specifications. Do not inspect the source repository, sibling run workspaces, or private chat history. Do not re-investigate.

Normalize equivalent conclusions, distinguish silence from contradiction, preserve material conditions, and retain meaningful dissent. Every mandatory consensus-derived requirement needs at least two supporting slots, and the whole mandatory package must share the same strict majority. Never synthesize rotating majorities.

## 5. Write the proposal

Write a `proposal.json` to an absolute path the user can see, outside any published artifact directory, containing:

- `requirements` — each with `requirement` (`id`, `text`, `acceptance`, `condition`, `governing: false`), `supporters` (slot names), and optional `source_refs` keyed by slot;
- `comparison_md` — the readable reconciliation;
- `selection_rationale` — why this package represents the shared result;
- `dissent` — meaningful remaining disagreement;
- `counterexample_blocks` — whether a concrete counterexample prevents an implementation-ready package.

Do not manufacture supporter sets to force a package through. Do not add unrelated scope and do not re-derive the request.

## 6. Finalize

```sh
orchestrate consensus finalize --effort "<effort>" --bundle "<absolute path to proposal.json>"
```

Omit `--opinion` so Rust infers the single eligible finalized Discovery artifact per slot. If Rust reports a missing or ambiguous candidate, show the error and its candidate artifact IDs to the user, ask which artifacts to use, and re-run with all three explicit `--opinion "<artifact-id>"` selectors.

## 7. Report and stop

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
