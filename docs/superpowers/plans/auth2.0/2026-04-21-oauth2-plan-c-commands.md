# OAuth2 Rust Foundation — Plan C: Tauri Commands

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add three new Tauri commands (`oauth2_get_token`, `oauth2_refresh_token`, `oauth2_decode_jwt`) that expose the OAuth2Service and JWT decoding to the frontend. Add system browser + TCP callback as an alternative to the webview flow.

**Architecture:** Tauri commands wrap `OAuth2Service` methods. `oauth2_get_token` dispatches to the service for client_credentials/password, and handles browser/webview orchestration for authorization_code/implicit (since these need `AppHandle`). System browser flow re-implements TCP callback locally in the command module. Existing `oauth2_auth_code_flow` becomes a private helper.

**Tech Stack:** Rust, Tauri v2 (AppHandle, WebviewWindowBuilder, opener plugin), tokio, rocket-app, rocket-http

**Spec:** `docs/superpowers/specs/2026-04-21-oauth2-rust-foundation-design.md`

**Prerequisite:** Plan A + Plan B complete.

---

## File Map

| File | Action | Purpose |
|---|---|---|
| `src-tauri/src/commands/oauth2.rs` | Modify | Add oauth2_get_token, oauth2_refresh_token, oauth2_decode_jwt; refactor existing code |
| `src-tauri/src/lib.rs` | Modify | Register new commands, add OAuth2Service to managed state |

---

### Task 1: Add `oauth2_decode_jwt` command and register OAuth2Service state

**Files:**
- Modify: `src-tauri/src/commands/oauth2.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Read current `src-tauri/src/lib.rs`**

Understand how services are registered as managed state. Look for how `RequestExecutionService` is created and added to the Tauri app builder.

```bash
grep -n "manage\|RequestExecutionService\|State" src-tauri/src/lib.rs | head -20
```

- [ ] **Step 2: Add OAuth2Service to managed state in lib.rs**

Find where `RequestExecutionService` is created (likely in the `run()` function or similar setup). Create `OAuth2Service` using the same `env_repo` and `collection_repo` instances (or clones). Add it to Tauri's managed state:

```rust
use rocket_app::oauth2_service::OAuth2Service;

// After creating the env_repo and collection_repo:
let oauth2_svc = OAuth2Service::new(
    // Use the same repo instances or create new ones pointed at the same paths
    Box::new(/* FsEnvironmentRepo */),
    Box::new(/* FsCollectionRepo */),
);

// In the app builder:
.manage(oauth2_svc)
```

Note: Read the actual construction pattern in `lib.rs` to match the existing style. The repos may need `Arc` wrapping or separate instances.

- [ ] **Step 3: Add `oauth2_decode_jwt` command**

In `src-tauri/src/commands/oauth2.rs`, add at the top:

```rust
use rocket_http::{decode_jwt, JwtClaims};
```

Add the command:

```rust
/// Decodes a JWT token without signature verification.
/// Used for displaying ID token metadata in the UI.
#[tauri::command]
pub fn oauth2_decode_jwt(token: String) -> Result<JwtClaims, DomainError> {
    decode_jwt(&token)
}
```

- [ ] **Step 4: Register the new command**

In `src-tauri/src/lib.rs`, find the `generate_handler![]` macro call and add:

```rust
commands::oauth2::oauth2_decode_jwt,
```

- [ ] **Step 5: Verify compilation**

```bash
cargo check --workspace
```

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/commands/oauth2.rs src-tauri/src/lib.rs
git commit -m "feat: add oauth2_decode_jwt command and OAuth2Service managed state"
```

---

### Task 2: Add `oauth2_get_token` and `oauth2_refresh_token` commands

