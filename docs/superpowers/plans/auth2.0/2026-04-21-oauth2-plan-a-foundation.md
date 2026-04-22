# OAuth2 Rust Foundation — Plan A: Core Types & JWT

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Extract reusable `build_variable_context` from execution_service, extend `OAuthToken` with `id_token`, add `AdditionalParam` type to rocket-http, and implement JWT decoding.

**Architecture:** Refactor `resolve_request()` to extract variable context building into a public method. Add `id_token` to the existing `OAuthToken` response struct. Add `AdditionalParam` struct and helper functions to rocket-http. Create `jwt.rs` module in rocket-http using `jsonwebtoken` crate for payload decoding without signature verification.

**Tech Stack:** Rust, serde, jsonwebtoken, rocket-http, rocket-app, rocket-environment

**Spec:** `docs/superpowers/specs/2026-04-21-oauth2-rust-foundation-design.md`

---

## File Map

| File | Action | Purpose |
|---|---|---|
| `crates/rocket-app/src/execution_service.rs` | Modify | Extract `build_variable_context()` as public method |
| `crates/rocket-http/src/oauth2.rs` | Modify | Add `id_token` to `OAuthToken`, add `AdditionalParam` + helpers |
| `crates/rocket-http/src/jwt.rs` | Create | JWT decoding → `JwtClaims` |
| `crates/rocket-http/src/lib.rs` | Modify | Export `jwt` module |
| `crates/rocket-http/Cargo.toml` | Modify | Add `jsonwebtoken` dependency |

---

### Task 1: Extract `build_variable_context` from `execution_service.rs`

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs`

- [ ] **Step 1: Read the current `resolve_request` method**

Read the full `resolve_request` method to understand the variable context building logic:

```bash
grep -n "fn resolve_request" crates/rocket-app/src/execution_service.rs
```

Then read the method body to identify the lines that build the `VariableContext`.

- [ ] **Step 2: Add the public `build_variable_context` method**

Add this method to `impl RequestExecutionService`, **before** `resolve_request`:

```rust
    /// Builds a flattened variable map from all backend-accessible scopes
    /// (collection, environment, folder-chain, request-level).
    ///
    /// Reused by `resolve_request()`, `run_load_test()`, and OAuth2 commands.
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
                let val = if cv.value.is_empty() {
                    cv.initial_value.clone()
                } else {
                    cv.value.clone()
                };
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
                    let val = if cv.value.is_empty() {
                        cv.initial_value.clone()
                    } else {
                        cv.value.clone()
                    };
                    ctx.folder.insert(cv.key.clone(), val);
                }
            }
        }

        if let (Some(col), Some(path)) = (collection, request_path) {
            if let Ok(request_vars) = self.collection_repo.get_request_variables(col, path) {
                for cv in request_vars.iter().filter(|v| v.enabled) {
                    let val = if cv.value.is_empty() {
                        cv.initial_value.clone()
                    } else {
                        cv.value.clone()
                    };
                    ctx.request.insert(cv.key.clone(), val);
                }
            }
        }

        ctx.flatten()
    }
```

- [ ] **Step 3: Refactor `resolve_request` to use `build_variable_context`**

Replace the variable context building block inside `resolve_request` (the `let mut ctx = VariableContext::default();` through `let vars = ctx.flatten();` block) with:

```rust
        let vars = self.build_variable_context(
            input.collection.as_deref(),
            input.environment_name.as_deref(),
            input.request_path.as_deref(),
        );
```

Remove the now-unused `let mut ctx = VariableContext::default();` and all the scope-building code that was inlined.

- [ ] **Step 4: Verify compilation and tests pass**

```bash
cargo check -p rocket-app
cargo test -p rocket-app
```

Expected: All existing tests pass. This is a pure refactor — no behavior change.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/execution_service.rs
git commit -m "refactor: extract build_variable_context from resolve_request for reuse"
```

---

