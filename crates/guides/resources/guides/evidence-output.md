# Durable output for a role that cannot write files

Your role guide names the exact evidence your action owes: a receipt at the `result` path, a report
at the `report` path, and for the final Audit an assessment at the `assessment` path. These are the
paths `action.json` gives you. Write those files directly when your permission lets you.

A role running under `read_only` permission cannot create files, so it states the same evidence in
its **final response** instead. The controller reads the response from its own transport log,
validates each artifact by exactly the rules it applies to a written file, and writes it to the
named path. Nothing about what the controller then checks changes.

End your final response with these blocks. Use a fence of at least three backticks, and use four
when the content itself contains a fence, so the controller can find the real end of yours.

`````text
```orchestrate-receipt
{"action_id": "<action_id>", "scope": "<scope>", "outcome": "<the outcome from your role guide>", "commit": "<the commit you inspected, when your role guide requires one>"}
```

````orchestrate-report
<the report the file would hold: your role guide's report content, in markdown>
````
`````

The final Audit adds one more block:

`````text
````orchestrate-assessment
{<the exact assessment JSON the final Audit guide specifies>}
````
`````

- A block holds the artifact itself, never a description of it. The receipt and the assessment hold
  JSON only and nothing else inside their fences; the report holds only your report.
- Emit each block once, at the end of your final response.
- If your permission did let you write the files, do not repeat them as blocks: a written file is
  always the authority, and the controller ignores a block for an artifact that already exists.
- A response that carries no valid artifact leaves your evidence missing. The controller stops with
  a receipt failure instead of assuming an outcome from your prose.
