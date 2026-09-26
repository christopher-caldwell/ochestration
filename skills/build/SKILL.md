---
name: build
description: Prepare or explain an Orchestrate Build without starting its CLI driver.
disable-model-invocation: true
---

`$build` is not authorization to run an Orchestrate CLI command. Do not execute `orchestrate build guide`, `orchestrate build prepare`, `scaffold`, `preflight`, or the Build driver from this skill. Do not start or resume a Build, spend inference on live preflight, or dispatch provider work.

Use the canonical workflow in `crates/guides/resources/guides/build.md` when the repository is available. Help the user prepare by inspecting repository files and explaining the next step. The CLI's non-executing help command is `orchestrate build prepare`; ask for explicit authorization before running it. `scaffold`, `preflight`, `resolve`, and `orchestrate build --effort <id>` are separate operations and require explicit authorization for that command or clearly defined operation.

If the human explicitly authorizes the exact Build launch, the Rust driver owns the full Work/Review/Unblock/Audit loop without approval at each internal transition. Never infer that launch authorization from `$build`, a request to prepare, or a readiness discussion.
