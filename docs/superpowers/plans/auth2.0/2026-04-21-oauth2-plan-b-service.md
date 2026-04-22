# OAuth2 Rust Foundation — Plan B: OAuth2Service

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create `OAuth2Service` in `rocket-app` that handles variable resolution + token acquisition for client_credentials, password, and refresh flows. Authorization_code and implicit browser orchestration are handled in Plan C (Tauri commands).

**Architecture:** `OAuth2Service` takes `EnvironmentRepository` + `CollectionRepository` refs (same as `RequestExecutionService`). It builds the variable context using the extracted `build_variable_context` pattern, resolves all OAuth2 fields, then makes HTTP calls via `reqwest::Client`. Does NOT depend on Tauri.

**Tech Stack:** Rust, reqwest, serde, rocket-app, rocket-http, rocket-environment

**Spec:** `docs/superpowers/specs/2026-04-21-oauth2-rust-foundation-design.md`

**Prerequisite:** Plan A complete (build_variable_context extracted, AdditionalParam + OAuthToken extended).

---

## File Map

| File | Action | Purpose |
|---|---|---|
| `crates/rocket-app/src/oauth2_service.rs` | Create | OAuth2Service with get_token + refresh_token methods |
| `crates/rocket-app/src/lib.rs` | Modify | Export oauth2_service module |

---

### Task 1: OAuth2Service — struct, constructor, variable resolution helper

**Files:**
- Create: `crates/rocket-app/src/oauth2_service.rs`
- Modify: `crates/rocket-app/src/lib.rs`

- [ ] **Step 1: Create the module file with types and constructor**

Create `crates/rocket-app/src/oauth2_service.rs`:

```rust
use std::collections::HashMap;

use rocket_collection::CollectionRepository;
use rocket_environment::{resolve, EnvironmentRepository, VariableContext};
use rocket_http::{
    acquire_token, apply_params_to_body, apply_params_to_url, AdditionalParam, OAuthConfig,
    OAuthToken,
};
use rocket_shared::error::{DomainError, DomainResult};
use serde::Deserialize;

// ─── Input types ───────────────────────────────────────────────────

/// Request to acquire a new OAuth2 token (all grant types).
/// Authorization_code and implicit browser orchestration is handled
/// by the Tauri command layer — this service handles the HTTP parts.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2GetTokenRequest {
    pub grant_type: String,

    // URLs
    pub authorization_url: Option<String>,
    pub token_url: Option<String>,
    pub callback_url: Option<String>,

    // Credentials
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scope: Option<String>,
    pub state: Option<String>,

    // Password grant
    pub username: Option<String>,
    pub password: Option<String>,

    // Options
    pub client_authentication: Option<String>,
    pub use_pkce: Option<bool>,
    pub use_system_browser: Option<bool>,
    pub verify_ssl: Option<bool>,

    // Additional parameters
    pub auth_params: Option<Vec<AdditionalParam>>,
    pub token_params: Option<Vec<AdditionalParam>>,
    pub refresh_params: Option<Vec<AdditionalParam>>,

    // Variable resolution context
    pub collection: Option<String>,
    pub environment_name: Option<String>,
    pub request_path: Option<String>,
}

/// Request to refresh an existing OAuth2 token.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OAuth2RefreshRequest {
    pub refresh_token: String,
    pub token_url: String,
    pub refresh_token_url: Option<String>,
    pub client_id: String,
    pub client_secret: Option<String>,
    pub scope: Option<String>,
    pub client_authentication: Option<String>,
    pub verify_ssl: Option<bool>,
    pub refresh_params: Option<Vec<AdditionalParam>>,

    // Variable resolution context
    pub collection: Option<String>,
    pub environment_name: Option<String>,
    pub request_path: Option<String>,
}

/// Internal struct with all fields resolved (no more {{variables}}).
#[derive(Debug, Clone)]
pub(crate) struct ResolvedOAuth2Config {
    pub grant_type: String,
    pub authorization_url: String,
    pub token_url: String,
    pub callback_url: String,
    pub client_id: String,
    pub client_secret: String,
    pub scope: Option<String>,
    pub state: Option<String>,
    pub username: String,
    pub password: String,
    pub client_authentication: String,
    pub use_pkce: bool,
    pub use_system_browser: bool,
    pub verify_ssl: bool,
    pub auth_params: Vec<AdditionalParam>,
    pub token_params: Vec<AdditionalParam>,
    pub refresh_params: Vec<AdditionalParam>,
}

// ─── Service ───────────────────────────────────────────────────────

pub struct OAuth2Service {
    env_repo: Box<dyn EnvironmentRepository>,
    collection_repo: Box<dyn CollectionRepository>,
}

impl OAuth2Service {
    pub fn new(
        env_repo: Box<dyn EnvironmentRepository>,
        collection_repo: Box<dyn CollectionRepository>,
    ) -> Self {
        Self {
            env_repo,
            collection_repo,
        }
    }

    /// Builds a flattened variable map from all backend-accessible scopes.
    fn build_variable_context(
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

    /// Resolves all {{variables}} in the get-token request fields.
    pub(crate) fn resolve_get_token_request(
        &self,
        req: &OAuth2GetTokenRequest,
    ) -> ResolvedOAuth2Config {
        let vars = self.build_variable_context(
            req.collection.as_deref(),
            req.environment_name.as_deref(),
            req.request_path.as_deref(),
        );
        let r = |s: &str| resolve(s, &vars).output;

        // Resolve additional param values (keys and values both).
        let resolve_params = |params: &Option<Vec<AdditionalParam>>| -> Vec<AdditionalParam> {
            params
                .as_deref()
                .unwrap_or_default()
                .iter()
                .map(|p| AdditionalParam {
                    key: r(&p.key),
                    value: r(&p.value),
                    send_in: p.send_in.clone(),
                    enabled: p.enabled,
                })
                .collect()
        };

        ResolvedOAuth2Config {
            grant_type: req.grant_type.clone(),
            authorization_url: r(req.authorization_url.as_deref().unwrap_or_default()),
            token_url: r(req.token_url.as_deref().unwrap_or_default()),
            callback_url: r(req.callback_url.as_deref().unwrap_or_default()),
            client_id: r(&req.client_id),
            client_secret: r(req.client_secret.as_deref().unwrap_or_default()),
            scope: req.scope.as_deref().map(|s| r(s)).filter(|s| !s.is_empty()),
            state: req.state.as_deref().map(|s| r(s)).filter(|s| !s.is_empty()),
            username: r(req.username.as_deref().unwrap_or_default()),
            password: r(req.password.as_deref().unwrap_or_default()),
            client_authentication: req
                .client_authentication
                .clone()
                .unwrap_or_else(|| "body".into()),
            use_pkce: req.use_pkce.unwrap_or(true),
            use_system_browser: req.use_system_browser.unwrap_or(false),
            verify_ssl: req.verify_ssl.unwrap_or(true),
            auth_params: resolve_params(&req.auth_params),
            token_params: resolve_params(&req.token_params),
            refresh_params: resolve_params(&req.refresh_params),
        }
    }
}
```

- [ ] **Step 2: Export from lib.rs**

In `crates/rocket-app/src/lib.rs`, add:

```rust
pub mod oauth2_service;
```

- [ ] **Step 3: Verify compilation**

```bash
cargo check -p rocket-app
```

Expected: Clean compilation.

- [ ] **Step 4: Commit**

```bash
git add crates/rocket-app/src/oauth2_service.rs crates/rocket-app/src/lib.rs
git commit -m "feat: OAuth2Service struct with variable resolution for get-token requests"
```

---

### Task 2: client_credentials and password flows

**Files:**
- Modify: `crates/rocket-app/src/oauth2_service.rs`

- [ ] **Step 1: Write failing tests**