**Files:**
- Modify: `src-tauri/src/commands/oauth2.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add imports**

At the top of `src-tauri/src/commands/oauth2.rs`, add:

```rust
use rocket_app::oauth2_service::{OAuth2GetTokenRequest, OAuth2RefreshRequest, OAuth2Service};
use tauri::State;
```

- [ ] **Step 2: Add `oauth2_refresh_token` command**

This is the simpler one — no browser orchestration:

```rust
/// Refreshes an OAuth2 token using a refresh token.
#[tauri::command]
pub async fn oauth2_refresh_token(
    svc: State<'_, OAuth2Service>,
    request: OAuth2RefreshRequest,
) -> Result<OAuthToken, DomainError> {
    svc.refresh_token(&request).await
}
```

- [ ] **Step 3: Add `oauth2_get_token` command**

This dispatches based on grant type:

```rust
/// Unified OAuth2 token acquisition for all grant types.
///
/// - client_credentials / password: delegates directly to OAuth2Service.
/// - authorization_code: opens webview or system browser, gets code, exchanges via service.
/// - implicit: opens webview or system browser, extracts token from URL fragment.
#[tauri::command]
pub async fn oauth2_get_token(
    app: AppHandle,
    svc: State<'_, OAuth2Service>,
    request: OAuth2GetTokenRequest,
) -> Result<OAuthToken, DomainError> {
    let config = svc.resolve_get_token_request(&request);

    match config.grant_type.as_str() {
        "client_credentials" | "password" => svc.get_token_direct(&config).await,
        "authorization_code" => auth_code_flow(&app, &svc, &config).await,
        "implicit" => implicit_flow(&app, &config).await,
        other => Err(DomainError::InvalidInput(format!(
            "Unsupported grant type: {other}"
        ))),
    }
}
```

- [ ] **Step 4: Implement `auth_code_flow` private function**

Refactor the existing `oauth2_auth_code_flow` logic into a private function that uses `ResolvedOAuth2Config`:

```rust
use rocket_http::{generate_pkce, PkcePair};

