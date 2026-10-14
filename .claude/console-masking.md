# Console secret masking

Issue #43. Spec: `docs/superpowers/specs/2026-10-15-console-mask-secrets-design.md`.

## What is masked

Console HTTP rows written by `sendRequest` carry a masked copy (`HttpConsoleEntry.masked`), fixed at send time by `maskHttpExchange` in `src/lib/console-redaction.ts`. The marker is `••••••`.

- Secret values of the active environment, the global environment and the collection, folder and request variables, in the URL, headers and bodies of the request and the response. Values shorter than 6 characters stay visible, like history and the script engine.
- The values of `Authorization`, `Proxy-Authorization`, `Cookie`, `Set-Cookie` and `X-Api-Key`. The header name stays visible.
- Credentials in the URL (`user:pass@host`) and the value of an API key sent as a query parameter.
- The credentials of the auth block (bearer token, basic and similar passwords, API key value).

## Reveal

The Console detail has a per-row `Show secrets` toggle. It shows the raw fields, which stay in frontend memory only and are never written to disk. Collapsing the row, opening another row or closing the panel masks it again. Search matches the displayed text.

## Rules that must hold

- A value from a secret manager is never resolved, shown or stored raw by the frontend. The helper has no input for it, and a `{{provider.name}}` template passes through unchanged.
- Masking fails closed. When an environment is missing from the query cache, `uncertain` is set: the structural masks (sensitive headers, URL credentials, API key parameter) still apply, the row shows `May contain secrets`, and it cannot be revealed.
- Any copy action added to the Console must copy `selectHttpView(entry, revealed)`, never the raw fields.

## Out of scope

Flow rows (`FlowPane.tsx`) and the Runner still write unmasked rows. The Rust redaction (`redact_secrets`, the script engine) and history are unchanged (#46).

## Manual check

1. Send a request that uses a secret environment variable and a Bearer token. The Console row shows `••••••` for both, and the `Authorization` header name stays visible.
2. Open the row and press `Show secrets`. The raw values appear. Collapse and reopen the row: it is masked again.
3. Send a request whose environment is not loaded. The row shows `May contain secrets` and has no toggle.
