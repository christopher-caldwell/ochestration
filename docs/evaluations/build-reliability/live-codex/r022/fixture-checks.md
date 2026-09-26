# Live preflight fixture checks

The independent checks below were run by the host shell after the role turns. Empty status output means the fixture has no remaining Git changes.

| Role | `git status --porcelain=v1` | `git rev-parse HEAD` | Observation |
|---|---|---|---|
| Worker, immediately after preflight | `?? preflight-worker.txt` (also reported by the provider in `probe-worker/transport.jsonl`) | `ace2992da97ad98f351cce477084dd7e34cfdaea` | Worker wrote the requested one-line file but no commit was created. |
| Reviewer | empty | `4b567d4a12410a18452fdcc57c732f5a55b9996c` | Unique token and hash are present in raw transport; fixture stayed unchanged. |
| Unblocker | empty | `9174603ba9d812c88fbc5339046d4dd3baa83be4` | Unique token and hash are present in raw transport; fixture stayed unchanged. |
| Worker, after R-019 resume permission probe | `?? preflight-worker.txt`, `?? resume-permission-probe.txt` | `ace2992da97ad98f351cce477084dd7e34cfdaea` | Resume write stayed in the disposable checkout; HEAD remained unchanged. |

The live-preflight result remains the controller's source of capability outcomes. These shell checks corroborate the read-role non-mutation and distinguish the resumed permission probe from the initial Worker probe; they do not convert `cannot_commit` or `no_probe_evidence` into successful controller probe outcomes.
