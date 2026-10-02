# Flow Auth Node — Design

Date: 2026-10-02
Status: Implemented

## Problem

A request dragged into a Flow with auth set to `inherit` returns 401 when the
flow runs, while the same request works in the Request tab.

Cause: the Request tab resolves inherited collection auth in the webview. It
reads `collection-auth-store` (Zustand, in-memory) and runs OAuth2 auto-fetch
and refresh there. Flow runs entirely in Rust (`FlowExecutionService::run`),
and `runFlow` sends only `{collection, flowName, environmentName,
globalEnvName}`. The backend loads the collection's OAuth2 *config* from yml
but never has a token, because OAuth2 tokens are deliberately never persisted.

Separate, already-in-progress fix (uncommitted at time of writing): preserve
`inherit` through `toApiAuth` and `build_step_input`. It stays; it is
necessary but not sufficient for OAuth2.

## Goals

- A Flow can authenticate with any supported auth type and use the resulting
  credential in its requests.
- Interactive grants (authorization code, implicit) prompt before the run
  starts, the same way the collection Authentication tab does.
- Non-interactive grants (client credentials, password) and static types work
  headless, with no UI.
- A missing credential that cannot be obtained fails the run before the first
  node, with a message naming the Auth node.
- Dragged-in requests set to `inherit` just work, with no per-request wiring.
  Note: "inherit" here means inherit or none, because the backend treats `none`
  like `inherit` (new, imported and inline requests are all `none`).
- No token is ever written to disk or left in step output, history or logs.

## Non-goals

- Collection Runner support (it has no nodes). Follow-up: a run-level auth
  parameter reusing the same token pass-through and redaction.
- A backend token cache, or moving the Request tab's token handling.
- Mid-run token refresh. A token that expires during a run surfaces as a
  normal 401 from the request.
- "Use collection auth" as a node source (copy the collection's OAuth2
  config instead of re-entering it). Possible follow-up.

## Design

### Node

New `FlowNodeKind::Auth` in `rocket-flow`:

```rust
Auth {
    label: String,
    auth: rocket_shared::types::Auth,   // existing enum, all types
    #[serde(default = "default_true")]
    apply_to_inherit: bool,             // default true
}
```

- No input handles. One output, `result`.
- Persisted in the flow yml like other nodes. Only the auth *config* is
  stored (secrets as `{{vars}}` or vault references, as in collection auth);
  no token field exists.
- Validation: at most one Auth node with `apply_to_inherit = true` per flow.
- `rocket-flow` stays free of I/O and cross-domain dependencies beyond
  `rocket-shared`, which it already uses.

### What the node outputs

| Auth type | Credential | Applied to a request as |
|---|---|---|
| OAuth2 (any grant), Bearer, API key | token / header value | `Auth::Bearer` / API key |
| Basic, Digest, NTLM, WSSE, SigV4, OAuth1 | config with variables resolved | same type; the existing executor signs or answers the challenge per request |

The credential is kept in run context, not as a wire value. The node's wire
output is the raw token string (when the type has one) so custom header wires
keep working. Its reported step value is masked.

For a static Bearer or API key that holds `{{variables}}`, the wire output is
a run-start snapshot (no wire value if a variable is still unset then;
`{{$dynamic}}` values are generated once), while each request resolves the
credential at send time with its own scopes, so a token a Login script writes
during the run is sent; the value each request sends is masked in that
request's debug record, exchange, history and console.

### How requests receive it

1. **Automatic:** a Request node whose resolved auth is `inherit` or `none`
   (the backend treats `none` like `inherit`) uses the flow's auto-apply Auth
   credential instead of the collection auth.
2. **Explicit:** a new wire target `auth` from an Auth node sets a request's
   auth, overriding both `inherit` and the request's own auth.
3. `apply_wired_overrides` gains the `auth` target. An `auth` wire from a
   non-Auth node is a validation error.

Auto-apply Auth nodes are resolved at run start, before the node loop, so
ordering never depends on edges and a failure aborts before any request is
sent.

