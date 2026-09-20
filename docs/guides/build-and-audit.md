# External Build and Audit

Build is intentionally external. An adoption receipt authorizes Build only against one exact
implementation-ready Reconciled Discovery. Register a committed result with
`orchestrate implementation register`; Orchestrate verifies baseline ancestry and retains the
exact commit and tree snapshot.

Audit may inspect that snapshot and run relevant tests. It covers every binding Reconciled
Discovery requirement exactly once. Technical suggestions have no Audit requirement IDs and need
no coverage. Rust derives `PASS`, `CHANGES_REQUIRED`, or `BLOCKED` from the coverage rows and
implementation status.
