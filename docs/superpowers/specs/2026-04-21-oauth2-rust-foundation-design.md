# OAuth 2.0 Rust Foundation — Design Spec (Phase 1 of 2)

**Date:** 2026-04-21
**Status:** Approved
**Feature:** Full Bruno-parity OAuth2 — Rust backend consolidation

---

## Context

RocketAPI's OAuth2 implementation is split across two layers: the authorization_code flow runs through a Rust Tauri command (`oauth2_auth_code_flow`), while client_credentials and password grant types are handled entirely in the frontend's `handleGetToken` via `executeRequest`. Refresh token logic also lives in the frontend (`handleRefreshToken`).

This split causes several problems:
- Variable resolution for OAuth2 fields only happens in the frontend (process env, global, runtime scopes) but misses backend-accessible scopes (collection, environment, folder-chain, request-level) unless the frontend pre-resolves them.
- Additional parameters (a Bruno feature) need to be injected at three phases: authorization URL, token exchange POST, and refresh POST. The authorization URL is built in Rust, making a frontend-only approach impossible for auth_code.
- JWT decoding for ID Token display requires either a JS library or a Rust crate — Rust is more consistent with the architecture.
- System browser support for OAuth needs Tauri's opener plugin + a localhost TCP callback server, both Rust-side.

Phase 1 consolidates all OAuth2 token logic into Rust. Phase 2 (separate spec) redesigns the frontend AuthEditor UI to match Bruno's layout.

---

## Phase 1 Scope

| Feature | Description |
|---|---|
| Unified `oauth2_get_token` | Single Tauri command for all 4 grant types |
| `oauth2_refresh_token` | Dedicated refresh command with separate refresh URL support |
| Variable resolution | Full 7-scope precedence for all OAuth2 fields |
| Additional parameters | auth_params, token_params, refresh_params at correct phases |
| JWT decoding | `oauth2_decode_jwt` command for ID Token display |
| PKCE toggle | User-controllable PKCE on/off for auth_code flow |
| System browser | Alternative to webview using OS browser + localhost callback |
| Extended token response | `id_token` field in token result for OIDC providers |

---

## 1. Unified `oauth2_get_token` Command