### Obtaining the token

```
 UI (webview)                                  Backend (Rust)
 Run clicked
   preflight for each Auth node:
     interactive grant, no valid token  ──► oauth2GetToken (prompt, same as
                                            Authentication tab)
     expired + refresh token            ──► oauth2RefreshToken
     token held in flow-auth store (memory only, never persisted)
   runFlow(…, authTokens: {nodeId: token}) ──► run start, per Auth node:
                                                1. supplied token → use it
                                                2. non-interactive grant →
                                                   fetch via oauth2_service
                                                3. static type → build directly
                                                4. else → fail run, naming
                                                   the node
```

- `authTokens` is a new optional field on `RunFlowInputDto` (camelCase IPC
  DTO only), keyed by node id.
- Headless entry points that cannot prompt skip the preflight; step 2–4 above
  then decide.
- Backend fetch reuses `OAuth2Service::resolve_get_token_request_with_secrets`
  and `get_token_direct`, so environment client certificates, variable and
  vault resolution behave as they do today.

### Security

- Tokens exist only in webview memory and in the run's memory. Dropped when
  the run ends. No persistence, no cache, no read command.
- Every supplied or fetched token (and refresh/secret fields) is registered
  with `redaction.rs` for the run, so step output, debug exchange, history
  and script logs show it masked.
- The credential is applied in `execute_node`, before `resolve_request`. The
  request guard only governs script `req.setUrl` redirects, so injection
  bypasses nothing. A Bearer credential goes through `bearer_auth`, so
  `Authorization` is stripped on cross-host redirects like other auth.
- No new IPC command exposes a token.

### Failure messages

- Interactive grant, no token, run not prompted: `Auth node "<label>" needs
  you to authenticate first.`
- Non-interactive fetch failed: the provider error, prefixed with the node
  label.
- Misconfigured node (e.g. missing token URL): validation error naming the
  field.

## Components touched

| Area | Change |
|---|---|
| `rocket-flow` | `FlowNodeKind::Auth`, validation rule, `auth` target handle, round-trip tests |
| `rocket-app` `flow_execution_service` | run-start credential resolution, `execute_node` Auth arm, `auth` target, inherit substitution |
| `src-tauri` `commands/flow.rs` | `authTokens` on `RunFlowInputDto` |
| `src/lib/tauri-api.ts` | `runFlow` signature |
| `src/components/flow` | Auth node, palette entry, properties editor reusing `AuthEditor`, Authenticate button |
| `src/stores` | in-memory flow-auth store |
| Run path | preflight before `runFlow` |

Per `CLAUDE.md`: shadcn primitives and lucide icons only, narrow Zustand
selectors, no `unwrap()` in production paths, serde camelCase on the IPC DTO
only, `.yml` persistence only.

## Testing

- `rocket-flow`: Auth node yml round-trip; backward compatibility of old
  flows; at-most-one auto-apply rule; `auth` wire from a non-Auth node
  rejected.
- `rocket-app`: each auth type produces the right request auth; inherit
  substitution; explicit `auth` wire beats inherit; supplied token beats
  fetch; non-interactive fetch via a fake token client; interactive with no
  token fails before the first node; failure naming.
- Redaction: a run's step results, debug exchange and history never contain
  the raw token.
- Frontend (Vitest): editor renders per auth type; preflight prompts only
  when needed; tokens never written to persisted state.
- Verification: `cargo check`, focused `cargo test`, `yarn tsc --noEmit`,
  `yarn check`.

## Implementation outline (model per task)

| # | Task | Model |
|---|---|---|
| 1 | `rocket-flow` node, validation, tests | Sonnet |
| 2 | Backend executor, run-start resolution, `auth` target | Opus |
| 3 | IPC `authTokens` + redaction registration | Sonnet |
| 4 | Frontend node, palette, editor, Authenticate button | Sonnet |
| 5 | Preflight and flow-auth store | Sonnet |
| 6 | Tests (draft with Haiku, review with Sonnet) | Haiku/Sonnet |
| 7 | Security review and `verify-rocket` | Opus |

