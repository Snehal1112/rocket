# OAuth2 Force Re-Authentication on Clear Cache

**Date:** 2026-04-23  
**Status:** Approved

## Problem

When a user clicks "Get Access Token" on the `authorization_code` grant, the authorization server opens in a browser window or Tauri webview. If the server has an existing session (SSO cookie), it silently re-authenticates the same user without showing the login form. There is no mechanism to force the login dialog to appear, so users cannot switch accounts after clearing the local token cache.

## Solution

When the user clicks "Clear Cache", set a one-shot `forceReauth` flag in the OAuth2 UI state. The next "Get Access Token" call uses that flag to:

1. Append `prompt=login` to the authorization URL — works for all OIDC-compliant providers (Google, Azure AD, Keycloak, Auth0, Okta).
2. Clear the embedded webview's session data before opening — covers non-OIDC OAuth2 providers when using the in-app webview.

The flag is automatically cleared after the token is successfully acquired (or on failure), so subsequent clicks behave normally.

## Scope

- Applies only to `grantType === 'authorization_code'`. Password and client-credentials grants are unaffected.
- `forceReauth` is **not persisted** to disk — it is ephemeral UI state only.
- No new UI controls are added. The existing Clear Cache button is the sole trigger.

## Changes

### 1. Frontend type — `src/types/pane-types.ts`

Add `forceReauth?: boolean` to the `oauth2` object inside `AuthState`. This field is UI-only and never written to the collection file.

### 2. Frontend API — `src/lib/tauri-api.ts`

Add `forceReauth?: boolean` to `OAuth2GetTokenRequest`.

### 3. `OAuth2AuthEditor.tsx`

**`handleClearCache`:** Add `forceReauth: true` to the existing patch object (alongside the token field clearing).

**`handleGetToken`:** When `o.forceReauth` is true and `o.grantType === 'authorization_code'`:
- Merge a one-shot `{ key: 'prompt', value: 'login', send_in: 'query', enabled: true }` entry into the `authParams` passed to `oauth2GetToken`. This is built inline and never written back to `o.authParams`.
- Pass `forceReauth: true` in the request.
- After success or failure, call `patchOAuth2Ref.current({ forceReauth: false })` to reset the flag.

### 4. Rust — `OAuth2GetTokenRequest` in `crates/rocket-app/src/oauth2_service.rs`

Add `pub force_reauth: Option<bool>` field (camelCase on the wire: `forceReauth`).

### 5. Rust — `oauth2_get_token` command in `src-tauri/src/commands/oauth2.rs`

In the `"authorization_code"` branch, pass `force_reauth` down to `auth_code_flow`.

### 6. Rust — `auth_code_flow` in `src-tauri/src/commands/oauth2.rs`

Accept a `force_reauth: bool` parameter. When true and `use_system_browser` is false (webview path), clear the webview's stored data before opening by closing any existing `"oauth2-auth"` window and calling the appropriate Tauri/WebKit data-clearing API. The `prompt=login` param is already injected via `auth_params` from the frontend, so no further URL manipulation is needed in Rust.

**Webview cookie clearing (in-app webview only):**
- Close any existing `"oauth2-auth"` window (already done today).
- On Linux (WebKit2GTK): call `website_data_manager.clear()` for cookies and session storage before navigating.
- On macOS/Windows: the system browser is used by default — cookie clearing is not possible and `prompt=login` is the only mechanism available. No action needed in Rust for this path.

### 7. Rust — `ResolvedOAuth2Config` in `crates/rocket-app/src/oauth2_service.rs`

Add `pub force_reauth: bool` so the resolved config carries the flag through to `auth_code_flow`.

### 8. Rust — `resolve_get_token_request` in `crates/rocket-app/src/oauth2_service.rs`

Map `req.force_reauth.unwrap_or(false)` into `ResolvedOAuth2Config::force_reauth`.

## Data Flow

```
Clear Cache clicked
  → patchOAuth2({ ...token fields cleared..., forceReauth: true })

Get Access Token clicked (authorization_code, forceReauth=true)
  → build authParams = [...o.authParams, { key:'prompt', value:'login', send_in:'query', enabled:true }]
  → oauth2GetToken({ ..., authParams, forceReauth: true })
      → Rust: oauth2_get_token resolves config (force_reauth=true)
          → auth_code_flow(force_reauth=true)
              → [webview path] clear webview session data
              → build_auth_url_v2(...) → apply_params_to_url (appends prompt=login)
              → open webview/browser → user sees login form
              → code exchanged → OAuthToken returned
  → patchOAuth2({ accessToken, ..., forceReauth: false })
```

## What Does Not Change

- The stored `authParams` list in `OAuth2State` — `prompt=login` is injected transiently, never persisted.
- Password and client-credentials grant flows — completely unaffected.
- The authorization_code flow when `forceReauth` is false — identical behavior to today.
- `autoFetchToken` behavior — Clear Cache already sets it to false; that is unchanged.
