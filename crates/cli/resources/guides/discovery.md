# Orchestrate Discovery

Discovery answers: **what should be built, and why?** Work only in the run workspace supplied by `orchestrate discovery prepare`. Its `source/` directory is the committed baseline; do not inspect sibling runs, parent records, a dirty checkout, or unrelated paths.

Read `context.json`, investigate the frozen source, and treat request assertions as hypotheses rather than source truth. Record important questions, findings, decisions, and requirements in `graph/*.md`. Each node needs valid frontmatter and explicit dependency links. Every accepted finding needs at least one source reference. A resolved required question must lead to an accepted finding that directly depends on it; `no_change` may terminate without a requirement. Consider counterevidence and alternatives before accepting a conclusion.

Use `technical-spec.md` as the public result. It must stand alone and cover the interpreted request, current behavior, recommendation, required and unchanged behavior, decisions and rationale, requirements and acceptance criteria, conditions, alternatives or disagreement, limitations, and verification.

Run `orchestrate discovery validate` before finalization. Implementation-ready finalization requires every required question to be resolved or no-change and every mandatory requirement to trace to accepted evidence. If a required question is genuinely blocked, mark it `blocked` and finalize a blocked result instead. Do not start Consensus, adopt an Agreement, or edit the target repository. Stop after this phase.