### Task 2: Extend `OAuthToken` + add `AdditionalParam` and helpers

**Files:**
- Modify: `crates/rocket-http/src/oauth2.rs`

- [ ] **Step 1: Add `id_token` field to `OAuthToken`**

In `crates/rocket-http/src/oauth2.rs`, find the `OAuthToken` struct and add the `id_token` field:

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    pub token_type: String,
    pub expires_in: Option<u64>,
    pub refresh_token: Option<String>,
    pub scope: Option<String>,
    /// Raw ID token JWT string, if returned by the OIDC provider.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
}
```

- [ ] **Step 2: Fix the test helper**

Update the `make_token` test helper in the `#[cfg(test)]` block to include `id_token`:

```rust
    fn make_token(expires_in: Option<u64>) -> OAuthToken {
        OAuthToken {
            access_token: "access_tok".into(),
            token_type: "Bearer".into(),
            expires_in,
            refresh_token: None,
            scope: None,
            id_token: None,
        }
    }
```

- [ ] **Step 3: Add `AdditionalParam` struct and helper functions**

Add these types and functions after the `OAuthToken` impl block but before `pub async fn acquire_token`:

```rust
/// A user-defined parameter sent at a specific phase of the OAuth2 flow.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AdditionalParam {
    pub key: String,
    pub value: String,
    /// Where to send: "queryparams" or "body".
    pub send_in: String,
    pub enabled: bool,
}

/// Appends enabled query-type additional params to a URL string.
pub fn apply_params_to_url(url: &str, params: &[AdditionalParam]) -> String {
    let query_params: Vec<&AdditionalParam> = params
        .iter()
        .filter(|p| p.enabled && p.send_in == "queryparams")
        .collect();
    if query_params.is_empty() {
        return url.to_string();
    }
    let mut result = url.to_string();
    let separator = if result.contains('?') { '&' } else { '?' };
    for (i, p) in query_params.iter().enumerate() {
        if i == 0 {
            result.push(separator);
        } else {
            result.push('&');
        }
        result.push_str(&urlencoding::encode(&p.key));
        result.push('=');
        result.push_str(&urlencoding::encode(&p.value));
    }
    result
}

/// Appends enabled body-type additional params to a form data vec.
pub fn apply_params_to_body(form: &mut Vec<(String, String)>, params: &[AdditionalParam]) {
    for p in params.iter().filter(|p| p.enabled && p.send_in == "body") {
        form.push((p.key.clone(), p.value.clone()));
    }
}
```

- [ ] **Step 4: Add `urlencoding` dependency if not already present**

Check `crates/rocket-http/Cargo.toml` for `urlencoding`. If missing:

```bash
grep urlencoding crates/rocket-http/Cargo.toml
```

If not found, add under `[dependencies]`:

```toml
urlencoding = "2"
```

- [ ] **Step 5: Write tests for `AdditionalParam` helpers**

Add these tests to the existing `#[cfg(test)] mod tests` block:

```rust
    #[test]
    fn apply_params_to_url_adds_query_params() {
        let params = vec![
            AdditionalParam {
                key: "nonce".into(),
                value: "abc123".into(),
                send_in: "queryparams".into(),
                enabled: true,
            },
            AdditionalParam {
                key: "audience".into(),
                value: "api/v1".into(),
                send_in: "queryparams".into(),
                enabled: true,
            },
        ];
        let result = apply_params_to_url("https://auth.example.com/authorize", &params);
        assert!(result.starts_with("https://auth.example.com/authorize?"));
        assert!(result.contains("nonce=abc123"));
        assert!(result.contains("audience=api%2Fv1"));
    }

    #[test]
    fn apply_params_to_url_appends_to_existing_query() {
        let params = vec![AdditionalParam {
            key: "nonce".into(),
            value: "xyz".into(),
            send_in: "queryparams".into(),
            enabled: true,
        }];
        let result = apply_params_to_url("https://auth.example.com/authorize?foo=bar", &params);
        assert!(result.contains("foo=bar&nonce=xyz"));
    }

    #[test]
    fn apply_params_to_url_skips_disabled() {
        let params = vec![AdditionalParam {
            key: "nonce".into(),
            value: "abc".into(),
            send_in: "queryparams".into(),
            enabled: false,
        }];
        let result = apply_params_to_url("https://auth.example.com/authorize", &params);
        assert_eq!(result, "https://auth.example.com/authorize");
    }

    #[test]
    fn apply_params_to_url_skips_body_type() {
        let params = vec![AdditionalParam {
            key: "secret".into(),
            value: "val".into(),
            send_in: "body".into(),
            enabled: true,
        }];
        let result = apply_params_to_url("https://auth.example.com/authorize", &params);
        assert_eq!(result, "https://auth.example.com/authorize");
    }

    #[test]
    fn apply_params_to_body_adds_body_params() {
        let params = vec![
            AdditionalParam {
                key: "audience".into(),
                value: "api/v1".into(),
                send_in: "body".into(),
                enabled: true,
            },
            AdditionalParam {
                key: "nonce".into(),
                value: "abc".into(),
                send_in: "queryparams".into(),
                enabled: true,
            },
        ];
        let mut form = vec![("grant_type".to_string(), "client_credentials".to_string())];
        apply_params_to_body(&mut form, &params);
        assert_eq!(form.len(), 2);
        assert_eq!(form[1], ("audience".to_string(), "api/v1".to_string()));
    }

    #[test]
    fn oauth_token_deserialization_with_id_token() {
        let json = r#"{
            "access_token": "ya29.abc",
            "token_type": "Bearer",
            "expires_in": 3600,
            "id_token": "eyJhbGciOiJSUzI1NiJ9.eyJzdWIiOiIxMjM0In0.signature"
        }"#;
        let token: OAuthToken = serde_json::from_str(json).unwrap();
        assert_eq!(token.access_token, "ya29.abc");
        assert!(token.id_token.is_some());
        assert!(token.id_token.unwrap().starts_with("eyJ"));
    }

    #[test]
    fn oauth_token_deserialization_without_id_token() {
        let json = r#"{
            "access_token": "ya29.abc",
            "token_type": "Bearer"
        }"#;
        let token: OAuthToken = serde_json::from_str(json).unwrap();
        assert!(token.id_token.is_none());
    }
```

- [ ] **Step 6: Run tests**

```bash
cargo test -p rocket-http -- tests
```

Expected: All tests pass including the new ones.

- [ ] **Step 7: Verify full workspace compiles**

```bash
cargo check --workspace
```

Expected: Clean compilation. The `id_token` field uses `#[serde(default)]` so existing deserialization is backward-compatible.

- [ ] **Step 8: Commit**

```bash
git add crates/rocket-http/src/oauth2.rs crates/rocket-http/Cargo.toml
git commit -m "feat: add id_token to OAuthToken, AdditionalParam struct with URL/body helpers"
```

---

### Task 3: JWT Decoding Module

**Files:**
- Modify: `crates/rocket-http/Cargo.toml`
- Create: `crates/rocket-http/src/jwt.rs`
- Modify: `crates/rocket-http/src/lib.rs`

- [ ] **Step 1: Add `jsonwebtoken` dependency**

In `crates/rocket-http/Cargo.toml`, add under `[dependencies]`:

```toml
jsonwebtoken = "9"
```

- [ ] **Step 2: Write failing tests first**

Create `crates/rocket-http/src/jwt.rs` with the struct definitions and tests but no implementation:

```rust
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};

/// Decoded JWT claims for display purposes.
/// Extracted from both the JWT header and payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JwtClaims {
    /// sub claim
    pub subject: Option<String>,
    /// iss claim
    pub issuer: Option<String>,
    /// aud claim (stringified — array joined with space)
    pub audience: Option<String>,
    /// exp claim (unix timestamp)
    pub expiry: Option<u64>,
    /// iat claim (unix timestamp)
    pub issued_at: Option<u64>,
    /// scope or scp claim
    pub scope: Option<String>,
    /// typ from JWT header
    pub token_type: Option<String>,
    /// alg from JWT header
    pub algorithm: Option<String>,
    /// Full JSON payload as a pretty-printed string
    pub raw_payload: String,
}

/// Decodes a JWT token WITHOUT signature verification.
/// Used for display purposes only (showing token metadata in the UI).
pub fn decode_jwt(_token: &str) -> Result<JwtClaims, DomainError> {
    todo!()
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

    /// Helper: builds a JWT string from header and payload JSON (no real signature).
    fn fake_jwt(header_json: &str, payload_json: &str) -> String {
        let h = URL_SAFE_NO_PAD.encode(header_json.as_bytes());
        let p = URL_SAFE_NO_PAD.encode(payload_json.as_bytes());
        format!("{h}.{p}.fake_signature")
    }

    #[test]
    fn decodes_standard_oidc_claims() {
        let token = fake_jwt(
            r#"{"alg":"RS256","typ":"JWT"}"#,
            r#"{"sub":"user123","iss":"https://auth.example.com","aud":"my-client","exp":1700000000,"iat":1699996400,"scope":"openid email profile"}"#,
        );
        let claims = decode_jwt(&token).unwrap();
        assert_eq!(claims.subject.as_deref(), Some("user123"));
        assert_eq!(claims.issuer.as_deref(), Some("https://auth.example.com"));
        assert_eq!(claims.audience.as_deref(), Some("my-client"));
        assert_eq!(claims.expiry, Some(1700000000));
        assert_eq!(claims.issued_at, Some(1699996400));
        assert_eq!(claims.scope.as_deref(), Some("openid email profile"));
        assert_eq!(claims.algorithm.as_deref(), Some("RS256"));
        assert_eq!(claims.token_type.as_deref(), Some("JWT"));
    }

    #[test]
    fn decodes_azure_scp_claim() {
        let token = fake_jwt(
            r#"{"alg":"RS256"}"#,
            r#"{"sub":"user","scp":"User.Read Mail.Read"}"#,
        );
        let claims = decode_jwt(&token).unwrap();
        assert_eq!(claims.scope.as_deref(), Some("User.Read Mail.Read"));
    }

    #[test]
    fn decodes_array_audience() {
        let token = fake_jwt(
            r#"{"alg":"RS256"}"#,
            r#"{"aud":["client-1","client-2"]}"#,
        );
        let claims = decode_jwt(&token).unwrap();
        assert_eq!(claims.audience.as_deref(), Some("client-1 client-2"));
    }

    #[test]
    fn handles_minimal_token() {
        let token = fake_jwt(
            r#"{"alg":"none"}"#,
            r#"{}"#,
        );
        let claims = decode_jwt(&token).unwrap();
        assert!(claims.subject.is_none());
        assert!(claims.issuer.is_none());
        assert!(claims.expiry.is_none());
        assert_eq!(claims.algorithm.as_deref(), Some("none"));
    }

    #[test]
    fn rejects_malformed_token() {
        let result = decode_jwt("not.a.valid.jwt");
        assert!(result.is_err());
    }

    #[test]
    fn rejects_non_jwt_string() {
        let result = decode_jwt("just-a-random-string");
        assert!(result.is_err());
    }
}
```

- [ ] **Step 3: Export jwt module from lib.rs**

In `crates/rocket-http/src/lib.rs`, add:

```rust
pub mod jwt;
pub use jwt::{decode_jwt, JwtClaims};
```

- [ ] **Step 4: Run tests to verify they fail**

```bash
cargo test -p rocket-http -- jwt::tests 2>&1 | tail -5
```

Expected: FAIL — `todo!()` panics.

- [ ] **Step 5: Implement `decode_jwt`**

