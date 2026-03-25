# OAuth2 Authorization Code Flow with PKCE Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement the full OAuth2 Authorization Code grant with PKCE — browser consent, local callback server, code exchange, and token retrieval.

**Architecture:** PKCE generation and callback server live in `rocket-http`. A new Tauri command orchestrates the flow: generate PKCE, start local server, open browser, wait for callback, exchange code for token. Frontend adds Authorization URL field and loading state to AuthEditor.

**Tech Stack:** Rust (sha2, base64, rand, tokio TCP), Tauri (shell/opener plugin), React, TypeScript, shadcn/ui

**Spec:** `docs/superpowers/specs/2026-03-25-oauth2-authorization-code-design.md`

---

### File Structure

```
Rust:
  crates/rocket-http/Cargo.toml               # add rand, base64 deps
  crates/rocket-http/src/pkce.rs               # PKCE code_verifier + code_challenge
  crates/rocket-http/src/oauth2_callback.rs    # temporary localhost HTTP server
  crates/rocket-http/src/lib.rs                # export new modules
  src-tauri/src/commands/oauth2.rs             # Tauri command orchestrating the flow
  src-tauri/src/commands/mod.rs                # export oauth2
  src-tauri/src/lib.rs                         # register command

Frontend:
  src/types/pane-types.ts                      # add authorizationUrl to oauth2
  src/lib/tauri-api.ts                         # add oauth2AuthCodeFlow invoke
  src/components/request/AuthEditor.tsx         # Authorization URL field, enable Get Token, loading state
```

---

### Task 1: PKCE generation (code_verifier + code_challenge)

**Files:**
- Modify: `crates/rocket-http/Cargo.toml`
- Create: `crates/rocket-http/src/pkce.rs`
- Modify: `crates/rocket-http/src/lib.rs`

- [ ] **Step 1: Add rand and base64 deps**

In `crates/rocket-http/Cargo.toml`, add under `[dependencies]`:
```toml
rand = "0.8"
base64.workspace = true
```

(`sha2` is already a workspace dep in this crate.)

- [ ] **Step 2: Create pkce.rs with tests**

`crates/rocket-http/src/pkce.rs`:

```rust
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use sha2::{Digest, Sha256};

/// A PKCE pair: code_verifier (random, 43-128 chars) and code_challenge (SHA256 of verifier).
pub struct PkcePair {
    pub code_verifier: String,
    pub code_challenge: String,
}

/// Generates a PKCE code_verifier and code_challenge per RFC 7636.
pub fn generate_pkce() -> PkcePair {
    let mut rng = rand::thread_rng();
    let mut bytes = [0u8; 32];
    rng.fill(&mut bytes);
    let code_verifier = URL_SAFE_NO_PAD.encode(bytes);

    let mut hasher = Sha256::new();
    hasher.update(code_verifier.as_bytes());
    let digest = hasher.finalize();
    let code_challenge = URL_SAFE_NO_PAD.encode(digest);

    PkcePair { code_verifier, code_challenge }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pkce_verifier_length() {
        let pair = generate_pkce();
        assert!(pair.code_verifier.len() >= 43);
        assert!(pair.code_verifier.len() <= 128);
    }

    #[test]
    fn pkce_challenge_is_sha256_of_verifier() {
        let pair = generate_pkce();
        let mut hasher = Sha256::new();
        hasher.update(pair.code_verifier.as_bytes());
        let expected = URL_SAFE_NO_PAD.encode(hasher.finalize());
        assert_eq!(pair.code_challenge, expected);
    }

    #[test]
    fn pkce_pairs_are_unique() {
        let a = generate_pkce();
        let b = generate_pkce();
        assert_ne!(a.code_verifier, b.code_verifier);
    }
}
```

- [ ] **Step 3: Export from lib.rs**

In `crates/rocket-http/src/lib.rs`, add:
```rust
pub mod pkce;
pub use pkce::{generate_pkce, PkcePair};
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p rocket-http`
Expected: All pass including 3 new PKCE tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-http/
git commit -m "feat: PKCE code_verifier and code_challenge generation (RFC 7636)"
```

---

### Task 2: Temporary local HTTP callback server

**Files:**
- Create: `crates/rocket-http/src/oauth2_callback.rs`
- Modify: `crates/rocket-http/src/lib.rs`

- [ ] **Step 1: Create oauth2_callback.rs**

A minimal async HTTP server that:
1. Binds to `127.0.0.1:0` (OS assigns random available port)
2. Returns the bound port
3. Waits for one GET request to `/callback`
4. Extracts `code` and `state` from query params
5. Verifies `state` matches expected
6. Responds with HTML success page
7. Shuts down

```rust
use rocket_shared::error::{DomainError, DomainResult};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::time::timeout;

