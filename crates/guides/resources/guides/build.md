# Build

## Human-controlled CLI boundary

Invoking `$build`, asking for preparation, or discussing readiness does not authorize the chat to run any Orchestrate CLI command. In particular, `$build` never runs `orchestrate build guide`, `prepare`, `preflight`, `scaffold`, or the Build driver. It never starts or resumes a Build, spends inference on a live preflight, or dispatches provider work. The skill may inspect repository files and explain the next steps without invoking the CLI.

The CLI's `orchestrate build prepare` command prints this canonical guide and changes nothing. A human must explicitly ask for that exact help command before chat runs it. `orchestrate build scaffold`, `preflight`, `resolve`, and the driver are separate operations; chat runs one only when the human explicitly authorizes that command or clearly defines that operation. An explicit launch authorizes the Rust controller to own the complete Work/Review/Unblock/Audit loop; it does not require approval at each internal transition.

An explicit human instruction to launch Build authorizes one exact implementation-ready Reconciled Discovery. It binds that artifact and creates or reuses its exact Adoption receipt.

Build requires the exact effort, the exact implementation-ready Reconciled Discovery, a detailed implementation plan, and the target repository. When the user supplied a Reconciled Discovery explicitly, bind that artifact; do not infer a different one.

Resolve the effort and Build directory:

```sh
orchestrate --root "<root>" status --effort "<effort>"
```

Read the rich Reconciled Discovery artifact as Build's authoritative contract. **Reconciled Discovery is binding WHAT; the detailed implementation plan is HOW and ordering.** The plan may describe approach, task grouping, ordering, and delivery phases only. It must not add or remove product requirements, weaken acceptance criteria, or contradict the Reconciled Discovery. If the plan materially conflicts with binding authority, stop before implementation and surface the conflict. Do not silently rewrite the plan or improvise around it.

The repository may have advanced since Discovery. Discovery remains frozen to its recorded baseline, while implementation must respect the current code and preserve the reconciled contract. Build is allowed when that Discovery baseline is an ancestor of current `HEAD`; it need not equal current `HEAD`.

A new Build requires a clean Git-visible product checkout before the worker takes ownership. Commit, move, or remove staged changes, unstaged changes, and ordinary untracked files before starting. The recorded Build starting commit/tree is the worker's starting boundary. Git-ignored local environment files are allowed. This check applies when Build state is first created, not when an active Build resumes.

## First preparation

Preparation in chat is non-executing. Do not run `build prepare` or any other Orchestrate command just because a user invoked `$build` or asked to prepare. Show the human the command to run or request separate authorization for the specific command. The CLI help path is `orchestrate build prepare`; it prints guidance only.

Copy the detailed implementation plan into the Build directory, then materialize templates:

```sh
orchestrate --root "<root>" build scaffold --effort "<effort>"
```

This writes `plan.json` and `config.toml` and refuses to overwrite existing files. Fill `plan.json` with schema version 2, the exact Reconciled Discovery reference (`kind`, `artifact_id`, and `digest`), the relative detailed-plan path, and the ordered delivery-phase grouping. Each task belongs to one phase; task IDs do not create extra stops.

Configure worker and independent reviewer adapters separately in `config.toml`. Supported adapters are `codex`, `claude`, and `cursor`. Each role takes `adapter`, and optionally provider-neutral `model_strength`, `reasoning_effort`, and `permission`:

```toml
schema_version = 3

[worker]
adapter = "cursor"
model_strength = "standard"       # optional: standard | strong
reasoning_effort = "medium"       # optional: low | medium | high
permission = "workspace_write"    # optional: read_only | workspace_write | full_access

[reviewer]
adapter = "codex"
permission = "read_only"

# Optional. Absent, Unblock turns use the reviewer's settings.
# [unblocker]
# adapter = "codex"
# model_strength = "standard"
# reasoning_effort = "medium"
# permission = "read_only"

# Optional advisory whole-implementation once-over. Absent, it never runs.
# [once_over]
# adapter = "codex"
# model_strength = "standard"
# permission = "read_only"
```

