# Discovery

Discovery is open engineering investigation over a frozen source checkout. It can inspect source,
Git history, tests, authorized experiments, and relevant authoritative documentation. It asks the
user material questions in its own model window.

Each run records `Question`, `Finding`, `Decision`, and `Requirement` nodes plus a self-contained
`technical-spec.md`. Question states are `open`, `answered`, `no_change`, and `blocked`; a material
blocked question produces a blocked Discovery. Accepted Findings need source references and
mandatory Requirements must trace to accepted evidence.

Discovery runs are independent and need no provider slots. Launch another model window whenever
another investigation would be useful.
