# Orchestrate Discovery

Discovery answers: **what should be built, and why?** Work only in the run workspace supplied by `orchestrate discovery prepare`. Its `source/` directory is a clean detached checkout of the cohort baseline with Git history. Do not inspect sibling runs, parent records, a dirty checkout, or unrelated paths.

Start by reading `run.json`, `request.md`, and `context.json`. `run.json` defines this Discovery run: its slot, effort, cohort, baseline, host, provider, model, and model effort. `request.md` is the original user input and must be read as written. `context.json` supplies the same frozen request for convenience plus explicit user-supplied constraints. Constraints are explicit user-supplied clarifications or governing instructions; do not extract new immutable constraints from a ticket.

When `request_kind` is `ticket`, the ticket is the best available authority for requested behavior and intended scope. Preserve its wording, distinctions, conditions, and explicit scope; do not silently summarize, generalize, narrow, or replace it. A ticket is not infallible about facts. Claims about current code, vendor behavior, regressions, causes, or fallback behavior are hypotheses to investigate. If credible evidence materially conflicts with the ticket, do not silently prefer either source: explain what the ticket says, what the evidence says, why it matters, and ask the user for a decision. Record the clarification as evidence.

When `request_kind` is `freeform`, treat the request as the user's stated goal and context, without inventing requirements that are not there. Ask the user when missing information creates a material product or engineering choice. A later explicit user clarification takes precedence over a conflicting earlier ticket statement; preserve and document the conflict.

Record important questions, findings, decisions, and requirements in `graph/*.md`. Each node needs valid frontmatter and explicit dependency links. Every accepted finding needs at least one source reference. Question states are only `open`, `answered`, `no_change`, and `blocked`:

- `answered` means the question has an evidence-supported answer the Discovery can rely on. A required answered Question needs an accepted Finding that directly depends on it.
- `no_change` means the investigation establishes no implementation change or requirement.
- `blocked` means the answer is not established and can materially change requested behavior, implementation scope, compatibility, acceptance criteria, whether a requirement exists, or whether a proposed solution is safe or correct.
- `open` is still being investigated and is not a final disposition.

Evidence that only establishes "unknown," "not found in the repository," or "vendor behavior unconfirmed" is not an answer. Do not resolve material ambiguity by assumption. Investigate first; use relevant source, tests, Git history, authoritative external documentation, or authorized controlled experiments. In particular, consider `git log`, `git blame`, and `git show` when current code does not explain environment-specific behavior, compatibility guards, constants, regressions, or historical intent. Do not perform meaningless Git archaeology.

If evidence answers a material question, record the answer. If it materially conflicts with the request, ask the user. If it remains unknown and the user can reasonably clarify it, ask directly in the current host conversation and record the clarification. If it remains unknown and materially affects the implementation contract, mark the required Question `blocked`; do not produce an implementation-ready result.

Use `technical-spec.md` as the public result. It must stand alone and cover the interpreted request, current behavior, recommendation, required and unchanged behavior, decisions and rationale, requirements and acceptance criteria, conditions, alternatives or disagreement, limitations, and verification.

Run `orchestrate discovery validate` whenever an intermediate check is useful. You do not have to run it before finalization: `orchestrate discovery finalize` validates the workspace before it publishes anything. The outcome is derived mechanically and cannot be overridden. Any `blocked` Question produces a blocked result that is ineligible for Consensus; otherwise the workspace must be implementation-ready, which requires every required Question to be `answered` or `no_change` and every mandatory Requirement to trace to accepted evidence. Do not start Consensus, adopt an Agreement, or edit the target repository. Stop after this phase.