/// Result of a successful OAuth2 callback.
pub struct CallbackResult {
    pub code: String,
    pub port: u16,
}

const SUCCESS_HTML: &str = r#"<!DOCTYPE html>
<html><body style="font-family:system-ui;text-align:center;padding:60px">
<h2>Authorization Successful</h2>
<p>You can close this tab and return to Rocket.</p>
</body></html>"#;

const ERROR_HTML: &str = r#"<!DOCTYPE html>
<html><body style="font-family:system-ui;text-align:center;padding:60px">
<h2>Authorization Failed</h2>
<p>Something went wrong. Please try again.</p>
</body></html>"#;

/// Starts a temporary HTTP server on a random port and waits for the OAuth2 callback.
pub async fn wait_for_callback(
    expected_state: &str,
    timeout_secs: u64,
) -> DomainResult<CallbackResult> {
    // Bind to random available port on localhost only.
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to bind callback server: {e}")))?;

    let port = listener.local_addr()
        .map_err(|e| DomainError::Internal(format!("Failed to get port: {e}")))?
        .port();

    let expected = expected_state.to_string();

    let result = timeout(Duration::from_secs(timeout_secs), async {
        let (mut stream, _) = listener.accept().await
            .map_err(|e| DomainError::Internal(format!("Accept failed: {e}")))?;

        let mut buf = vec![0u8; 4096];
        let n = stream.read(&mut buf).await
            .map_err(|e| DomainError::Internal(format!("Read failed: {e}")))?;

        let request = String::from_utf8_lossy(&buf[..n]);

        // Parse the GET request line to extract query params.
        let path = request.lines().next()
            .and_then(|line| line.split_whitespace().nth(1))
            .unwrap_or("");

        let query = path.split('?').nth(1).unwrap_or("");
        let params: std::collections::HashMap<&str, &str> = query
            .split('&')
            .filter_map(|p| p.split_once('='))
            .collect();

        // Check for error response from auth server.
        if let Some(error) = params.get("error") {
            let desc = params.get("error_description").unwrap_or(error);
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n{ERROR_HTML}");
            let _ = stream.write_all(response.as_bytes()).await;
            return Err(DomainError::Internal(format!("Authorization denied: {desc}")));
        }

        // Verify state.
        let state = params.get("state").copied().unwrap_or("");
        if state != expected {
            let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n{ERROR_HTML}");
            let _ = stream.write_all(response.as_bytes()).await;
            return Err(DomainError::Internal("State mismatch — possible CSRF attack.".into()));
        }

        // Extract code.
        let code = params.get("code")
            .ok_or_else(|| DomainError::Internal("No authorization code in callback.".into()))?
            .to_string();

        let response = format!("HTTP/1.1 200 OK\r\nContent-Type: text/html\r\n\r\n{SUCCESS_HTML}");
        let _ = stream.write_all(response.as_bytes()).await;

        Ok(CallbackResult { code, port })
    }).await;

    match result {
        Ok(inner) => inner,
        Err(_) => Err(DomainError::Internal(
            "Authorization timed out (120s). Please try again.".into(),
        )),
    }
}
```

- [ ] **Step 2: Export from lib.rs**

Add to `crates/rocket-http/src/lib.rs`:
```rust
pub mod oauth2_callback;
pub use oauth2_callback::{wait_for_callback, CallbackResult};
```

- [ ] **Step 3: Run tests**

Run: `cargo check -p rocket-http`
Expected: Compiles clean.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-http/
git commit -m "feat: temporary localhost callback server for OAuth2 auth code flow"
```

---

### Task 3: Tauri command — orchestrate the full flow

