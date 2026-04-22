# OAuth2 Force Re-Authentication Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the user clicks "Clear Cache" on the authorization_code OAuth2 flow, set a one-shot `forceReauth` flag so the next "Get Access Token" call appends `prompt=login` to the auth URL and (on Linux webview) clears the webview session — forcing the authorization server to show its login dialog.

**Architecture:** The flag lives exclusively in frontend UI state (`OAuth2State.forceReauth`), is passed transiently to the Rust `oauth2_get_token` command, flows through `ResolvedOAuth2Config`, and is cleared after every token acquisition attempt. No persistence to disk. The `prompt=login` param is injected by the frontend into `authParams` for the single request — it never touches the stored `o.authParams`.

**Tech Stack:** React/TypeScript (frontend state + Tauri IPC), Rust/Tauri (command layer), webkit2gtk (Linux webview cookie clearing).

---

### Task 1: Add `forceReauth` to frontend types

**Files:**
- Modify: `src/types/pane-types.ts`
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Add `forceReauth` to `OAuth2State` in `pane-types.ts`**

In `src/types/pane-types.ts`, inside the `oauth2?: { ... }` block, after the `accessTokenClaims` line (line ~230), add:

```typescript
    // Ephemeral — NOT persisted. Set by Clear Cache, consumed by Get Access Token.
    forceReauth?: boolean;
```

The full block ending should look like:
```typescript
    idTokenClaims: OAuth2JwtClaims | null;
    accessTokenClaims: OAuth2JwtClaims | null;
    // Ephemeral — NOT persisted. Set by Clear Cache, consumed by Get Access Token.
    forceReauth?: boolean;
  };
```

- [ ] **Step 2: Add `forceReauth` to `OAuth2GetTokenRequest` in `tauri-api.ts`**

In `src/lib/tauri-api.ts`, inside `export interface OAuth2GetTokenRequest { ... }`, after the `requestPath?: string;` line (~line 712), add:

```typescript
  forceReauth?: boolean;
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/types/pane-types.ts src/lib/tauri-api.ts
git commit -m "feat(oauth2): add forceReauth field to OAuth2 frontend types"
```

---

### Task 2: Wire `forceReauth` into `handleClearCache` and `handleGetToken`

**Files:**
- Modify: `src/components/request/oauth2/OAuth2AuthEditor.tsx`

- [ ] **Step 1: Update `handleClearCache` to set `forceReauth: true`**

In `src/components/request/oauth2/OAuth2AuthEditor.tsx`, find `handleClearCache` (around line 178). The patch object currently ends with `autoFetchToken: false`. Add `forceReauth: true` after it:

```typescript
  const handleClearCache = useCallback(() => {
    patchOAuth2Ref.current({
      accessToken: '',
      refreshToken: '',
      expiresIn: null,
      tokenAcquiredAt: null,
      idToken: '',
      tokenType: '',
      responseScope: '',
      idTokenClaims: null,
      accessTokenClaims: null,
      autoFetchToken: false,
      forceReauth: true,
    });
    setTokenError('');
  }, []);
```

- [ ] **Step 2: Update `handleGetToken` to inject `prompt=login` and pass `forceReauth`**

Find `handleGetToken` (around line 65). Replace the entire function body with:

```typescript
  const handleGetToken = useCallback(async () => {
    setGettingToken(true);
    setTokenError('');
    const isForceReauth = o.forceReauth && o.grantType === 'authorization_code';
    // Build a one-shot authParams list — adds prompt=login when forcing re-auth.
    // Never written back to o.authParams.
    const authParamsForRequest = isForceReauth
      ? [
          ...o.authParams,
          { key: 'prompt', value: 'login', sendIn: 'query' as const, enabled: true },
        ]
      : o.authParams.length
        ? o.authParams
        : undefined;
    try {
      const result = await oauth2GetToken({
        grantType: o.grantType,
        authorizationUrl: o.authorizationUrl || undefined,
        tokenUrl: o.tokenUrl || undefined,
        callbackUrl: o.callbackUrl || undefined,
        clientId: o.clientId,
        clientSecret: o.clientSecret || undefined,
        scope: o.scope || undefined,
        state: o.state || undefined,
        username: o.username || undefined,
        password: o.password || undefined,
        clientAuthentication: o.clientAuthentication,
        usePkce: o.usePkce,
        useSystemBrowser: o.useSystemBrowser,
        verifySsl: o.verifySsl,
        authParams: authParamsForRequest,
        tokenParams: o.tokenParams.length ? o.tokenParams : undefined,
        refreshParams: o.refreshParams.length ? o.refreshParams : undefined,
        collection,
        environmentName,
        requestPath,
        forceReauth: isForceReauth || undefined,
      });
      patchOAuth2Ref.current({
        accessToken: result.access_token,
        refreshToken: result.refresh_token || '',
        expiresIn: typeof result.expires_in === 'number' ? result.expires_in : null,
        tokenAcquiredAt: Math.floor(Date.now() / 1000),
        idToken: result.id_token || '',
        tokenType: result.token_type || '',
        responseScope: result.scope || '',
        idTokenClaims: null,
        accessTokenClaims: null,
        forceReauth: false,
      });
      setTimeout(
        () => tokenDisplayRef.current?.scrollIntoView({ behavior: 'smooth', block: 'nearest' }),
        0,
      );
      try {
        const claims = await oauth2DecodeJwt(result.access_token);
        patchOAuth2Ref.current({ accessTokenClaims: claims });
      } catch {
        // Opaque access tokens are fine — decode is best-effort.
      }
      if (result.id_token) {
        try {
          const claims = await oauth2DecodeJwt(result.id_token);
          patchOAuth2Ref.current({ idTokenClaims: claims });
        } catch {
          // JWT decode is best-effort.
        }
      }
    } catch (err) {
      // Always clear the flag even on failure so the user can retry normally.
      patchOAuth2Ref.current({ forceReauth: false });
      setTokenError(err instanceof Error ? err.message : String(err));
    } finally {
      setGettingToken(false);
    }
  }, [o, collection, environmentName, requestPath]);
```

- [ ] **Step 3: Verify TypeScript compiles**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/request/oauth2/OAuth2AuthEditor.tsx
git commit -m "feat(oauth2): inject prompt=login and forceReauth on Clear Cache for auth code flow"
```

---

### Task 3: Add `force_reauth` to Rust `OAuth2GetTokenRequest` and `ResolvedOAuth2Config`

**Files:**
- Modify: `crates/rocket-app/src/oauth2_service.rs`

- [ ] **Step 1: Add `force_reauth` to `OAuth2GetTokenRequest`**

In `crates/rocket-app/src/oauth2_service.rs`, inside `pub struct OAuth2GetTokenRequest { ... }`, after the `pub request_path: Option<String>,` line (~line 50), add:

```rust
    pub force_reauth: Option<bool>,
```

The end of the struct should look like:
```rust
    // Variable resolution context
    pub collection: Option<String>,
    pub environment_name: Option<String>,
    pub request_path: Option<String>,
    pub force_reauth: Option<bool>,
}
```

- [ ] **Step 2: Add `force_reauth` to `ResolvedOAuth2Config`**

In the same file, inside `pub struct ResolvedOAuth2Config { ... }`, after the `pub refresh_params: Vec<AdditionalParam>,` line (~line 96), add:

```rust
    pub force_reauth: bool,
```

- [ ] **Step 3: Map `force_reauth` in `resolve_get_token_request`**

In `resolve_get_token_request` (around line 418), add `force_reauth` to the `ResolvedOAuth2Config { ... }` initialiser. The block currently ends with `refresh_params: resolve_params(&req.refresh_params),`. Add after it:

```rust
            force_reauth: req.force_reauth.unwrap_or(false),
```

- [ ] **Step 4: Run Rust tests**

```bash
cargo test -p rocket-app
```

Expected: all existing tests pass (the new field has a default so nothing breaks).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/oauth2_service.rs
git commit -m "feat(oauth2): add force_reauth field to OAuth2GetTokenRequest and ResolvedOAuth2Config"
```

---

### Task 4: Thread `force_reauth` through the Tauri command and clear webview session on Linux

**Files:**
- Modify: `src-tauri/src/commands/oauth2.rs`

- [ ] **Step 1: Pass `force_reauth` from `oauth2_get_token` into `auth_code_flow`**

In `src-tauri/src/commands/oauth2.rs`, find the `match config.grant_type.as_str()` block inside `oauth2_get_token` (~line 281). Change the `"authorization_code"` arm from:

```rust
        "authorization_code" => auth_code_flow(&app, &svc, &config).await,
```

to:

```rust
        "authorization_code" => auth_code_flow(&app, &svc, &config, config.force_reauth).await,
```

- [ ] **Step 2: Update `auth_code_flow` signature to accept `force_reauth`**

Find `async fn auth_code_flow(` (~line 292). Change its signature from:

```rust
async fn auth_code_flow(
    app: &AppHandle,
    svc: &OAuth2Service,
    config: &ResolvedOAuth2Config,
) -> Result<OAuthToken, DomainError> {
```

to:

```rust
async fn auth_code_flow(
    app: &AppHandle,
    svc: &OAuth2Service,
    config: &ResolvedOAuth2Config,
    force_reauth: bool,
) -> Result<OAuthToken, DomainError> {
```

