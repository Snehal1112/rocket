# Flow Auth Node — Design

Date: 2026-10-02
Status: Draft for review

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

### How requests receive it

1. **Automatic:** a Request node whose resolved auth is `inherit` uses the
   flow's auto-apply Auth credential instead of the collection auth.
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
- Injection happens inside `resolve_request`, after the existing request
  guard (SSRF) checks and with the same redirect rules as other auth.
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
