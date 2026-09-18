---
artifact: technical-guidance
product: Orchestrate
specification_id: orchestrate-product-v0.1
status: advisory-subordinate-to-product-specification
date: "2026-09-17"
---

# Orchestrate — Technical guidance

This document suggests how to enforce the product contract in `ORCHESTRATE-SPEC.md`. It does not add an autonomous workflow, select a Build engine, prescribe a universal sandbox, or require translating Python internals. Where a mechanism does not help the north star, omit it.

# 1. Build around four application boundaries

Use one Cargo workspace and one distributed executable. Cargo workspaces coordinate related packages and shared build settings; they do not make workspace members automatically depend on one another. Explicit dependencies and private surfaces are useful for this product’s module boundaries. [T1]

A practical starting organization is a CLI/composition package, narrow public artifact contracts, the Discovery/Consensus/Audit implementations, and reusable storage/source/execution adapters. An implementation contract package for Build may be useful, but an empty build engine or provider-per-possible-host framework is not.

Choose crates at boundaries that earn separate compilation/visibility, not one crate for every folder, operation, architectural layer, or noun. Producer-owned public contracts can be separate small crates or a narrowly governed contract package. They contain wire types, identity, and structural validation—not another phase’s operational state or its semantic policy.

The allowed dependency shape is:

```text
CLI/composition ──► phase entrypoints
phase entrypoints ──► their own application/domain logic
phase entrypoints ──► narrow public artifact contracts
phase entrypoints ──► needed low-level ports/adapters

Consensus ─X─► Discovery private runtime/database
Audit     ─X─► Consensus private runtime/database
Audit     ─X─► Build private runtime/database
```

Tests should inspect declared dependency edges, aliases, features, and re-exports as well as compile-fail fixtures. Rust rejects an inaccessible item, but someone can still edit Cargo.toml or make an API public. Runtime source visibility needs its own enforcement and tests.

# 2. Keep working state and published authority separate

A phase may use SQLite, append-only records, or ordinary files for active state. Do not choose the database by inheriting a prototype. The important distinction is **mutable private working state** versus **committed public artifact**.

Each operation should have an identity, intent, input binding, and known outcome. A finalizer assembles required payloads in staging, validates them, computes their declared byte identities, commits the public manifest/finalization boundary, and then acknowledges the caller. A consumer reads only a complete validated publication.

Exactly how files, flushes, atomic replacement, or a local transaction establish this boundary is part of the technical contract. Test every chosen cut point. A rename-based publication alone is not a basis for broad power-loss guarantees. The initial product promises honest process-crash recovery, with stronger durability claims only after corresponding qualification.

The global index, when useful, is a cache of immutable records and discoverable active runs. It may speed lookup, but a phase never asks the index which different artifact it should quietly substitute for a missing parent. Deleting the index must not delete knowledge or authority.

Source ownership markers and finalization records are different concepts. A scratch ownership marker permits safe cleanup of that resource; it does not finalize the results found inside it.

# 3. Define the public schemas before writing phase consumers

At minimum define these new-version shapes:

- Request/context revision and three-slot cohort, including baseline and constraint references.
- Final Discovery opinion with full readable content and compact adopted records.
- Consensus comparison, source-position matrix, selected package, and readiness findings.
- Agreement candidate and separate adoption record.
- External implementation declaration/material identity and observation receipts.
- Audit coverage/finding/result artifact and separate invalidation notices.

These are contract families, not a demand for six independently versioned packages or exactly one file each. A small common envelope should contain only mechanical provenance. Keep phase semantics in phase-specific payloads.

Use exact original payload bytes for file digests. Separately specify a deterministic encoding for generated records if canonical hashes are needed. Use published/fixed vectors and independent calculations in tests; serializing and deserializing with the same broken helper can hide a common-mode error.

Every artifact reference includes type, stable ID, version, and digest. Local paths are locators. Do not hash an object including its own hash. Consider a detached manifest digest or an explicitly excluded integrity field, documented precisely.

Required references should survive moving the retained bundle tree. External-only references must say that they are external and may become unavailable. A minimal downstream input projection should be assembled from explicit contracts—not by recursively scanning everything a producer retained.

# 4. Materialize source from committed objects

Resolve the local HEAD once for the cohort. Prepare each investigator’s exact source from committed material, never by copying the dirty working directory and labeling it with that SHA. Verify delivered content and supported file modes against its manifest.

Prefer a snapshot arrangement that does not modify the source repository’s administration or expose unrelated refs/history. Git’s linked-worktree documentation explicitly describes shared repository state and configuration; a worktree at another path is not by itself the isolation or zero-source-write property required here. [T2]

If the chosen preparation mechanism transforms files, omits required material, exposes other branches, follows outside symlinks, or runs repository hooks/filters, the corresponding guarantee must be addressed explicitly. Do not assume that a familiar Git operation satisfies this product’s input contract automatically.

