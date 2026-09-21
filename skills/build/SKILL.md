---
name: build
description: Prepare the contained Build inputs for an adopted Orchestrate effort, then run the unattended Build driver.
disable-model-invocation: true
---

# Build

Use this only after an Adoption exists and the user has an approved detailed implementation plan
whose delivery-phase grouping is settled. Do not invent phases from Markdown or change the adopted
authority.

Resolve the exact effort and Build directory from `orchestrate status --effort "<effort>"`.
Copy the detailed implementation plan into that Build directory, then create `plan.json` and
`config.toml` from the checked-in templates. Fill `plan.json` with the exact Adoption reference,
the relative copied-plan path, and the agreed ordered delivery-phase/task-ID grouping. Configure
the worker and independent reviewer adapters separately. Do not put credentials, command strings,
or provider homes in either file.

Validate the files by running `orchestrate build --effort "<effort>"` from the target repository.
The driver handles all ordinary work, review, correction, registration, and final Audit transitions.

