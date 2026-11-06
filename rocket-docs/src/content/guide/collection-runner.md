# Collection Runner

The Collection Runner executes a whole collection, or a single folder,
sequentially — every request in order, with its pre-request/test scripts and
assertions, and live streamed progress as it goes. When it finishes, you get a
results summary: what passed, what failed, and where.

## Telling runs apart in History

Every History entry carries a **run-source badge** — `manual`, `runner`,
`load test`, `flow`, or `agent` — recording how that request actually ran.
That makes it straightforward to filter History down to just the runner's own
executions, separate from requests you sent by hand or that a Flow triggered.

## Related

- [Scripting & testing](/guide/scripting-and-testing) — what runs during each
  request in the sequence.
- [Load testing](/guide/load-testing) — for concurrent, phase-based runs
  rather than a single sequential pass.
- [Flow](/guide/flow) — for conditional branching between requests instead of
  a straight-through sequence.
