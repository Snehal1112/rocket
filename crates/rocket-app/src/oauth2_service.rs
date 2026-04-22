use std::collections::HashMap;

use rocket_collection::CollectionRepository;
use rocket_environment::{resolve, EnvironmentRepository, VariableContext};
use rocket_http::{apply_params_to_body, apply_params_to_url, AdditionalParam, OAuthToken};
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
    /// How to send client credentials: "header" (HTTP Basic) or "body"
    /// (form fields). Defaults to "body" when unset.
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

    pub force_reauth: Option<bool>,
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
    /// How to send client credentials: "header" (HTTP Basic) or "body"
    /// (form fields). Defaults to "body" when unset.
    pub client_authentication: Option<String>,
    pub verify_ssl: Option<bool>,
    pub refresh_params: Option<Vec<AdditionalParam>>,

    // Variable resolution context
    pub collection: Option<String>,
    pub environment_name: Option<String>,
    pub request_path: Option<String>,
}

/// Internal struct with all fields resolved (no more {{variables}}).
/// `pub` so Tauri command layer (Plan C) can construct + pass this
/// across the auth_code flow boundary without re-resolving.
#[derive(Debug, Clone)]
pub struct ResolvedOAuth2Config {
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
    pub force_reauth: bool,
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
    /// Mirrors `RequestExecutionService::build_variable_context` — kept here
    /// to avoid cross-service coupling.
    pub(crate) fn build_variable_context(
        &self,
        collection: Option<&str>,
        environment_name: Option<&str>,
        request_path: Option<&str>,
    ) -> HashMap<String, String> {
        // Precedence (lowest → highest): collection < env < folder < request.
        let mut ctx = VariableContext::default();

        let effective_val = |cv: &rocket_collection::CollectionVariable| -> String {
            if cv.value.is_empty() {
                cv.initial_value.clone()
            } else {
                cv.value.clone()
            }
        };

        if let Some(col) = collection {
            let settings = self.collection_repo.get_settings(col).unwrap_or_default();
            for cv in settings.variables.iter().filter(|v| v.enabled) {
                ctx.collection.insert(cv.key.clone(), effective_val(cv));
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
                    ctx.folder.insert(cv.key.clone(), effective_val(cv));
                }
            }
        }

        if let (Some(col), Some(path)) = (collection, request_path) {
            if let Ok(request_vars) = self.collection_repo.get_request_variables(col, path) {
                for cv in request_vars.iter().filter(|v| v.enabled) {
                    ctx.request.insert(cv.key.clone(), effective_val(cv));
                }
            }
        }