`standard` and `strong` select a documented adapter mapping; `low`, `medium`, and `high` describe requested reasoning effort; `read_only` prohibits workspace edits while allowing safe inspection commands, `workspace_write` allows edits and local commands within the configured workspace, and `full_access` names the one explicit unrestricted mapping per adapter, which no role receives unless an operator writes it. A role whose permission prevents it from writing its evidence files states them in its final response instead, following the `output_contract` guide `action.json` names. These are contracts, not assertions that provider models are equally capable. The complete per-setting translation and provider-specific limits are in the [provider translation reference](../../../../docs/guides/provider-mappings.md). Unsupported combinations fail before dispatch and name the role, field, value, and adapter. Requested generic values and the exact mapped arguments are recorded separately from provider-observed values. An unset attribute uses the provider's configured default and remains unknown. Final Audit always uses the reviewer's settings; an absent `unblocker` falls back to them; an absent `once_over` means zero once-over invocations. Do not put credentials, command strings, or provider homes in this file.

Schema version 2 is supported only as frozen legacy configuration: its provider-native `model`, Codex-only native effort, and `full_access` retain their original meaning. Do not edit it in place. For a stopped v2 Build, write a separate schema-v3 config and pass it to `build resolve --kind environment_repair --config <file>`; this creates an immutable config-history overlay for future invocations. For a new Build, use the v3 scaffold template. A running Build's frozen inputs and prior invocation records remain unchanged.

## Preflight before expensive work

```sh
orchestrate --root "<root>" build preflight --effort "<effort>"
```

Static preflight resolves each role's executable from this launch environment, reports its version, the exact provider-native arguments the role would dispatch with (prompt elided), the configured-versus-default settings, and every capability it cannot establish locally — including whether the provider accepts the configured model or effort and whether a role can commit under its effective sandbox. It runs no provider inference and changes nothing. The driver performs the same bounded, inference-free check of the required worker and reviewer executables itself before any product-changing dispatch, and again for the configuration an applied overlay makes effective, so a missing or unlaunchable required executable stops the Build before a worker can change the product. The optional unblocker and the advisory once-over are reported here but never gate a run.

A live probe is optional and paid: it dispatches real inferences for the configured roles inside disposable owned Git fixtures under the Build directory, asks the worker probe to make and commit a change (so an environment that can edit but not commit fails that probe), and retains its raw output as evidence. A read/command probe passes only on evidence the fixture itself supplies: the role's retained report must carry the fixture's own probe token and the commit id the requested command printed. A process that completes without reading the fixture, a generic answer, a missing report or a mismatched result is reported as unverified rather than counted as capability, and every probe states that it proves capability inside that isolated fixture only. Live probes run only with explicit authorization:

```sh
orchestrate --root "<root>" build preflight --effort "<effort>" --live --authorize-live
```

Without `--authorize-live` it refuses and prints the cost and side effects instead of spending anything. It never touches the product checkout, Build state, credentials, provider configuration, or an installed CLI.

## Existing prepared or active Build

If Build files or state already exist for this exact effort, do not scaffold again and do not replace the detailed plan, `plan.json`, or `config.toml`. Verify the existing exact Build authority and invoke or resume the driver. If the current invocation supplies a Reconciled Discovery or detailed plan that differs from the existing Build's frozen inputs, stop and report the mismatch; do not resume under different authority and do not replace the frozen files. Never delete or recreate Build state merely to satisfy this guide.

After explicit human authorization to launch this exact Build operation, invoke the driver from the target repository:

```sh
orchestrate --root "<root>" build --effort "<effort>"
```

The driver binds the exact Reconciled Discovery, creates or reuses Adoption, records current Build starting commit/tree separately from the Discovery baseline, verifies ancestry, and handles work, review, correction, implementation registration, and final Audit until completion or until it cannot safely continue automatically. Once started, the Rust controller owns these transitions without approval at each step. An external user or infrastructure requirement is one common blocker. Do not perform those role turns yourself.

The Build prints compact progress, transition lines and periodic heartbeats to **stderr**, so a quiet run is visibly in flight. Its **stdout stays exactly one JSON result**, which is the parseable outcome: `BUILD_COMPLETE` or `BLOCKED`. A process exit of zero with a semantic `BLOCKED` result is a stopped acceptance, not a completed one. Activity and heartbeats show that the controller is alive; they never mean the work is progressing or accepted.

Watch a run without disturbing it:

```sh
orchestrate --root "<root>" build status --effort "<effort>"
```

Status is read-only: it writes nothing and dispatches nothing. It reports the current phase, action, scope, dispatch state, the role and the exact configured adapter/model/effort/permission (including an explicit full-access label), accepted phases out of the plan's total, invocation elapsed time and the age of the last provider activity, the durable stop with its exact trigger, and the latest published Audit with its unresolved requirement IDs. It reports controller liveness from the lock separately from provider uncertainty: `terminal: null` never means a live provider, and an absent lock never proves a provider stopped.

## A stopped Build