Add a `#[cfg(test)]` module at the bottom of `oauth2_service.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::{CollectionRepository, CollectionSummary, CollectionSettings, CollectionVariable};
    use rocket_environment::{Environment, EnvironmentRepository, EnvironmentVariable};
    use rocket_shared::error::DomainResult;

    // ─── Stub repos ──────────────────────────────────────

    struct StubEnvRepo {
        vars: HashMap<String, String>,
    }

    impl StubEnvRepo {
        fn empty() -> Self {
            Self { vars: HashMap::new() }
        }
        fn with_vars(vars: &[(&str, &str)]) -> Self {
            Self {
                vars: vars.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(),
            }
        }
    }

    impl EnvironmentRepository for StubEnvRepo {
        fn list(&self) -> DomainResult<Vec<String>> { Ok(vec![]) }
        fn get(&self, _name: &str) -> DomainResult<Environment> {
            let env_vars: Vec<EnvironmentVariable> = self.vars.iter().map(|(k, v)| {
                EnvironmentVariable {
                    key: k.clone(),
                    value: v.clone(),
                    enabled: true,
                    secret: false,
                }
            }).collect();
            Ok(Environment::new("test", env_vars))
        }
        fn save(&self, _env: &Environment) -> DomainResult<()> { Ok(()) }
        fn delete(&self, _name: &str) -> DomainResult<()> { Ok(()) }
        fn rename(&self, _old: &str, _new: &str) -> DomainResult<()> { Ok(()) }
    }

    struct StubCollectionRepo;

    impl CollectionRepository for StubCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> { Ok(vec![]) }
        fn get_settings(&self, _name: &str) -> DomainResult<CollectionSettings> {
            Ok(CollectionSettings::default())
        }
        fn save_settings(&self, _name: &str, _settings: &CollectionSettings) -> DomainResult<()> {
            Ok(())
        }
        fn get_folder_chain_variables(&self, _col: &str, _path: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn get_request_variables(&self, _col: &str, _path: &str) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
    }

    fn make_service() -> OAuth2Service {
        OAuth2Service::new(
            Box::new(StubEnvRepo::empty()),
            Box::new(StubCollectionRepo),
        )
    }

    fn make_service_with_env(vars: &[(&str, &str)]) -> OAuth2Service {
        OAuth2Service::new(
            Box::new(StubEnvRepo::with_vars(vars)),
            Box::new(StubCollectionRepo),
        )
    }

    // ─── Tests ───────────────────────────────────────────

    #[test]
    fn resolve_replaces_variables_in_all_fields() {
        let svc = make_service_with_env(&[
            ("base_url", "https://auth.example.com"),
            ("my_client", "client-123"),
            ("my_secret", "secret-456"),
        ]);
        let req = OAuth2GetTokenRequest {
            grant_type: "client_credentials".into(),
            authorization_url: None,
            token_url: Some("{{base_url}}/token".into()),
            callback_url: None,
            client_id: "{{my_client}}".into(),
            client_secret: Some("{{my_secret}}".into()),
            scope: Some("openid".into()),
            state: None,
            username: None,
            password: None,
            client_authentication: None,
            use_pkce: None,
            use_system_browser: None,
            verify_ssl: None,
            auth_params: None,
            token_params: Some(vec![AdditionalParam {
                key: "audience".into(),
                value: "{{base_url}}/api".into(),
                send_in: "body".into(),
                enabled: true,
            }]),
            refresh_params: None,
            collection: None,
            environment_name: Some("test".into()),
            request_path: None,
        };
        let resolved = svc.resolve_get_token_request(&req);
        assert_eq!(resolved.token_url, "https://auth.example.com/token");
        assert_eq!(resolved.client_id, "client-123");
        assert_eq!(resolved.client_secret, "secret-456");
        assert_eq!(resolved.token_params[0].value, "https://auth.example.com/api");
    }

    #[test]
    fn build_client_credentials_form_body_auth() {
        let config = ResolvedOAuth2Config {
            grant_type: "client_credentials".into(),
            authorization_url: String::new(),
            token_url: "https://auth.example.com/token".into(),
            callback_url: String::new(),
            client_id: "my-client".into(),
            client_secret: "my-secret".into(),
            scope: Some("openid".into()),
            state: None,
            username: String::new(),
            password: String::new(),
            client_authentication: "body".into(),
            use_pkce: false,
            use_system_browser: false,
            verify_ssl: true,
            auth_params: vec![],
            token_params: vec![AdditionalParam {
                key: "audience".into(),
                value: "api/v1".into(),
                send_in: "body".into(),
                enabled: true,
            }],
            refresh_params: vec![],
        };
        let (form, headers) = OAuth2Service::build_token_request_parts(&config);
        assert!(form.iter().any(|(k, v)| k == "grant_type" && v == "client_credentials"));
        assert!(form.iter().any(|(k, v)| k == "client_id" && v == "my-client"));
        assert!(form.iter().any(|(k, v)| k == "client_secret" && v == "my-secret"));
        assert!(form.iter().any(|(k, v)| k == "scope" && v == "openid"));
        assert!(form.iter().any(|(k, v)| k == "audience" && v == "api/v1"));
        assert!(headers.is_empty()); // body auth means no auth header
    }

    #[test]
    fn build_client_credentials_header_auth() {
        let config = ResolvedOAuth2Config {
            grant_type: "client_credentials".into(),
            authorization_url: String::new(),
            token_url: "https://auth.example.com/token".into(),
            callback_url: String::new(),
            client_id: "my-client".into(),
            client_secret: "my-secret".into(),
            scope: None,
            state: None,
            username: String::new(),
            password: String::new(),
            client_authentication: "header".into(),
            use_pkce: false,
            use_system_browser: false,
            verify_ssl: true,
            auth_params: vec![],
            token_params: vec![],
            refresh_params: vec![],
        };
        let (form, headers) = OAuth2Service::build_token_request_parts(&config);
        // client_id and client_secret should NOT be in form body
        assert!(!form.iter().any(|(k, _)| k == "client_id"));
        assert!(!form.iter().any(|(k, _)| k == "client_secret"));
        // Should have Authorization: Basic header
        assert!(headers.iter().any(|(k, v)| k == "Authorization" && v.starts_with("Basic ")));
    }

    #[test]
    fn build_password_grant_includes_username_password() {
        let config = ResolvedOAuth2Config {
            grant_type: "password".into(),
            authorization_url: String::new(),
            token_url: "https://auth.example.com/token".into(),
            callback_url: String::new(),
            client_id: "my-client".into(),
            client_secret: "my-secret".into(),
            scope: None,
            state: None,
            username: "user@example.com".into(),
            password: "p@ssw0rd".into(),
            client_authentication: "body".into(),
            use_pkce: false,
            use_system_browser: false,
            verify_ssl: true,
            auth_params: vec![],
            token_params: vec![],
            refresh_params: vec![],
        };
        let (form, _) = OAuth2Service::build_token_request_parts(&config);
        assert!(form.iter().any(|(k, v)| k == "username" && v == "user@example.com"));
        assert!(form.iter().any(|(k, v)| k == "password" && v == "p@ssw0rd"));
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p rocket-app -- oauth2_service::tests 2>&1 | tail -10
```

