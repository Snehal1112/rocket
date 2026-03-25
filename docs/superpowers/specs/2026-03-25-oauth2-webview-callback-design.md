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
  → Open a Tauri WebviewWindow navigating to the auth URL
  → User logs in and consents inside the window
  → Auth provider redirects to callback URL with ?code=xxx&state=yyy
  → on_navigation intercepts the redirect, extracts code + state
  → Window closes, main app receives code via oneshot channel
  → Verify state matches
  → Exchange code for token via POST (existing acquire_token)
  → Show toast in main window on success, or tokenError on failure
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

    // Build auth URL with response_type, client_id, redirect_uri,
    // code_challenge (S256), state, and optional scope.
    let auth_url = build_auth_url(&authorization_url, &client_id, &redirect_uri, &pkce, &state, &scope);

    // Channel for the navigation callback to send the result.
    let (tx, rx) = tokio::sync::oneshot::channel::<Result<String, String>>();
    let tx = std::sync::Mutex::new(Some(tx));
    let redirect_prefix = redirect_uri_prefix(&redirect_uri);

    // Open webview window.
    let window = tauri::WebviewWindowBuilder::new(
        &app,
        "oauth2-auth",
        tauri::WebviewUrl::External(auth_url.parse().unwrap()),
    )
    .title("Sign In")
    .inner_size(500.0, 700.0)
    .on_navigation(move |url| {
        if url.as_str().starts_with(&redirect_prefix) {
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

    let code = match result {
        Ok(Ok(Ok(code))) => code,
        Ok(Ok(Err(err))) => return Err(DomainError::Internal(format!("Authorization denied: {err}"))),
        Ok(Err(_)) => return Err(DomainError::Internal(
            "Authorization window was closed before completing sign-in.".into(),
        )),
        Err(_) => return Err(DomainError::Internal(
            "Authorization timed out. Please try again.".into(),
        )),
    };

    // Verify state (extracted alongside code in extract_code_or_error).
    // Exchange code for token.
    let client = reqwest::Client::new();
    let config = OAuthConfig {
        grant_type: "authorization_code".into(),
        client_id,
        client_secret,
        token_url,
        scope,
        code: Some(code),
        redirect_uri: Some(redirect_uri),
        code_verifier: Some(pkce.code_verifier),
        ..Default::default()
    };
    acquire_token(&config, &client).await
}
```

### Helper Functions

`redirect_uri_prefix` strips the fragment from the callback URL so `starts_with` matching works correctly against the redirect URL which will have query params inserted before the fragment:

```rust
fn redirect_uri_prefix(redirect_uri: &str) -> String {
    // "https://exchange4all.local/webapp/#oidc-callback"
    //  → "https://exchange4all.local/webapp/"
    redirect_uri.split('#').next().unwrap_or(redirect_uri).to_string()
}
```

`extract_code_or_error` parses query params from the intercepted URL:

```rust
fn extract_code_or_error(url: &url::Url) -> Result<String, String> {
    let params: HashMap<String, String> = url.query_pairs().map(|(k, v)| (k.into(), v.into())).collect();
    if let Some(error) = params.get("error") {
        let desc = params.get("error_description").unwrap_or(error);
        return Err(desc.clone());
    }
    params.get("code").cloned().ok_or_else(|| "No authorization code in callback.".into())
}
```

State verification is handled by also extracting `state` from `query_pairs()` and comparing before exchanging the code.

### Error Handling

| Scenario | Behavior |
|----------|----------|
| User closes window | `oneshot::Receiver` gets `RecvError` → "Authorization window was closed before completing sign-in." |
| 120s timeout | `tokio::time::timeout` fires → "Authorization timed out. Please try again." |
| Auth provider returns `?error=` | Extracted in `on_navigation`, sent through channel → "Authorization denied: {description}" |
| State mismatch | Checked after receiving code → "State mismatch — possible CSRF attack." |
| Window already open | Close existing `oauth2-auth` window before creating a new one |

### Window Closed Detection

When the user manually closes the webview window, the `oneshot::Sender` inside the `on_navigation` closure is dropped (the closure is dropped with the window). This causes `rx.await` to return `Err(RecvError)`, which we map to the "window was closed" error message.

### Frontend Changes

Two small changes:

1. **Default callback URL** — Update in `AuthEditor.tsx` and `pane-utils.ts`:
   - Old: `http://localhost:9876/callback`
   - New: `https://exchange4all.local/webapp/#oidc-callback`

2. **Success toast** — Listen for a `oauth2-token-acquired` Tauri event and show a brief toast notification. Error display continues to use the existing `tokenError` state.

The Callback URL field remains editable. The Tauri API bridge (`tauri-api.ts`) and type definitions (`pane-types.ts`) are unchanged.

### PKCE

Kept as-is, always enabled. `generate_pkce()` from `pkce.rs` produces the code_verifier and code_challenge (S256). No changes needed.

## Files Changed

| File | Action | Description |
|------|--------|-------------|
| `src-tauri/src/commands/oauth2.rs` | Rewrite | Replace TCP listener with WebviewWindow + `on_navigation` |
| `crates/rocket-http/src/oauth2_callback.rs` | Delete | Localhost callback server no longer needed |
| `crates/rocket-http/src/lib.rs` | Edit | Remove `oauth2_callback` module and exports |
| `src/components/request/AuthEditor.tsx` | Edit | Change default callback URL, add toast listener |
| `src/lib/pane-utils.ts` | Edit | Change default callback URL |
| `src-tauri/Cargo.toml` | Edit | Verify `tauri` features include webview window support |

## Files Unchanged

- `crates/rocket-http/src/pkce.rs` — PKCE generation stays.
- `crates/rocket-http/src/oauth2.rs` — `acquire_token`, `OAuthConfig`, `OAuthToken` stay.
- `src/lib/tauri-api.ts` — Same function signature.
- `src/types/pane-types.ts` — Same AuthState shape.

## Net Effect

Deletes more code than it adds. The entire TCP server (~170 lines), HTTP parsing, favicon handling, and IPv4/IPv6 dual-stack logic are replaced by ~50 lines of Tauri webview logic.
