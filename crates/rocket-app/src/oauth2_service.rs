use std::borrow::Cow;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use rocket_collection::CollectionRepository;
use rocket_environment::{
    resolve, EnvironmentRepository, EnvironmentRepositoryFactory, SecretManagerRepository,
    SecretStore, VariableContext, VaultSecretFetcher,
};
use rocket_http::{
    apply_params_to_body, apply_params_to_url, AdditionalParam, OAuthToken,
    ResolvedClientCertificate, TokenClientProvider,
};
use rocket_shared::error::{DomainError, DomainResult};
use serde::Deserialize;

use crate::process_env::{ProcessEnv, ProcessEnvScopeInput};
use crate::redaction::{redact_secrets, MIN_REDACTION_LEN};

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

    /// Which host environment `{{process.env.*}}` may read. Omitted means closed.
    #[serde(default)]
    pub process_env: ProcessEnvScopeInput,
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

    /// Which host environment `{{process.env.*}}` may read. Omitted means closed.
    #[serde(default)]
    pub process_env: ProcessEnvScopeInput,
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
    /// The selected environment's client certificates, for a token endpoint that needs mutual
    /// TLS. The token client picks the one whose domain matches the token URL.
    pub client_certificates: Vec<ResolvedClientCertificate>,
}

/// Form body params and extra HTTP headers for an OAuth2 token request.
type FormAndHeaders = (Vec<(String, String)>, Vec<(String, String)>);

// ─── Service ───────────────────────────────────────────────────────

pub struct OAuth2Service {
    env_repo: Box<dyn EnvironmentRepository>,
    collection_repo: Box<dyn CollectionRepository>,
    /// Picks a collection's own environments, which is where client certificates live.
    /// Without it, `env_repo` is used for every collection.
    collection_env_repo_factory: Option<Box<dyn EnvironmentRepositoryFactory>>,
    /// Builds the client for token requests. Without it, a plain client is used and client
    /// certificates are not presented.
    token_client_provider: Option<Arc<dyn TokenClientProvider>>,
    /// Lets a token request fetch a RocketVault certificate selected for the token URL.
    /// Without it, such a certificate fails the token request.
    vault_access: Option<OAuth2VaultAccess>,
}

/// The RocketVault pieces `OAuth2Service` needs to fetch a certificate.
struct OAuth2VaultAccess {
    connections: Box<dyn SecretManagerRepository>,
    secret_store: Arc<dyn SecretStore>,
    fetcher: Arc<dyn VaultSecretFetcher>,
}

impl OAuth2Service {
    pub fn new(
        env_repo: Box<dyn EnvironmentRepository>,
        collection_repo: Box<dyn CollectionRepository>,
    ) -> Self {
        Self {
            env_repo,
            collection_repo,
            collection_env_repo_factory: None,
            token_client_provider: None,
            vault_access: None,
        }
    }

    pub fn with_collection_env_repo_factory(
        mut self,
        factory: Box<dyn EnvironmentRepositoryFactory>,
    ) -> Self {
        self.collection_env_repo_factory = Some(factory);
        self
    }

    pub fn with_token_client_provider(mut self, provider: Arc<dyn TokenClientProvider>) -> Self {
        self.token_client_provider = Some(provider);
        self
    }

    /// Lets token requests fetch a RocketVault certificate selected for the token URL.
    pub fn with_vault_access(
        mut self,
        connections: Box<dyn SecretManagerRepository>,
        secret_store: Arc<dyn SecretStore>,
        fetcher: Arc<dyn VaultSecretFetcher>,
    ) -> Self {
        self.vault_access = Some(OAuth2VaultAccess {
            connections,
            secret_store,
            fetcher,
        });
        self
    }

    /// The client certificates of the named environment, with placeholders, relative paths and
    /// RocketVault references resolved the same way request execution does.
    fn client_certificates(
        &self,
        collection: Option<&str>,
        environment_name: Option<&str>,
        vars: &HashMap<String, String>,
        external_secrets: &HashMap<String, String>,
    ) -> Vec<ResolvedClientCertificate> {
        let factory = self.collection_env_repo_factory.as_ref();
        let base = collection.and_then(|c| factory?.collection_dir(c));
        match (factory, collection) {
            (Some(f), Some(col)) => {
                let repo = f.for_collection(col);
                crate::client_certificates::environment_client_certificates(
                    repo.as_ref(),
                    environment_name,
                    base.as_deref(),
                    vars,
                    external_secrets,
                )
            }
            _ => crate::client_certificates::environment_client_certificates(
                self.env_repo.as_ref(),
                environment_name,
                base.as_deref(),
                vars,
                external_secrets,
            ),
        }
    }

    /// The client for a token request to `url` that sends no client certificate.
    /// Used by the legacy authorization-code command, so it honors the app proxy setting.
    pub fn plain_token_client(&self, url: &str, verify_ssl: bool) -> DomainResult<reqwest::Client> {
        self.token_client(url, verify_ssl, &[])
    }

    /// The client for a token request to `url`.
    ///
    /// Production always wires a provider (which applies the app proxy). The fallback below is
    /// only for tests and callers that never set one, and it ignores the proxy setting.
    fn token_client(
        &self,
        url: &str,
        verify_ssl: bool,
        certificates: &[ResolvedClientCertificate],
    ) -> DomainResult<reqwest::Client> {
        match &self.token_client_provider {
            Some(provider) => provider.client_for(url, verify_ssl, certificates),
            None => reqwest::ClientBuilder::new()
                .danger_accept_invalid_certs(!verify_ssl)
                .build()
                .map_err(|e| DomainError::Internal(format!("Failed to build HTTP client: {e}"))),
        }
    }