Order: 1 → (2 ∥ 4) → 3 → 5 → 6 → 7.

## Security review

Reviewed 2026-10-02 (read-only, `main..feat/flow-auth-node`). No Critical or
High findings. One Medium (F1, a token reused across environments) was fixed;
the Low and Info findings are accepted, deliberate, or deferred as noted below.

| ID | Severity | Finding | Resolution |
|---|---|---|---|
| F1 | Medium | In-memory token key had no environment, so a token fetched under "prod" was reused under "staging" | Fixed: `fix(flow): scope in-memory Auth tokens to the active environment` |
| F2 | Low | Step `error` text is not redacted (failed-send URL, If node value, script exceptions can quote a token) | Deferred: same gap exists for vault secrets; follow-up |
| F3 | Low | Provider error body in the run error is unbounded and unredacted | Deferred: the Authentication tab already behaves this way; follow-up |
| F4 | Low | Literal secrets of non-token auth types (Basic, Digest, etc.) are not masked | Deferred: matches collection-level auth today; narrow exposure |
| F5 | Low | `{{flow-auth.<node id>}}` resolves in every field and script of the run | Accepted: resolves like `{{vault.x}}`; no new capability beyond wiring an Auth node into a URL or header |
| F6 | Low | Backend does not check that a supplied token matches the config it was fetched for (flow file edited during sign-in) | Deferred: needs a config fingerprint; follow-up |
| F7 | Low | Webview tokens live for the whole session and are never cleared; key has no workspace | Deferred: follow-up (the environment part is fixed under F1) |
| F8 | Low | History URL redaction misses percent-encoded token forms | Deferred: also affects vault secrets today; follow-up |
| F9 | Info | Output node masks only Auth tokens, not secret variables | Deliberate: Output shows secret variables raw by an existing test; Auth tokens are masked |
| F10 | Info | Redacted wire value can differ from the sent value if a folder or request variable shadows the token variable | Addressed by the send-time masking commit: each request masks the credential value it actually sends |
| F11 | Info | Spec said injection happens inside `resolve_request` after the request guard | Documentation fix: Security section corrected above; nothing is bypassed |
| F12 | Info | A token is re-resolved as a template before use | Accepted: needs a hostile token endpoint, which the flow author already controls; a token containing `{{x}}` can also make the wire value differ from the sent value |
| F13 | Info | Imported flows fetch tokens without asking | Informational, no change: equivalent to an inline Request node using `{{vault.x}}` |
| F14 | Info | Tokens are plain `String`, not zeroized | Informational, no change |

### Follow-ups

- F2: redact every step error (`redact_url_secrets` in the run loop; consider `reqwest::Error::without_url()`).
- F3: truncate provider error bodies and redact echoed credentials.
- F6: send a config hash with each supplied token and reject a mismatch.
- F7 (partial): in-memory tokens are cleared on node removal and tab close; flow delete/rename have no UI yet — call `clearFlow` when they are added. Still open: clear on workspace and environment delete or rename events; add the workspace id to the key.
- F8: use `redact_url_secrets` for the history URL.
- Send-time masking caveats (a static Bearer or API key template is resolved
  and masked per request):
  - A `repeat_until` attempt after a script rotates the variable may send a
    value that is not in that request's mask, unless the variable is a secret
    variable.
  - `{{$dynamic}}` placeholders inside a static token are not masked at send
    time: the masked value and the sent value are generated separately.
  - A partly resolved template (`{{token}}-{{unset}}`) is not masked by the
    Auth node: the resolved part of the sent text is an unmasked secret unless
    another path (e.g. a secret variable) masks it.
  - `flow-auth-sent.<id>` adds the sent value to that request's secrets, which
    widens the script-write hold-back for that request (a script write
    containing the value is kept in memory instead of persisted).
- Literal secrets typed into an Auth node (client secret, password, token) are
  stored in plaintext in the flow yml, like collection auth. Use `{{vars}}` or
  RocketVault references; consider a UI warning.
