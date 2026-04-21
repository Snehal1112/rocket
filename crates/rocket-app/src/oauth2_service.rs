use std::collections::HashMap;

use rocket_collection::CollectionRepository;
use rocket_environment::{resolve, EnvironmentRepository, VariableContext};
use rocket_http::AdditionalParam;
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
#[allow(dead_code)]
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
