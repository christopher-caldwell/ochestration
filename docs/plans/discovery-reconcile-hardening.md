# Planned specification: Discovery and Reconcile hardening

Status: planned; implementation and model evaluation have not started.

Prepared against commit `1ffc97370b9de4652ebc31df0c289f3273c5d691` on September 22, 2026.

## Objective

Make four existing workflow obligations reliable at the publication boundary:

1. Discovery uses the exact frozen request, constraints, and source baseline.
2. A finalized Discovery contains a public specification beyond the generated stub.
3. Reconcile preserves the verification classifications recorded by its cited Findings.
4. Rendered reconciliation citations retain available evidence-node identifiers.

These changes preserve the intended workflow:

```text
Prepare → independent Discovery × N → Reconcile → STOP
Later, explicitly: Build
```

Models retain responsibility for investigation, interpretation, evidence quality, and selecting the best-supported direction. Rust validates input identity, minimum deliverable presence, evidence metadata, and traceability. Successful validation will not certify engineering truth or model compliance with every instruction.

This document is an implementation specification. It is not a prepared Discovery request, a Reconciled Discovery, or authorization to launch the unattended Build driver. Do not fabricate those artifacts to execute this plan.

## Evidence motivating the scope

Reviews and disposable CLI probes against `125ebbd` and `1ffc973` established that:

- Changing `context.json.request` or `context.json.constraints`, while retaining identity fields, does not prevent ready publication.
- Modifying tracked source or committing a changed source HEAD after preparation does not prevent ready publication.
- The unchanged generated `technical-spec.md` heading passes alongside a valid graph.
- A synthesis entry citing an inspection Finding can publish with `verification_methods: ["experiment"]`.
- Evidence-synthesis and rejected-alternative citations retain node IDs in JSON but omit them in Markdown.

The user's recovered project history confirms that these behaviors conflict with the intended frozen-input, public-deliverable, and evidence-fidelity contracts. It also distinguishes them from unresolved policy choices below.

## Scope boundaries

Preserve the following behavior:

- Independent investigations use the same reviewed prepared request and frozen baseline. Discovery-time answers remain local to that run.
- Discovery can inspect Git history and conduct experiments in scratch. Reconcile uses only selected public Discovery outputs as engineering evidence.
- The operator chooses the model windows and exact selected input set. At least two eligible inputs are allowed; no fixed provider panel, voting, quorum, or effort-wide completion barrier is added.
- Finalized BLOCKED artifacts remain immutable and excluded under the current policy. Contextual inclusion is still an open product question.
- Skills remain dispatchers to CLI-owned guides. Workflow instructions must not be added to the skill files.
- Reconcile chooses one direction and stops. Binding requirements, advisory suggestions, and direct user authority remain distinct.

Do not include new policies for compromised-provenance admission, authenticated user-answer receipts, exhaustive per-source dispositions, closure of all optional questions, mandatory graph shapes, accepted-only citations, or filesystem sandboxing. Their absence does not imply the user rejected them; they are outside this corrective scope.

The later model-quality analysis, including use of the user's three older completed Discovery outputs, is deferred. No discovery reruns, reconciliation model calls, store migration, or modification of those artifacts belongs in this implementation.

## HF-1: Validate the complete frozen workspace inputs

### Required behavior

Both `discovery validate` and `discovery finalize` must perform the same checks against the authoritative effort. Finalization must run them itself; a previous successful validate call is not a publication receipt.

For `context.json`, deserialize and compare all model-facing frozen values written by preparation:

- `context_id` and `request_kind`;
- `request`, including its exact string content;
- `constraints`, preserving the exact ordered list and string values;
- `baseline_commit` and `baseline_tree`.

Missing required fields must fail. Reformatting JSON or changing object-key order must not fail when the decoded values are unchanged. Continue the existing byte-for-byte comparison of `request.md`. Do not normalize away changed request whitespace, rewrite constraints, or replace workspace content from the effort.

For the run's `source/` checkout, verify at validation/publication time:

- It exists and is the actual Git worktree root being checked. Git must not silently discover a different enclosing repository.
- Its current HEAD equals the frozen baseline commit.
- Its HEAD tree equals the frozen baseline tree.
- Its index and tracked working-tree content are unchanged.
- It contains no extra untracked or ignored working files/directories outside Git's own administrative metadata. Writable experiment and generated files belong in the sibling `scratch/` directory or another permitted temporary location.

