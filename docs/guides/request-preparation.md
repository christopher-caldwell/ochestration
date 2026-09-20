# Request preparation

Use `$prep-discovery-ticket` for an existing ticket and `$prep-discovery-freeform` for a freeform
request. Both produce a reviewed Markdown file with exactly this frontmatter shape:

```yaml
---
root: /absolute/path/to/.orchestration
project: /absolute/path/to/repository
effort: example-effort
request_kind: ticket
constraints: []
---
```

The Markdown body is opaque request authority. Ticket preparation retains the placeholder so the
user can paste the original ticket verbatim. The preparation phase does not interpret the ticket or
start Discovery.