### Input

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2GetTokenRequest {
    pub grant_type: String, // "client_credentials" | "password" | "authorization_code" | "implicit"

    // URLs
    pub authorization_url: Option<String>,     // auth_code + implicit
    pub token_url: Option<String>,             // all except implicit
    pub callback_url: Option<String>,          // auth_code + implicit

    // Credentials
    pub client_id: String,
    pub client_secret: Option<String>,         // not used for implicit
    pub scope: Option<String>,
    pub state: Option<String>,                 // auth_code + implicit; auto-generated if empty

    // Password grant
    pub username: Option<String>,
    pub password: Option<String>,

    // Options
    pub client_authentication: Option<String>, // "header" | "body" (default: "body")
    pub use_pkce: Option<bool>,                // default: true for auth_code
    pub use_system_browser: Option<bool>,      // default: false (use webview)
    pub verify_ssl: Option<bool>,

    // Additional parameters (3 phases)
    pub auth_params: Option<Vec<AdditionalParam>>,
    pub token_params: Option<Vec<AdditionalParam>>,
    pub refresh_params: Option<Vec<AdditionalParam>>,

    // Variable resolution context
    pub collection: Option<String>,
    pub environment_name: Option<String>,
    pub request_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdditionalParam {
    pub key: String,
    pub value: String,
    pub send_in: String,  // "queryparams" | "body"
    pub enabled: bool,
}
```

### Output

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2TokenResult {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    pub id_token: Option<String>, // raw JWT string if returned by provider
}
```

### Flow per grant type

**client_credentials:**
1. Resolve all fields via `build_variable_context`
2. Build form body: `grant_type=client_credentials`, scope (if set)
3. Apply `token_params` (body → form params, queryparams → URL query)
4. Apply client authentication (Basic Auth header or form body params)
5. POST to token_url
6. Parse response → `OAuth2TokenResult`

**password:**
1. Same as client_credentials but adds `username` and `password` to form body

**authorization_code:**
1. Resolve all fields
2. Generate PKCE pair if `use_pkce` is true (default)
3. Generate state if not provided
4. Build authorization URL with: `response_type=code`, `client_id`, `redirect_uri`, `scope`, `state`, `code_challenge` (if PKCE), `code_challenge_method` (if PKCE)
5. Apply `auth_params` to authorization URL (both queryparams and body treated as queryparams since it's a browser redirect)
6. Open webview or system browser based on `use_system_browser`
7. Wait for callback (120s timeout)
8. Verify state
9. Exchange code for token: POST to token_url with `grant_type=authorization_code`, `code`, `redirect_uri`, `client_id`, `client_secret`, `code_verifier` (if PKCE)
10. Apply `token_params` to exchange POST
11. Apply client authentication
12. Parse response → `OAuth2TokenResult`

**implicit:**
1. Resolve all fields
2. Generate state if not provided
3. Build authorization URL with: `response_type=token`, `client_id`, `redirect_uri`, `scope`, `state`
4. Apply `auth_params` to authorization URL
5. Open webview or system browser
6. Intercept redirect — token is in URL fragment (`#access_token=...&token_type=...`)
7. Parse fragment → `OAuth2TokenResult`

### Webview vs System Browser

**Webview (default, `use_system_browser: false`):**
- Current approach: `WebviewWindowBuilder` + `on_navigation` intercept
- Callback URL from user config (or default `https://exchange4all.local/webapp/#oidc-callback`)
- No real server needed — navigation intercepted before loading

**System browser (`use_system_browser: true`):**
- Bind `TcpListener` on `127.0.0.1:0` (random port)
- Override callback URL to `http://localhost:{port}/callback` regardless of user config
- Open OS browser via Tauri opener plugin
- Wait for redirect on TCP listener
- Parse code + state from query params
- Respond with "Authorization successful. You can close this tab."
- Shut down listener

Implemented as two private async functions in the Tauri command module:

```rust
async fn auth_code_via_webview(app: &AppHandle, config: &ResolvedOAuth2Config) -> Result<AuthCodeResult, DomainError>
async fn auth_code_via_system_browser(app: &AppHandle, config: &ResolvedOAuth2Config) -> Result<AuthCodeResult, DomainError>
```

---

## 2. `oauth2_refresh_token` Command

### Input

```rust
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2RefreshRequest {
    pub refresh_token: String,
    pub token_url: String,
    pub refresh_token_url: Option<String>, // separate refresh URL (Bruno feature)
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scope: Option<String>,
    pub client_authentication: Option<String>, // "header" | "body"
    pub verify_ssl: Option<bool>,
    pub refresh_params: Option<Vec<AdditionalParam>>,

    // Variable resolution context
    pub collection: Option<String>,
    pub environment_name: Option<String>,
    pub request_path: Option<String>,
}
```

### Output

Same `OAuth2TokenResult` as `oauth2_get_token`.

### Flow

1. Resolve all fields via `build_variable_context`
2. Determine target URL: use `refresh_token_url` if provided, otherwise `token_url`
3. Build form body: `grant_type=refresh_token`, `refresh_token`, scope (if set)
4. Apply `refresh_params` (body → form params, queryparams → URL query)
5. Apply client authentication (Basic Auth header or form body params)
6. POST to target URL
7. Parse response → `OAuth2TokenResult`

---

## 3. Variable Resolution

### Extracted method

Extract from `RequestExecutionService::resolve_request()`:

```rust
impl RequestExecutionService {
    /// Builds the flattened variable map from all backend-accessible scopes.
    /// Reused by execute(), run_load_test(), and OAuth2 commands.
    pub fn build_variable_context(
        &self,
        collection: Option<&str>,
        environment_name: Option<&str>,
        request_path: Option<&str>,
    ) -> HashMap<String, String> {
        let mut ctx = VariableContext::default();

        if let Some(col) = collection {
            let settings = self.collection_repo.get_settings(col).unwrap_or_default();
            for cv in settings.variables.iter().filter(|v| v.enabled) {
                let val = if cv.value.is_empty() { cv.initial_value.clone() } else { cv.value.clone() };
                ctx.collection.insert(cv.key.clone(), val);
            }
        }

        if let Some(name) = environment_name {
            if let Ok(env) = self.env_repo.get(name) {
                for (k, v) in env.enabled_variables() {
                    ctx.env.insert(k.to_string(), v.to_string());
                }
            }
        }

        if let (Some(col), Some(path)) = (collection, request_path) {
            if let Ok(folder_vars) = self.collection_repo.get_folder_chain_variables(col, path) {
                for cv in folder_vars.iter().filter(|v| v.enabled) {
                    let val = if cv.value.is_empty() { cv.initial_value.clone() } else { cv.value.clone() };
                    ctx.folder.insert(cv.key.clone(), val);
                }
            }
        }

        if let (Some(col), Some(path)) = (collection, request_path) {
            if let Ok(request_vars) = self.collection_repo.get_request_variables(col, path) {
                for cv in request_vars.iter().filter(|v| v.enabled) {
                    let val = if cv.value.is_empty() { cv.initial_value.clone() } else { cv.value.clone() };
                    ctx.request.insert(cv.key.clone(), val);
                }
            }
        }

        ctx.flatten()
    }
}
```

### Refactor `resolve_request`

`resolve_request()` calls `self.build_variable_context(...)` instead of inlining the scope-building logic. No behavior change — purely a refactor.

### What gets resolved in OAuth2 commands

All string fields that can contain `{{variables}}`:
- `authorization_url`, `token_url`, `callback_url`, `refresh_token_url`
- `client_id`, `client_secret`
- `username`, `password`
- `scope`, `state`
- Additional param keys and values

Not resolved (enums/booleans):
- `grant_type`, `send_in`, `client_authentication`
- `use_pkce`, `use_system_browser`, `verify_ssl`, `enabled`

### Dual resolution

Frontend resolves process env, global env, and runtime scopes before sending to Rust.
Rust resolves collection, environment, folder-chain, and request-level scopes.
Same pattern as existing `sendRequest` → `execute_request`.

---

## 4. JWT Decoding

### Command

```rust
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct JwtClaims {
    pub subject: Option<String>,     // sub
    pub issuer: Option<String>,      // iss
    pub audience: Option<String>,    // aud (stringified — can be string or array)
    pub expiry: Option<u64>,         // exp (unix timestamp)
    pub issued_at: Option<u64>,      // iat
    pub scope: Option<String>,       // scope or scp claim
    pub token_type: Option<String>,  // typ from JWT header
    pub algorithm: Option<String>,   // alg from JWT header
    pub raw_payload: String,         // full JSON payload string
}

#[tauri::command]
pub fn oauth2_decode_jwt(token: String) -> Result<JwtClaims, DomainError>
```

### Implementation

- Use `jsonwebtoken` crate with `decode_header()` for header fields
- Use `dangerous_insecure_decode()` (or equivalent) for payload without signature verification
- Extract standard OIDC claims from the payload JSON
- For `aud`: if array, join with space; if string, use as-is
- For `scope`: check both `scope` and `scp` claims (Azure AD uses `scp`)
- Return `raw_payload` as pretty-printed JSON string for the frontend to display in an expandable section

### Dependency

Add to `crates/rocket-http/Cargo.toml`:
```toml
jsonwebtoken = "9"
```

---

## 5. PKCE Toggle

### Behavior

`use_pkce: Option<bool>` on `OAuth2GetTokenRequest`:
- `Some(true)` or `None` (default for auth_code): generate PKCE pair, include `code_challenge` + `code_challenge_method` in auth URL, include `code_verifier` in token exchange
- `Some(false)`: skip all PKCE parameters

### Implementation

Conditional in the authorization_code flow:

```rust
let pkce = if config.use_pkce.unwrap_or(true) {
    Some(generate_pkce())
} else {
    None
};

// In build_auth_url:
if let Some(pkce) = &pkce {
    url.push_str(&format!("&code_challenge={}&code_challenge_method=S256", pkce.challenge));
}

// In token exchange:
if let Some(pkce) = &pkce {
    params.push(("code_verifier", pkce.verifier.as_str()));
}
```

No changes to the existing `generate_pkce()` or `PkcePair` types.

---

## 6. Additional Parameters

### Data structure

```rust
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdditionalParam {
    pub key: String,
    pub value: String,
    pub send_in: String,  // "queryparams" | "body"
    pub enabled: bool,
}
```

### Helper functions

```rust
/// Appends enabled queryparams-type additional params to a URL.
fn apply_params_to_url(url: &str, params: &[AdditionalParam]) -> String {
    let mut result = url.to_string();
    let separator = if result.contains('?') { '&' } else { '?' };
    let mut first = true;
    for p in params.iter().filter(|p| p.enabled && p.send_in == "queryparams") {
        if first {
            result.push(separator);
            first = false;
        } else {
            result.push('&');
        }
        result.push_str(&urlencoding::encode(&p.key));
        result.push('=');
        result.push_str(&urlencoding::encode(&p.value));
    }
    result
}

/// Appends enabled body-type additional params to form data.
fn apply_params_to_body(form: &mut Vec<(String, String)>, params: &[AdditionalParam]) {
    for p in params.iter().filter(|p| p.enabled && p.send_in == "body") {
        form.push((p.key.clone(), p.value.clone()));
    }
}
```

### Application per phase

| Phase | Grant types | queryparams | body |
|---|---|---|---|
| Authorization | auth_code, implicit | Appended to auth URL | Treated as queryparams (browser redirect) |
| Token exchange | client_credentials, password, auth_code | Appended to token URL | Added to POST form body |
| Refresh | all (when refreshing) | Appended to refresh URL | Added to POST form body |

---

## 7. File Structure

### New files

| File | Purpose |
|---|---|
| `crates/rocket-app/src/oauth2_service.rs` | `OAuth2Service` — variable resolution + token flows for client_credentials, password, refresh |
| `crates/rocket-http/src/jwt.rs` | `decode_jwt()` → `JwtClaims` |

### Modified files

| File | Change |
|---|---|
| `crates/rocket-app/src/execution_service.rs` | Extract `build_variable_context()` as public method; refactor `resolve_request()` to use it |
| `crates/rocket-app/src/lib.rs` | Export `oauth2_service` module |
| `crates/rocket-http/src/oauth2.rs` | Add `id_token: Option<String>` to `OAuthToken`; add `AdditionalParam` struct |
| `crates/rocket-http/src/lib.rs` | Export `jwt` module |
| `crates/rocket-http/Cargo.toml` | Add `jsonwebtoken = "9"` dependency |
| `src-tauri/src/commands/oauth2.rs` | Add `oauth2_get_token`, `oauth2_refresh_token`, `oauth2_decode_jwt` commands; refactor existing `oauth2_auth_code_flow` as internal helper |
| `src-tauri/src/lib.rs` | Register new Tauri commands |

### Module dependency

```
src-tauri/commands/oauth2.rs
  → rocket-app/oauth2_service.rs (variable resolution + non-browser flows)
  → rocket-http/oauth2.rs (acquire_token, OAuthToken)
  → rocket-http/pkce.rs (generate_pkce)
  → rocket-http/jwt.rs (decode_jwt)
```

The Tauri command layer handles webview/browser orchestration (needs `AppHandle`).
`OAuth2Service` handles variable resolution and HTTP-only flows (no Tauri dependency).

---

## 8. Testing Strategy

### Unit tests (Rust)

- `jwt.rs`: decode valid JWT, handle malformed JWT, handle missing claims, extract `scp` vs `scope`
- `oauth2_service.rs`: variable resolution applies to all fields, additional params applied correctly per phase, client_authentication header vs body
- `execution_service.rs`: `build_variable_context` returns correct precedence (refactor — existing tests should still pass)
- Additional params helpers: `apply_params_to_url`, `apply_params_to_body`

### Integration tests (ignored, manual)

- Full auth_code flow via webview against a test OIDC provider
- Full auth_code flow via system browser
- client_credentials flow with additional token params
- Refresh flow with separate refresh URL

---

## Out of Scope (Phase 2 — Frontend)

- Bruno-style section layout (Configuration / Token / Advanced Settings / Settings / Additional Parameters)
- Collapsible token display panels (Access Token + ID Token)
- "Use system browser" checkbox UI
- "Use PKCE" checkbox UI
- Settings checkboxes (auto-fetch token, auto-refresh)
- Additional Parameters tabbed key-value editor UI
- Token Source / Token ID fields
- Wiring frontend to new Rust commands (replacing JS-based token logic)