Use Git to inspect checkout identity and state, with explicit arguments for complete untracked/ignored reporting. Do not treat changes to Git's administrative bookkeeping as source changes. Preserve access to history; this fix must not replace the checkout with a history-free archive.

Apply these checks to every new Discovery finalization, including BLOCKED outputs: blocked status does not authorize a different frozen question or source baseline. Existing published bundles are covered by the compatibility rules below rather than mutable-workspace revalidation.

### Failure and recovery

Report which input differs and identify the run/file or checkout. For Git drift, include expected and actual identity or concise dirty-state information. Avoid dumping the full request or constraints into an error unnecessarily.

Fail before creating a published artifact. A diagnostic journal event may still be written. Do not reset, clean, stash, recommit, repair, or delete anything automatically. The investigator must determine whether the run remains trustworthy; moving files back cannot retroactively prove which inputs were used.

End-of-run checks do not detect transient edits restored before validation or source access outside the workspace. Do not present them as a sandbox or a complete interaction audit.

### Acceptance checks

- Unmodified prepared workspace plus a valid result publishes successfully.
- Independent edits to the context request and constraints each fail, even with original identity fields.
- Missing request/constraints fields fail; equivalent JSON formatting succeeds.
- Changed `request.md` still fails.
- Staged changes, unstaged changes, an added untracked file, and an ignored generated file each fail.
- A clean checkout at a different commit fails, including a different commit with the same tree.
- Missing source or a source path that resolves to a different Git worktree root fails.
- Files created only in `scratch/` do not fail the source check.
- Validate can succeed, then a subsequent source/context mutation causes finalize to fail.
- Failed finalization creates no immutable Discovery bundle and leaves the offending workspace untouched.
- A valid BLOCKED result can still publish; drifted inputs cannot bypass checks by selecting BLOCKED.

### Expected implementation locations

- `crates/discovery/src/lib.rs`: expand `WorkspaceContext`; integrate validation before returning `ValidatedDiscovery`.
- `crates/core/src/lib.rs`: add/reuse a focused Git checkout verification helper if needed; preparation and publication should use consistent identity definitions.
- `crates/cli/tests/e2e.rs`: isolated Git fixtures and publication-boundary regression cases.

## HF-2: Reject an absent public specification

### Required behavior

Retain UTF-8 and nonempty-text validation. Also reject the untouched generated template, including equivalent LF/CRLF line endings and surrounding blank whitespace.

Require content beyond Markdown ATX heading lines and blank lines. A document containing only `# Technical specification`, another ATX title, or several ATX headings is still an absent body. Permit a concise body, a list-based result, or a headingless prose specification. Do not introduce a word count, prescribed section sequence, mandatory implementation requirement, or model-like quality score.

The guide's substantive obligations remain in force: the public result must stand alone, explain its recommendation or no-change result, and preserve relevant evidence, scope, limitations, and verification. The mechanical minimum detects an obviously absent deliverable; it does not certify completeness, reasoning quality, or factual support. It need not attempt to recognize every possible Markdown placeholder or formatting trick.

Apply the minimum to ready and blocked publications. A blocked specification must at least explain the unresolved issue rather than remain a generated heading.

### Acceptance checks

- Empty/whitespace-only input fails as before.
- The exact generated template, its CRLF equivalent, surrounding-whitespace variants, and other ATX-heading-only documents fail.
- A valid Finding/Requirement graph cannot compensate for a missing specification body.
- A short specification explaining an evidence-backed no-change result succeeds with an otherwise valid no-change graph.
- Headingless prose and a normal specification with a body succeed.
- A substantive blocked result succeeds under the existing graph policy.
- No new mandatory Finding/Decision/Requirement count or optional-question closure rule is imposed.

### Expected implementation locations

- `crates/discovery/src/lib.rs`: `validate_technical_spec` and focused unit tests.
- `crates/core/src/lib.rs`: existing `TECHNICAL_SPEC_TEMPLATE` remains the preparation source of truth.
- `crates/cli/tests/e2e.rs`: demonstrate rejection through actual finalization with an otherwise valid graph.

