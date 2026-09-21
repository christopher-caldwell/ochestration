# Build

Use this only after an Adoption exists and the user has an approved detailed implementation plan whose delivery-phase grouping is settled. Do not invent phases from Markdown or change the adopted authority.

Resolve the exact effort and Build directory from `orchestrate status`:

```sh
orchestrate --root "<root>" status --effort "<effort>"
```

The result includes `build_dir`. Copy the detailed implementation plan into that directory.

Materialize the current templates from the installed CLI. This writes `plan.json` and `config.toml` and refuses to overwrite a file that already exists:

```sh
orchestrate --root "<root>" build scaffold --effort "<effort>"
```

Fill `plan.json` with schema version 1, the exact Adoption reference (`kind`, `artifact_id`, and `digest`), the relative copied-plan path, and the agreed ordered delivery-phase grouping. Each phase has an id and the task ids that belong only to that phase. Task ids are not extra stops; they record which work belongs to which phase.

Configure the worker and independent reviewer adapters separately in `config.toml`. Supported adapters are `codex`, `claude`, and `cursor`. OpenCode is not supported. Their host configuration must already allow the intended edits and checks; Build does not disable provider safeguards. Do not put credentials, command strings, or provider homes in either file.

From the target repository, invoke the driver yourself after writing the contained inputs:

```sh
orchestrate --root "<root>" build --effort "<effort>"
```

This is part of Build, not a second user action. The driver handles all ordinary work, review, correction, registration, and final Audit transitions until completion or a genuine external requirement. Do not perform those role turns yourself.