Start with a deliberately bounded source-support profile. Handle ordinary committed files, executable modes, and safe internal links faithfully; fail clearly for unsupported required submodule/LFS or external layouts. Better a precise limitation than an incomplete snapshot called complete. Non-Git and unborn-repository workflows are outside the first committed-baseline profile.

# 5. Use two distinct isolation concepts

**Supplied-context exclusion** is required: no sibling output is copied, embedded, retrieved through Orchestrate’s own tools, or made part of the provided source environment.

**Enforced access restriction** is stronger: the actual host/model tools cannot read a known sibling path or obtain its contents indirectly. That needs a qualified boundary over every relevant tool, not just directory organization.

For an ordinary host with broad user-account filesystem access, record cooperative isolation and require a fresh context. Do not create an OS security platform merely to claim universality. But do not label that ordinary mode access-enforced. A user requiring enforced isolation must select a supported qualified host/runner boundary, or the operation must report the missing capability.

A restricted managed role can receive only embedded frozen source text and no open-ended file tools. Discovery often needs richer research access, so its host capability envelope may differ from Consensus. The product does not require one execution mechanism for every phase.

Known contamination creates an immutable notice and disqualifies that opinion as a clean independent slot. Do not erase the run. Automatic prevention, observed compliance, user attestation, and historical uncertainty should be separate fields.

# 6. Make operation scope deterministic without pretending it authenticates a human

Bind every public mutation to a specific run/phase identity. Operations verify that identity, parent type, baseline, and lifecycle state. The active role cannot mutate a different phase through a helper that silently changes context.

A skill with a hard-coded guide command avoids intent classification. It does not prove who typed a shell command or prevent an unrestricted model from launching another program. Adoption receipts record the source of authorization; this personal tool does not need enterprise identity infrastructure to claim otherwise.

Keep authorization coarse and useful: one deliberate phase request can authorize its ordinary research/recording work and declared bounded provider calls. A new phase, risky external operation, contract change, or exhausted limit needs an explicit user action. Do not ask for approval of every record insertion.

# 7. Keep operator guides separate from role prompts

The Discovery operator guide may contain substantial investigator instructions because that host is doing the research. Consensus/Audit operator guides mainly prepare the request, invoke the selected phase, inspect its result, and present it. The isolated reconciler/assessor receives its own purpose-built semantic prompt.

Each role should receive only the instructions it needs. Avoid a giant shared guide containing contradictory “investigate freely,” “never investigate,” “implement,” and “remain read-only” directives. A request-shaped injection inside supplied evidence is data, not authority to switch roles.

Embed or bundle guides with the Rust release. Record the exact versions/digests a run used. Do not claim a new guide flushes an old host conversation; fresh independent contexts are still necessary. On incompatible resume, stop without data loss instead of silently changing semantics.

# 8. Package one canonical skill collection with tiny host metadata

The initial installed collection contains Discovery, Consensus, and Audit. All wrappers load the matching `orchestrate guide <phase>` and stop at the phase boundary. Do not advertise Build execution before it exists.

Current official host documentation supports explicit-only invocation, with different metadata locations:

**Codex:** `agents/openai.yaml` supports `policy.allow_implicit_invocation: false`; explicit invocation remains available. [T3]

```yaml
policy:
  allow_implicit_invocation: false
```

**Claude Code:** skill frontmatter supports `disable-model-invocation: true`. [T4]

**Cursor:** skill frontmatter supports `disable-model-invocation: true`. [T5]

```yaml
disable-model-invocation: true
```

These settings concern skill activation, not filesystem confinement or authentication. Keep the semantic body canonical and add only genuine host metadata. Verify exact install locations and host behavior against the versions actually tested; do not rely on remembered home-directory conventions or assume nested directory discovery is identical everywhere.

An installer should own only its executable/skill resources, detect name collisions and modified files, and leave other host configuration alone. One local install command need not imply installing into every host automatically.

# 9. Give the provider adapter a narrow job

The adapter launches the selected model execution, applies available tool/capability restrictions, streams and parses output, records observed identity/usage, and returns a typed success/failure/unknown result. It does not decide consensus, rewrite the Agreement, or infer correctness from a process exiting zero.

A first managed adapter is enough. Codex is a reference implementation candidate, not an excuse to bake its protocol into every phase. Hosting Discovery in Claude or Cursor does not automatically require a managed Claude/Cursor adapter.

Prefer explicit argument arrays, bounded captured output, monotonic deadlines, and owned process handles. Keep provider credential access separate from model-visible packets. Validate response attribution and final payloads before publication. Late responses to canceled attempts are diagnostics, not current results.

Avoid promising hard token caps unless the provider supports an actual enforcement point. Admission control stops future calls, not necessarily an already-running call. Preserve unknown usage rather than recording zero. Cost reporting is optional and must not be invented from incomplete metadata.