## HF-3: Bind verification labels to precisely cited Findings

### Required behavior

For each proposed `EvidenceSynthesis` entry, resolve its references against the exact selected, integrity-checked public Discovery bundles. Validate reference membership and node existence as today.

Use the following deliberately bounded rule for `verification_methods`:

1. Collect the recorded verification classification of each directly cited Finding node in that entry's `source_refs`.
2. Compare the distinct set of supplied methods with the distinct set of those recorded classifications. Order and repetition do not change the meaning of a method set.
3. Reject a mismatch before publication, identifying the synthesis entry and the expected/declared methods. Do not silently repair an authored proposal.

A direct reference is a selected artifact ID plus the ID of a Finding in that artifact. Do not infer a method from an unrelated Finding elsewhere in the artifact, automatically traverse a cited Decision/Requirement's dependencies, inspect source code, or infer a method from prose.

Artifact-level citations and citations to Questions, Decisions, Requirements, or rejected Findings remain allowed. They can establish context, authority, or a discussed alternative. They do not themselves grant an unrecorded verification classification:

- An entry with no direct Finding citations may use an empty method list; render that as “Not attributed to specific Finding nodes,” not as an experimental or other verification claim.
- An entry claiming a method must directly cite the Finding(s) to which the classification applies. Broader citations may coexist.
- Include directly cited rejected/invalidated Findings' recorded methods in the comparison. Their status and the quality of their evidence remain visible source facts for the model to discuss; they do not trigger an accepted-only admission rule.

This is method preservation, not a ranking or endorsement of evidence. The model remains responsible for explaining whether a cited experiment is representative, whether a rejected claim should be reconsidered, and what the evidence establishes. An omitted method from a directly cited Finding is a mismatch just as an added method is.

Update the embedded Reconcile guide to state this attribution rule and the valid empty-list case. Do not change the optional nature of `DiscoverySourceRef.node_id` generally or require node IDs for all requirements, suggestions, or alternatives.

### Acceptance checks

- An inspection-only citation labeled experiment fails; the same entry labeled inspection succeeds.
- An experiment Finding relabeled inspection or corroborated also fails.
- Cited inspection and experiment Findings require both methods; missing one or adding an unsupported method fails.
- Repeated citations/method labels and differing method order do not cause a semantic mismatch.
- A real experiment elsewhere in the same artifact does not justify labeling a cited inspection Finding as experiment.
- Two selected artifacts with the same local node ID resolve independently by `(artifact_id, node_id)`; their classifications are not conflated.
- Artifact-only and non-Finding-only references with an empty method list remain valid; a nonempty method list without direct Finding attribution fails clearly.
- Mixed broad references and direct Finding references are valid when methods match the directly cited Findings.
- A rejected Finding can still be discussed and attributed with its actual recorded method.
- Existing missing-node, unselected-artifact, cross-effort, duplicate-input, and minority-source behaviors remain intact.
- Invalid proposals create no published Reconciled Discovery; they are not silently relabeled.

### Expected implementation locations

- `crates/reconcile/src/lib.rs`: source resolution and method validation using selected public graph data. Reuse loaded bundle data where practical.
- `crates/contracts/src/lib.rs`: retain the existing field shapes; structural validation remains distinct from store-dependent attribution validation.
- `crates/guides/resources/guides/reconcile.md`: concise attribution guidance.
- `crates/cli/tests/e2e.rs` and focused reconciliation tests: mismatches, valid combinations, and reference edge cases.

## HF-4: Render complete source references consistently

### Required behavior

Use a shared rendering helper for source citations in binding requirements, evidence synthesis, rejected alternatives, and technical suggestions.

Preserve the stable human source label and artifact ID. Append the node ID exactly when present, using the existing requirement/suggestion format, for example:

```text
codex_example (`discovery-example`) / F-1
```

For an artifact-level citation with no node ID, render only the source label and artifact ID. Do not invent a node ID, emit a dangling separator, or duplicate an ID already rendered by the shared helper.

Retain deterministic Markdown generation from the authoritative JSON. Do not change stored source references, selected parents, or digest calculations to fix presentation.

### Acceptance checks