Expected: FAIL — `build_token_request_parts` doesn't exist yet. The `resolve_replaces_variables_in_all_fields` test should pass (resolution was implemented in Task 1).

- [ ] **Step 3: Implement `build_token_request_parts`**

Add this static method to `impl OAuth2Service`:

```rust
    /// Builds form body params and extra headers for a token request.
    /// Used by client_credentials, password, and the code-exchange step of auth_code.
    pub(crate) fn build_token_request_parts(
        config: &ResolvedOAuth2Config,
    ) -> (Vec<(String, String)>, Vec<(String, String)>) {
        let mut form: Vec<(String, String)> = vec![
            ("grant_type".into(), config.grant_type.clone()),
        ];
        let mut headers: Vec<(String, String)> = vec![];

        // Scope
        if let Some(scope) = &config.scope {
            form.push(("scope".into(), scope.clone()));
        }

        // Client authentication
        if config.client_authentication == "header" {
            use base64::Engine;
            let credentials = format!("{}:{}", config.client_id, config.client_secret);
            let encoded = base64::engine::general_purpose::STANDARD.encode(credentials.as_bytes());
            headers.push(("Authorization".into(), format!("Basic {encoded}")));
        } else {
            form.push(("client_id".into(), config.client_id.clone()));
            form.push(("client_secret".into(), config.client_secret.clone()));
        }

        // Password grant fields
        if config.grant_type == "password" {
            form.push(("username".into(), config.username.clone()));
            form.push(("password".into(), config.password.clone()));
        }

        // Additional token params
        apply_params_to_body(&mut form, &config.token_params);

        (form, headers)
    }
```

