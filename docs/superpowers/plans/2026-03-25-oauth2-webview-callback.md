# OAuth2 Webview Callback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the localhost TCP callback server with a Tauri WebviewWindow that intercepts navigation to capture the OAuth2 authorization code.

**Architecture:** Open an in-app webview for the auth URL, intercept the redirect via `on_navigation`, extract code+state from the URL, exchange for a token. Removes all TCP/HTTP server code.

**Tech Stack:** Tauri v2 (WebviewWindowBuilder, on_navigation), Rust, tokio oneshot channel, existing PKCE + acquire_token.

**Spec:** `docs/superpowers/specs/2026-03-25-oauth2-webview-callback-design.md`

---

### Task 1: Remove oauth2_callback module from rocket-http

**Files:**
- Delete: `crates/rocket-http/src/oauth2_callback.rs`
- Modify: `crates/rocket-http/src/lib.rs`

- [ ] **Step 1: Remove the module declaration and re-exports from lib.rs**

Edit `crates/rocket-http/src/lib.rs` — remove the `oauth2_callback` module and its re-exports:

```rust
// REMOVE these two lines:
pub mod oauth2_callback;
pub use oauth2_callback::{wait_for_callback, CallbackResult};
```

The file should look like:

```rust
pub mod aws_sig;
pub mod pkce;
pub mod cookie;
pub mod cookie_repository;
pub mod executor;
pub mod oauth2;
pub mod request;
pub mod response;

pub use aws_sig::{sign_request, AwsCredentials, SignedHeaders};
pub use cookie::{Cookie, CookieJar};
pub use cookie_repository::CookieRepository;
pub use executor::HttpExecutor;
pub use oauth2::{acquire_token, OAuthConfig, OAuthToken};
pub use request::{HttpRequest, RequestOptions};
pub use response::HttpResponse;
pub use pkce::{generate_pkce, PkcePair};
```

- [ ] **Step 2: Delete the callback server file**

```bash
rm crates/rocket-http/src/oauth2_callback.rs
```

- [ ] **Step 3: Verify rocket-http compiles**