/// Authorization code flow: webview or system browser → code → exchange.
async fn auth_code_flow(
    app: &AppHandle,
    svc: &OAuth2Service,
    config: &rocket_app::oauth2_service::ResolvedOAuth2Config,
) -> Result<OAuthToken, DomainError> {
    // Generate PKCE if enabled.
    let pkce: Option<PkcePair> = if config.use_pkce {
        Some(generate_pkce())
    } else {
        None
    };

    // Generate state (use provided or auto-generate).
    let state = config
        .state
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    // Build authorization URL.
    let mut auth_url = build_auth_url_v2(
        &config.authorization_url,
        &config.client_id,
        &config.callback_url,
        pkce.as_ref(),
        &state,
        &config.scope,
    );

    // Apply additional auth params to the URL.
    auth_url = apply_params_to_url(&auth_url, &config.auth_params);

    // Get authorization code via webview or system browser.
    let (code, actual_redirect_uri) = if config.use_system_browser {
        auth_code_via_system_browser(app, &auth_url, &state).await?
    } else {
        let code = auth_code_via_webview(app, &auth_url, &config.callback_url, &state, config.verify_ssl).await?;
        (code, config.callback_url.clone())
    };

    // Exchange code for token.
    svc.exchange_code_for_token(
        config,
        &code,
        &actual_redirect_uri,
        pkce.as_ref().map(|p| p.code_verifier.as_str()),
    )
    .await
}
```

- [ ] **Step 5: Add `build_auth_url_v2` (PKCE-optional version)**

Replace or complement the existing `build_auth_url`:

```rust
/// Builds the authorization URL. PKCE params are only added if a PkcePair is provided.
fn build_auth_url_v2(
    authorization_url: &str,
    client_id: &str,
    redirect_uri: &str,
    pkce: Option<&PkcePair>,
    state: &str,
    scope: &Option<String>,
) -> String {
    let sep = if authorization_url.contains('?') { "&" } else { "?" };
    let mut url = format!(
        "{}{sep}response_type=code&client_id={}&redirect_uri={}&state={}",
        authorization_url,
        urlencoding_encode(client_id),
        urlencoding_encode(redirect_uri),
        urlencoding_encode(state),
    );
    if let Some(pkce) = pkce {
        url.push_str(&format!(
            "&code_challenge={}&code_challenge_method=S256",
            urlencoding_encode(&pkce.code_challenge),
        ));
    }
    if let Some(scope) = scope {
        url.push_str(&format!("&scope={}", urlencoding_encode(scope)));
    }
    url
}
```

- [ ] **Step 6: Add `auth_code_via_webview` refactored from existing code**

Extract the webview logic from the current `oauth2_auth_code_flow` into:

```rust
/// Opens a Tauri webview, intercepts the redirect, returns the authorization code.
async fn auth_code_via_webview(
    app: &AppHandle,
    auth_url: &str,
    callback_url: &str,
    expected_state: &str,
    verify_ssl: bool,
) -> Result<String, DomainError> {
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<AuthCodeResult, String>>();
    let tx = Mutex::new(Some(tx));
    let redirect_prefix = redirect_uri_prefix(callback_url);
    let expected_state_owned = expected_state.to_string();

    if let Some(existing) = app.get_webview_window("oauth2-auth") {
        let _ = existing.close();
    }

    let parsed_auth_url: url::Url = auth_url
        .parse()
        .map_err(|e| DomainError::Internal(format!("Invalid auth URL: {e}")))?;

    let window = tauri::WebviewWindowBuilder::new(
        app,
        "oauth2-auth",
        tauri::WebviewUrl::External("about:blank".parse().unwrap()),
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

    // TLS policy for self-signed certs.
    #[cfg(target_os = "linux")]
    if !verify_ssl {
        let webview = window.webview();
        webview.with_webview(|wv| {
            use webkit2gtk::WebViewExt;
            let wk = wv.inner();
            if let Some(ctx) = wk.network_session() {
                use webkit2gtk::gio::prelude::TlsDatabaseExt;
                // Allow all certificates.
                ctx.set_tls_errors_policy(webkit2gtk::TLSErrorsPolicy::Ignore);
            }
        }).ok();
    }

    let _ = window.navigate(parsed_auth_url);

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

    if auth_result.state != expected_state_owned {
        return Err(DomainError::Internal(
            "State mismatch — possible CSRF attack.".into(),
        ));
    }

    Ok(auth_result.code)
}
```

- [ ] **Step 7: Add `auth_code_via_system_browser`**

```rust
use tauri_plugin_opener::OpenerExt;
use tokio::io::AsyncWriteExt;

const BROWSER_CALLBACK_HTML: &str = r#"<!DOCTYPE html><html><body><h2>Authorization successful</h2><p>You can close this tab.</p></body></html>"#;
const BROWSER_ERROR_HTML: &str = r#"<!DOCTYPE html><html><body><h2>Authorization failed</h2><p>Please return to the app.</p></body></html>"#;

/// Opens the system browser, starts a localhost TCP callback server, returns (code, redirect_uri).
async fn auth_code_via_system_browser(
    app: &AppHandle,
    auth_url: &str,
    expected_state: &str,
) -> Result<(String, String), DomainError> {
    // Bind to a random port.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to bind callback server: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| DomainError::Internal(format!("Failed to get port: {e}")))?
        .port();
    let redirect_uri = format!("http://localhost:{port}/callback");

    // Replace the callback URL in the auth URL with the actual localhost URL.
    // The auth_url was built with the user's configured callback — we need to replace it.
    // For system browser, we override to localhost.
    let auth_url_with_redirect = auth_url.replace(
        &urlencoding_encode(""),  // This won't work for replacement — need a different approach.
        "",
    );
    // Actually, the auth URL already has the configured callback_url encoded in it.
    // For system browser, we need to rebuild the URL with the localhost callback.
    // The simplest approach: the caller (auth_code_flow) should pass the raw auth URL
    // WITHOUT redirect_uri, and we add it here. But that requires refactoring build_auth_url_v2.
    //
    // Simpler approach: just replace redirect_uri in the URL query string.
    let final_auth_url = replace_redirect_uri_in_url(auth_url, &redirect_uri);

    // Open system browser.
    app.opener()
        .open_url(&final_auth_url, None::<&str>)
        .map_err(|e| DomainError::Internal(format!("Failed to open browser: {e}")))?;

    // Wait for callback.
    let expected = expected_state.to_string();
    let result = tokio::time::timeout(Duration::from_secs(120), async {
        let (mut stream, _) = listener
            .accept()
            .await
            .map_err(|e| DomainError::Internal(format!("Callback accept failed: {e}")))?;

        let mut buf = vec![0u8; 4096];
        let n = tokio::io::AsyncReadExt::read(&mut stream, &mut buf)
            .await
            .map_err(|e| DomainError::Internal(format!("Callback read failed: {e}")))?;

        let request_str = String::from_utf8_lossy(&buf[..n]);
        let first_line = request_str.lines().next().unwrap_or("");

        // Parse: "GET /callback?code=xxx&state=yyy HTTP/1.1"
        let path = first_line
            .split_whitespace()
            .nth(1)
            .unwrap_or("");

        let query = path
            .split('?')
            .nth(1)
            .unwrap_or("");

        let params: HashMap<String, String> = query
            .split('&')
            .filter_map(|pair| {
                let mut parts = pair.splitn(2, '=');
                Some((parts.next()?.to_string(), parts.next().unwrap_or("").to_string()))
            })
            .collect();

        // Check for error.
        if let Some(error) = params.get("error") {
            let desc = params.get("error_description").unwrap_or(error);
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n{BROWSER_ERROR_HTML}");
            let _ = stream.write_all(response.as_bytes()).await;
            return Err(DomainError::Internal(format!("Authorization denied: {desc}")));
        }

        // Verify state.
        let state = params.get("state").map(|s| s.as_str()).unwrap_or("");
        if state != expected {
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n{BROWSER_ERROR_HTML}");
            let _ = stream.write_all(response.as_bytes()).await;
            return Err(DomainError::Internal("State mismatch — possible CSRF attack.".into()));
        }

        let code = params
            .get("code")
            .cloned()
            .ok_or_else(|| DomainError::Internal("No authorization code in callback.".into()))?;

        let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n{BROWSER_CALLBACK_HTML}");
        let _ = stream.write_all(response.as_bytes()).await;

        Ok(code)
    })
    .await;

    match result {
        Ok(Ok(code)) => Ok((code, redirect_uri)),
        Ok(Err(e)) => Err(e),
        Err(_) => Err(DomainError::Internal(
            "Authorization timed out (120s). Please try again.".into(),
        )),
    }
}

/// Replaces the redirect_uri parameter value in a URL query string.
fn replace_redirect_uri_in_url(url: &str, new_redirect_uri: &str) -> String {
    // Parse and reconstruct to safely replace the redirect_uri param.
    if let Ok(mut parsed) = url::Url::parse(url) {
        let pairs: Vec<(String, String)> = parsed
            .query_pairs()
            .map(|(k, v)| {
                if k == "redirect_uri" {
                    (k.into_owned(), new_redirect_uri.to_string())
                } else {
                    (k.into_owned(), v.into_owned())
                }
            })
            .collect();
        parsed.query_pairs_mut().clear();
        for (k, v) in &pairs {
            parsed.query_pairs_mut().append_pair(k, v);
        }
        parsed.to_string()
    } else {
        url.to_string()
    }
}
```

- [ ] **Step 8: Add stub `implicit_flow`**

Implicit flow is complex (fragment-based token extraction). Stub it for now:

```rust
/// Implicit flow: opens browser, extracts token from URL fragment.
/// Currently returns an error — full implementation is a follow-up.
async fn implicit_flow(
    _app: &AppHandle,
    _config: &rocket_app::oauth2_service::ResolvedOAuth2Config,
) -> Result<OAuthToken, DomainError> {
    Err(DomainError::InvalidInput(
        "Implicit grant flow is not yet implemented. Use Authorization Code with PKCE instead."
            .into(),
    ))
}
```

- [ ] **Step 9: Add `apply_params_to_url` import**

Add to the imports at the top of the file:

```rust
use rocket_http::apply_params_to_url;
```

- [ ] **Step 10: Register new commands in lib.rs**

In `src-tauri/src/lib.rs`, find the `generate_handler![]` call and add:

```rust
commands::oauth2::oauth2_get_token,
commands::oauth2::oauth2_refresh_token,
```

- [ ] **Step 11: Verify compilation**

```bash
cargo check --workspace
```

Fix any compilation errors — this task touches many imports and type references. Common issues:
- `ResolvedOAuth2Config` needs to be `pub` or `pub(crate)` in `oauth2_service.rs`
- Missing imports for `apply_params_to_url`
- `OAuth2Service` state management syntax

- [ ] **Step 12: Commit**

```bash
git add src-tauri/src/commands/oauth2.rs src-tauri/src/lib.rs
git commit -m "feat: oauth2_get_token, oauth2_refresh_token commands with system browser support"
```

---

### Task 3: Deprecate old `oauth2_auth_code_flow` command and verify

**Files:**
- Modify: `src-tauri/src/commands/oauth2.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Keep the old command temporarily**

Don't remove `oauth2_auth_code_flow` yet — the frontend still calls it. Mark it with a deprecation comment:

```rust
/// DEPRECATED: Use `oauth2_get_token` with grant_type "authorization_code" instead.
/// Kept for backward compatibility until the frontend is updated in Phase 2.
#[tauri::command]
pub async fn oauth2_auth_code_flow(
    // ... existing signature unchanged ...
```

- [ ] **Step 2: Run full workspace build and tests**

```bash
cargo check --workspace
cargo test --workspace
```

Expected: Clean compilation, all tests pass.

- [ ] **Step 3: Verify the old command still works**

The existing frontend calls `oauth2_auth_code_flow` — verify it's still registered in `generate_handler![]` and hasn't been broken by imports.

- [ ] **Step 4: Run the app manually (smoke test)**

```bash
cd /home/numericlabs/data/Rust/Rocket && cargo tauri dev
```

- Open a request, select OAuth2 auth, try "Get Token" with client_credentials
- Verify it still works via the existing frontend path (which calls `executeRequest` directly)
- The new commands aren't wired to the frontend yet — that's Phase 2

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/commands/oauth2.rs src-tauri/src/lib.rs
git commit -m "chore: deprecate oauth2_auth_code_flow, verify backward compatibility"
```