When the driver stops, it prints a durable `trigger`, the exact `stopped_action`, and the state path; the same facts are in Build state and the journal. Never hand-edit `state.json`, the action, or a receipt to move a stopped Build forward.

Resolve the stop with the exact stopped action:

```sh
orchestrate --root "<root>" build resolve --effort "<effort>" --action "<stopped_action>" --kind "<kind>" --note "<what changed>"
```

`--kind` names the intervention: `existing_authority_clarification` (the role already had authority), `environment_repair` (access, tooling or environment was repaired), `new_verification_evidence` (a final-scope Audit derived BLOCKED and new evidence is available), or `authority_change` (a proposed change to the adopted contract, which Build refuses and records with successor guidance). Add `--evidence` for each verification artifact you supply. Add `--confirm-not-running` only when you have confirmed that an accepted-but-uncertain provider action is no longer running; without that recorded confirmation Build will not create a fresh continuation for it. `--config` supplies new role configuration for future invocations and is accepted only with `environment_repair`; the original `config.toml` and its frozen digest are preserved, and prior records keep the settings they ran under.

Resolution records the intervention and authorizes one distinct continuation. It dispatches no provider action itself: run `orchestrate build` again to continue. An authority amendment must go through new Discovery and Reconcile and an explicitly linked successor Build.

A resolved continuation receives the recorded intervention, the exact predecessor action with its report/result/transport (or their recorded absence), and the original review or Audit correction unchanged; an Unblock diagnosis is supplied as recovery context and never replaces the correction it was diagnosing.

## After the Build

Generated Cargo products accumulate inside the contained review, Audit and once-over checkouts. Once an owning action's evidence is durable and the Build is no longer using that checkout, release them:

```sh
orchestrate --root "<root>" build cleanup --effort "<effort>" --dry-run
orchestrate --root "<root>" build cleanup --effort "<effort>"
```

Cleanup acts only on checkouts the controller itself recorded, checks canonical containment and the exact recorded commit, and removes known generated Cargo output **entry by entry**. Tracked source, history, evidence, the product checkout and provider homes are never touched. Anything inside a product directory the controller cannot positively identify as generated output — an unfamiliar profile, a retained research file, a user file — is preserved and reported, and so is any fixture the action's own durable evidence cites, including each path it kept and why. A checkout whose HEAD moved is skipped with its reason, and a legacy workspace with no ownership record or no recognizable generated content is skipped rather than guessed at; that leaves the attempt reported as incomplete rather than silently deleting the unknown content. Cleanup is idempotent, dry-runnable, and its failures are recorded separately from any stop or verdict.

Export the effort's evidence with verified completeness:

```sh
orchestrate --root "<root>" build export --effort "<effort>" --output "<path>.zip"
```

Export derives its expected members from durable state and from what the controller's own writers owe before it traverses or copies anything, excludes bulky contained-checkout product trees and Discovery source/scratch trees by class, and reads the partial archive back to confirm every selected member is present with its recorded hash before promoting it to the final name. What is required follows the records' own history: an action that recorded a completed provider transport must still have its packet, transport and receipt, and a completed final Audit its assessment, so losing one of those is lost evidence and fails the collection; an action that was interrupted, never dispatched, or written by an older controller is disclosed as the legitimate absence it is. Evidence a durable record references is collected too — nested Build records and live-probe directories are walked with containment checks, and the operator evidence a resolution names is read from where that immutable record says it is and checked against the digest it published. A missing required member, an unreadable one, a symlink, an escape out of the effort, a source file that changed during collection, or any omission makes the export incomplete: it exits nonzero and leaves no final archive. **A valid ZIP or a matching checksum inventory does not by itself prove a complete capture.** Each export owns its scratch paths exclusively: a pre-existing file under a similar name is reported and left exactly where it is, and only that operation's own staging and partial files are ever removed.

The collected Git evidence is independently usable: the history bundle carries the whole history reachable from the recorded commit and therefore needs no excluded baseline, so it restores into an empty repository even when the source never changed after Build started, a run stopped before its first commit is disclosed rather than faked, and the tracked changes a stopped run left uncommitted travel as a patch while untracked files are listed by name only.

Export dispatches no Build, changes no frozen input or state, chooses no role and judges no acceptance: a semantically BLOCKED Build exports successfully, and a PASS Build can never hide a collection failure.

`docs/examples/capture-build-run.sh` wraps either an existing stopped run (`MODE=collect`, the default, which dispatches nothing) or a fresh one (`MODE=run`) and reports the Build exit, the export exit and the semantic outcome as separate facts.
