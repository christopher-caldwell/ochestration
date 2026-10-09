# Investigate

Investigate runs a configured fixed cohort of fresh agents over the same frozen question and pinned inputs. It is opt-in and independent of Build and canonical Audit. Consensus answers whether the configured threshold voted GO, separately from the evidence-based recommendation. Wide retains distinct findings and contradictions without a vote score.

Prepare an explicit version-1 TOML configuration and a request Markdown file. Review the request and selected provider settings with the user. An authorized investigation request with an explicit configuration permits its lane and reconciler calls; merely reading this guide does not launch anything.

```sh
orchestrate --root /absolute/orchestration/root investigate run --config /absolute/config.toml
orchestrate --root /absolute/orchestration/root investigate inspect --run investigation-... --json
```

The default root is ~/.orchestration. Configuration paths are relative to the config file. The root must be outside the target repository. Results stay local under <root>/investigations/<run-id>. Native adapters are codex, claude and cursor; model, effort and args follow existing provider conventions. Omitted native model/effort settings use provider defaults and are not invented in provenance. A reconciler configuration is mandatory. Unknown config fields are rejected.

```toml
schema_version = 1
mode = "consensus"
question_kind = "binary"
request = "./request.md"
target = "../repository"
revision = "FULL_COMMIT_ID"
max_parallel = 5
inputs = ["./supplied-evidence.txt"]

[[lanes]]
adapter = "codex"
count = 3

[[lanes]]
adapter = "claude"
count = 2

[reconciler]
adapter = "codex"

[completion]
min_completed = 5

[consensus]
min_go_votes = 4
```

Counts must be positive. min_completed defaults to all configured lanes. max_parallel defaults to the cohort size. For five lanes, valid majority thresholds are 3, 4 and 5. The denominator always includes unavailable, invalid and failed lanes. No default lane count or vendor-diversity policy is imposed. provider_timeout_seconds is a positive per-provider-call limit, defaulting to 1800 seconds (30 minutes), for each lane and the reconciler. Timed-out calls are failed attempts with retained partial transport, never valid completed lanes. After all attempts finish or expire, eligible evidence may produce a final result when min_completed is met; otherwise the run remains INCOMPLETE/INCONCLUSIVE. There is no retry. Build does not use this investigation limit.

For wide, set mode = "wide" and remove [consensus]. question_kind can be "open_ended" or "binary", regardless of mode; consensus requires binary. Generic requests need request and question_kind. Optional repository inputs require an exact full commit ID, never moving HEAD. Supplied input files are copied and hashed before agents launch. For repositories with submodules, gitlink paths and exact dependency commits are frozen in repository.submodules; their contents are not checked out, fetched or included as inspectable source. This limitation is included in the report and both lane and reconciler inputs.

For a conformance investigation, add [audit] with effort and exact artifact references:

```toml
[audit]
effort = "existing-effort"
[audit.reconciled]
kind = "reconciled_discovery"
artifact_id = "EXACT_ARTIFACT_ID"
digest = "EXACT_MANIFEST_DIGEST"
[audit.adoption]
kind = "adoption"
artifact_id = "EXACT_ARTIFACT_ID"
digest = "EXACT_MANIFEST_DIGEST"
[audit.implementation]
kind = "implementation"
artifact_id = "EXACT_ARTIFACT_ID"
digest = "EXACT_MANIFEST_DIGEST"
```

The target and revision are derived from registered Implementation; optional supplied values must match. An omitted request uses the standard full-contract conformance question with inspection-only authority; supply a request authorizing bounded verification when executable checks are wanted. All binding requirements are covered, including governing: false. Advisory technical suggestions are not mandatory. This publishes an investigation record, never a canonical Audit PASS or a Build gate result.

Lanes use separate frozen source checkouts and scratch directories under the orchestration root, following Discovery. The frozen request defines whether bounded verification is authorized. Inspection-only requests do not permit tests, builds or executable probes. Relevant tests and experiments may run in scratch only when the request authorizes them; absent or unclear authority leaves runtime verification unperformed and explicitly limited. Native permission flags do not expand the frozen request. They may not alter the target repository, frozen source checkout or Git state, commit changes, inspect peer outputs, or access unrelated paths. Tests and generated files belong in scratch. There is no extra OS sandbox or locking system; these are trusted local-agent instructions backed by input/source validation, not a security boundary.

Each lane returns observations, findings, challenges and its own conclusion. Rust rejects invalid graphs and verifies execution claims against captured native tool records. An unsupported test assertion remains testimony. Lane validation.json records the lane_graph stage and graph_valid, before cohort checks. The later cohort-validation.json and result.lanes determine final eligibility, including duplicate-session exclusion; a valid sealed graph can therefore remain available while its lane is ineligible. Offline inspect revalidates both stages. Older lane records use eligible for the lane stage only. Static inspection can establish an obligation when it actually proves it. Missing execution metadata stays visible.

After all lane attempts finish, one fresh reconciler classifies existing evidence. It cannot investigate again, rewrite the request, add claims without provenance, or outvote a material minority objection. Numeric threshold_met and recommendation are separate. A demonstrated material defect prevents GO; an unresolved material objection remains REVIEW_REQUIRED.

Wide includes supported single-lane findings, retains contradictions and records rejected or insufficient claims with their basis. It does not claim to have found every actual defect.

Runs are single invocation, with no automatic retry or resume. A retry creates a new run. Failures and interrupted runs retain partial files; inspect verifies existing manifests offline and reports unfinished runs as INCOMPLETE/INCONCLUSIVE. Inspect never calls a provider or writes storage. Exit 0 means a valid result was produced, including incomplete results or negative recommendations; malformed configuration and integrity/inspection errors use the CLI's error exit 2. Read completion and recommendation fields to determine the domain result.

On Unix, investigate run handles Ctrl+C (SIGINT) and normal process termination (SIGTERM), returning exit 130 or 143 after stopping active provider process groups and their ordinary tool children. Interruption prevents further queued lanes, reconciliation and final publication. Retained completed lane evidence remains useful, but the run stays INCOMPLETE/INCONCLUSIVE. Repeated interrupts do not bypass cleanup. SIGKILL, OS crashes, SIGHUP and descendants escaping their process group are outside this guarantee; Windows termination behavior is unchanged. Build keeps its existing interruption and execution behavior.