Replace the `todo!()` in `decode_jwt` with the implementation:

```rust
pub fn decode_jwt(token: &str) -> Result<JwtClaims, DomainError> {
    // Split the JWT into its 3 parts.
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return Err(DomainError::InvalidInput(
            "Invalid JWT: expected 3 dot-separated parts.".into(),
        ));
    }

    // Decode header.
    let header_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[0])
        .map_err(|e| DomainError::InvalidInput(format!("Invalid JWT header encoding: {e}")))?;
    let header: serde_json::Value = serde_json::from_slice(&header_bytes)
        .map_err(|e| DomainError::InvalidInput(format!("Invalid JWT header JSON: {e}")))?;

    // Decode payload.
    let payload_bytes = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .map_err(|e| DomainError::InvalidInput(format!("Invalid JWT payload encoding: {e}")))?;
    let payload: serde_json::Value = serde_json::from_slice(&payload_bytes)
        .map_err(|e| DomainError::InvalidInput(format!("Invalid JWT payload JSON: {e}")))?;

    // Extract audience — can be string or array of strings.
    let audience = match payload.get("aud") {
        Some(serde_json::Value::String(s)) => Some(s.clone()),
        Some(serde_json::Value::Array(arr)) => {
            let parts: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(" "))
            }
        }
        _ => None,
    };

    // Extract scope — check both "scope" (standard) and "scp" (Azure AD).
    let scope = payload
        .get("scope")
        .and_then(|v| v.as_str())
        .or_else(|| payload.get("scp").and_then(|v| v.as_str()))
        .map(String::from);

    let raw_payload = serde_json::to_string_pretty(&payload)
        .unwrap_or_else(|_| payload.to_string());

    Ok(JwtClaims {
        subject: payload.get("sub").and_then(|v| v.as_str()).map(String::from),
        issuer: payload.get("iss").and_then(|v| v.as_str()).map(String::from),
        audience,
        expiry: payload.get("exp").and_then(|v| v.as_u64()),
        issued_at: payload.get("iat").and_then(|v| v.as_u64()),
        scope,
        token_type: header.get("typ").and_then(|v| v.as_str()).map(String::from),
        algorithm: header.get("alg").and_then(|v| v.as_str()).map(String::from),
        raw_payload,
    })
}
```

Note: We're doing manual base64 + serde_json decoding instead of using the `jsonwebtoken` crate's decode function, because `jsonwebtoken`'s insecure decode still requires specifying an algorithm and validation config. Manual decoding is simpler for display-only use. However, keep the `jsonwebtoken` dependency — it will be useful if we add signature verification later.

Actually, on second thought — if we're doing manual decoding, we don't need `jsonwebtoken` at all for this task. **Remove the `jsonwebtoken` dependency added in Step 1** if this is the approach. The `base64` crate is already a workspace dependency.

- [ ] **Step 6: Re-evaluate jsonwebtoken dependency**

If the manual decode approach works (Step 5), remove `jsonwebtoken` from `Cargo.toml` since it's not needed:

```bash
grep jsonwebtoken crates/rocket-http/Cargo.toml
```

If present, remove the line. We're only using `base64` (already a dep) and `serde_json` (already a dep).

- [ ] **Step 7: Add base64 import at the top of jwt.rs**

Ensure the import is present at the top of the file:

```rust
use base64::Engine;
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
```

- [ ] **Step 8: Run tests**

```bash
cargo test -p rocket-http -- jwt::tests
```

Expected: All 6 tests pass.

- [ ] **Step 9: Run full workspace check**

```bash
cargo check --workspace
cargo test --workspace
```

Expected: Clean compilation, all tests pass.

- [ ] **Step 10: Commit**

```bash
git add crates/rocket-http/src/jwt.rs crates/rocket-http/src/lib.rs crates/rocket-http/Cargo.toml
git commit -m "feat: JWT decoding module for OAuth2 ID Token display"
```
