# OAuth2 Webview Callback Design

Replace the localhost TCP callback server with a Tauri WebviewWindow that intercepts navigation to capture the authorization code.

## Motivation

The current implementation spins up a temporary `TcpListener` on localhost to receive the OAuth2 redirect. This causes:

- IPv4/IPv6 resolution mismatches (browser resolves `localhost` to `[::1]`, server listens on `127.0.0.1`).
- Port conflicts if the configured port is already in use.
- Firewall and proxy interference on some systems.
- Complexity: HTTP request parsing, favicon handling, preflight request loops.

Postman Desktop solves this by opening an in-app browser window and intercepting the redirect URL before it loads. Tauri supports the same pattern via `WebviewWindowBuilder::on_navigation`.

## Design

### Flow

```
User clicks "Get Token"
  → Generate PKCE pair + random state
  → Build authorization URL with redirect_uri, code_challenge, state
  → Close any existing oauth2-auth window
  → Open a Tauri WebviewWindow navigating to the auth URL
  → User logs in and consents inside the window
  → Auth provider redirects to callback URL with ?code=xxx&state=yyy
  → on_navigation checks URL prefix AND presence of code/error param
  → Extracts code + state, sends through oneshot channel
  → Window closes, main app receives code + state
  → Verify state matches expected value
  → Exchange code for token via POST (existing acquire_token)
  → Return token to frontend (success updates UI, error shows tokenError)
```

### Default Callback URL

`https://exchange4all.local/webapp/#oidc-callback`

The URL does not need to resolve to a real server. Navigation is intercepted before loading. Users can still customize this in the Callback URL field.

### Tauri Command (Rust)

Rewrite `src-tauri/src/commands/oauth2.rs`:

```rust
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
    let state = Uuid::new_v4().to_string();
    let redirect_uri = callback_url
        .unwrap_or_else(|| "https://exchange4all.local/webapp/#oidc-callback".into());

    // Build auth URL inline (replaces the current format! construction).
    // Uses urlencoding_encode for client_id, redirect_uri, and scope.
    // Includes: response_type=code, client_id, redirect_uri,
    // code_challenge (S256), code_challenge_method, state, scope.
    let auth_url = build_auth_url(
        &authorization_url, &client_id, &redirect_uri, &pkce, &state, &scope,
    );

    // Channel for the navigation callback to send the result.
    // Sends AuthCodeResult on success, or error description string on failure.
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<AuthCodeResult, String>>();
    let tx = std::sync::Mutex::new(Some(tx));
    let redirect_prefix = redirect_uri_prefix(&redirect_uri);

    // Close any existing auth window from a previous attempt.
    if let Some(existing) = app.get_webview_window("oauth2-auth") {
        let _ = existing.close();
    }

    // Open webview window.
    // Note: on_navigation receives &url::Url and must be Fn (not FnOnce),
    // hence the Mutex<Option<Sender>> pattern to move the sender out once.
    let window = tauri::WebviewWindowBuilder::new(
        &app,
        "oauth2-auth",
        tauri::WebviewUrl::External(
            auth_url.parse().map_err(|e| DomainError::Internal(format!("Invalid auth URL: {e}")))?,
        ),
    )
    .title("Sign In")
    .inner_size(500.0, 700.0)
    .on_navigation(move |url| {
        // Only intercept if URL matches callback prefix AND has code or error.
        if url.as_str().starts_with(&redirect_prefix) && has_auth_params(url) {
            if let Some(tx) = tx.lock().unwrap().take() {
                let result = extract_code_or_error(url);
                let _ = tx.send(result);
            }
            return false; // Block navigation.
        }
        true // Allow all other navigations.
    })
    .build()
    .map_err(|e| DomainError::Internal(format!("Failed to open auth window: {e}")))?;

    // Wait for result with timeout.
    let result = tokio::time::timeout(Duration::from_secs(120), rx).await;
    let _ = window.close();

    let auth_result = match result {
        Ok(Ok(Ok(r))) => r,
        Ok(Ok(Err(err))) => return Err(DomainError::Internal(
            format!("Authorization denied: {err}"),
        )),
        Ok(Err(_)) => return Err(DomainError::Internal(
            "Authorization window was closed before completing sign-in.".into(),
        )),
        Err(_) => return Err(DomainError::Internal(
            "Authorization timed out. Please try again.".into(),
        )),
    };

    // Verify CSRF state before exchanging the code.
    if auth_result.state != state {
        return Err(DomainError::Internal(
            "State mismatch — possible CSRF attack.".into(),
        ));
    }

    // Exchange code for token.
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
```

### Helper Types and Functions

```rust
/// Result extracted from the OAuth2 callback URL.
struct AuthCodeResult {
    code: String,
    state: String,
}
```

`redirect_uri_prefix` strips the fragment from the callback URL so `starts_with` matching works correctly against the redirect URL (query params are inserted before the fragment per RFC 3986):

