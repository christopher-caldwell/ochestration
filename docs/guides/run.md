# Run Orchestrate

Prepare a request with `$prep-discovery-ticket` or `$prep-discovery-freeform`. Review the generated
file, including its exact original ticket content where applicable.

Open as many independent model windows as useful and invoke:

```text
$discovery "/absolute/path/request.prepared.md"
```

Every invocation safely initializes or reuses the same frozen effort and creates one new isolated
Discovery. A completed Discovery publishes a self-contained technical specification and evidence
graph. A blocked Discovery remains visible but cannot be reconciled.

Give `$reconcile` two or more finalized Discovery directories. The skill validates the exact,
explicit input set before reasoning. It reads only those artifacts and the frozen request; it never
opens the repository, source checkout, tests, Git history, web, private chats, or mutable
workspaces. Review its Reconciled Discovery. If you explicitly approve it, the skill records an
adoption receipt.

Build happens outside Orchestrate. Commit the result, then register its exact revision:

```sh
orchestrate implementation register --effort "<effort>"
```

Finally use `$audit`. Audit examines the retained implementation snapshot against only the binding
requirements in the exact adopted Reconciled Discovery. It never treats advisory technical
suggestions as requirements.

For inspection, use `orchestrate status`, `orchestrate journal`, `orchestrate inspect`, and
`orchestrate lineage` with the effort and artifact IDs reported by the skills.