Run: `cargo check --lib -p rocket-http`
Expected: compiles with no errors (may have warnings about the Tauri crate, that's fine)

- [ ] **Step 4: Run rocket-http tests**

Run: `cargo test -p rocket-http`
Expected: All existing tests pass (pkce, oauth2 tests). No oauth2_callback tests remain.

- [ ] **Step 5: Commit**

```bash
git add -A crates/rocket-http/
git commit -m "refactor: remove localhost TCP callback server from rocket-http"
```

---

### Task 2: Rewrite the Tauri OAuth2 command with webview approach

**Files:**
- Rewrite: `src-tauri/src/commands/oauth2.rs`

- [ ] **Step 1: Rewrite oauth2.rs with the webview implementation**

Replace the entire contents of `src-tauri/src/commands/oauth2.rs` with:

```rust
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Duration;

use rocket_http::{acquire_token, generate_pkce, OAuthConfig, OAuthToken, PkcePair};
use rocket_shared::error::DomainError;
use tauri::AppHandle;

/// Result extracted from the OAuth2 callback URL.
struct AuthCodeResult {
    code: String,
    state: String,
}

/// Runs the full OAuth2 Authorization Code flow with PKCE.
///
/// 1. Generates PKCE code_verifier + code_challenge.
/// 2. Opens an in-app webview to the authorization URL.
/// 3. Intercepts the redirect via on_navigation.
/// 4. Exchanges authorization code for access token.
#[tauri::command]
pub async fn oauth2_auth_code_flow(
    app: AppHandle,
    authorization_url: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    scope: Option<String>,
    callback_url: Option<String>,
) -> Result<OAuthToken, DomainError> {
    let pkce = generate_pkce();
    let state = uuid::Uuid::new_v4().to_string();
    let redirect_uri = callback_url
        .unwrap_or_else(|| "https://exchange4all.local/webapp/#oidc-callback".into());

    let auth_url = build_auth_url(
        &authorization_url,
        &client_id,
        &redirect_uri,
        &pkce,
        &state,
        &scope,
    );

    // Channel for the navigation callback to send the result.
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<AuthCodeResult, String>>();
    let tx = Mutex::new(Some(tx));
    let redirect_prefix = redirect_uri_prefix(&redirect_uri);

    // Close any existing auth window from a previous attempt.
    if let Some(existing) = app.get_webview_window("oauth2-auth") {
        let _ = existing.close();
    }

    // Open webview window.
    // on_navigation must be Fn (not FnOnce), hence Mutex<Option<Sender>>.
    let window = tauri::WebviewWindowBuilder::new(
        &app,
        "oauth2-auth",
        tauri::WebviewUrl::External(
            auth_url
                .parse()
                .map_err(|e| DomainError::Internal(format!("Invalid auth URL: {e}")))?,
        ),
    )
    .title("Sign In")
    .inner_size(500.0, 700.0)
    .on_navigation(move |url| {
        if url.as_str().starts_with(&redirect_prefix) && has_auth_params(url) {
            if let Some(tx) = tx.lock().unwrap().take() {
                let _ = tx.send(extract_code_or_error(url));
            }
            return false;
        }
        true
    })
    .build()
    .map_err(|e| DomainError::Internal(format!("Failed to open auth window: {e}")))?;

    // Wait for the callback with a 120s timeout.
    let result = tokio::time::timeout(Duration::from_secs(120), rx).await;
    let _ = window.close();

    let auth_result = match result {
        Ok(Ok(Ok(r))) => r,
        Ok(Ok(Err(err))) => {
            return Err(DomainError::Internal(format!("Authorization denied: {err}")))
        }
        Ok(Err(_)) => {
            return Err(DomainError::Internal(
                "Authorization window was closed before completing sign-in.".into(),
            ))
        }
        Err(_) => {
            return Err(DomainError::Internal(
                "Authorization timed out. Please try again.".into(),
            ))
        }
    };

    // Verify CSRF state before exchanging the code.
    if auth_result.state != state {
        return Err(DomainError::Internal(
            "State mismatch — possible CSRF attack.".into(),
        ));
    }

    // Exchange the authorization code for an access token.
    let client = reqwest::Client::new();
    let config = OAuthConfig {
        grant_type: "authorization_code".into(),
        client_id,
        client_secret,
        token_url,
        scope,
        username: None,
        password: None,
        code: Some(auth_result.code),
        redirect_uri: Some(redirect_uri),
        code_verifier: Some(pkce.code_verifier),
    };
    acquire_token(&config, &client).await
}

/// Builds the full authorization URL with all required query parameters.
fn build_auth_url(
    authorization_url: &str,
    client_id: &str,
    redirect_uri: &str,
    pkce: &PkcePair,
    state: &str,
    scope: &Option<String>,
) -> String {
    let sep = if authorization_url.contains('?') {
        "&"
    } else {
        "?"
    };
    let scope_param = scope
        .as_ref()
        .map(|s| format!("&scope={}", urlencoding_encode(s)))
        .unwrap_or_default();
    format!(
        "{}{sep}response_type=code&client_id={}&redirect_uri={}&code_challenge={}&code_challenge_method=S256&state={}{scope_param}",
        authorization_url,
        urlencoding_encode(client_id),
        urlencoding_encode(redirect_uri),
        urlencoding_encode(&pkce.code_challenge),
        urlencoding_encode(state),
    )
}

/// Strips the fragment from a URL so starts_with matching works.
/// "https://example.com/cb#frag" → "https://example.com/cb"
fn redirect_uri_prefix(redirect_uri: &str) -> String {
    redirect_uri
        .split('#')
        .next()
        .unwrap_or(redirect_uri)
        .to_string()
}

/// Checks that the URL has OAuth2 callback params (code or error).
fn has_auth_params(url: &url::Url) -> bool {
    url.query_pairs()
        .any(|(k, _)| k == "code" || k == "error")
}

/// Extracts code+state or an error description from the callback URL.
fn extract_code_or_error(url: &url::Url) -> Result<AuthCodeResult, String> {
    let params: HashMap<String, String> = url
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if let Some(error) = params.get("error") {
        let desc = params.get("error_description").unwrap_or(error);
        return Err(desc.clone());
    }
    let code = params
        .get("code")
        .cloned()
        .ok_or_else(|| "No authorization code in callback.".to_string())?;
    let state = params
        .get("state")
        .cloned()
        .ok_or_else(|| "Auth provider did not return a state parameter.".to_string())?;
    Ok(AuthCodeResult { code, state })
}

/// Percent-encodes a string for use as a URL query parameter value.
fn urlencoding_encode(s: &str) -> String {
    let mut result = String::with_capacity(s.len() * 3);
    for byte in s.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                result.push(byte as char);
            }
            _ => {
                result.push_str(&format!("%{byte:02X}"));
            }
        }
    }
    result
}
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo check -p rocket`
Expected: Compiles with no errors. The `url` crate is already a dependency (`url = "2"` in `src-tauri/Cargo.toml`). The `tauri-plugin-opener` import is gone but the plugin is still registered in `lib.rs` for other uses.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/commands/oauth2.rs
git commit -m "feat: replace TCP callback with WebviewWindow navigation interception"
```

---

### Task 3: Update frontend default callback URL

**Files:**
- Modify: `src/components/request/AuthEditor.tsx:58`
- Modify: `src/lib/pane-utils.ts:36`

- [ ] **Step 1: Update the default in AuthEditor.tsx**

In `src/components/request/AuthEditor.tsx`, change line 58:

Old:
```typescript
          callbackUrl: 'http://localhost:9876/callback',
```
New:
```typescript
          callbackUrl: 'https://exchange4all.local/webapp/#oidc-callback',
```

- [ ] **Step 2: Update the default in pane-utils.ts**

In `src/lib/pane-utils.ts`, change line 36:

Old:
```typescript
          callbackUrl: (a.callbackUrl as string) ?? 'http://localhost:9876/callback',
```
New:
```typescript
          callbackUrl: (a.callbackUrl as string) ?? 'https://exchange4all.local/webapp/#oidc-callback',
```

- [ ] **Step 3: Verify the frontend builds**

Run: `yarn build` (or `npm run build` if npm is used)
Expected: Builds with no TypeScript errors. No other files reference the old localhost URL.

- [ ] **Step 4: Verify no stale references to old callback URL**

Run: `grep -r "localhost:9876" src/`
Expected: No matches. Both defaults have been updated.

- [ ] **Step 5: Commit**

```bash
git add src/components/request/AuthEditor.tsx src/lib/pane-utils.ts
git commit -m "feat: update default OAuth2 callback URL to exchange4all.local"
```

---

### Task 4: Remove unused dependencies from oauth2 command

**Files:**
- Check: `src-tauri/Cargo.toml`
- Check: `src-tauri/src/commands/oauth2.rs` (already done in task 2)

- [ ] **Step 1: Verify no unused imports**

The rewritten `oauth2.rs` no longer imports `tauri_plugin_opener::OpenerExt` or `wait_for_callback`. Confirm this is the case by checking the file header. It should NOT contain:

```rust
use tauri_plugin_opener::OpenerExt; // REMOVED
use rocket_http::wait_for_callback;  // REMOVED
```

- [ ] **Step 2: Full project compile check**

Run: `cargo check -p rocket`
Expected: Clean compile, no warnings about unused imports.

- [ ] **Step 3: Run all Rust tests**

Run: `cargo test -p rocket-http`
Expected: All tests pass (pkce and oauth2 tests).

- [ ] **Step 4: Commit (if any cleanup needed)**

Only commit if Step 1-2 revealed additional cleanup. Otherwise skip.

---

### Task 5: Manual integration test

This task cannot be automated — it requires a real OAuth2 provider.

- [ ] **Step 1: Build the app**

Run: `cargo tauri dev`

- [ ] **Step 2: Test the auth code flow**

1. Create a new request, select Auth → OAuth2
2. Set Grant Type to "Authorization Code"
3. Fill in Authorization URL, Token URL, Client ID, Client Secret for a real provider (e.g., Google, GitHub, or your own)
4. Verify the Callback URL field shows `https://exchange4all.local/webapp/#oidc-callback`
5. Click "Get Token"
6. A new "Sign In" window should open inside the app
7. Log in and consent
8. The window should close automatically
9. The Access Token field should populate with the token
10. No "Authorization timed out" error

- [ ] **Step 3: Test error cases**

1. Click "Get Token", then close the Sign In window manually → should see "Authorization window was closed before completing sign-in."
2. Click "Get Token" twice quickly → first window should close, second should open cleanly

- [ ] **Step 4: Final commit with any fixes**

If any fixes were needed during testing, commit them. Otherwise:

```bash
git log --oneline -5
```

Expected commits (newest first):
- `feat: update default OAuth2 callback URL to exchange4all.local`
- `feat: replace TCP callback with WebviewWindow navigation interception`
- `refactor: remove localhost TCP callback server from rocket-http`