Add `use base64::Engine;` at the top of the file if not already imported.

- [ ] **Step 4: Run tests**

```bash
cargo test -p rocket-app -- oauth2_service::tests
```

Expected: All tests pass.

- [ ] **Step 5: Implement `client_credentials_flow` and `password_flow`**

Add these methods to `impl OAuth2Service`:

```rust
    /// Executes client_credentials or password grant.
    /// Returns the token response.
    pub async fn get_token_direct(
        &self,
        config: &ResolvedOAuth2Config,
    ) -> DomainResult<OAuthToken> {
        let token_url = if config.token_url.is_empty() {
            return Err(DomainError::InvalidInput("Token URL is required.".into()));
        } else {
            &config.token_url
        };

        let (form, extra_headers) = Self::build_token_request_parts(config);

        // Apply additional token params to URL (queryparams type).
        let url = apply_params_to_url(token_url, &config.token_params);

        let client = reqwest::ClientBuilder::new()
            .danger_accept_invalid_certs(!config.verify_ssl)
            .build()
            .map_err(|e| DomainError::Internal(format!("Failed to build HTTP client: {e}")))?;

        let mut request = client
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded");

        for (key, value) in &extra_headers {
            request = request.header(key, value);
        }

        let body = form
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        let resp = request
            .body(body)
            .send()
            .await
            .map_err(|e| DomainError::Internal(format!("Token request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(DomainError::Internal(format!(
                "Token endpoint returned {status}: {body}"
            )));
        }

        resp.json::<OAuthToken>()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to parse token response: {e}")))
    }
```

Add `use urlencoding;` to the imports if needed.

- [ ] **Step 6: Implement `refresh_token`**

Add this method:

```rust
    /// Refreshes an OAuth2 token.
    pub async fn refresh_token(
        &self,
        req: &OAuth2RefreshRequest,
    ) -> DomainResult<OAuthToken> {
        let vars = self.build_variable_context(
            req.collection.as_deref(),
            req.environment_name.as_deref(),
            req.request_path.as_deref(),
        );
        let r = |s: &str| resolve(s, &vars).output;

        let token_url = r(&req.token_url);
        let refresh_url = req
            .refresh_token_url
            .as_deref()
            .map(|u| r(u))
            .filter(|u| !u.is_empty())
            .unwrap_or_else(|| token_url.clone());
        let client_id = r(&req.client_id);
        let client_secret = r(req.client_secret.as_deref().unwrap_or_default());
        let refresh_token = r(&req.refresh_token);
        let scope = req.scope.as_deref().map(|s| r(s)).filter(|s| !s.is_empty());
        let client_auth = req
            .client_authentication
            .clone()
            .unwrap_or_else(|| "body".into());
        let verify_ssl = req.verify_ssl.unwrap_or(true);

        // Resolve additional refresh params.
        let refresh_params: Vec<AdditionalParam> = req
            .refresh_params
            .as_deref()
            .unwrap_or_default()
            .iter()
            .map(|p| AdditionalParam {
                key: r(&p.key),
                value: r(&p.value),
                send_in: p.send_in.clone(),
                enabled: p.enabled,
            })
            .collect();

        // Build form body.
        let mut form: Vec<(String, String)> = vec![
            ("grant_type".into(), "refresh_token".into()),
            ("refresh_token".into(), refresh_token),
        ];
        if let Some(s) = &scope {
            form.push(("scope".into(), s.clone()));
        }

        let mut extra_headers: Vec<(String, String)> = vec![];
        if client_auth == "header" {
            use base64::Engine;
            let creds = format!("{client_id}:{client_secret}");
            let encoded = base64::engine::general_purpose::STANDARD.encode(creds.as_bytes());
            extra_headers.push(("Authorization".into(), format!("Basic {encoded}")));
        } else {
            form.push(("client_id".into(), client_id));
            form.push(("client_secret".into(), client_secret));
        }

        // Apply additional refresh params.
        apply_params_to_body(&mut form, &refresh_params);
        let url = apply_params_to_url(&refresh_url, &refresh_params);

        let client = reqwest::ClientBuilder::new()
            .danger_accept_invalid_certs(!verify_ssl)
            .build()
            .map_err(|e| DomainError::Internal(format!("Failed to build HTTP client: {e}")))?;

        let mut request = client
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded");

        for (key, value) in &extra_headers {
            request = request.header(key, value);
        }

        let body = form
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        let resp = request
            .body(body)
            .send()
            .await
            .map_err(|e| DomainError::Internal(format!("Refresh token request failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(DomainError::Internal(format!(
                "Refresh endpoint returned {status}: {body}"
            )));
        }

        resp.json::<OAuthToken>()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to parse refresh response: {e}")))
    }
```

