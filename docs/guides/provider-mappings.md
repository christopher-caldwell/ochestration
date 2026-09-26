# Provider translation reference

This is the complete translation the Build controller performs between a role's provider-neutral
settings and each provider CLI. It is a translation table, not a policy.

- The controller chooses nothing about a provider beyond what the role configuration states.
- It sets **no** environment variable for a provider process and clears none: each role inherits the
  launching environment exactly, so the backend, endpoint and credential are whatever the host and
  that provider's own configuration already select.
- It reads and writes no provider configuration, credential store or session store. Session ids and
  reported models are taken from the provider's own output stream, never from its files.
- Everything else it passes is transport: how to run the CLI headlessly, which session to resume,
  which directory the role works in, and where its action packet is.

The generic vocabulary is in [`build.md`](../../crates/guides/resources/guides/build.md) and
[`build-and-audit.md`](build-and-audit.md); this document is the provider-native side of it. The
mapping lives in one function (`attribute_arguments_for` in `crates/build/src/lib.rs`), used by
dispatch, by static preflight and by every invocation record, so a reported mapping is the mapping
that runs.

## 1. What a role declares

| Field | Values | Meaning |
| --- | --- | --- |
| `adapter` | `codex`, `claude`, `cursor` | Which provider CLI runs this role. Required. |
| `model_strength` | `standard`, `strong` | A named quality tier. Maps to a fixed native model id per adapter. |
| `reasoning_effort` | `low`, `medium`, `high` | Requested reasoning effort. Maps per adapter; see §4. |
| `permission` | `read_only`, `workspace_write`, `full_access` | What the role may do to the workspace. See §5. |

Every attribute is optional except the adapter. An attribute that is not set sends **no flag at
all** (§6), and any value without a mapping is refused before dispatch (§10).

## 2. Adapter selection

| `adapter` | Program launched (resolved on the launching `PATH`) | Provider-specific choices made by the controller |
| --- | --- | --- |
| `codex` | `codex` | none |
| `claude` | `claude` | none |
| `cursor` | `cursor-agent` | none |

## 3. `model_strength` → native model id

Passed as `--model <id>` for every adapter.

| `model_strength` | codex | claude | cursor (no `reasoning_effort`) | cursor (with `reasoning_effort`) |
| --- | --- | --- | --- | --- |
| `standard` | `gpt-5.5` | `deepseek-flash` | `gpt-5.5` | `gpt-5.5-<effort>` |
| `strong` | `gpt-5.6-sol` | `deepseek-v4-pro` | `gpt-5.6-sol-high` | `gpt-5.6-sol-<effort>` |

Two asymmetries are deliberate and worth reading twice:

- **Cursor encodes effort inside the model id**, so its id depends on both fields. With
  `model_strength` and no `reasoning_effort`, `strong` selects `gpt-5.6-sol-high` while `standard`
  selects the bare `gpt-5.5`.
- **The claude row names DeepSeek model ids.** They are the mapping for a host whose Claude Code is
  configured against that endpoint. On a host configured for a different backend, omit
  `model_strength`: no `--model` is then passed and the host's own configured model governs. See §9.

## 4. `reasoning_effort` → native effort

This one does not fold into the model table because the three adapters treat it differently: Codex
has a native effort knob, Claude uses a differently-named level set, and Cursor has no separate knob
at all.

| `reasoning_effort` | codex | claude | cursor |
| --- | --- | --- | --- |
| `low` | `-c model_reasoning_effort=low` | `--effort low` | model-id suffix `-low` |
| `medium` | `-c model_reasoning_effort=medium` | `--effort high` | model-id suffix `-medium` |
| `high` | `-c model_reasoning_effort=high` | `--effort max` | model-id suffix `-high` |
| set without `model_strength` | passed through | passed through | **refused**: Cursor selects reasoning through model ids, so the error names `model_strength` |

Notes:

- Codex takes the value through a config override rather than a flag, because `exec resume` accepts
  `-c` overrides but omits the top-level `--sandbox` flag (§9).
- Claude's level set is smaller than the generic one, so `medium` and `high` intentionally map to
  stronger native levels. The requested generic value and the mapped native value are recorded
  separately.
- No adapter reports its effective effort back, so `observed.reasoning_effort` stays unknown even
  when a mapping ran.