        ctx.flatten()
    }

    /// Builds the `Authorization: Basic <base64(client_id:client_secret)>` header tuple
    /// used for `client_authentication = "header"`.
    fn basic_auth_header(client_id: &str, client_secret: &str) -> (String, String) {
        use base64::Engine;
        let creds = format!("{client_id}:{client_secret}");
        let encoded = base64::engine::general_purpose::STANDARD.encode(creds.as_bytes());
        ("Authorization".into(), format!("Basic {encoded}"))
    }

    /// Builds form body params and extra headers for a token request.
    /// Used by client_credentials, password, and the code-exchange step of auth_code.
    pub(crate) fn build_token_request_parts(
        config: &ResolvedOAuth2Config,
    ) -> (Vec<(String, String)>, Vec<(String, String)>) {
        let mut form: Vec<(String, String)> = vec![
            ("grant_type".into(), config.grant_type.clone()),
        ];
        let mut headers: Vec<(String, String)> = vec![];

        // Scope (applied before client auth to keep ordering predictable).
        if let Some(scope) = &config.scope {
            form.push(("scope".into(), scope.clone()));
        }

        // Client authentication: header = HTTP Basic, body = form fields.
        if config.client_authentication == "header" {
            headers.push(Self::basic_auth_header(&config.client_id, &config.client_secret));
        } else {
            form.push(("client_id".into(), config.client_id.clone()));
            form.push(("client_secret".into(), config.client_secret.clone()));
        }

        // Password grant fields.
        if config.grant_type == "password" {
            form.push(("username".into(), config.username.clone()));
            form.push(("password".into(), config.password.clone()));
        }

        // Additional token params (body-type only).
        apply_params_to_body(&mut form, &config.token_params);

        (form, headers)
    }

    /// Shared HTTP dispatch for all OAuth2 token endpoint POSTs.
    /// Builds the reqwest client, posts the form, and parses the OAuthToken response.
    async fn post_token_request(
        url: &str,
        form: &[(String, String)],
        extra_headers: &[(String, String)],
        verify_ssl: bool,
    ) -> DomainResult<OAuthToken> {
        let client = reqwest::ClientBuilder::new()
            .danger_accept_invalid_certs(!verify_ssl)
            .build()
            .map_err(|e| DomainError::Internal(format!("Failed to build HTTP client: {e}")))?;

        let mut request = client.post(url).form(form);
        for (key, value) in extra_headers {
            request = request.header(key, value);
        }

        let resp = request
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

    /// Executes client_credentials or password grant against the token endpoint.
    pub async fn get_token_direct(
        &self,
        config: &ResolvedOAuth2Config,
    ) -> DomainResult<OAuthToken> {
        if config.token_url.is_empty() {
            return Err(DomainError::InvalidInput("Token URL is required.".into()));
        }

        let (form, extra_headers) = Self::build_token_request_parts(config);
        // Apply queryparam-type extra token params to the URL (body-type were added to `form`).
        let url = apply_params_to_url(&config.token_url, &config.token_params);
        Self::post_token_request(&url, &form, &extra_headers, config.verify_ssl).await
    }

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
            .map(r)
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

        let mut form: Vec<(String, String)> = vec![
            ("grant_type".into(), "refresh_token".into()),
            ("refresh_token".into(), refresh_token),
        ];
        if let Some(s) = &scope {
            form.push(("scope".into(), s.clone()));
        }

        let mut extra_headers: Vec<(String, String)> = vec![];
        if client_auth == "header" {
            extra_headers.push(Self::basic_auth_header(&client_id, &client_secret));
        } else {
            form.push(("client_id".into(), client_id));
            form.push(("client_secret".into(), client_secret));
        }

        apply_params_to_body(&mut form, &refresh_params);
        let url = apply_params_to_url(&refresh_url, &refresh_params);

        Self::post_token_request(&url, &form, &extra_headers, verify_ssl).await
    }

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

        // Client authentication: when `header`, credentials go in an HTTP Basic header;
        // when `body`, they go in the form body here.
        if config.client_authentication != "header" {
            form.push(("client_id".into(), config.client_id.clone()));
            form.push(("client_secret".into(), config.client_secret.clone()));
        }

        if let Some(scope) = &config.scope {
            form.push(("scope".into(), scope.clone()));
        }

        // Additional body-type token params (e.g. `resource`).
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

        let mut extra_headers: Vec<(String, String)> = vec![];
        if config.client_authentication == "header" {
            extra_headers.push(Self::basic_auth_header(&config.client_id, &config.client_secret));
        }

        Self::post_token_request(&url, &form, &extra_headers, config.verify_ssl).await
    }

    /// Resolves all {{variables}} in the get-token request fields.
    pub fn resolve_get_token_request(
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

        // Empty resolved scope/state are treated as absent (RFC 6749 §3.3: empty
        // scope is equivalent to omitting the parameter).
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
            force_reauth: req.force_reauth.unwrap_or(false),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_collection::{
        Collection, CollectionRepository, CollectionSettings, CollectionSummary,
        CollectionVariable, Request as CollectionRequest,
    };
    use rocket_environment::{Environment, EnvironmentRepository, Variable};
    use rocket_shared::error::{DomainError, DomainResult};

    // ─── Stub repos ──────────────────────────────────────

    struct StubEnvRepo {
        env: Option<Environment>,
    }

    impl StubEnvRepo {
        fn empty() -> Self {
            Self { env: None }
        }
        fn with_vars(vars: &[(&str, &str)]) -> Self {
            let mut env = Environment::new("test");
            for (k, v) in vars {
                env.set_variable(Variable::new(*k, *v));
            }
            Self { env: Some(env) }
        }
    }

    impl EnvironmentRepository for StubEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.env.iter().cloned().collect())
        }
        fn get(&self, _name: &str) -> DomainResult<Environment> {
            self.env
                .clone()
                .ok_or_else(|| DomainError::NotFound("env".into()))
        }
        fn save(&self, _env: &Environment) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _name: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    struct StubCollectionRepo;

    impl CollectionRepository for StubCollectionRepo {
        fn list(&self) -> DomainResult<Vec<CollectionSummary>> {
            Ok(vec![])
        }
        fn get(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn create(&self, _: &str) -> DomainResult<Collection> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn delete(&self, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn rename(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn get_request(&self, _: &str, _: &str) -> DomainResult<CollectionRequest> {
            Err(DomainError::NotFound("stub".into()))
        }
        fn save_request(
            &self,
            _: &str,
            path: &str,
            _: &CollectionRequest,
        ) -> DomainResult<String> {
            Ok(path.to_string())
        }
        fn rename_request(&self, _: &str, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete_request(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn create_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn delete_folder(&self, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn move_item(&self, _: &str, _: &str, _: &str, _: &str) -> DomainResult<()> {
            Ok(())
        }
        fn reorder_items(&self, _: &str, _: &str, _: &[String]) -> DomainResult<()> {
            Ok(())
        }
        fn get_settings(&self, _: &str) -> DomainResult<CollectionSettings> {
            Ok(CollectionSettings::default())
        }
        fn save_settings(&self, _: &str, _: &CollectionSettings) -> DomainResult<()> {
            Ok(())
        }
        fn get_folder_chain_variables(
            &self,
            _: &str,
            _: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn get_folder_variables(
            &self,
            _: &str,
            _: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_folder_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            Ok(())
        }
        fn get_request_variables(
            &self,
            _: &str,
            _: &str,
        ) -> DomainResult<Vec<CollectionVariable>> {
            Ok(vec![])
        }
        fn save_request_variables(
            &self,
            _: &str,
            _: &str,
            _: Vec<CollectionVariable>,
        ) -> DomainResult<()> {
            Ok(())
        }
    }

    #[allow(dead_code)]
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
            force_reauth: None,
        };
        let resolved = svc.resolve_get_token_request(&req);
        assert_eq!(resolved.token_url, "https://auth.example.com/token");
        assert_eq!(resolved.client_id, "client-123");
        assert_eq!(resolved.client_secret, "secret-456");
        assert_eq!(resolved.token_params[0].value, "https://auth.example.com/api");
    }

    fn cc_config() -> ResolvedOAuth2Config {
        ResolvedOAuth2Config {
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
            force_reauth: false,
        }
    }

    #[test]
    fn build_client_credentials_form_body_auth() {
        let (form, headers) = OAuth2Service::build_token_request_parts(&cc_config());
        assert!(form
            .iter()
            .any(|(k, v)| k == "grant_type" && v == "client_credentials"));
        assert!(form
            .iter()
            .any(|(k, v)| k == "client_id" && v == "my-client"));
        assert!(form
            .iter()
            .any(|(k, v)| k == "client_secret" && v == "my-secret"));
        assert!(form.iter().any(|(k, v)| k == "scope" && v == "openid"));
        assert!(form.iter().any(|(k, v)| k == "audience" && v == "api/v1"));
        assert!(headers.is_empty());
    }

    #[test]
    fn build_client_credentials_header_auth() {
        let mut config = cc_config();
        config.client_authentication = "header".into();
        config.scope = None;
        config.token_params = vec![];
        let (form, headers) = OAuth2Service::build_token_request_parts(&config);
        assert!(!form.iter().any(|(k, _)| k == "client_id"));
        assert!(!form.iter().any(|(k, _)| k == "client_secret"));
        assert!(headers
            .iter()
            .any(|(k, v)| k == "Authorization" && v.starts_with("Basic ")));
    }

    #[test]
    fn build_password_grant_includes_username_password() {
        let mut config = cc_config();
        config.grant_type = "password".into();
        config.username = "user@example.com".into();
        config.password = "p@ssw0rd".into();
        config.scope = None;
        config.token_params = vec![];
        let (form, _) = OAuth2Service::build_token_request_parts(&config);
        assert!(form
            .iter()
            .any(|(k, v)| k == "username" && v == "user@example.com"));
        assert!(form
            .iter()
            .any(|(k, v)| k == "password" && v == "p@ssw0rd"));
    }

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
            force_reauth: false,
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
    fn build_code_exchange_form_no_pkce_header_auth() {
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
            force_reauth: false,
        };
        let form = OAuth2Service::build_code_exchange_form(
            &config,
            "CODE",
            "http://localhost/cb",
            None,
        );
        // No PKCE verifier when None passed.
        assert!(!form.iter().any(|(k, _)| k == "code_verifier"));
        // Header auth: client_id/secret NOT in form.
        assert!(!form.iter().any(|(k, _)| k == "client_id"));
        assert!(!form.iter().any(|(k, _)| k == "client_secret"));
    }
}
