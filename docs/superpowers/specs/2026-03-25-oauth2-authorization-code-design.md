# OAuth 2.0 Authorization Code Flow with PKCE — Design Spec

**Date:** 2026-03-25
**Status:** Approved

## Problem

The Authorization Code grant type in the OAuth2 auth editor is a stub — the "Get Token" button is disabled with "coming soon." The Rust backend has partial support (OAuthConfig has `code`, `redirect_uri`, `code_verifier` fields) but the full browser-based consent flow is not implemented.

## Solution

Implement the complete Authorization Code flow with PKCE for the Tauri desktop app using a temporary local HTTP server as the redirect URI callback receiver.

## Flow

```
1. User selects "Authorization Code" grant type
2. User fills in: Authorization URL, Token URL, Client ID, Client Secret, Scope
3. User clicks "Get Token"
4. Frontend calls Tauri command: oauth2_auth_code_flow(config)
5. Rust:
   a. Generates PKCE code_verifier (43-128 chars, URL-safe random)
   b. Computes code_challenge = BASE64URL(SHA256(code_verifier))
   c. Starts temporary HTTP server on random available port (e.g. 9876)
   d. Opens browser with authorization URL:
      {authorizationUrl}?response_type=code
        &client_id={clientId}
        &redirect_uri=http://localhost:{port}/callback
        &code_challenge={code_challenge}
        &code_challenge_method=S256
        &scope={scope}
        &state={random_state}
   e. Waits for callback (max 120 seconds timeout)
   f. Browser redirects to http://localhost:{port}/callback?code=ABC&state=XYZ
   g. Server captures the code, verifies state, responds with "You can close this tab"
   h. Server shuts down
   i. Exchanges code for token: POST tokenUrl with:
      grant_type=authorization_code
      code=ABC
      redirect_uri=http://localhost:{port}/callback
      client_id={clientId}
      client_secret={clientSecret}
      code_verifier={code_verifier}
   j. Returns OAuthToken to frontend
6. Frontend stores access_token and refresh_token in auth state
```

## Rust Changes

### New Tauri command: `oauth2_auth_code_flow`

In `src-tauri/src/commands/` (new file or extend existing):

```rust
#[tauri::command]
pub async fn oauth2_auth_code_flow(
    authorization_url: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    scope: Option<String>,
) -> Result<OAuthToken, DomainError>
```

### PKCE generation

In `crates/rocket-http/src/oauth2.rs`, add:

```rust
pub fn generate_pkce() -> (String, String) {
    // code_verifier: 43-128 char URL-safe random string
    // code_challenge: BASE64URL(SHA256(code_verifier))
}
```

Dependencies: `sha2` for SHA-256, `base64` for URL-safe encoding, `rand` for random bytes. Check if these are already in the workspace.

### Temporary local HTTP server

In `crates/rocket-http/src/oauth2.rs` or a new `oauth2_callback.rs`:

```rust
pub async fn wait_for_auth_callback(port: u16, expected_state: &str, timeout_secs: u64)
    -> Result<String, DomainError>
```

- Bind to `127.0.0.1:{port}`
- Listen for one GET request to `/callback`
- Extract `code` and `state` from query params
- Verify `state` matches `expected_state`
- Respond with HTML: "Authorization successful. You can close this tab."
- Return the `code`
- Shut down after receiving the callback or after timeout

Use `tokio::net::TcpListener` + minimal HTTP parsing, or `hyper` if already in deps. Keep it minimal — no full web framework.

### Open browser

Use Tauri's shell API to open the authorization URL:
```rust
tauri::api::shell::open(&app_handle.shell_scope(), &url, None)
```
Or use the `opener` plugin already in the project.

### State parameter

Generate a random state string (UUID or random hex). Verify it matches on callback to prevent CSRF.

## Frontend Changes

### AuthState — add fields

In `src/types/pane-types.ts`, add to `oauth2`:

