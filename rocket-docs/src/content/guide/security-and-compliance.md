# Security & compliance

Beyond masking secrets (see [Secrets & vault](/guide/secrets-and-vault)),
Rocket gates elevated capabilities explicitly rather than granting them
implicitly the moment you open a collection.

## The collection trust gate

A collection can ask for capabilities that go beyond a plain request/response
cycle — developer-mode script access, extra script `require()` roots, letting
the [AI Assistant](/guide/ai-assistant) run requests on your behalf, host
environment-variable access, or RocketVault secret/certificate access. The
trust gate shows exactly what's **requested** against what you've **allowed**,
per computer — opening a collection someone else authored doesn't silently
grant it anything. A collection imported, cloned, or opened from disk starts
with every one of these denied by default, even when the collection file
itself requests them. Rocket surfaces the request and lets you approve it at
the point you actually try to use the capability — there isn't a separate
settings page to go find and pre-approve things in ahead of time.

## `process.env` placeholders

A `{{process.env.NAME}}` placeholder reads the host's real OS environment
(and workspace/collection/environment `.env` files, with the OS value winning
on conflicts) — but only in a collection you've explicitly allowed. It's read
on HTTP sends, the Collection Runner, Flow, OAuth2 token requests, WebSocket,
GraphQL subscriptions, and gRPC. In a collection you haven't allowed, the
placeholder is sent exactly as written, with a single Console warning, and no
`.env` file is read at all. Values pulled this way are masked in History, the
Console, and Flow traces, same as any other secret.

## Script sandboxing

Scripts run in **Safe Mode** by default — no filesystem or system access.
**Developer Mode** is available for scripts that need more, but it's an
explicit opt-in every time, never something inherited automatically from a
collection. See [Scripting & testing](/guide/scripting-and-testing).

## The Audit Log

Every workspace has an **Audit Log** tab: a filterable, timestamped event
table (filter by actor, kind, or control ID) where each row lists the event
and the compliance controls it maps to — for example `SOC2 CC6.1`,
`ISO27001 A.8.15`, `ISO27001 A.5.15`, `CSA_STAR IAM-09` — plus a **hash** per
row, confirming the log is tamper-evident and hash-chained rather than a plain
editable list. A **Profile** button lets you pick which compliance profile's
controls to track, and **Export evidence** produces a file you can hand to an
auditor directly from the app.

![The Audit Log — filterable events, each tagged with its compliance controls and a hash](/screenshots/audit-log.png)

This is useful, concrete evidence for a security review or a regulated
environment — not just an internal debug log nobody reads.
