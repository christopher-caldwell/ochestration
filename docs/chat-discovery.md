# Chat Discovery

Use this guide in a fresh ChatGPT conversation when collaborative Discovery is sufficient. Work with the user to determine what should be built and why. Challenge assumptions, identify material uncertainties, and distinguish explicit user requirements from your analysis and technical suggestions. Record meaningful decisions, rejected directions, compatibility concerns, and unresolved blockers. Describe only investigation or verification that actually occurred. This conversation is **one collaborative Discovery source**, never multiple independent runs.

When the user explicitly requests a handoff, create `chat-discovery.zip` with this small layout:

```text
chat-discovery.zip
├── discovery.json
└── build/                         optional
    └── phase_01_delivery/
        └── phase.md
```

`discovery.json` is UTF-8 JSON. Required fields are `effort` (a short human slug), `request` (the original request), `problem`, `selected_direction`, `evidence_summary`, and a nonempty `requirements` array. Each requirement has `id`, `text`, `acceptance`, and `authority` (`explicit_user` or `chat_analysis`), with optional `condition`. An `explicit_user` requirement also needs a nonempty `user_statement` quoting or faithfully recording the supporting user instruction; a `chat_analysis` requirement must not include `user_statement`. Preserve additional explicit user constraints in `constraints`. Optional arrays are `findings`, `decisions`, `disagreements`, `product_behavior_changed`, `product_behavior_unchanged`, `technical_behavior_changed`, `technical_behavior_unchanged`, `rejected_alternatives` (objects with `direction` and `reason`), `implementation_risks`, `compatibility_concerns`, `caveats`, `advisory_technical_suggestions`, and `blockers`. Use empty arrays or omit optional fields when there is nothing to record. Do not claim local repository inspection or experiments in `evidence_summary` unless they occurred. Resolve material blockers before import; a bundle with blockers cannot become implementation-ready.

For example:

```json
{
  "effort": "status-page",
  "request": "Add a compact status page",
  "constraints": ["Keep the existing API"],
  "problem": "Status is hard to find",
  "selected_direction": "Add one compact status page",
  "evidence_summary": "The user chose this direction in conversation; current repository behavior was not inspected.",
  "requirements": [
    {"id": "R-1", "text": "Show status on one page", "acceptance": "The page displays current status", "authority": "chat_analysis"},
    {"id": "R-2", "text": "Keep the public command name", "acceptance": "Existing callers use the same command", "authority": "explicit_user", "user_statement": "Do not rename the existing command."}
  ]
}
```

Optional `build/phase_XX_name/phase.md` files use two decimal digits and a nonempty name. They provide implementation guidance and ordering, not binding authority. The chat cannot know the local Reconciled artifact ID or choose local provider settings; do not include `plan.json` or `config.toml`.

When supplying optional Build phases, use the same phase boundary as normal Build preparation: each phase should be a substantial, coherent implementation slice that can be built, meaningfully verified, and reviewed efficiently as one unit. Optimize for efficient Work → Review cycles and useful accepted checkpoints, and include the tests or verification naturally associated with the slice. Group multiple related tasks when together they make a stronger coherent Review unit; task enumeration alone does not call for another Review. Let phase count follow the work without a numeric target. If substantial reviewable implementation has a natural earlier checkpoint before later independently blocking work—such as external credentials, environment-specific integration, or real-service acceptance—plan that later work separately when useful; this is conditional, not a blanket extra-phase rule.

In the target Git repository, import with `orchestrate import ./chat-discovery.zip` (and `--root <store>` if needed). Import freezes the repository's current committed HEAD, publishes one Discovery and one Reconciled Discovery, and prepares the normal Build directory. Omitting phases is valid: import invents no phase, and the generated plan has an empty phase list until real phases are supplied. Build validation rejects that incomplete plan on launch. Review the result and stop. Producing or importing the ZIP does **not** authorize Build. A later explicit `$build` launch is required for Adoption, Work, Review, and Audit.