- Each of the four citation-bearing sections displays the precise node ID when present.
- Artifact-only references render cleanly in all four sections.
- Multiple nodes from one artifact remain distinguishable, as do the same local node IDs from different artifacts.
- Requirements and suggestions do not gain duplicated node suffixes during helper consolidation.
- Rendering the same contract produces the same Markdown; reference JSON is unchanged.

### Expected implementation locations

- `crates/reconcile/src/lib.rs`: `source_name` or a dedicated reference formatter and its call sites.
- Focused renderer tests plus one CLI-produced Markdown assertion in `crates/cli/tests/e2e.rs`.

## Compatibility and existing completed Discovery outputs

At the planning baseline, the artifact and store format versions are both 6. No field-shape change, schema bump, migration, automatic import, or CLI-interface change is planned for these four fixes. New publication will become stricter without rewriting existing immutable artifacts.

Keep loading supported existing published bundles through the current integrity and lineage checks. Do not require their old mutable workspace or source checkout to still exist, and do not re-finalize them merely to apply new checks. New method validation applies to newly submitted reconciliation proposals; existing immutable reconciled bundles are not silently repaired or rerendered in place.

Compatibility with an older producer is not proof that its Discovery passed the new checks. Preserve its original provenance. Version strings alone are insufficient to establish what checks ran; retain guide digests and exact available metadata, and mark missing historical evidence as unknown.

The user has three completed Discovery outputs from an earlier tool version. Their paths, schema versions, and common lineage have not yet been inspected. They may be useful for later analysis; this plan makes no claim that they can be directly reconciled by the current CLI.

When the user returns for that analysis:

1. Inventory the supplied public bundles read-only: manifests, schema/store versions where available, hashes, request/context/baseline identities, outcomes, producer/guide metadata, specifications, and graphs.
2. Separate technical loadability, comparability of the frozen question/baseline, and suitability as a synthesis evaluation case. Passing one does not establish the others.
3. If they are supported, comparable, and eligible, they may be selected unchanged. Record that their production predates the fixes and cannot retroactively prove new validation happened.
4. If the stored format is unsupported or the material is only exported text, retain the originals. A separately labeled analysis packet may support qualitative evaluation, but is not an authenticated current-format artifact. Any future migration needs its own reviewed scope; do not patch version numbers or fabricate missing provenance.
5. Do not retrieve mutable source or outside engineering evidence to fill gaps for the reconciler. Missing evidence is an observed limitation of the supplied outputs.

Add compatibility regression coverage using representative already-published version-6 bundles. Demonstrate that loading does not consult removed mutable workspaces, and that a new correctly attributed reconciliation can use supported historical Discovery bundles. Preserve rejection of unsupported formats rather than introducing a bypass.

## Implementation sequence

1. Implement HF-1 with Git/context regression cases and non-mutation assertions.
2. Implement HF-2 with focused text validation tests and ready/blocked/no-change CLI coverage.
3. Implement HF-3, its guide clarification, and method-attribution regression cases.
4. Implement HF-4 and renderer/compatibility coverage.
5. Run the workspace suite and review the diff against the scope boundaries. Report behavioral changes, compatibility limits, and any new failures before claiming completion.

The changes can be reviewed in that order without requiring a new Discovery cohort, changing blocked-input policy, or building a model-evaluation framework.

## Validation and definition of done

Use disposable Git repositories and isolated stores. Never exercise negative probes against the user's historical Discovery artifacts or real working repositories.

During implementation, run focused package/integration tests for each changed boundary. On completion run `cargo test --workspace`; apply the repository's normal formatting check to changed Rust files. No live-model test, paid API call, deployment, or unattended Build invocation is required for this corrective work.

Completion requires:

- All four behaviors and their acceptance checks are implemented.
- The previously accepted drift, generated-stub, and method-mismatch reproductions now fail before publication.
- Valid ready, blocked, no-change, mixed-evidence, and supported historical-input cases still work within existing policy.
- Rendered citations expose the node identities present in JSON.
- Failure leaves source, context, published artifacts, and historical stores unchanged, apart from permitted diagnostic journal events and disposable test fixtures.
- Runtime guidance stays in embedded CLI guides; dispatchers remain unchanged.
- Tests pass, and the delivery summary explicitly states that model-quality evaluation is still pending.

Stop after these targeted fixes and their verification. The later analysis of the user's three completed Discovery outputs is a separate follow-up.
