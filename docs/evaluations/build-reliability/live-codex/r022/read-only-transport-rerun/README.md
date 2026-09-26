# R-022 transport-evidence rerun

This rerun followed the controller correction that lets read-only roles report their probe result in the final provider message. The controller copies completed `agent_message` text from `transport.jsonl` into its evidence folder when the role cannot write `report.md`. The read-only prompt now prohibits file writes and asks for exactly the unique fixture token and `git rev-parse HEAD` output.

## Requested settings

- Worker: Codex `gpt-5.6-luna`, reasoning `medium`, sandbox `workspace-write`.
- Reviewer: Codex `gpt-5.6-sol`, reasoning `low`, sandbox `read-only`.
- Unblocker: Codex `gpt-5.6-sol`, reasoning `low`, sandbox `read-only`.
- Once-over: absent.
- Installed CLI: `codex-cli 0.156.1`.

The exact Build config, temporary launcher and requested invocation metadata are in [settings](settings/). The first three rows in `sandbox-enforcement.tsv` are CLI version lookups performed by static preflight; the final three rows are the Worker, Reviewer and Unblocker live invocations in that order. Only the live invocations received the explicit per-role `--sandbox` argument.

## Observed results

- Worker: the requested file was created, but the Codex workspace-write sandbox denied `.git/index.lock`; HEAD stayed at the fixture baseline and `preflight-worker.txt` remained untracked. The controller returned `cannot_commit`.
- Reviewer: returned the unique fixture token and exact HEAD; controller returned `read_and_command_verified`; fixture Git status was clean.
- Unblocker: returned its unique fixture token and exact HEAD; controller returned `read_and_command_verified`; fixture Git status was clean.

The `report.md` files for all three roles are controller-retained copies of their completed final messages extracted from the matching raw transport. Original per-role transports, stderr, exit records and fixture files are retained alongside them. [preflight-result.json](preflight-result.json) is the machine-readable controller response; [cli-stdout.log](cli-stdout.log) and [cli-stderr.log](cli-stderr.log) retain the complete command output. [fixture-checks.json](fixture-checks.json) records independent post-call Git state. Process exit code was 0, which reports successful execution of the preflight command; it does not make the Worker commit probe pass.

This verifies the Reviewer and Unblocker probes in disposable fixtures. R-022 remains incomplete because Worker could not commit under the requested sandbox. No permission was broadened.