## 5. `permission` → sandbox and approval

| `permission` | codex | claude | cursor |
| --- | --- | --- | --- |
| `read_only` | `-c sandbox_mode="read-only" -c approval_policy="never"` | `--permission-mode plan` + `--settings '{"sandbox":{"enabled":true,"allowUnsandboxedCommands":false}}'` | `--trust --mode ask --sandbox enabled` |
| `workspace_write` | **Unsupported.** Refused before dispatch: Codex protects Git metadata in `workspace-write`, so it cannot stage and commit; the error names `full_access` as the explicit alternative | `--permission-mode acceptEdits` + the same `--settings` object | `--trust --force --sandbox enabled` |
| `full_access` | `--dangerously-bypass-approvals-and-sandbox` | `--dangerously-skip-permissions` | `--force` |
| not set | nothing: the provider's configured default governs | | |

- `read_only` and `workspace_write` are contracts, not assertions that a provider honours them
  perfectly; each adapter maps them to that provider's own controls, and the static preflight
  reports what it cannot establish locally as unknown.
- `full_access` is the one explicit unrestricted mapping per adapter. Nothing selects it on a role's
  behalf, and a narrower permission that cannot satisfy a role's contract is refused rather than
  widened into it.
- The Cursor read-only mode is `ask`, not `plan`: `plan` ends its turn requesting plan approval,
  which a role whose deliverable is its own report never receives. `ask` is read-only without that
  approval turn.

## 6. Attributes that are not set

An unset `model_strength`, `reasoning_effort` or `permission` adds no argument. The provider's own
configured default then governs, and it is recorded as *unknown* — a provider default is not a
frozen effective value:

```json
{"attribute": "model_strength", "configured": null, "source": "provider default (not a frozen effective value)"}
```

This is how a role uses "whatever my provider is configured for": leave the attribute out.

## 7. Transport scaffolding

Independent of the role configuration, every dispatch is built as follows. The prompt is always the
final element, and it is the only part elided from retained records.

| | codex | claude | cursor |
| --- | --- | --- | --- |
| Non-interactive, structured output | `exec --json` | `-p --verbose --output-format stream-json` | `-p --output-format stream-json` |
| Resume the role's session | `resume <session-id>` (subcommand) | `--resume <session-id>` | `--resume <session-id>` |
| Working directory | `-C <dir>` | the child process `cwd` | the child process `cwd` |
| Action-packet directory grant | `--add-dir <build dir>` | `--add-dir <build dir>` | none passed |
| Separator before the prompt | — | `--` (Claude's `--add-dir` is variadic and would otherwise consume the prompt) | — |

Argument order is: the transport arguments above, then the §3–§5 mapping for the role, then the
prompt.

## 8. What one dispatch records

| Recorded | Source |
| --- | --- |
| `requested` | the role configuration as written, per attribute, or `null` with `source: provider default` |
| `mapped.arguments` | the exact provider-native argument array for this dispatch |
| `mapped.environment` | always `{}`: the controller sets no provider environment |
| `command.form` | program plus argv with only the prompt elided |
| `command.working_directory`, `command.directory_grants` | this dispatch's workspace and packet access |
| `observed.model` | the provider's own stream, when it reports one |
| `observed.reasoning_effort` | always unknown; no adapter reports it |
| `mode`, `session.requested`, `session.observed` | whether this fresh or resumed dispatch, and the session the provider reported |

## 9. Behaviour notes

- **Claude `--settings` precedence.** The `--settings` JSON is a higher-precedence settings source
  than the user's own settings files for the keys it sets (`sandbox.enabled`,
  `sandbox.allowUnsandboxedCommands`), so a host with its own Claude sandbox policy has those two
  keys governed by the role's `permission` during a dispatch. It is deliberately how `read_only` and
  `workspace_write` are made enforceable for Claude; no other Claude setting is touched.
- **Cursor `--trust`.** Passed for `read_only` and `workspace_write` so a headless run does not stop
  at an interactive workspace-trust prompt. `full_access` maps to `--force` alone, mirroring the
  frozen legacy mapping.
- **Native names are not expressible in v3.** A role cannot pin a provider-native model or effort
  level; that is the point of the generic vocabulary. Use the named tier, or omit the attribute and
  let the host's configuration govern. Frozen schema-v2 configurations keep their provider-native
  fields (§11).