# 10. Keep checking separate from judgment

A bounded check runner can execute approved commands in a disposable copy and produce receipts. The assessor interprets whether those observations support the relevant requirement. This avoids giving a no-tools model the impossible task of independently observing runtime behavior.

A check receipt should record the exact inspected source/target, command, relevant environment identity, assertion/result evidence, exit status, timing, and bounded logs. Classify infrastructure failure separately from behavioral assertion failure. A script can print PASS while failing; another can exit zero after running no tests. These are reasons to retain observations and evaluate their meaning, not infer correctness from one signal.

Execution permission is scoped to owned disposable resources. Dependency/service preparation belongs to that scope and must be recorded; no production mutation or secret-bearing environment is inherited casually. A check that modifies source exercises a probe variant, not the immutable implementation under Audit.

# 11. Implement consensus and verdict checks as small explicit rules

The model identifies positions and proposes a scoped package. The machine counts original opinions and checks structural coherence of the record. In the initial cohort, require three eligible slots and a common supporting intersection of at least two for consensus-derived mandatory content.

Do not count governing user requirements as invented opinion support. Do not drop them either. They have direct source authority and must be covered. If a required rule has no adequate supported implementation direction, the candidate is not ready.

Render normative Agreement content from validated adopted records. Keep optional discussion out of the builder’s mandatory input. This closes the extra-prose bypass but does not prove the meaning of every paraphrase; live semantic tests remain necessary.

Audit differs from Consensus. It checks requirements against evidence; it does not vote on truth. Derive PASS/CHANGES_REQUIRED/BLOCKED from validated coverage and findings. A single supported defect prevents PASS, but an unsupported suggestion does not create a defect.

# 12. Build the test harness before broad implementation

Start with a tiny synthetic repository and independently checked good/bad targets. The offline harness drives the actual CLI and replaces only the semantic agent boundary. It does not install real user credentials, expose grading keys, or write accepted results behind the production API.

Cargo supports conventional unit/integration tests; use ordinary Rust tooling and execute the real built binary for CLI tests. Exact test-target names are implementation choices, not already available commands. [T6]

State-machine tests should compare public observations with an independent minimal model and retain failing sequences. Actual races need multi-process tests with handshakes/barriers rather than guessed sleeps. Hand-seeded guard mutations can be more useful than a universal mutation-percentage target.

Vary publication failures and process-kill points deliberately. SQLite’s published testing approach is a useful reference for I/O and crash injection, not a requirement to copy its harness or database design. [T7]

For live agents, separate the actual environment outcome from the agent’s claim about what happened. Isolated trials, independent rubrics, and repeated observations are important; Anthropic’s evaluation guidance discusses these distinctions directly. [T8]

# 13. A small implementation sequence

First implement identity, preserved context/baselines, safe public publication, and an inspectable external store. Prove no source pollution, wrong-parent rejection, committed-source fidelity, and crash recovery before adding model-driven complexity.

Next implement one vertical scripted journey through Discovery public operations, Consensus, Agreement adoption, external implementation registration, and Audit. Include the faulty-target and no-change variants so both rejection and successful closure are possible.

Then add the actual managed adapter and installed phase skills. Qualify low-strength host smoke and the intended-profile semantic cases. Refine Discovery’s operator actions to hide bookkeeping without removing evidence obligations.

Only after this chain is demonstrated should an internal Build strategy be selected. Treat that as a separate product decision with its own specification and tests against the already defined Agreement/implementation boundary.

# 14. Mechanism-to-goal check

Before accepting an implementation mechanism, ask: what required outcome becomes unreliable without it, which test proves the problem, and what is the smallest boundary that addresses it?

A useful mechanism makes a false success difficult to publish. An unnecessary mechanism merely adds another status, reviewer, database table, or approval the user must manage. The specification favors the former.

# Verified technical references

Accessed September 17, 2026. These support implementation facts, not claims of Orchestrate completion.

- **T1 — Cargo workspaces:** `https://doc.rust-lang.org/cargo/reference/workspaces.html`
- **T2 — Git linked worktrees and shared state:** `https://git-scm.com/docs/git-worktree`
- **T3 — OpenAI skills and invocation policy:** `https://learn.chatgpt.com/docs/build-skills` (official developer documentation redirect from `https://developers.openai.com/codex/skills/`)
- **T4 — Claude Code skills:** `https://code.claude.com/docs/en/skills`
- **T5 — Cursor Agent Skills:** `https://cursor.com/docs/skills`
- **T6 — Cargo test:** `https://doc.rust-lang.org/cargo/commands/cargo-test.html`
- **T7 — How SQLite is tested:** `https://www.sqlite.org/testing.html`
- **T8 — Anthropic, Demystifying evals for AI agents:** `https://www.anthropic.com/engineering/demystifying-evals-for-ai-agents`