**Files:**
- Create: `src-tauri/src/commands/oauth2.rs`
- Modify: `src-tauri/src/commands/mod.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Create the Tauri command**

`src-tauri/src/commands/oauth2.rs`:

```rust
use rocket_http::{
    generate_pkce, wait_for_callback, acquire_token, OAuthConfig, OAuthToken,
};
use rocket_shared::error::DomainError;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub async fn oauth2_auth_code_flow(
    app: AppHandle,
    authorization_url: String,
    token_url: String,
    client_id: String,
    client_secret: String,
    scope: Option<String>,
) -> Result<OAuthToken, DomainError> {
    // 1. Generate PKCE pair.
    let pkce = generate_pkce();

    // 2. Generate random state for CSRF protection.
    let state = uuid::Uuid::new_v4().to_string();

    // 3. Start callback server (binds to random port).
    // We need the port before opening the browser.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| DomainError::Internal(format!("Failed to bind: {e}")))?;
    let port = listener.local_addr()
        .map_err(|e| DomainError::Internal(format!("No port: {e}")))?
        .port();
    let redirect_uri = format!("http://localhost:{port}/callback");

    // 4. Build authorization URL with all params.
    let mut auth_url = url::Url::parse(&authorization_url)
        .map_err(|e| DomainError::InvalidInput(format!("Invalid authorization URL: {e}")))?;
    auth_url.query_pairs_mut()
        .append_pair("response_type", "code")
        .append_pair("client_id", &client_id)
        .append_pair("redirect_uri", &redirect_uri)
        .append_pair("code_challenge", &pkce.code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", &state);
    if let Some(ref s) = scope {
        auth_url.query_pairs_mut().append_pair("scope", s);
    }

    // 5. Open browser.
    app.opener()
        .open_url(auth_url.as_str(), None::<&str>)
        .map_err(|e| DomainError::Internal(format!("Failed to open browser: {e}")))?;

    // 6. Wait for callback (120s timeout).
    // Use the already-bound listener via wait_for_callback_with_listener.
    let state_clone = state.clone();
    let callback = rocket_http::oauth2_callback::wait_for_callback_with_listener(
        listener, &state_clone, 120,
    ).await?;

    // 7. Exchange code for token.
    let client = reqwest::Client::new();
    let config = OAuthConfig {
        grant_type: "authorization_code".into(),
        client_id,
        client_secret,
        token_url,
        scope,
        username: None,
        password: None,
        code: Some(callback.code),
        redirect_uri: Some(redirect_uri),
        code_verifier: Some(pkce.code_verifier),
    };
    acquire_token(&config, &client).await
}
```

Note: We need a variant of `wait_for_callback` that accepts a pre-bound `TcpListener` (so we know the port before opening the browser). Add `wait_for_callback_with_listener` to `oauth2_callback.rs`.

- [ ] **Step 2: Add wait_for_callback_with_listener**

In `crates/rocket-http/src/oauth2_callback.rs`, add a version that takes a pre-bound listener:

```rust
pub async fn wait_for_callback_with_listener(
    listener: TcpListener,
    expected_state: &str,
    timeout_secs: u64,
) -> DomainResult<CallbackResult> {
    let port = listener.local_addr()
        .map_err(|e| DomainError::Internal(format!("No port: {e}")))?
        .port();

    // ... same logic as wait_for_callback but uses the passed listener ...
}
```

Refactor the original `wait_for_callback` to call this with a freshly bound listener.

- [ ] **Step 3: Add uuid and url deps to src-tauri**

In `src-tauri/Cargo.toml`, add:
```toml
url = "2"
```
(`uuid` should already be available from `rocket-collection`.)

- [ ] **Step 4: Register in mod.rs and lib.rs**

`src-tauri/src/commands/mod.rs`:
```rust
pub mod oauth2;
```

`src-tauri/src/lib.rs` — add to `generate_handler![]`:
```rust
commands::oauth2::oauth2_auth_code_flow,
```

- [ ] **Step 5: Fix compilation**

Run: `cargo check --workspace`

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-http/ src-tauri/
git commit -m "feat: Tauri command for OAuth2 authorization code flow with PKCE"
```

---

### Task 4: Frontend — add authorizationUrl field and enable Get Token

**Files:**
- Modify: `src/types/pane-types.ts`
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/components/request/AuthEditor.tsx`

- [ ] **Step 1: Add authorizationUrl to AuthState**

In `src/types/pane-types.ts`, add `authorizationUrl: string` to the `oauth2` type:

```typescript
oauth2?: {
    grantType: 'client_credentials' | 'password' | 'authorization_code';
    authorizationUrl: string;  // NEW
    clientId: string;
    clientSecret: string;
    tokenUrl: string;
    scope: string;
    accessToken: string;
    refreshToken: string;
};
```

- [ ] **Step 2: Add invoke function to tauri-api.ts**

```typescript
export interface OAuth2TokenResponse {
  access_token: string;
  token_type: string;
  expires_in?: number;
  refresh_token?: string;
  scope?: string;
}

export const oauth2AuthCodeFlow = (
  authorizationUrl: string,
  tokenUrl: string,
  clientId: string,
  clientSecret: string,
  scope?: string,
) => invoke<OAuth2TokenResponse>("oauth2_auth_code_flow", {
  authorizationUrl, tokenUrl, clientId, clientSecret, scope,
});
```

- [ ] **Step 3: Update AuthEditor**

Read `src/components/request/AuthEditor.tsx`. Make these changes:

**a)** Where `oauth2` defaults are set (around line 40-43), add `authorizationUrl: ''`:
```typescript
next.oauth2 = {
  grantType: 'client_credentials',
  authorizationUrl: '',
  clientId: '', clientSecret: '', tokenUrl: '', scope: '',
  accessToken: '', refreshToken: '',
};
```

**b)** In `handleGetToken` (line 73-134), replace the `authorization_code` early return with the real flow:

```typescript
if (oauth.grantType === 'authorization_code') {
  if (!oauth.authorizationUrl) return;
  setGettingToken(true);
  setTokenError('');
  try {
    const token = await oauth2AuthCodeFlow(
      oauth.authorizationUrl,
      oauth.tokenUrl,
      oauth.clientId,
      oauth.clientSecret,
      oauth.scope || undefined,
    );
    patchOAuth2({
      accessToken: token.access_token,
      refreshToken: token.refresh_token || '',
    });
  } catch (err) {
    setTokenError(err instanceof Error ? err.message : String(err));
  } finally {
    setGettingToken(false);
  }
  return;
}
```

Add state: `const [gettingToken, setGettingToken] = useState(false);`

Import `oauth2AuthCodeFlow` from `@/lib/tauri-api`.

**c)** Show "Authorization URL" input when grant type is `authorization_code` (in the JSX, before Token URL):

```tsx
{auth.oauth2.grantType === 'authorization_code' && (
  <div>
    <label className="text-[11px] font-medium text-muted-foreground mb-1 block">
      Authorization URL
    </label>
    <Input
      className="text-xs h-8 font-mono"
      placeholder="https://auth.example.com/authorize"
      value={auth.oauth2.authorizationUrl}
      onChange={(e) => patchOAuth2({ authorizationUrl: e.target.value })}
    />
  </div>
)}
```

**d)** Update the "Get Token" button:
- Remove the `disabled` condition for `authorization_code`
- Show "Waiting..." when `gettingToken` is true
- Disable while waiting

```tsx
<Button
  type="button"
  variant="outline"
  size="sm"
  className="h-8 shrink-0 px-2 text-xs"
  disabled={!auth.oauth2.tokenUrl || gettingToken}
  onClick={handleGetToken}
>
  {gettingToken ? 'Waiting...' : 'Get Token'}
</Button>
```

**e)** Remove the "coming soon" tooltip for authorization_code.

- [ ] **Step 4: Verify TypeScript**

Run: `npx tsc --noEmit`
Expected: Clean.

- [ ] **Step 5: Commit**

```bash
git add src/
git commit -m "feat: Authorization Code flow UI — authorizationUrl field, Get Token enabled, loading state"
```

---

### Task 5: End-to-end verification

- [ ] **Step 1: Restart yarn tauri dev** (Rust changes)
- [ ] **Step 2: Test the flow** with a real OAuth2 provider (e.g., Google, GitHub, or a test provider like https://oauth.tools)
  - Select Authorization Code grant type
  - Fill in Authorization URL, Token URL, Client ID, Client Secret, Scope
  - Click Get Token
  - Browser opens → user consents → redirect to localhost callback
  - Token appears in the Access Token field
- [ ] **Step 3: Test error cases**
  - Close browser without consenting → timeout error after 120s
  - Invalid authorization URL → error message
  - Invalid client credentials → error from token exchange
- [ ] **Step 4: Commit any fixes**
