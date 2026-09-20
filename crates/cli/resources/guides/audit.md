# Orchestrate Audit

Audit answers: **does this exact implementation satisfy this exact adopted Reconciled Discovery?** Read the exact Reconciled Discovery, adoption, implementation record, and supplied immutable implementation source. Inspect code and run relevant tests using available host tools, but do not edit source or authority.

Return exactly one coverage row for every binding Reconciled Discovery requirement: `pass`, `fail`, `unknown`, or justified `not_applicable`. Give bounded evidence and correction guidance for failures. Technical suggestions are advisory and have no coverage rows. Do not introduce architectural preferences as contract failures.

Rust, not the assessor, derives the verdict: any failure is `CHANGES_REQUIRED`; any unknown or missing row is `BLOCKED`; otherwise pass or justified not-applicable rows produce `PASS`. A partial or blocked implementation cannot pass. Do not start another phase. Stop after the Audit result.