- **Sessions are resumed by identity, never guessed.** The id comes from the provider's own stream on
  the dispatch that opened it, and only the roles that hold a persistent session (Work, Review) ever
  pass one.
- **Nothing is retried by adjusting a mapping.** A provider failure stops the Build through the
  recovery ladder; the controller never escalates a permission or substitutes a model to get past it.

## 10. Unsupported combinations

Refused before any provider is launched, with the role, field, value and adapter in the message:

| Refused | Why |
| --- | --- |
| codex `workspace_write` | Codex keeps Git metadata read-only in `workspace-write`, so it cannot meet the commit contract; name `full_access` explicitly if that is what the role needs |
| cursor `reasoning_effort` without `model_strength` | Cursor selects reasoning through model ids |
| any `model` (native name) in schema v3 | v3 is provider-neutral; use `model_strength`, or a v2 config for a frozen legacy Build |
| unknown adapter, strength, effort or permission value | the vocabulary is closed; nothing is dropped silently |
| unknown key in a role table | role tables reject unknown fields instead of ignoring them |

## 11. Frozen schema-v2 configurations (legacy)

Schema version 2 remains readable so an existing Build keeps its frozen meaning. Its fields are
provider-native and are translated as follows; it is never rewritten in place.

| v2 field | Translation |
| --- | --- |
| `model` | `--model <native name>` for every adapter |
| `reasoning_effort` (Codex only: `minimal`, `low`, `medium`, `high`) | `-c model_reasoning_effort=<value>` |
| `permission = "full_access"` | codex `--dangerously-bypass-approvals-and-sandbox`; claude `--dangerously-skip-permissions`; cursor `--force` |

For a stopped v2 Build, write a separate schema-v3 config and pass it to
`orchestrate build resolve --kind environment_repair --config <file>`; the controller records an
immutable config-history overlay for future actions, and prior actions keep the settings they ran
under. No migration guesses at an equivalent model.

## 12. Where this is enforced

| Test | What it holds |
| --- | --- |
| `provider_neutral_v3_attributes_map_identically_on_fresh_and_resumed_calls` | each adapter's mapped arguments appear on fresh and resumed dispatches alike |
| `role_attributes_route_exactly_and_unsupported_values_fail_clearly` | exact mapping, and refusal naming role/field/value/adapter |
| `v3_unsupported_values_name_role_field_value_and_adapter` | the v3 refusals, including the Codex `workspace_write` rejection |
| `full_access_is_explicit_and_maps_at_every_adapter_edge` | the explicit unrestricted mapping per adapter, unchanged between fresh and resumed |
| `provider_commands_inherit_the_launch_environment` | a dispatch command carries **no** environment of its own, for any adapter |
| `provider_reported_model_labels_are_separate_from_requested_values` | observed labels are the provider's, not the requested value restated |
| `opt_in_live_provider_preflight_checks_commit_resume_and_roles` (opt-in) | a real dispatch's recorded mapping equals the static preflight mapping, per role |

## Upstream references

- [Codex configuration reference](https://developers.openai.com/codex/config-reference) documents
  `sandbox_mode`, `approval_policy`, model, and reasoning controls.
- [Claude Code CLI reference](https://docs.anthropic.com/en/docs/claude-code/cli-usage),
  [settings](https://code.claude.com/docs/en/settings),
  [permissions](https://code.claude.com/docs/en/permissions), and
  [sandboxing](https://code.claude.com/docs/en/sandboxing) document the local CLI arguments, the
  settings precedence this document relies on, and the permission boundary.
- [DeepSeek's Claude Code integration](https://api-docs.deepseek.com/quick_start/agent_integrations/claude_code/),
  [Anthropic API compatibility](https://api-docs.deepseek.com/guides/anthropic_api/), and
  [thinking/effort mapping](https://api-docs.deepseek.com/guides/thinking_mode/) document that
  backend's model ids and API behaviour.
- [Cursor CLI parameters](https://docs.cursor.com/en/cli/reference/parameters),
  [permissions](https://docs.cursor.com/cli/reference/permissions),
  [authentication](https://docs.cursor.com/en/cli/reference/authentication), and
  [headless output](https://docs.cursor.com/en/cli/headless) document this adapter's CLI contract.