```typescript
oauth2?: {
  grantType: 'client_credentials' | 'password' | 'authorization_code';
  authorizationUrl: string;  // NEW — only used for authorization_code
  clientId: string;
  clientSecret: string;
  tokenUrl: string;
  scope: string;
  accessToken: string;
  refreshToken: string;
};
```

### AuthEditor — show Authorization URL field

When `grantType === 'authorization_code'`, show an additional "Authorization URL" input field above Token URL.

### AuthEditor — enable Get Token for authorization_code

Remove line 76: `if (oauth.grantType === 'authorization_code') return;`

Replace with a call to the new Tauri command:

```typescript
if (oauth.grantType === 'authorization_code') {
  const token = await invoke('oauth2_auth_code_flow', {
    authorizationUrl: oauth.authorizationUrl,
    tokenUrl: oauth.tokenUrl,
    clientId: oauth.clientId,
    clientSecret: oauth.clientSecret,
    scope: oauth.scope || undefined,
  });
  patchOAuth2({
    accessToken: token.access_token,
    refreshToken: token.refresh_token || '',
  });
  return;
}
```

### AuthEditor — loading state

While waiting for the browser flow (can take 30+ seconds), show:
- "Get Token" button changes to "Waiting for authorization..." with a spinner
- Disable the button to prevent double-clicks
- Show a "Cancel" option that aborts the flow

### Redirect URI display

Show a read-only field: `Redirect URI: http://localhost:{port}/callback`

Since the port is determined by the Rust side at runtime, this can either:
- Be left blank until the flow starts
- Show a placeholder: "Assigned automatically when you click Get Token"

For simplicity, don't show it as an input. Just document that the redirect URI is auto-assigned.

### AuthEditor — remove "coming soon" tooltip

Line 362-364 in current code:
```tsx
auth.oauth2.grantType === 'authorization_code'
  ? 'Authorization code flow coming soon.'
```
Replace with standard tooltip or remove the conditional entirely.

## Security

- **PKCE required** — always use S256 code challenge. No plain challenge.
- **State verification** — random state prevents CSRF attacks.
- **Localhost only** — callback server binds to 127.0.0.1, not 0.0.0.0.
- **Auto-shutdown** — server shuts down after receiving one callback or after timeout.
- **Timeout** — 120 seconds max wait. If user doesn't complete consent, the flow fails gracefully.
- **No secrets in browser URL** — client_secret is only sent in the token exchange POST, never in the browser redirect.

## Error Handling

- User closes browser without consenting → timeout after 120s → show error
- Auth server returns error in callback (`?error=access_denied`) → show error message
- Token exchange fails → show error from auth server response
- Port already in use → try next port (retry 3 times)

## Dependencies

Check workspace for existing deps. May need to add:
- `sha2` — for PKCE SHA-256
- `base64` — for URL-safe base64 encoding
- `rand` — for random bytes (code_verifier + state)
- `tokio` — already present (for async TCP listener)

## Files

### Rust
- Modify: `crates/rocket-http/src/oauth2.rs` (add PKCE generation)
- Create: `crates/rocket-http/src/oauth2_callback.rs` (temporary HTTP server)
- Modify: `crates/rocket-http/src/lib.rs` (export new module)
- Create: `src-tauri/src/commands/oauth2.rs` (new Tauri command)
- Modify: `src-tauri/src/commands/mod.rs` (export new module)
- Modify: `src-tauri/src/lib.rs` (register command)

### Frontend
- Modify: `src/types/pane-types.ts` (add authorizationUrl to oauth2)
- Modify: `src/components/request/AuthEditor.tsx` (authorization URL field, enable Get Token, loading state)
- Modify: `src/lib/tauri-api.ts` (add invoke for oauth2_auth_code_flow)

## Out of scope

- Token refresh (auto-refresh before request execution) — separate feature
- Token expiry tracking — separate feature
- Password grant dedicated username/password fields — separate fix
- PKCE for other grant types — not applicable