- [ ] **Step 7: Verify compilation**

```bash
cargo check -p rocket-app
```

Expected: Clean. Note: the stub repos in tests may need additional trait methods implemented depending on the `CollectionRepository` trait. Read compile errors and add stub implementations as needed (return `Ok(default)` for any unimplemented methods).

- [ ] **Step 8: Run tests**

```bash
cargo test -p rocket-app -- oauth2_service::tests
```

Expected: All tests pass.

- [ ] **Step 9: Commit**

```bash
git add crates/rocket-app/src/oauth2_service.rs
git commit -m "feat: OAuth2Service — client_credentials, password, and refresh token flows"
```

---

### Task 3: Code exchange helper for authorization_code

**Files:**
- Modify: `crates/rocket-app/src/oauth2_service.rs`

The Tauri command will handle the browser/webview part and get back a `code`. It then needs to exchange that code for a token. This method handles that exchange.

- [ ] **Step 1: Write test for code exchange**

Add to the test module:

```rust
    #[test]
    fn build_code_exchange_form() {
        let config = ResolvedOAuth2Config {
            grant_type: "authorization_code".into(),
            authorization_url: "https://auth.example.com/authorize".into(),
            token_url: "https://auth.example.com/token".into(),
            callback_url: "http://localhost:9876/callback".into(),
            client_id: "my-client".into(),
            client_secret: "my-secret".into(),
            scope: Some("openid".into()),
            state: None,
            username: String::new(),
            password: String::new(),
            client_authentication: "body".into(),
            use_pkce: true,
            use_system_browser: false,
            verify_ssl: true,
            auth_params: vec![],
            token_params: vec![AdditionalParam {
                key: "resource".into(),
                value: "https://api.example.com".into(),
                send_in: "body".into(),
                enabled: true,
            }],
            refresh_params: vec![],
        };
        let form = OAuth2Service::build_code_exchange_form(
            &config,
            "AUTH_CODE_123",
            "http://localhost:9876/callback",
            Some("verifier_abc"),
        );
        assert!(form.iter().any(|(k, v)| k == "grant_type" && v == "authorization_code"));
        assert!(form.iter().any(|(k, v)| k == "code" && v == "AUTH_CODE_123"));
        assert!(form.iter().any(|(k, v)| k == "redirect_uri" && v == "http://localhost:9876/callback"));
        assert!(form.iter().any(|(k, v)| k == "code_verifier" && v == "verifier_abc"));
        assert!(form.iter().any(|(k, v)| k == "client_id" && v == "my-client"));
        assert!(form.iter().any(|(k, v)| k == "resource" && v == "https://api.example.com"));
    }

    #[test]
    fn build_code_exchange_form_no_pkce() {
        let config = ResolvedOAuth2Config {
            grant_type: "authorization_code".into(),
            authorization_url: String::new(),
            token_url: "https://auth.example.com/token".into(),
            callback_url: String::new(),
            client_id: "my-client".into(),
            client_secret: "my-secret".into(),
            scope: None,
            state: None,
            username: String::new(),
            password: String::new(),
            client_authentication: "header".into(),
            use_pkce: false,
            use_system_browser: false,
            verify_ssl: true,
            auth_params: vec![],
            token_params: vec![],
            refresh_params: vec![],
        };
        let form = OAuth2Service::build_code_exchange_form(
            &config,
            "CODE",
            "http://localhost/cb",
            None,
        );
        assert!(!form.iter().any(|(k, _)| k == "code_verifier"));
        // header auth: client_id/secret NOT in form
        assert!(!form.iter().any(|(k, _)| k == "client_id"));
    }
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cargo test -p rocket-app -- oauth2_service::tests::build_code_exchange 2>&1 | tail -5
```