```rust
fn redirect_uri_prefix(redirect_uri: &str) -> String {
    // "https://exchange4all.local/webapp/#oidc-callback"
    //  → "https://exchange4all.local/webapp/"
    redirect_uri.split('#').next().unwrap_or(redirect_uri).to_string()
}
```

`has_auth_params` checks that the URL actually carries OAuth2 callback parameters. This prevents intercepting unrelated navigations to the same domain prefix (defense-in-depth):

```rust
fn has_auth_params(url: &url::Url) -> bool {
    url.query_pairs().any(|(k, _)| k == "code" || k == "error")
}
```

`extract_code_or_error` parses both `code` and `state` from the intercepted URL:

```rust
fn extract_code_or_error(url: &url::Url) -> Result<AuthCodeResult, String> {
    let params: HashMap<String, String> =
        url.query_pairs().map(|(k, v)| (k.into(), v.into())).collect();
    if let Some(error) = params.get("error") {
        let desc = params.get("error_description").unwrap_or(error);
        return Err(desc.clone());
    }
    let code = params.get("code")
        .cloned()
        .ok_or_else(|| "No authorization code in callback.".to_string())?;
    let state = params.get("state")
        .cloned()
        .ok_or_else(|| "Auth provider did not return a state parameter.".to_string())?;
    Ok(AuthCodeResult { code, state })
}
```

`build_auth_url` replaces the current inline `format!` construction. Uses the same `urlencoding_encode` helper that exists today:

```rust
fn build_auth_url(
    authorization_url: &str,
    client_id: &str,
    redirect_uri: &str,
    pkce: &PkcePair,
    state: &str,
    scope: &Option<String>,
) -> String {
    let sep = if authorization_url.contains('?') { "&" } else { "?" };
    let scope_param = scope.as_ref()
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
```

### Error Handling

| Scenario | Behavior |
|----------|----------|
| User closes window | `oneshot::Receiver` gets `RecvError` → "Authorization window was closed before completing sign-in." |
| 120s timeout | `tokio::time::timeout` fires → "Authorization timed out. Please try again." |
| Auth provider returns `?error=` | Extracted in `on_navigation`, sent through channel → "Authorization denied: {description}" |
| State mismatch | Checked after receiving code, before token exchange → "State mismatch — possible CSRF attack." |
| Window already open | Close existing `oauth2-auth` window before creating a new one |

### Window Closed Detection

When the user manually closes the webview window, the `oneshot::Sender` inside the `on_navigation` closure is dropped (the closure is dropped with the window). This causes `rx.await` to return `Err(RecvError)`, which we map to the "window was closed" error message.

### Frontend Changes

Two small changes:

1. **Default callback URL** — Update in `AuthEditor.tsx` and `pane-utils.ts`:
   - Old: `http://localhost:9876/callback`
   - New: `https://exchange4all.local/webapp/#oidc-callback`

2. **Success/error feedback** — No separate Tauri event needed. The `handleGetToken` function already `await`s the Tauri command result: success patches the token into state, errors are displayed via the existing `tokenError` state and the red error text below the Get Token button.

The Callback URL field remains editable. The Tauri API bridge (`tauri-api.ts`) and type definitions (`pane-types.ts`) are unchanged.

### PKCE

Kept as-is, always enabled. `generate_pkce()` from `pkce.rs` produces the code_verifier and code_challenge (S256). No changes needed.

### Closure Constraints

The `on_navigation` callback must be `Fn` (not `FnOnce` or `FnMut`). The `Mutex<Option<Sender>>` pattern satisfies this: `Mutex::lock` requires only `&self`, and `Option::take` moves the sender out on first interception so subsequent calls are no-ops.

## Files Changed

| File | Action | Description |
|------|--------|-------------|
| `src-tauri/src/commands/oauth2.rs` | Rewrite | Replace TCP listener with WebviewWindow + `on_navigation` |
| `crates/rocket-http/src/oauth2_callback.rs` | Delete | Localhost callback server no longer needed |
| `crates/rocket-http/src/lib.rs` | Edit | Remove `oauth2_callback` module and exports |
| `src/components/request/AuthEditor.tsx` | Edit | Change default callback URL |
| `src/lib/pane-utils.ts` | Edit | Change default callback URL |
| `src-tauri/Cargo.toml` | Check | `WebviewWindowBuilder` is available by default in Tauri v2; no extra features expected |

Note: The `tauri-plugin-opener` import in `oauth2.rs` is removed since we no longer open the system browser. The plugin may still be used elsewhere in the app.

## Files Unchanged

- `crates/rocket-http/src/pkce.rs` — PKCE generation stays.
- `crates/rocket-http/src/oauth2.rs` — `acquire_token`, `OAuthConfig`, `OAuthToken` stay.
- `src/lib/tauri-api.ts` — Same function signature.
- `src/types/pane-types.ts` — Same AuthState shape.

## Net Effect

Deletes more code than it adds. The entire TCP server (~170 lines), HTTP parsing, favicon handling, and IPv4/IPv6 dual-stack logic are replaced by ~60 lines of Tauri webview logic.