    /// `certificates` with the entry selected for `url` fetched when it is a RocketVault
    /// certificate. A list with nothing to fetch is not copied.
    async fn certificates_for<'c>(
        &self,
        url: &str,
        certificates: &'c [ResolvedClientCertificate],
    ) -> Cow<'c, [ResolvedClientCertificate]> {
        if !crate::vault_certificates::needs_fetch(certificates, &[url]) {
            return Cow::Borrowed(certificates);
        }
        let access =
            self.vault_access
                .as_ref()
                .map(|v| crate::vault_certificates::VaultCertificateAccess {
                    connections: v.connections.as_ref(),
                    secret_store: v.secret_store.as_ref(),
                    fetcher: v.fetcher.as_ref(),
                });
        let mut fetched = certificates.to_vec();
        crate::vault_certificates::materialize_selected(&mut fetched, &[url], access.as_ref())
            .await;
        Cow::Owned(fetched)
    }

    /// Builds a flattened variable map from all backend-accessible scopes.
    /// Mirrors `RequestExecutionService::build_variable_context` — kept here
    /// to avoid cross-service coupling.
    pub(crate) fn build_variable_context(
        &self,
        collection: Option<&str>,
        environment_name: Option<&str>,
        request_path: Option<&str>,
        external_secrets: &HashMap<String, String>,
        process_env: &HashMap<String, String>,
    ) -> HashMap<String, String> {
        // Precedence (lowest → highest): process env < collection < env < folder < request.
        // RocketVault values, keyed `alias.secretName`, are fetched by the caller.
        let mut ctx = VariableContext {
            external_secrets: external_secrets.clone(),
            process_env: process_env.clone(),
            ..VariableContext::default()
        };

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
                match rocket_environment::flatten_with_repo(&env, self.env_repo.as_ref()) {
                    Ok(flat) => {
                        for warning in &flat.warnings {
                            tracing::warn!("{warning}");
                        }
                        for (k, v) in flat.enabled_pairs() {
                            ctx.env.insert(k.to_string(), v.to_string());
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "environment not flattened, using its own variables");
                        for (k, v) in env.enabled_variables() {
                            ctx.env.insert(k.to_string(), v.to_string());
                        }
                    }
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

        ctx.flatten_with_process_env()
    }

    /// `error` with the `.env` values of `process_env` masked, keeping its variant.
    fn mask_dot_env(error: DomainError, process_env: &ProcessEnv) -> DomainError {
        let secrets: HashSet<String> = process_env
            .dot_env_keys
            .iter()
            .filter_map(|k| process_env.vars.get(k))
            .filter(|v| v.len() >= MIN_REDACTION_LEN)
            .cloned()
            .collect();
        if secrets.is_empty() {
            return error;
        }
        let mask = |s: String| redact_secrets(&s, &secrets);
        match error {
            DomainError::Internal(m) => DomainError::Internal(mask(m)),
            DomainError::InvalidInput(m) => DomainError::InvalidInput(mask(m)),
            DomainError::NotFound(m) => DomainError::NotFound(mask(m)),
            DomainError::AlreadyExists(m) => DomainError::AlreadyExists(mask(m)),
            DomainError::Io(m) => DomainError::Io(mask(m)),
            DomainError::Serialization(m) => DomainError::Serialization(mask(m)),
            DomainError::Http(m) => DomainError::Http(mask(m)),
            DomainError::Conflict(m) => DomainError::Conflict(mask(m)),
            other => other,
        }
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
    pub(crate) fn build_token_request_parts(config: &ResolvedOAuth2Config) -> FormAndHeaders {
        let mut form: Vec<(String, String)> =
            vec![("grant_type".into(), config.grant_type.clone())];
        let mut headers: Vec<(String, String)> = vec![];

        // Scope (applied before client auth to keep ordering predictable).
        if let Some(scope) = &config.scope {
            form.push(("scope".into(), scope.clone()));
        }

        // Client authentication: header = HTTP Basic, body = form fields.
        if config.client_authentication == "header" {
            headers.push(Self::basic_auth_header(
                &config.client_id,
                &config.client_secret,
            ));
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
        client: &reqwest::Client,
        url: &str,
        form: &[(String, String)],
        extra_headers: &[(String, String)],
    ) -> DomainResult<OAuthToken> {
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
        let certificates = self
            .certificates_for(&url, &config.client_certificates)
            .await;
        let client = self.token_client(&url, config.verify_ssl, &certificates)?;
        Self::post_token_request(&client, &url, &form, &extra_headers).await
    }

    /// `get_token_direct` with `.env` values of `process_env` masked in its error.
    pub async fn get_token_with_host_env(
        &self,
        config: &ResolvedOAuth2Config,
        process_env: &ProcessEnv,
    ) -> DomainResult<OAuthToken> {
        self.get_token_direct(config)
            .await
            .map_err(|e| Self::mask_dot_env(e, process_env))
    }

    /// Refreshes an OAuth2 token.
    pub async fn refresh_token(&self, req: &OAuth2RefreshRequest) -> DomainResult<OAuthToken> {
        self.refresh_token_with_secrets(req, &HashMap::new()).await
    }

    /// Refreshes an OAuth2 token, resolving `{{alias.secretName}}` references with the
    /// RocketVault values in `external_secrets`.
    pub async fn refresh_token_with_secrets(
        &self,
        req: &OAuth2RefreshRequest,
        external_secrets: &HashMap<String, String>,
    ) -> DomainResult<OAuthToken> {
        self.refresh_token_with_host_env(req, external_secrets, &ProcessEnv::default())
            .await
    }

    /// Refreshes an OAuth2 token like `refresh_token_with_secrets`, and resolves
    /// `{{process.env.*}}` from `process_env` too. `.env` values are masked in an error.
    pub async fn refresh_token_with_host_env(
        &self,
        req: &OAuth2RefreshRequest,
        external_secrets: &HashMap<String, String>,
        process_env: &ProcessEnv,
    ) -> DomainResult<OAuthToken> {
        self.refresh_token_resolved(req, external_secrets, process_env)
            .await
            .map_err(|e| Self::mask_dot_env(e, process_env))
    }

    async fn refresh_token_resolved(
        &self,
        req: &OAuth2RefreshRequest,
        external_secrets: &HashMap<String, String>,
        process_env: &ProcessEnv,
    ) -> DomainResult<OAuthToken> {
        let vars = self.build_variable_context(
            req.collection.as_deref(),
            req.environment_name.as_deref(),
            req.request_path.as_deref(),
            external_secrets,
            &process_env.vars,
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
        let scope = req.scope.as_deref().map(r).filter(|s| !s.is_empty());
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

        let environment_certificates = self.client_certificates(
            req.collection.as_deref(),
            req.environment_name.as_deref(),
            &vars,
            external_secrets,
        );
        let certificates = self.certificates_for(&url, &environment_certificates).await;
        let client = self.token_client(&url, verify_ssl, &certificates)?;
        Self::post_token_request(&client, &url, &form, &extra_headers).await
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
            extra_headers.push(Self::basic_auth_header(
                &config.client_id,
                &config.client_secret,
            ));
        }

        let certificates = self
            .certificates_for(&url, &config.client_certificates)
            .await;
        let client = self.token_client(&url, config.verify_ssl, &certificates)?;
        Self::post_token_request(&client, &url, &form, &extra_headers).await
    }

    /// `exchange_code_for_token` with `.env` values of `process_env` masked in its error.
    pub async fn exchange_code_for_token_with_host_env(
        &self,
        config: &ResolvedOAuth2Config,
        code: &str,
        redirect_uri: &str,
        code_verifier: Option<&str>,
        process_env: &ProcessEnv,
    ) -> DomainResult<OAuthToken> {
        self.exchange_code_for_token(config, code, redirect_uri, code_verifier)
            .await
            .map_err(|e| Self::mask_dot_env(e, process_env))
    }

    /// Resolves all {{variables}} in the get-token request fields.
    pub fn resolve_get_token_request(&self, req: &OAuth2GetTokenRequest) -> ResolvedOAuth2Config {
        self.resolve_get_token_request_with_secrets(req, &HashMap::new())
    }

    /// Resolves all {{variables}} in the get-token request fields, including
    /// `{{alias.secretName}}` references to RocketVault secrets in `external_secrets`.
    pub fn resolve_get_token_request_with_secrets(
        &self,
        req: &OAuth2GetTokenRequest,
        external_secrets: &HashMap<String, String>,
    ) -> ResolvedOAuth2Config {
        self.resolve_get_token_request_with_host_env(req, external_secrets, &ProcessEnv::default())
    }

    /// Like `resolve_get_token_request_with_secrets`, and resolves `{{process.env.*}}` from
    /// `process_env` as the lowest layer.
    pub fn resolve_get_token_request_with_host_env(
        &self,
        req: &OAuth2GetTokenRequest,
        external_secrets: &HashMap<String, String>,
        process_env: &ProcessEnv,
    ) -> ResolvedOAuth2Config {
        let vars = self.build_variable_context(
            req.collection.as_deref(),
            req.environment_name.as_deref(),
            req.request_path.as_deref(),
            external_secrets,
            &process_env.vars,
        );
        let r = |s: &str| resolve(s, &vars).output;

        // URL fields are trimmed after resolution — stray whitespace from copy-paste
        // causes the webview to navigate to a path like "/authorize%20?" (not found).
        let ru = |s: &str| r(s).trim().to_string();

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
            authorization_url: ru(req.authorization_url.as_deref().unwrap_or_default()),
            token_url: ru(req.token_url.as_deref().unwrap_or_default()),
            callback_url: ru(req.callback_url.as_deref().unwrap_or_default()),
            client_id: r(&req.client_id),
            client_secret: r(req.client_secret.as_deref().unwrap_or_default()),
            scope: req.scope.as_deref().map(r).filter(|s| !s.is_empty()),
            state: req.state.as_deref().map(r).filter(|s| !s.is_empty()),
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
            client_certificates: self.client_certificates(
                req.collection.as_deref(),
                req.environment_name.as_deref(),
                &vars,
                external_secrets,
            ),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::client_certificates::describe_all;
    use crate::test_doubles::NullCollectionRepo;
    use rocket_collection::{CollectionRepository, CollectionSettings, CollectionVariable};
    use rocket_environment::{Environment, EnvironmentRepository, Variable};
    use rocket_shared::certificate::ClientCertificate;
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

    /// A service over empty stub repos. Also used by the Flow Auth real-fetch
    /// tests (`flow_auth.rs`, `flow_execution_service.rs`).
    pub(crate) fn make_service() -> OAuth2Service {
        OAuth2Service::new(
            Box::new(StubEnvRepo::empty()),
            Box::new(NullCollectionRepo::new()),
        )
    }

    fn make_service_with_env(vars: &[(&str, &str)]) -> OAuth2Service {
        OAuth2Service::new(
            Box::new(StubEnvRepo::with_vars(vars)),
            Box::new(NullCollectionRepo::new()),
        )
    }

    // ─── Tests ───────────────────────────────────────────

    struct ChainEnvRepo(Vec<Environment>);

    impl EnvironmentRepository for ChainEnvRepo {
        fn list(&self) -> DomainResult<Vec<Environment>> {
            Ok(self.0.clone())
        }
        fn get(&self, name: &str) -> DomainResult<Environment> {
            self.0
                .iter()
                .find(|e| e.name == name)
                .cloned()
                .ok_or_else(|| DomainError::NotFound(name.into()))
        }
        fn save(&self, _env: &Environment) -> DomainResult<()> {
            Ok(())
        }
        fn delete(&self, _name: &str) -> DomainResult<()> {
            Ok(())
        }
    }

    #[test]
    fn oauth_context_includes_inherited_variables() {
        let mut base = Environment::new("base");
        base.set_variable(Variable::new("auth_host", "https://auth.example.com"));
        let mut child = Environment::new("child");
        child.extends = Some("base".into());
        let svc = OAuth2Service::new(
            Box::new(ChainEnvRepo(vec![base, child])),
            Box::new(NullCollectionRepo::new()),
        );
        let vars =
            svc.build_variable_context(None, Some("child"), None, &HashMap::new(), &HashMap::new());
        assert_eq!(
            vars.get("auth_host").map(String::as_str),
            Some("https://auth.example.com")
        );
    }

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
            process_env: Default::default(),
        };
        let resolved = svc.resolve_get_token_request(&req);
        assert_eq!(resolved.token_url, "https://auth.example.com/token");
        assert_eq!(resolved.client_id, "client-123");
        assert_eq!(resolved.client_secret, "secret-456");
        assert_eq!(
            resolved.token_params[0].value,
            "https://auth.example.com/api"
        );
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
            client_certificates: Vec::new(),
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
        assert!(form.iter().any(|(k, v)| k == "password" && v == "p@ssw0rd"));
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
            client_certificates: Vec::new(),
        };
        let form = OAuth2Service::build_code_exchange_form(
            &config,
            "AUTH_CODE_123",
            "http://localhost:9876/callback",
            Some("verifier_abc"),
        );
        assert!(form
            .iter()
            .any(|(k, v)| k == "grant_type" && v == "authorization_code"));
        assert!(form
            .iter()
            .any(|(k, v)| k == "code" && v == "AUTH_CODE_123"));
        assert!(form
            .iter()
            .any(|(k, v)| k == "redirect_uri" && v == "http://localhost:9876/callback"));
        assert!(form
            .iter()
            .any(|(k, v)| k == "code_verifier" && v == "verifier_abc"));
        assert!(form
            .iter()
            .any(|(k, v)| k == "client_id" && v == "my-client"));
        assert!(form
            .iter()
            .any(|(k, v)| k == "resource" && v == "https://api.example.com"));
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
            client_certificates: Vec::new(),
        };
        let form =
            OAuth2Service::build_code_exchange_form(&config, "CODE", "http://localhost/cb", None);
        // No PKCE verifier when None passed.
        assert!(!form.iter().any(|(k, _)| k == "code_verifier"));
        // Header auth: client_id/secret NOT in form.
        assert!(!form.iter().any(|(k, _)| k == "client_id"));
        assert!(!form.iter().any(|(k, _)| k == "client_secret"));
    }

    // ─── Client certificates (mTLS) for token requests ──────

    /// Records what the service asks for, then fails, so no request is ever sent.
    struct CapturingProvider {
        seen: std::sync::Mutex<Vec<(String, bool, Vec<ResolvedClientCertificate>)>>,
    }

    impl CapturingProvider {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                seen: std::sync::Mutex::new(Vec::new()),
            })
        }
    }

    impl TokenClientProvider for CapturingProvider {
        fn client_for(
            &self,
            token_url: &str,
            verify_ssl: bool,
            certificates: &[ResolvedClientCertificate],
        ) -> DomainResult<reqwest::Client> {
            self.seen.lock().unwrap().push((
                token_url.to_string(),
                verify_ssl,
                certificates.to_vec(),
            ));
            Err(DomainError::InvalidInput(
                "certificate cannot be loaded".into(),
            ))
        }
    }

    fn pkcs12(domain: &str, path: &str) -> ClientCertificate {
        ClientCertificate::Pkcs12 {
            domain: domain.into(),
            pkcs12_file_path: path.into(),
            pkcs12_secret: None,
            passphrase: None,
        }
    }

    fn env_with_certificates(certs: Vec<ClientCertificate>) -> Environment {
        let mut env = Environment::new("dev");
        env.set_variable(Variable::new("certDir", "/certs"));
        env.client_certificates = certs;
        env
    }

    fn service_with_certificates(
        certs: Vec<ClientCertificate>,
        provider: &Arc<CapturingProvider>,
    ) -> OAuth2Service {
        OAuth2Service::new(
            Box::new(StubEnvRepo {
                env: Some(env_with_certificates(certs)),
            }),
            Box::new(NullCollectionRepo::new()),
        )
        .with_token_client_provider(provider.clone())
    }

    fn get_token_request() -> OAuth2GetTokenRequest {
        OAuth2GetTokenRequest {
            grant_type: "client_credentials".into(),
            authorization_url: None,
            token_url: Some("https://idp.example.com/token".into()),
            callback_url: None,
            client_id: "id".into(),
            client_secret: Some("secret".into()),
            scope: None,
            state: None,
            username: None,
            password: None,
            client_authentication: None,
            use_pkce: None,
            use_system_browser: None,
            verify_ssl: Some(false),
            auth_params: None,
            token_params: None,
            refresh_params: None,
            collection: None,
            environment_name: Some("dev".into()),
            request_path: None,
            force_reauth: None,
            process_env: Default::default(),
        }
    }

    #[test]
    fn the_plain_token_client_comes_from_the_provider_so_it_honors_the_proxy() {
        let provider = CapturingProvider::new();
        let svc = service_with_certificates(vec![], &provider);
        let err = svc
            .plain_token_client("https://idp.example.com/token", false)
            .unwrap_err();
        assert!(err.to_string().contains("certificate cannot be loaded"));
        let seen = provider.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "https://idp.example.com/token");
        assert!(!seen[0].1);
        assert!(seen[0].2.is_empty());
    }

    #[test]
    fn resolving_a_get_token_request_carries_the_environment_certificates() {
        let provider = CapturingProvider::new();
        let svc = service_with_certificates(
            vec![pkcs12("idp.example.com", "{{certDir}}/client.p12")],
            &provider,
        );
        let config = svc.resolve_get_token_request(&get_token_request());
        assert_eq!(
            describe_all(&config.client_certificates),
            ["pkcs12 idp.example.com file:/certs/client.p12 pass:-"]
        );
    }

    #[test]
    fn no_environment_means_no_certificates() {
        let svc = make_service();
        let mut req = get_token_request();
        req.environment_name = None;
        assert!(svc
            .resolve_get_token_request(&req)
            .client_certificates
            .is_empty());
    }

    #[tokio::test]
    async fn a_direct_grant_asks_the_provider_for_a_client_with_the_certificates() {
        let provider = CapturingProvider::new();
        let svc = service_with_certificates(vec![pkcs12("idp.example.com", "/c.p12")], &provider);
        let config = svc.resolve_get_token_request(&get_token_request());

        let err = svc.get_token_direct(&config).await.unwrap_err();

        // The provider failed, so the error is returned and nothing is sent.
        assert!(
            err.to_string().contains("certificate cannot be loaded"),
            "{err}"
        );
        let seen = provider.seen.lock().unwrap();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].0, "https://idp.example.com/token");
        assert!(!seen[0].1, "verify_ssl false is passed through");
        assert_eq!(
            describe_all(&seen[0].2),
            ["pkcs12 idp.example.com file:/c.p12 pass:-"]
        );
    }

    #[tokio::test]
    async fn the_code_exchange_uses_the_same_client_source() {
        let provider = CapturingProvider::new();
        let svc = service_with_certificates(vec![pkcs12("idp.example.com", "/c.p12")], &provider);
        let mut req = get_token_request();
        req.grant_type = "authorization_code".into();
        let config = svc.resolve_get_token_request(&req);

        let err = svc
            .exchange_code_for_token(&config, "code", "https://app/cb", None)
            .await
            .unwrap_err();

        assert!(
            err.to_string().contains("certificate cannot be loaded"),
            "{err}"
        );
        assert_eq!(provider.seen.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn a_refresh_resolves_certificates_from_its_own_environment() {
        let provider = CapturingProvider::new();
        let svc = service_with_certificates(
            vec![pkcs12("idp.example.com", "{{certDir}}/client.p12")],
            &provider,
        );
        let req = OAuth2RefreshRequest {
            refresh_token: "r".into(),
            token_url: "https://idp.example.com/token".into(),
            refresh_token_url: None,
            client_id: "id".into(),
            client_secret: None,
            scope: None,
            client_authentication: None,
            verify_ssl: None,
            refresh_params: None,
            collection: None,
            environment_name: Some("dev".into()),
            request_path: None,
            process_env: Default::default(),
        };

        let err = svc.refresh_token(&req).await.unwrap_err();

        assert!(
            err.to_string().contains("certificate cannot be loaded"),
            "{err}"
        );
        let seen = provider.seen.lock().unwrap();
        assert_eq!(
            describe_all(&seen[0].2),
            ["pkcs12 idp.example.com file:/certs/client.p12 pass:-"]
        );
        assert!(seen[0].1, "verify_ssl defaults to true");
    }

    fn vault_secrets() -> HashMap<String, String> {
        HashMap::from([
            ("vault.clientSecret".to_string(), "s3cret".to_string()),
            ("vault.certPass".to_string(), "p4ss".to_string()),
        ])
    }

    #[test]
    fn vault_references_resolve_in_a_get_token_request_when_secrets_are_given() {
        let provider = CapturingProvider::new();
        let svc = service_with_certificates(
            vec![ClientCertificate::Pkcs12 {
                domain: "idp.example.com".into(),
                pkcs12_file_path: "/c.p12".into(),
                pkcs12_secret: None,
                passphrase: Some("{{vault.certPass}}".into()),
            }],
            &provider,
        );
        let mut req = get_token_request();
        req.client_secret = Some("{{vault.clientSecret}}".into());

        let config = svc.resolve_get_token_request_with_secrets(&req, &vault_secrets());

        assert_eq!(config.client_secret, "s3cret");
        assert_eq!(
            describe_all(&config.client_certificates),
            ["pkcs12 idp.example.com file:/c.p12 pass:p4ss"]
        );
    }

    #[test]
    fn without_secrets_a_vault_reference_stays_unresolved() {
        let svc = make_service();
        let mut req = get_token_request();
        req.client_secret = Some("{{vault.clientSecret}}".into());
        let config = svc.resolve_get_token_request(&req);
        assert_eq!(config.client_secret, "{{vault.clientSecret}}");
    }

    #[tokio::test]
    async fn a_refresh_resolves_a_vault_passphrase_for_the_certificate() {
        let provider = CapturingProvider::new();
        let svc = service_with_certificates(
            vec![ClientCertificate::Pkcs12 {
                domain: "idp.example.com".into(),
                pkcs12_file_path: "/c.p12".into(),
                pkcs12_secret: None,
                passphrase: Some("{{vault.certPass}}".into()),
            }],
            &provider,
        );
        let req = OAuth2RefreshRequest {
            refresh_token: "r".into(),
            token_url: "https://idp.example.com/token".into(),
            refresh_token_url: None,
            client_id: "id".into(),
            client_secret: None,
            scope: None,
            client_authentication: None,
            verify_ssl: None,
            refresh_params: None,
            collection: None,
            environment_name: Some("dev".into()),
            request_path: None,
            process_env: Default::default(),
        };

        let _ = svc.refresh_token_with_secrets(&req, &vault_secrets()).await;

        let seen = provider.seen.lock().unwrap();
        assert_eq!(
            describe_all(&seen[0].2),
            ["pkcs12 idp.example.com file:/c.p12 pass:p4ss"]
        );
    }

    mod vault_certificates {
        use super::*;
        use rocket_http::{CertificateMaterial, CertificateSource};

        fn vault_p12(domain: &str, reference: &str) -> ClientCertificate {
            ClientCertificate::Pkcs12 {
                domain: domain.into(),
                pkcs12_file_path: String::new(),
                pkcs12_secret: Some(reference.into()),
                passphrase: None,
            }
        }

        fn secrets(pairs: &[(&str, &str)]) -> HashMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        }

        #[test]
        fn vault_certificates_a_get_token_request_carries_inline_material_from_the_vault() {
            let provider = CapturingProvider::new();
            let svc = service_with_certificates(
                vec![vault_p12("idp.example.com", "vault.bundle")],
                &provider,
            );
            let config = svc.resolve_get_token_request_with_secrets(
                &get_token_request(),
                &secrets(&[("vault.bundle", "AQIDBAU=")]),
            );
            let CertificateMaterial::Pkcs12 {
                bundle: CertificateSource::Inline(bytes),
                ..
            } = &config.client_certificates[0].material
            else {
                panic!("expected inline PKCS12 material");
            };
            assert_eq!(&bytes[..], &[1u8, 2, 3, 4, 5][..]);
        }

        #[tokio::test]
        async fn vault_certificates_a_refresh_resolves_inline_material_for_the_provider() {
            let provider = CapturingProvider::new();
            let svc = service_with_certificates(
                vec![vault_p12("idp.example.com", "vault.bundle")],
                &provider,
            );
            let req = OAuth2RefreshRequest {
                refresh_token: "r".into(),
                token_url: "https://idp.example.com/token".into(),
                refresh_token_url: None,
                client_id: "id".into(),
                client_secret: None,
                scope: None,
                client_authentication: None,
                verify_ssl: None,
                refresh_params: None,
                collection: None,
                environment_name: Some("dev".into()),
                request_path: None,
                process_env: Default::default(),
            };

            // The capturing provider fails on purpose, so only what it saw matters.
            let _ = svc
                .refresh_token_with_secrets(&req, &secrets(&[("vault.bundle", "AQIDBAU=")]))
                .await;

            let seen = provider.seen.lock().expect("lock");
            assert!(matches!(
                &seen[0].2[0].material,
                CertificateMaterial::Pkcs12 {
                    bundle: CertificateSource::Inline(_),
                    ..
                }
            ));
        }

        // Review Focus item 1, for OAuth2 token requests.
        #[test]
        fn vault_certificates_oauth_missing_secret_on_another_domain_leaves_the_selected_certificate_usable(
        ) {
            let provider = CapturingProvider::new();
            let svc = service_with_certificates(
                vec![
                    vault_p12("idp.example.com", "vault.ok"),
                    vault_p12("other.example.com", "vault.missing"),
                ],
                &provider,
            );
            let config = svc.resolve_get_token_request_with_secrets(
                &get_token_request(),
                &secrets(&[("vault.ok", "AQIDBAU=")]),
            );
            let selected = rocket_http::client_cert::find_certificate(
                &config.client_certificates,
                "https://idp.example.com/token",
            )
            .expect("the certificate for the token host is selected");
            assert!(matches!(
                selected.material,
                CertificateMaterial::Pkcs12 {
                    bundle: CertificateSource::Inline(_),
                    ..
                }
            ));
        }

        #[test]
        fn vault_certificates_without_secrets_a_reference_is_unavailable_not_a_panic() {
            let provider = CapturingProvider::new();
            let svc = service_with_certificates(
                vec![vault_p12("idp.example.com", "vault.bundle")],
                &provider,
            );
            let config = svc.resolve_get_token_request(&get_token_request());
            let CertificateMaterial::Unavailable { reason } =
                &config.client_certificates[0].material
            else {
                panic!("expected an unavailable certificate");
            };
            assert!(reason.contains("vault.bundle"), "{reason}");
        }
    }

    /// A factory that knows where the collection lives, like the real workspace one.
    struct DirFactory {
        env: Environment,
    }

    impl EnvironmentRepositoryFactory for DirFactory {
        fn for_collection(&self, _: &str) -> Box<dyn EnvironmentRepository> {
            Box::new(StubEnvRepo {
                env: Some(self.env.clone()),
            })
        }

        fn collection_dir(&self, collection: &str) -> Option<std::path::PathBuf> {
            Some(std::path::PathBuf::from("/ws/collections").join(collection))
        }
    }

    #[test]
    fn a_collection_environment_is_used_and_relative_paths_join_its_folder() {
        let provider = CapturingProvider::new();
        // The fixed repo has no certificates, so they can only come from the factory.
        let svc = OAuth2Service::new(
            Box::new(StubEnvRepo::empty()),
            Box::new(NullCollectionRepo::new()),
        )
        .with_token_client_provider(provider)
        .with_collection_env_repo_factory(Box::new(DirFactory {
            env: env_with_certificates(vec![pkcs12("idp.example.com", "certs/client.p12")]),
        }));
        let mut req = get_token_request();
        req.collection = Some("api".into());
        let config = svc.resolve_get_token_request(&req);
        assert_eq!(
            describe_all(&config.client_certificates),
            ["pkcs12 idp.example.com file:/ws/collections/api/certs/client.p12 pass:-"]
        );
    }

    #[test]
    fn variable_context_follows_the_active_workspace() {
        use std::sync::Mutex;
        let a = tempfile::tempdir().expect("tempdir");
        let b = tempfile::tempdir().expect("tempdir");
        let path = Arc::new(Mutex::new(a.path().to_path_buf()));
        let svc = OAuth2Service::new(
            Box::new(rocket_infra::SharedPathEnvironmentRepo::with_secret_store(
                Arc::clone(&path),
                Arc::new(rocket_environment::NullSecretStore),
            )),
            Box::new(rocket_infra::SharedPathCollectionRepo::new(Arc::clone(
                &path,
            ))),
        );
        // Each workspace has a collection "api" and a global environment "g".
        for (dir, value) in [(a.path(), "a"), (b.path(), "b")] {
            let collections = rocket_infra::SharedPathCollectionRepo::new(Arc::new(Mutex::new(
                dir.to_path_buf(),
            )));
            collections.create("api").expect("create collection");
            let settings = CollectionSettings {
                variables: vec![CollectionVariable {
                    key: "col".into(),
                    value: value.into(),
                    initial_value: String::new(),
                    enabled: true,
                    secret: false,
                }],
                ..CollectionSettings::default()
            };
            collections
                .save_settings("api", &settings)
                .expect("settings");
            let mut env = Environment::new("g");
            env.set_variable(Variable::new("glob", value));
            rocket_infra::FsEnvironmentRepo::new(dir.join("environments"))
                .save(&env)
                .expect("global env");
        }
        let ctx = |svc: &OAuth2Service| {
            let vars = svc.build_variable_context(
                Some("api"),
                Some("g"),
                None,
                &HashMap::new(),
                &HashMap::new(),
            );
            (vars.get("col").cloned(), vars.get("glob").cloned())
        };

        assert_eq!(ctx(&svc), (Some("a".into()), Some("a".into())));
        *path.lock().expect("lock") = b.path().to_path_buf();
        assert_eq!(ctx(&svc), (Some("b".into()), Some("b".into())));
        *path.lock().expect("lock") = a.path().to_path_buf();
        assert_eq!(ctx(&svc), (Some("a".into()), Some("a".into())));
    }

    // ─── Process env ─────────────────────────────────────

    fn client_credentials_request(token_url: &str) -> OAuth2GetTokenRequest {
        serde_json::from_value(serde_json::json!({
            "grantType": "client_credentials",
            "tokenUrl": token_url,
            "clientId": "id",
        }))
        .expect("get-token request")
    }

    fn host_env(pairs: &[(&str, &str)], dot_env: &[&str]) -> ProcessEnv {
        ProcessEnv {
            vars: pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
            dot_env_keys: dot_env.iter().map(|k| k.to_string()).collect(),
            allowed: true,
        }
    }

    #[test]
    fn oauth2_service_resolves_process_env_map_as_lowest_layer() {
        let svc = make_service_with_env(&[("my_secret", "env-wins")]);
        let mut req = client_credentials_request("https://auth.example.com/token");
        req.client_secret = Some("{{my_secret}}".into());
        req.scope = Some("{{process.env.ROCKET_X}}".into());
        req.environment_name = Some("test".into());
        let env = host_env(&[("my_secret", "host"), ("ROCKET_X", "hostx")], &[]);

        let config = svc.resolve_get_token_request_with_host_env(&req, &HashMap::new(), &env);
        assert_eq!(config.client_secret, "env-wins");
        assert_eq!(config.scope.as_deref(), Some("hostx"));

        let closed = svc.resolve_get_token_request_with_host_env(
            &req,
            &HashMap::new(),
            &ProcessEnv::default(),
        );
        assert_eq!(closed.scope.as_deref(), Some("{{process.env.ROCKET_X}}"));
    }

    #[test]
    fn oauth2_get_token_request_defaults_to_closed_scope() {
        let get: OAuth2GetTokenRequest = serde_json::from_value(serde_json::json!({
            "grantType": "client_credentials", "clientId": "id"
        }))
        .expect("get-token");
        assert_eq!(get.process_env, ProcessEnvScopeInput::Closed);
        let refresh: OAuth2RefreshRequest = serde_json::from_value(serde_json::json!({
            "refreshToken": "r", "tokenUrl": "https://x/token", "clientId": "id"
        }))
        .expect("refresh");
        assert_eq!(refresh.process_env, ProcessEnvScopeInput::Closed);
        let scratch: OAuth2GetTokenRequest = serde_json::from_value(serde_json::json!({
            "grantType": "client_credentials", "clientId": "id",
            "processEnv": { "kind": "scratch" }
        }))
        .expect("scratch");
        assert_eq!(scratch.process_env, ProcessEnvScopeInput::Scratch);
    }

    #[tokio::test]
    async fn oauth2_get_token_error_masks_dot_env_value() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(
                wiremock::ResponseTemplate::new(400)
                    .set_body_string("bad secret dotenv-secret-654321"),
            )
            .mount(&server)
            .await;
        let svc = make_service_with_env(&[]);
        let mut req = client_credentials_request(&format!("{}/token", server.uri()));
        req.client_secret = Some("{{process.env.ROCKET_CS_KEY}}".into());
        let env = host_env(
            &[("ROCKET_CS_KEY", "dotenv-secret-654321")],
            &["ROCKET_CS_KEY"],
        );
        let config = svc.resolve_get_token_request_with_host_env(&req, &HashMap::new(), &env);
        assert_eq!(config.client_secret, "dotenv-secret-654321");
        let err = svc
            .get_token_with_host_env(&config, &env)
            .await
            .expect_err("400");
        assert!(!err.to_string().contains("dotenv-secret-654321"), "{err}");
        assert!(err.to_string().contains("400"), "{err}");
    }

    #[tokio::test]
    async fn oauth2_refresh_resolves_process_env_map() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .and(wiremock::matchers::body_string_contains(
                "client_secret=dotenv-secret-654321",
            ))
            .respond_with(
                wiremock::ResponseTemplate::new(200).set_body_json(
                    serde_json::json!({"access_token": "t", "token_type": "bearer"}),
                ),
            )
            .expect(1)
            .mount(&server)
            .await;
        let svc = make_service_with_env(&[]);
        let req: OAuth2RefreshRequest = serde_json::from_value(serde_json::json!({
            "refreshToken": "r",
            "tokenUrl": format!("{}/token", server.uri()),
            "clientId": "id",
            "clientSecret": "{{process.env.ROCKET_CS_KEY}}",
        }))
        .expect("refresh");
        let env = host_env(
            &[("ROCKET_CS_KEY", "dotenv-secret-654321")],
            &["ROCKET_CS_KEY"],
        );
        let token = svc
            .refresh_token_with_host_env(&req, &HashMap::new(), &env)
            .await
            .expect("refresh");
        assert_eq!(token.access_token, "t");
    }

    #[tokio::test]
    async fn oauth2_refresh_error_masks_dot_env_value() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(
                wiremock::ResponseTemplate::new(400)
                    .set_body_string("bad secret dotenv-secret-654321"),
            )
            .mount(&server)
            .await;
        let svc = make_service_with_env(&[]);
        let req: OAuth2RefreshRequest = serde_json::from_value(serde_json::json!({
            "refreshToken": "r",
            "tokenUrl": format!("{}/token", server.uri()),
            "clientId": "id",
            "clientSecret": "{{process.env.ROCKET_CS_KEY}}",
        }))
        .expect("refresh");
        let env = host_env(
            &[("ROCKET_CS_KEY", "dotenv-secret-654321")],
            &["ROCKET_CS_KEY"],
        );
        let err = svc
            .refresh_token_with_host_env(&req, &HashMap::new(), &env)
            .await
            .expect_err("400");
        assert!(!err.to_string().contains("dotenv-secret-654321"), "{err}");
        assert!(err.to_string().contains("400"), "{err}");
    }

    #[tokio::test]
    async fn oauth2_code_exchange_error_masks_dot_env_value() {
        let server = wiremock::MockServer::start().await;
        wiremock::Mock::given(wiremock::matchers::method("POST"))
            .respond_with(
                wiremock::ResponseTemplate::new(400)
                    .set_body_string("bad secret dotenv-secret-654321"),
            )
            .mount(&server)
            .await;
        let svc = make_service_with_env(&[]);
        let mut req = client_credentials_request(&format!("{}/token", server.uri()));
        req.grant_type = "authorization_code".into();
        req.client_secret = Some("{{process.env.ROCKET_CS_KEY}}".into());
        let env = host_env(
            &[("ROCKET_CS_KEY", "dotenv-secret-654321")],
            &["ROCKET_CS_KEY"],
        );
        let config = svc.resolve_get_token_request_with_host_env(&req, &HashMap::new(), &env);
        assert_eq!(config.client_secret, "dotenv-secret-654321");
        let err = svc
            .exchange_code_for_token_with_host_env(&config, "code", "http://cb", None, &env)
            .await
            .expect_err("400");
        assert!(!err.to_string().contains("dotenv-secret-654321"), "{err}");
        assert!(err.to_string().contains("400"), "{err}");
    }
}