- [ ] **Step 3: Implement `build_code_exchange_form`**

Add this static method to `impl OAuth2Service`:

```rust
    /// Builds the form body for exchanging an authorization code for a token.
    /// Called by the Tauri command after the browser flow returns a code.
    pub(crate) fn build_code_exchange_form(
        config: &ResolvedOAuth2Config,
        code: &str,
        redirect_uri: &str,
        code_verifier: Option<&str>,
    ) -> Vec<(String, String)> {
        let mut form: Vec<(String, String)> = vec![
            ("grant_type".into(), "authorization_code".into()),
            ("code".into(), code.into()),
            ("redirect_uri".into(), redirect_uri.into()),
        ];

        if let Some(verifier) = code_verifier {
            form.push(("code_verifier".into(), verifier.into()));
        }

        // Client authentication
        if config.client_authentication != "header" {
            form.push(("client_id".into(), config.client_id.clone()));
            form.push(("client_secret".into(), config.client_secret.clone()));
        }

        if let Some(scope) = &config.scope {
            form.push(("scope".into(), scope.clone()));
        }

        // Additional token params (body type)
        apply_params_to_body(&mut form, &config.token_params);

        form
    }

    /// Exchanges an authorization code for a token.
    /// Called by the Tauri command after the browser/webview flow completes.
    pub async fn exchange_code_for_token(
        &self,
        config: &ResolvedOAuth2Config,
        code: &str,
        redirect_uri: &str,
        code_verifier: Option<&str>,
    ) -> DomainResult<OAuthToken> {
        if config.token_url.is_empty() {
            return Err(DomainError::InvalidInput("Token URL is required.".into()));
        }

        let form = Self::build_code_exchange_form(config, code, redirect_uri, code_verifier);

        let url = apply_params_to_url(&config.token_url, &config.token_params);

        let client = reqwest::ClientBuilder::new()
            .danger_accept_invalid_certs(!config.verify_ssl)
            .build()
            .map_err(|e| DomainError::Internal(format!("Failed to build HTTP client: {e}")))?;

        let mut request = client
            .post(&url)
            .header("Content-Type", "application/x-www-form-urlencoded");

        // Add Basic Auth header if configured.
        if config.client_authentication == "header" {
            use base64::Engine;
            let creds = format!("{}:{}", config.client_id, config.client_secret);
            let encoded = base64::engine::general_purpose::STANDARD.encode(creds.as_bytes());
            request = request.header("Authorization", format!("Basic {encoded}"));
        }

        let body = form
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");

        let resp = request
            .body(body)
            .send()
            .await
            .map_err(|e| DomainError::Internal(format!("Code exchange failed: {e}")))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(DomainError::Internal(format!(
                "Token endpoint returned {status}: {body}"
            )));
        }

        resp.json::<OAuthToken>()
            .await
            .map_err(|e| DomainError::Internal(format!("Failed to parse token response: {e}")))
    }
```

- [ ] **Step 4: Run tests**

```bash
cargo test -p rocket-app -- oauth2_service::tests
```

Expected: All tests pass.

- [ ] **Step 5: Run full workspace check**

```bash
cargo check --workspace
cargo test --workspace
```

Expected: Clean.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/oauth2_service.rs
git commit -m "feat: OAuth2Service — code exchange helper for authorization_code flow"
```
