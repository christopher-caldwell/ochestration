---
name: reconcile
description: Reconcile finalized Orchestrate Discovery output directories into one Agreement candidate and report the finalized document path.
disable-model-invocation: true
---

# Reconcile Discovery outputs

Use this skill when the user provides the output directories from multiple Discovery sessions and
asks for the reconciled result. The paths may be separated by spaces or commas. The user should not
have to type Orchestrate CLI commands; you run them.

## 1. Resolve the CLI

Resolve `orchestrate` with `command -v orchestrate`, then fall back to `$CARGO_HOME/bin/orchestrate`
or `~/.cargo/bin/orchestrate`. If it is not found, stop and tell the user.

## 2. Read the supplied Discovery directories

For every path supplied (splitting on commas and whitespace):

- Confirm it is a finalized Discovery artifact directory containing `manifest.json`,
  `discovery.json`, and `technical-spec.md`.
- Read `manifest.json`. Record `artifact_id`, `effort_id`, `kind`, and `outcome`.
- Read `discovery.json`. Record `slot`.
- Require `kind == "discovery"` and `outcome == "IMPLEMENTATION_READY"`. If a path is missing,
  blocked, or not a Discovery artifact, stop and tell the user which path needs attention.

All paths must belong to the same `effort_id` and cover every cohort slot exactly once. If they do
not, stop and show the user the mismatch.

The store root is the part of each output directory before the `/projects/` segment. If the paths
are ambiguous, ask the user for the store root.

## 3. Confirm the input set with the CLI

Use one `--opinion` selector per slot:

```sh
orchestrate --root "<store root>" consensus inputs \
  --effort "<effort_id>" \
  --opinion "<artifact id 1>" \
  --opinion "<artifact id 2>" \
  --opinion "<artifact id 3>"
```

Add or remove `--opinion` lines to match the exact cohort. If Rust reports a problem, show it to
the user before continuing.

## 4. Load the controlling instructions

```sh
orchestrate guide consensus
```

Follow that guide for the rest of this phase.

## 5. Read only the public results

Read the frozen request/context and each selected Discovery `technical-spec.md` plus its evidence
graph. Do not re-investigate the repository, read private chats, or inspect sibling workspaces.

## 6. Write the proposal

Write `proposal.json` to an absolute path outside the target repository and outside any published
artifact directory. Follow the `orchestrate guide consensus` schema and quorum rules. Do not
manufacture supporter sets.

## 7. Finalize Consensus

```sh
orchestrate --root "<store root>" consensus finalize \
  --effort "<effort_id>" \
  --opinion "<artifact id 1>" \
  --opinion "<artifact id 2>" \
  --opinion "<artifact id 3>" \
  --bundle "<absolute path to proposal.json>"
```

Pass exactly the artifact IDs resolved in step 2. Add one `--opinion` per cohort slot.

## 8. Report the finalized document

- On `NO_CONSENSUS`, report it plainly, explain why no implementation-ready package survived, and
  stop.
- On `ELIGIBLE_CANDIDATE`, capture `details.agreement.artifact_id`. Locate and show the final
  document:

```sh
ls -d "<store root>"/projects/*/efforts/"<effort_id>"/agreement/"<agreement id>"/agreement.md
```

Show the Agreement to the user and report its path. Do not adopt the Agreement.