- [ ] **Step 3: Clear webview session data on Linux when `force_reauth` is true**

In `auth_code_flow`, find the `let use_system_browser = config.use_system_browser;` line (~line 323). Immediately after it, insert the webview session clearing block:

```rust
    // When forcing re-authentication via the in-app webview, clear the
    // WebKit session (cookies, storage) so the auth server cannot reuse
    // an existing SSO session. On macOS/Windows the system browser is used
    // by default — prompt=login (already in auth_params) is the only lever.
    #[cfg(target_os = "linux")]
    if force_reauth && !use_system_browser {
        if let Some(existing) = app.get_webview_window("oauth2-auth") {
            let _ = existing.close();
        }
        // Build a temporary webview so we can access its WebsiteDataManager.
        // We immediately close it after clearing — it never navigates anywhere.
        if let Ok(tmp) = tauri::WebviewWindowBuilder::new(
            app,
            "oauth2-auth-clear",
            tauri::WebviewUrl::External("about:blank".parse().unwrap()),
        )
        .visible(false)
        .build()
        {
            let cleared = tmp.with_webview(|webview| {
                use webkit2gtk::{WebViewExt, WebsiteDataManagerExt, WebsiteDataTypes};
                let wv = webview.inner();
                if let Some(dm) = wv.website_data_manager() {
                    // Clear all cookies and session storage (no async callback needed
                    // for the types that support synchronous removal).
                    dm.clear(
                        WebsiteDataTypes::COOKIES
                            | WebsiteDataTypes::SESSION_STORAGE
                            | WebsiteDataTypes::LOCAL_STORAGE,
                        glib::TimeSpan::from_seconds(0),
                        None::<&gio::Cancellable>,
                        |_| {},
                    );
                }
            });
            let _ = cleared;
            let _ = tmp.close();
        }
    }
    #[cfg(not(target_os = "linux"))]
    let _ = force_reauth; // prompt=login in auth_params covers macOS/Windows.
```

- [ ] **Step 4: Fast-validate Rust compiles (macOS — Linux block is cfg-gated)**

```bash
cargo check -p rocket-app
cargo check --package rocket-app
```

Also check the tauri crate compiles:
```bash
cargo check --manifest-path src-tauri/Cargo.toml
```

Expected: no errors. The `#[cfg(target_os = "linux")]` block is not compiled on macOS so webkit2gtk-specific types are not evaluated.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/oauth2.rs
git commit -m "feat(oauth2): thread force_reauth through auth_code_flow; clear webview session on Linux"
```

---

### Task 5: Manual verification checklist

This feature requires a live OAuth2 provider to verify end-to-end. The steps below are a manual testing script.

- [ ] **Step 1: Verify normal flow is unchanged**

1. Open a request with `authorization_code` grant configured.
2. Click "Get Access Token" — browser/webview opens, complete login.
3. Token appears in the UI. Click "Get Access Token" again without clearing cache.
4. Expected: provider silently re-uses session — no login form shown, token refreshed.

- [ ] **Step 2: Verify Clear Cache sets the flag (dev tools)**

1. Open browser devtools on the Tauri webview (or add a `console.log` temporarily).
2. Click "Clear Cache".
3. Expected: `forceReauth` is `true` in the oauth2 state patch.

- [ ] **Step 3: Verify `prompt=login` appears in the authorization URL**

1. After clicking Clear Cache, click "Get Access Token".
2. Watch the URL opened in the browser/webview.
3. Expected: URL contains `prompt=login` as a query parameter.

- [ ] **Step 4: Verify the auth server shows the login form**

1. After clicking Clear Cache + Get Access Token, complete login as a different user.
2. Expected: new token is issued for the new user; `forceReauth` is reset to `false`.

- [ ] **Step 5: Verify `forceReauth` is cleared on failure**

1. After clicking Clear Cache, click "Get Access Token" but close/cancel the auth window.
2. Expected: error message shown; clicking "Get Access Token" again does NOT inject `prompt=login` (flag was cleared on failure).

- [ ] **Step 6: Verify password and client_credentials grants unaffected**

1. Switch to `client_credentials` grant, click "Clear Cache", then "Get Access Token".
2. Expected: no `prompt=login` in any request; flow completes normally.

- [ ] **Step 7: Commit spec and plan**

```bash
git add docs/superpowers/specs/2026-04-23-oauth2-force-reauth-design.md \
        docs/superpowers/plans/2026-04-23-oauth2-force-reauth.md
git commit -m "docs: add OAuth2 force re-auth spec and implementation plan"
```
