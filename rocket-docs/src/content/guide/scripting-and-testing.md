# Scripting & testing

Requests can carry **pre-request**, **post-response** and **test** scripts,
written in the Monaco editor with real TypeScript IntelliSense for Rocket's
`rok.*` scripting API. A request tab's toolbar lines up Params, Body, Headers,
Auth, Variables, Scripts, **Assertions**, Vars and **Load test** as sub-tabs —
load testing is configured per request, right next to its scripts and tests,
not only as a separate top-level screen (see [Load testing](/guide/load-testing)).

## `rok.*` and Bruno parity

The `rok.*` API is built for near-complete parity with Bruno's own JavaScript
API — sync state, async `sendRequest`/`runRequest`/`sleep`, cookies and the
cookie jar. Scripts written for Bruno generally port over with minimal
changes, so migrating a script library isn't a rewrite.

## Shared script files

Scripts can `require()` shared `.js` files local to the collection. Resolution
is sandboxed: it's restricted to the collection directory (or extra roots you
explicitly approve in Developer Mode), with path canonicalization so `..`
traversal and symlink escapes are blocked by construction, not just checked
after the fact.

## Safe Mode vs Developer Mode

Scripts run in **Safe Mode** by default — sandboxed, with no filesystem or
system access. **Developer Mode** exists for scripts that genuinely need more,
and it's an explicit, deliberate opt-in every time, rather than something a
collection you just opened can switch on for you.

![Rocket's script sandbox picker — Safe Mode by default, Developer Mode as an explicit opt-in](/screenshots/script-sandbox-modes.png)

## No-code assertions

If you don't want to write test scripts at all, a visual assertions builder
covers the common checks (status code, header values, body fields) without
touching JavaScript. Each assertion reads as a plain line — `res.status eq 200`,
`res.body.token isNotEmpty` — and the response panel's **Tests** tab shows how
many passed, for example `6 passed`.

A failed assertion shows its own reason inline (`Error: Expected 200, got 403`,
`path 'username' not found in response body`), and a script error surfaces as
a real stack-style message (`TypeError: res.getTime is not a function`) rather
than just a red X, so you can see why a check failed without re-running it in
a separate tool.

![A request with 9 failing assertions, each with its own error message](/screenshots/test-results.png)

## Running your tests

A single request's tests run with the request itself. To run every test
across a folder or collection in sequence, see
[Collection Runner](/guide/collection-runner).
