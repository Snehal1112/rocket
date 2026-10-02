//! Run-start credentials for Flow Auth nodes. See
//! `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md`.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use async_trait::async_trait;
use rocket_flow::{Flow, FlowNodeKind};
use rocket_http::{AdditionalParam, OAuthToken};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::oauth2::{
    OAuth2AdditionalParameter, OAuth2AdditionalParameters, OAuth2ClientCredentials, OAuth2Flow,
    OAuth2Settings, OAuth2TokenConfig,
};
use rocket_shared::types::Auth;

use crate::oauth2_service::{OAuth2GetTokenRequest, OAuth2Service};

/// A token the UI obtained before the run. Lives for one run, in memory only.
/// `Debug` never shows the value.
#[derive(Clone, PartialEq, Eq)]
pub struct SuppliedToken {
    pub access_token: String,
}

impl std::fmt::Debug for SuppliedToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("SuppliedToken(<redacted>)")
    }
}

/// Tokens the UI supplies, keyed by Auth node id.
pub type FlowAuthTokens = HashMap<String, SuppliedToken>;

/// What a token fetch needs to know about the run. `Debug` shows only the
/// collection and environment, because the maps hold secret values.
#[derive(Clone, Default)]
pub struct FetchContext {
    pub collection: String,
    pub environment_name: Option<String>,
    /// Every `{{variable}}` visible to the run (global < collection < environment).
    pub vars: HashMap<String, String>,
    /// RocketVault values keyed `alias.secretName`.
    pub external_secrets: HashMap<String, String>,
}

impl std::fmt::Debug for FetchContext {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FetchContext")
            .field("collection", &self.collection)
            .field("environment_name", &self.environment_name)
            .finish_non_exhaustive()
    }
}

/// Fetches a non-interactive OAuth2 token during a run. The adapter over
/// `OAuth2Service` is `OAuth2ServiceFetcher`; tests use `FakeFetcher`.
#[async_trait]
pub trait FlowTokenFetcher: Send + Sync {
    async fn fetch_token(&self, flow: &OAuth2Flow, ctx: &FetchContext) -> DomainResult<String>;
}

/// Default when nothing is wired: a fetch fails with a clear reason.
pub struct NoTokenFetcher;

#[async_trait]
impl FlowTokenFetcher for NoTokenFetcher {
    async fn fetch_token(&self, _flow: &OAuth2Flow, _ctx: &FetchContext) -> DomainResult<String> {
        Err(DomainError::InvalidInput(
            "this build cannot fetch OAuth2 tokens during a run".to_string(),
        ))
    }
}

/// The credentials a run resolved at start, one per Auth node. `Debug` shows
/// only counts, because the values are secrets.
#[derive(Clone, Default)]
pub(crate) struct FlowCredentials {
    by_node: HashMap<String, Auth>,
    /// The credential of the Auth node that applies to inherited auth.
    auto_apply: Option<Auth>,
    /// `(node id, value)` for every credential that has a plain token or key value.
    wire_values: Vec<(String, String)>,
}

impl std::fmt::Debug for FlowCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FlowCredentials")
            .field("nodes", &self.by_node.len())
            .field("auto_apply", &self.auto_apply.is_some())
            .finish_non_exhaustive()
    }
}

impl FlowCredentials {
    /// The credential of Auth node `node_id`.
    pub(crate) fn auth_for_node(&self, node_id: &str) -> Option<&Auth> {
        self.by_node.get(node_id)
    }

    /// Replaces `auth` with the auto-apply credential when `auth` is `inherit`
    /// or `none`. The backend (`merge_auth`) treats `none` like `inherit` (both
    /// fall back to the collection auth), and requests created in the app,
    /// saved requests with no auth block and inline Flow requests are all
    /// `none`. Any other (explicit) auth is left alone.
    pub(crate) fn apply_to_inherit(&self, auth: &mut Auth) {
        if matches!(auth, Auth::Inherit | Auth::None) {
            if let Some(credential) = &self.auto_apply {
                *auth = credential.clone();
            }
        }
    }

    /// The plain value an Auth node puts on its wire (a token or API key
    /// value). Types without one have none.
    pub(crate) fn wire_value(&self, node_id: &str) -> Option<&str> {
        self.wire_values
            .iter()
            .find(|(id, _)| id == node_id)
            .map(|(_, value)| value.as_str())
    }

    /// `(node id, secret)` pairs the run must never print.
    pub(crate) fn secrets(&self) -> impl Iterator<Item = (&str, &str)> {
        self.wire_values
            .iter()
            .map(|(id, value)| (id.as_str(), value.as_str()))
    }

    /// Every form of every secret that redaction must mask.
    pub(crate) fn secret_forms(&self) -> HashSet<String> {
        self.wire_values
            .iter()
            .flat_map(|(_, value)| crate::redaction::redaction_forms(value))
            .collect()
    }
}

/// How long a run-start token fetch may take. The fetch runs before the run
/// is registered, so Stop cannot reach it; without a bound, a token endpoint
/// that never answers would hang the run forever.
pub(crate) const TOKEN_FETCH_TIMEOUT: Duration = Duration::from_secs(30);

/// `30s` for whole seconds, `50ms` below a second.
fn describe_duration(d: Duration) -> String {
    if d.as_secs() >= 1 {
        format!("{}s", d.as_secs())
    } else {
        format!("{}ms", d.as_millis())
    }
}

fn is_non_interactive(flow: &OAuth2Flow) -> bool {
    matches!(
        flow,
        OAuth2Flow::ClientCredentials { .. } | OAuth2Flow::ResourceOwnerPassword { .. }
    )
}

/// Resolves `template` with the run-start variables. `None` when a
/// placeholder is left over, or when a variable's value itself contains
/// `{{`: the request would resolve that text again at send time, so the
/// value sent could differ from the value resolved here.
fn resolve_fully(template: &str, vars: &HashMap<String, String>) -> Option<String> {
    let resolved = rocket_environment::resolve(template, vars);
    (resolved.unresolved.is_empty() && !resolved.output.contains("{{")).then_some(resolved.output)
}

/// A static Bearer or API key credential and its wire value.
///
/// The token is resolved once, at run start, with the run's variables
/// (global < collection < environment, plus vault values), and the RESOLVED
/// credential is what requests send. So the value sent, the Auth node's wire
/// value and the masked secret are always the same value: a folder or
/// request variable with the same name, or a script that changes the
/// variable later in the run, cannot make a request send an unmasked token.
///
/// When a placeholder cannot be resolved at run start (typically a variable
/// that a script sets during the run), the credential is kept unresolved so
/// the request still resolves it at send time, as before. That credential
/// has no wire value and no masked secret, because its value is not known
/// yet; it is masked only if the variable itself is a secret.
fn resolve_static_token(auth: &Auth, vars: &HashMap<String, String>) -> (Auth, Option<String>) {
    let resolved = match auth {
        Auth::Bearer { token } => resolve_fully(token, vars)
            .filter(|t| !t.is_empty())
            .map(|token| Auth::Bearer { token }),
        Auth::ApiKey {
            key,
            value,
            placement,
        } => match (resolve_fully(key, vars), resolve_fully(value, vars)) {
            (Some(key), Some(value)) if !value.is_empty() => Some(Auth::ApiKey {
                key,
                value,
                placement: placement.clone(),
            }),
            _ => None,
        },
        _ => None,
    };
    match resolved {
        Some(credential) => {
            let wire = match &credential {
                Auth::Bearer { token } => Some(token.clone()),
                Auth::ApiKey { value, .. } => Some(value.clone()),
                _ => None,
            };
            (credential, wire)
        }
        None => (auth.clone(), None),
    }
}

/// Resolves every Auth node of `flow` into a credential, before the run starts.
///
/// Order per OAuth2 node: a supplied token wins; otherwise a non-interactive
/// grant is fetched, bounded by `fetch_timeout`; otherwise the run cannot
/// start. A static Bearer or API key is resolved here, at run start (see
/// `resolve_static_token`). Other auth types pass through with their
/// `{{variables}}` intact, because the request resolves them at send time
/// with its own scopes; they have no wire value.
pub(crate) async fn resolve_flow_credentials(
    flow: &Flow,
    supplied: &FlowAuthTokens,
    fetcher: &dyn FlowTokenFetcher,
    ctx: &FetchContext,
    fetch_timeout: Duration,
) -> DomainResult<FlowCredentials> {
    let mut creds = FlowCredentials::default();
    for node in &flow.nodes {
        let FlowNodeKind::Auth {
            label,
            auth,
            apply_to_inherit,
        } = &node.kind
        else {
            continue;
        };
        let (credential, wire) = match auth {
            Auth::None | Auth::Inherit => {
                return Err(DomainError::InvalidInput(format!(
                    "Auth node \"{label}\" needs an auth type other than none or inherit"
                )));
            }
            Auth::OAuth2(oauth) => {
                let token = match supplied
                    .get(&node.id)
                    .map(|t| t.access_token.trim())
                    .filter(|t| !t.is_empty())
                {
                    Some(token) => token.to_string(),
                    None if is_non_interactive(oauth) => {
                        tokio::time::timeout(fetch_timeout, fetcher.fetch_token(oauth, ctx))
                            .await
                            .map_err(|_| {
                                DomainError::InvalidInput(format!(
                                    "Auth node \"{label}\": the token request timed out after {}",
                                    describe_duration(fetch_timeout)
                                ))
                            })?
                            .map_err(|e| {
                                DomainError::InvalidInput(format!("Auth node \"{label}\": {e}"))
                            })?
                    }
                    None => {
                        return Err(DomainError::InvalidInput(format!(
                            "Auth node \"{label}\" needs you to authenticate first."
                        )));
                    }
                };
                (
                    Auth::Bearer {
                        token: token.clone(),
                    },
                    Some(token),
                )
            }
            other => resolve_static_token(other, &ctx.vars),
        };
        if let Some(value) = wire {
            creds.wire_values.push((node.id.clone(), value));
        }
        if *apply_to_inherit {
            creds.auto_apply = Some(credential.clone());
        }
        creds.by_node.insert(node.id.clone(), credential);
    }
    Ok(creds)
}

/// Fetches tokens through the existing `OAuth2Service`.
pub struct OAuth2ServiceFetcher {
    service: OAuth2Service,
}

impl OAuth2ServiceFetcher {
    pub fn new(service: OAuth2Service) -> Self {
        Self { service }
    }
}

#[async_trait]
impl FlowTokenFetcher for OAuth2ServiceFetcher {
    async fn fetch_token(&self, flow: &OAuth2Flow, ctx: &FetchContext) -> DomainResult<String> {
        let request = build_get_token_request(flow, ctx)?;
        let config = self
            .service
            .resolve_get_token_request_with_secrets(&request, &ctx.external_secrets);
        let token = self.service.get_token_direct(&config).await?;
        select_token(flow, token)
    }
}

/// Maps a client-credentials or password flow to a get-token request, with
/// every `{{variable}}` resolved against the run's variables. The service
/// only reads the global environment, so this resolution must happen here.
pub(crate) fn build_get_token_request(
    flow: &OAuth2Flow,
    ctx: &FetchContext,
) -> DomainResult<OAuth2GetTokenRequest> {
    let r = |s: &str| rocket_environment::resolve(s, &ctx.vars).output;
    let params = |list: Option<&Vec<OAuth2AdditionalParameter>>| -> Option<Vec<AdditionalParam>> {
        list.map(|items| {
            items
                .iter()
                .map(|p| AdditionalParam {
                    key: r(&p.name),
                    value: r(&p.value),
                    send_in: if p.placement.as_deref() == Some("query") {
                        "queryparams".to_string()
                    } else {
                        "body".to_string()
                    },
                    enabled: p.enabled,
                })
                .collect()
        })
    };
    let make = |grant_type: &str,
                token_url: &str,
                credentials: &OAuth2ClientCredentials,
                scope: Option<&String>,
                owner: Option<(&str, &str)>,
                additional: Option<&OAuth2AdditionalParameters>,
                settings: Option<&OAuth2Settings>| {
        OAuth2GetTokenRequest {
            grant_type: grant_type.to_string(),
            authorization_url: None,
            token_url: Some(r(token_url)),
            callback_url: None,
            client_id: r(&credentials.client_id),
            client_secret: Some(r(&credentials.client_secret)),
            scope: scope.map(|s| r(s)),
            state: None,
            username: owner.map(|(u, _)| r(u)),
            password: owner.map(|(_, p)| r(p)),
            client_authentication: Some(
                if credentials.placement.as_deref() == Some("basic_auth_header") {
                    "header".to_string()
                } else {
                    "body".to_string()
                },
            ),
            use_pkce: None,
            use_system_browser: None,
            verify_ssl: settings.and_then(|s| s.verify_ssl),
            auth_params: None,
            token_params: params(additional.and_then(|a| a.access_token_request.as_ref())),
            refresh_params: params(additional.and_then(|a| a.refresh_token_request.as_ref())),
            collection: Some(ctx.collection.clone()),
            environment_name: ctx.environment_name.clone(),
            request_path: None,
            force_reauth: None,
        }
    };
    match flow {
        OAuth2Flow::ClientCredentials {
            access_token_url,
            credentials,
            scope,
            additional_parameters,
            settings,
            ..
        } => Ok(make(
            "client_credentials",
            access_token_url,
            credentials,
            scope.as_ref(),
            None,
            additional_parameters.as_ref(),
            settings.as_ref(),
        )),
        OAuth2Flow::ResourceOwnerPassword {
            access_token_url,
            credentials,
            resource_owner,
            scope,
            additional_parameters,
            settings,
            ..
        } => Ok(make(
            "password",
            access_token_url,
            credentials,
            scope.as_ref(),
            resource_owner
                .as_ref()
                .map(|o| (o.username.as_str(), o.password.as_str())),
            additional_parameters.as_ref(),
            settings.as_ref(),
        )),
        _ => Err(DomainError::InvalidInput(
            "only client credentials and password grants can be fetched during a run".to_string(),
        )),
    }
}

/// Picks the access token or, when the flow's token config asks for it, the ID token.
fn select_token(flow: &OAuth2Flow, token: OAuthToken) -> DomainResult<String> {
    let token_config: Option<&OAuth2TokenConfig> = match flow {
        OAuth2Flow::ClientCredentials { token_config, .. }
        | OAuth2Flow::ResourceOwnerPassword { token_config, .. } => token_config.as_ref(),
        _ => None,
    };
    let wants_id_token = token_config.and_then(|t| t.source.as_deref()) == Some("idToken");
    if wants_id_token {
        token
            .id_token
            .filter(|t| !t.is_empty())
            .ok_or_else(|| DomainError::InvalidInput("the token response has no id_token".into()))
    } else {
        if token.access_token.trim().is_empty() {
            return Err(DomainError::InvalidInput(
                "the token response has an empty access token".into(),
            ));
        }
        Ok(token.access_token)
    }
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::sync::{Arc, Mutex};

    use async_trait::async_trait;
    use rocket_shared::error::{DomainError, DomainResult};
    use rocket_shared::oauth2::{OAuth2ClientCredentials, OAuth2Flow};

    use super::{FetchContext, FlowTokenFetcher};

    pub(crate) fn client_credentials() -> OAuth2Flow {
        OAuth2Flow::ClientCredentials {
            access_token_url: "https://idp.example.com/token".to_string(),
            refresh_token_url: None,
            credentials: OAuth2ClientCredentials {
                client_id: "{{clientId}}".to_string(),
                client_secret: "{{clientSecret}}".to_string(),
                placement: None,
            },
            scope: Some("read".to_string()),
            additional_parameters: None,
            token_config: None,
            settings: None,
        }
    }

    pub(crate) fn authorization_code() -> OAuth2Flow {
        OAuth2Flow::AuthorizationCode {
            authorization_url: "https://idp.example.com/authorize".to_string(),
            access_token_url: "https://idp.example.com/token".to_string(),
            refresh_token_url: None,
            callback_url: None,
            credentials: OAuth2ClientCredentials {
                client_id: "cid".to_string(),
                client_secret: "secret".to_string(),
                placement: None,
            },
            scope: None,
            state: None,
            pkce: None,
            additional_parameters: None,
            token_config: None,
            settings: None,
        }
    }

    /// A fetcher that returns a fixed result and counts its calls.
    pub(crate) struct FakeFetcher {
        result: Result<String, String>,
        calls: Mutex<u32>,
    }

    impl FakeFetcher {
        pub(crate) fn ok(token: &str) -> Arc<Self> {
            Arc::new(Self {
                result: Ok(token.to_string()),
                calls: Mutex::new(0),
            })
        }
        pub(crate) fn err(message: &str) -> Arc<Self> {
            Arc::new(Self {
                result: Err(message.to_string()),
                calls: Mutex::new(0),
            })
        }
        pub(crate) fn calls(&self) -> u32 {
            *self.calls.lock().expect("lock calls")
        }
    }

    #[async_trait]
    impl FlowTokenFetcher for FakeFetcher {
        async fn fetch_token(
            &self,
            _flow: &OAuth2Flow,
            _ctx: &FetchContext,
        ) -> DomainResult<String> {
            *self.calls.lock().expect("lock calls") += 1;
            self.result.clone().map_err(DomainError::Http)
        }
    }

    /// A fetcher whose token endpoint accepts the request and never answers.
    pub(crate) struct HangingFetcher;

    #[async_trait]
    impl FlowTokenFetcher for HangingFetcher {
        async fn fetch_token(
            &self,
            _flow: &OAuth2Flow,
            _ctx: &FetchContext,
        ) -> DomainResult<String> {
            std::future::pending::<DomainResult<String>>().await
        }
    }

    /// Hands one `Arc<FakeFetcher>` to a service expecting a `Box<dyn FlowTokenFetcher>`.
    pub(crate) struct SharedFetcher(pub(crate) Arc<FakeFetcher>);

    #[async_trait]
    impl FlowTokenFetcher for SharedFetcher {
        async fn fetch_token(&self, flow: &OAuth2Flow, ctx: &FetchContext) -> DomainResult<String> {
            self.0.fetch_token(flow, ctx).await
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use rocket_flow::{Flow, FlowNode, FlowNodeKind, NodePosition};
    use rocket_shared::types::Auth;

    use super::test_support::{authorization_code, client_credentials, FakeFetcher};
    use super::*;

    fn auth_node(id: &str, auth: Auth, apply_to_inherit: bool) -> FlowNode {
        FlowNode {
            id: id.to_string(),
            kind: FlowNodeKind::Auth {
                label: format!("Auth {id}"),
                auth,
                apply_to_inherit,
            },
            position: NodePosition { x: 0.0, y: 0.0 },
        }
    }

    fn flow_with(nodes: Vec<FlowNode>) -> Flow {
        Flow {
            name: "f".to_string(),
            nodes,
            edges: Vec::new(),
            callback_host: None,
        }
    }

    fn ctx() -> FetchContext {
        FetchContext {
            collection: "my-api".to_string(),
            vars: HashMap::from([("token".to_string(), "resolved-token-123".to_string())]),
            ..FetchContext::default()
        }
    }

    fn supplied(node_id: &str, token: &str) -> FlowAuthTokens {
        HashMap::from([(
            node_id.to_string(),
            SuppliedToken {
                access_token: token.to_string(),
            },
        )])
    }

    /// `resolve_flow_credentials` with the production fetch timeout.
    async fn resolve(
        flow: &Flow,
        supplied: &FlowAuthTokens,
        fetcher: &dyn FlowTokenFetcher,
        ctx: &FetchContext,
    ) -> DomainResult<FlowCredentials> {
        resolve_flow_credentials(flow, supplied, fetcher, ctx, TOKEN_FETCH_TIMEOUT).await
    }

    #[tokio::test]
    async fn a_token_fetch_that_never_answers_times_out_and_names_the_node() {
        use super::test_support::HangingFetcher;

        let flow = flow_with(vec![auth_node(
            "a",
            Auth::OAuth2(Box::new(client_credentials())),
            true,
        )]);

        // The outer guard turns a missing timeout into a failure, not a hang.
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            resolve_flow_credentials(
                &flow,
                &HashMap::new(),
                &HangingFetcher,
                &ctx(),
                Duration::from_millis(50),
            ),
        )
        .await
        .expect("the fetch must be bounded by the token fetch timeout");

        let message = result.expect_err("a hanging fetch must fail").to_string();
        assert!(
            message.contains("Auth a") && message.contains("timed out"),
            "got: {message}"
        );
    }

    #[tokio::test]
    async fn a_fast_fetch_is_unaffected_by_the_timeout() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::OAuth2(Box::new(client_credentials())),
            true,
        )]);
        let fetcher = FakeFetcher::ok("fetched-token-999");

        let creds = resolve_flow_credentials(
            &flow,
            &HashMap::new(),
            fetcher.as_ref(),
            &ctx(),
            Duration::from_millis(50),
        )
        .await
        .expect("a fast fetch resolves");

        assert_eq!(creds.wire_value("a"), Some("fetched-token-999"));
        assert_eq!(TOKEN_FETCH_TIMEOUT, Duration::from_secs(30));
    }

    #[tokio::test]
    async fn a_static_bearer_passes_through_and_resolves_its_wire_value() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::Bearer {
                token: "{{token}}".to_string(),
            },
            true,
        )]);
        let fetcher = FakeFetcher::ok("unused");

        let creds = resolve(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
            .await
            .expect("static auth resolves");

        assert_eq!(
            creds.auth_for_node("a"),
            Some(&Auth::Bearer {
                token: "resolved-token-123".to_string()
            }),
            "the credential is resolved at run start, so what is sent is what is masked"
        );
        assert_eq!(creds.wire_value("a"), Some("resolved-token-123"));
        assert!(creds.secret_forms().contains("resolved-token-123"));
        assert_eq!(fetcher.calls(), 0);
    }

    #[tokio::test]
    async fn a_static_api_key_is_resolved_at_run_start() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::ApiKey {
                key: "X-{{keyName}}".to_string(),
                value: "{{token}}".to_string(),
                placement: "header".to_string(),
            },
            true,
        )]);
        let mut context = ctx();
        context
            .vars
            .insert("keyName".to_string(), "Api-Key".to_string());
        let fetcher = FakeFetcher::ok("unused");

        let creds = resolve(&flow, &HashMap::new(), fetcher.as_ref(), &context)
            .await
            .expect("static auth resolves");

        assert_eq!(
            creds.auth_for_node("a"),
            Some(&Auth::ApiKey {
                key: "X-Api-Key".to_string(),
                value: "resolved-token-123".to_string(),
                placement: "header".to_string(),
            })
        );
        assert_eq!(creds.wire_value("a"), Some("resolved-token-123"));
        assert!(creds.secret_forms().contains("resolved-token-123"));
    }

    #[tokio::test]
    async fn a_placeholder_unresolved_at_run_start_is_left_for_the_request_and_has_no_wire_value() {
        let flow = flow_with(vec![
            auth_node(
                "a",
                Auth::Bearer {
                    token: "{{setByScript}}".to_string(),
                },
                true,
            ),
            auth_node(
                "k",
                Auth::ApiKey {
                    key: "X-Api-Key".to_string(),
                    value: "{{setByScript}}".to_string(),
                    placement: "query".to_string(),
                },
                false,
            ),
        ]);
        let fetcher = FakeFetcher::ok("unused");

        let creds = resolve(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
            .await
            .expect("an unresolved placeholder does not fail the run");

        assert_eq!(
            creds.auth_for_node("a"),
            Some(&Auth::Bearer {
                token: "{{setByScript}}".to_string()
            }),
            "the request still resolves it at send time"
        );
        assert_eq!(
            creds.auth_for_node("k"),
            Some(&Auth::ApiKey {
                key: "X-Api-Key".to_string(),
                value: "{{setByScript}}".to_string(),
                placement: "query".to_string(),
            })
        );
        assert_eq!(creds.wire_value("a"), None);
        assert_eq!(creds.wire_value("k"), None);
        assert_eq!(creds.secrets().count(), 0, "a template is never a secret");
    }

    #[tokio::test]
    async fn a_value_that_resolves_to_another_placeholder_is_left_for_the_request() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::Bearer {
                token: "{{token}}".to_string(),
            },
            true,
        )]);
        let context = FetchContext {
            vars: HashMap::from([("token".to_string(), "{{inner}}".to_string())]),
            ..ctx()
        };
        let fetcher = FakeFetcher::ok("unused");

        let creds = resolve(&flow, &HashMap::new(), fetcher.as_ref(), &context)
            .await
            .expect("resolves");

        assert_eq!(
            creds.auth_for_node("a"),
            Some(&Auth::Bearer {
                token: "{{token}}".to_string()
            })
        );
        assert_eq!(creds.wire_value("a"), None);
    }

    #[tokio::test]
    async fn a_supplied_token_beats_a_fetch() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::OAuth2(Box::new(client_credentials())),
            true,
        )]);
        let fetcher = FakeFetcher::ok("fetched-token-999");

        let creds = resolve(
            &flow,
            &supplied("a", "supplied-token-123"),
            fetcher.as_ref(),
            &ctx(),
        )
        .await
        .expect("resolves");

        assert_eq!(
            creds.auth_for_node("a"),
            Some(&Auth::Bearer {
                token: "supplied-token-123".to_string()
            })
        );
        assert_eq!(fetcher.calls(), 0);
    }

    #[tokio::test]
    async fn a_blank_supplied_token_counts_as_missing() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::OAuth2(Box::new(client_credentials())),
            true,
        )]);
        let fetcher = FakeFetcher::ok("fetched-token-999");

        let creds = resolve(&flow, &supplied("a", "  "), fetcher.as_ref(), &ctx())
            .await
            .expect("resolves");

        assert_eq!(creds.wire_value("a"), Some("fetched-token-999"));
        assert_eq!(fetcher.calls(), 1);
    }

    #[tokio::test]
    async fn a_non_interactive_grant_is_fetched_when_nothing_is_supplied() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::OAuth2(Box::new(client_credentials())),
            true,
        )]);
        let fetcher = FakeFetcher::ok("fetched-token-999");

        let creds = resolve(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
            .await
            .expect("resolves");

        assert_eq!(
            creds.auth_for_node("a"),
            Some(&Auth::Bearer {
                token: "fetched-token-999".to_string()
            })
        );
        assert_eq!(fetcher.calls(), 1);
    }

    #[tokio::test]
    async fn an_interactive_grant_without_a_token_fails_and_never_fetches() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::OAuth2(Box::new(authorization_code())),
            true,
        )]);
        let fetcher = FakeFetcher::ok("fetched-token-999");

        let err = resolve(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
            .await
            .expect_err("must fail");

        let message = err.to_string();
        assert!(
            message.contains("Auth a") && message.contains("needs you to authenticate first"),
            "got: {message}"
        );
        assert_eq!(fetcher.calls(), 0);
    }

    #[tokio::test]
    async fn a_failed_fetch_names_the_node() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::OAuth2(Box::new(client_credentials())),
            true,
        )]);
        let fetcher = FakeFetcher::err("invalid_client");

        let err = resolve(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
            .await
            .expect_err("must fail");

        let message = err.to_string();
        assert!(
            message.contains("Auth a") && message.contains("invalid_client"),
            "got: {message}"
        );
    }

    #[tokio::test]
    async fn a_none_or_inherit_config_is_rejected() {
        for auth in [Auth::None, Auth::Inherit] {
            let flow = flow_with(vec![auth_node("a", auth, false)]);
            let fetcher = FakeFetcher::ok("x");
            let err = resolve(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
                .await
                .expect_err("must fail");
            assert!(err.to_string().contains("Auth a"), "got: {err}");
        }
    }

    #[tokio::test]
    async fn only_the_flagged_node_applies_to_inherit_and_only_over_inherit() {
        let flow = flow_with(vec![
            auth_node(
                "a",
                Auth::Basic {
                    username: "u".to_string(),
                    password: "p".to_string(),
                },
                false,
            ),
            auth_node(
                "b",
                Auth::Bearer {
                    token: "{{token}}".to_string(),
                },
                true,
            ),
        ]);
        let fetcher = FakeFetcher::ok("x");
        let creds = resolve(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
            .await
            .expect("resolves");

        let mut inherited = Auth::Inherit;
        creds.apply_to_inherit(&mut inherited);
        assert_eq!(
            inherited,
            Auth::Bearer {
                token: "resolved-token-123".to_string()
            }
        );

        let mut own = Auth::Basic {
            username: "mine".to_string(),
            password: "mine".to_string(),
        };
        creds.apply_to_inherit(&mut own);
        assert_eq!(
            own,
            Auth::Basic {
                username: "mine".to_string(),
                password: "mine".to_string()
            },
            "a request with its own auth is never replaced"
        );
    }

    #[tokio::test]
    async fn apply_to_inherit_replaces_none_and_inherit_but_never_explicit_auth() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::Bearer {
                token: "t".to_string(),
            },
            true,
        )]);
        let fetcher = FakeFetcher::ok("x");
        let creds = resolve(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
            .await
            .expect("resolves");
        let bearer = Auth::Bearer {
            token: "t".to_string(),
        };

        for start in [Auth::None, Auth::Inherit] {
            let mut auth = start.clone();
            creds.apply_to_inherit(&mut auth);
            assert_eq!(auth, bearer, "{start:?} takes the credential");
        }
        for own in [
            Auth::Basic {
                username: "u".to_string(),
                password: "p".to_string(),
            },
            Auth::Bearer {
                token: "mine".to_string(),
            },
        ] {
            let mut auth = own.clone();
            creds.apply_to_inherit(&mut auth);
            assert_eq!(auth, own, "explicit auth is kept");
        }
    }

    #[tokio::test]
    async fn no_applying_node_leaves_none_alone() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::Bearer {
                token: "t".to_string(),
            },
            false,
        )]);
        let fetcher = FakeFetcher::ok("x");
        let creds = resolve(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
            .await
            .expect("resolves");
        let mut auth = Auth::None;
        creds.apply_to_inherit(&mut auth);
        assert_eq!(auth, Auth::None);
    }

    #[tokio::test]
    async fn no_flagged_node_leaves_inherit_alone() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::Bearer {
                token: "t".to_string(),
            },
            false,
        )]);
        let fetcher = FakeFetcher::ok("x");
        let creds = resolve(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
            .await
            .expect("resolves");

        let mut inherited = Auth::Inherit;
        creds.apply_to_inherit(&mut inherited);
        assert_eq!(inherited, Auth::Inherit);
    }

    #[tokio::test]
    async fn wire_values_are_exposed_as_secrets_for_redaction() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::OAuth2(Box::new(client_credentials())),
            true,
        )]);
        let fetcher = FakeFetcher::ok("fetched-token-999");
        let creds = resolve(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
            .await
            .expect("resolves");

        let secrets: Vec<(&str, &str)> = creds.secrets().collect();
        assert_eq!(secrets, vec![("a", "fetched-token-999")]);
        assert!(creds.secret_forms().contains("fetched-token-999"));
    }

    #[tokio::test]
    async fn debug_output_never_shows_a_token_or_a_secret_variable() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::OAuth2(Box::new(client_credentials())),
            true,
        )]);
        let fetcher = FakeFetcher::ok("fetched-token-999");
        let context = FetchContext {
            collection: "my-api".to_string(),
            vars: HashMap::from([("clientSecret".to_string(), "var-secret-777".to_string())]),
            external_secrets: HashMap::from([(
                "vault.key".to_string(),
                "vault-secret-555".to_string(),
            )]),
            ..FetchContext::default()
        };
        let creds = resolve(&flow, &HashMap::new(), fetcher.as_ref(), &context)
            .await
            .expect("resolves");
        let supplied = SuppliedToken {
            access_token: "supplied-token-123".to_string(),
        };

        let printed = format!("{creds:?} {context:?} {supplied:?}");

        for secret in [
            "fetched-token-999",
            "var-secret-777",
            "vault-secret-555",
            "supplied-token-123",
        ] {
            assert!(
                !printed.contains(secret),
                "Debug leaked {secret}: {printed}"
            );
        }
    }

    use rocket_shared::oauth2::{
        OAuth2AdditionalParameter, OAuth2AdditionalParameters, OAuth2ClientCredentials, OAuth2Flow,
        OAuth2ResourceOwner, OAuth2Settings, OAuth2TokenConfig,
    };

    fn vars_ctx() -> FetchContext {
        FetchContext {
            collection: "my-api".to_string(),
            environment_name: Some("dev".to_string()),
            vars: HashMap::from([
                ("clientId".to_string(), "cid-1".to_string()),
                ("clientSecret".to_string(), "sec-1".to_string()),
            ]),
            ..FetchContext::default()
        }
    }

    #[test]
    fn the_request_resolves_variables_from_the_run_context() {
        let request = build_get_token_request(&client_credentials(), &vars_ctx()).expect("maps");

        assert_eq!(request.grant_type, "client_credentials");
        assert_eq!(request.client_id, "cid-1");
        assert_eq!(request.client_secret.as_deref(), Some("sec-1"));
        assert_eq!(
            request.token_url.as_deref(),
            Some("https://idp.example.com/token")
        );
        assert_eq!(request.scope.as_deref(), Some("read"));
        assert_eq!(request.client_authentication.as_deref(), Some("body"));
        assert_eq!(request.collection.as_deref(), Some("my-api"));
        assert_eq!(request.environment_name.as_deref(), Some("dev"));
    }

    #[test]
    fn basic_auth_header_placement_maps_to_header_client_authentication() {
        let flow = OAuth2Flow::ClientCredentials {
            access_token_url: "https://idp.example.com/token".to_string(),
            refresh_token_url: None,
            credentials: OAuth2ClientCredentials {
                client_id: "id".to_string(),
                client_secret: "secret".to_string(),
                placement: Some("basic_auth_header".to_string()),
            },
            scope: None,
            additional_parameters: Some(OAuth2AdditionalParameters {
                authorization_request: None,
                access_token_request: Some(vec![
                    OAuth2AdditionalParameter {
                        name: "audience".to_string(),
                        value: "{{clientId}}".to_string(),
                        placement: Some("body".to_string()),
                        enabled: true,
                    },
                    OAuth2AdditionalParameter {
                        name: "tenant".to_string(),
                        value: "t1".to_string(),
                        placement: Some("query".to_string()),
                        enabled: false,
                    },
                ]),
                refresh_token_request: None,
            }),
            token_config: None,
            settings: Some(OAuth2Settings {
                auto_fetch_token: None,
                auto_refresh_token: None,
                verify_ssl: Some(false),
                use_system_browser: None,
            }),
        };

        let request = build_get_token_request(&flow, &vars_ctx()).expect("maps");

        assert_eq!(request.client_authentication.as_deref(), Some("header"));
        assert_eq!(request.verify_ssl, Some(false));
        let params = request.token_params.expect("token params");
        assert_eq!(params.len(), 2);
        assert_eq!(params[0].key, "audience");
        assert_eq!(params[0].value, "cid-1");
        assert_eq!(params[0].send_in, "body");
        assert!(params[0].enabled);
        assert_eq!(params[1].send_in, "queryparams");
        assert!(!params[1].enabled);
    }

    #[test]
    fn the_password_grant_carries_the_resource_owner() {
        let flow = OAuth2Flow::ResourceOwnerPassword {
            access_token_url: "https://idp.example.com/token".to_string(),
            refresh_token_url: None,
            credentials: OAuth2ClientCredentials {
                client_id: "id".to_string(),
                client_secret: "secret".to_string(),
                placement: None,
            },
            resource_owner: Some(OAuth2ResourceOwner {
                username: "{{clientId}}".to_string(),
                password: "pw".to_string(),
            }),
            scope: None,
            additional_parameters: None,
            token_config: None,
            settings: None,
        };

        let request = build_get_token_request(&flow, &vars_ctx()).expect("maps");

        assert_eq!(request.grant_type, "password");
        assert_eq!(request.username.as_deref(), Some("cid-1"));
        assert_eq!(request.password.as_deref(), Some("pw"));
    }

    #[test]
    fn an_interactive_grant_cannot_be_fetched() {
        let err = build_get_token_request(&authorization_code(), &vars_ctx())
            .expect_err("interactive grants are not fetched in the backend");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }

    #[test]
    fn the_id_token_is_chosen_when_the_token_config_says_so() {
        let mut flow = client_credentials();
        if let OAuth2Flow::ClientCredentials { token_config, .. } = &mut flow {
            *token_config = Some(OAuth2TokenConfig {
                id: None,
                source: Some("idToken".to_string()),
                placement: None,
            });
        }
        let token = rocket_http::OAuthToken {
            access_token: "access-1".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: None,
            refresh_token: None,
            scope: None,
            id_token: Some("id-1".to_string()),
        };
        assert_eq!(
            select_token(&flow, token.clone()).expect("id token"),
            "id-1"
        );
        assert_eq!(
            select_token(&client_credentials(), token).expect("access token"),
            "access-1"
        );
    }

    #[test]
    fn a_missing_id_token_is_an_error() {
        let mut flow = client_credentials();
        if let OAuth2Flow::ClientCredentials { token_config, .. } = &mut flow {
            *token_config = Some(OAuth2TokenConfig {
                id: None,
                source: Some("idToken".to_string()),
                placement: None,
            });
        }
        let token = rocket_http::OAuthToken {
            access_token: "access-1".to_string(),
            token_type: "Bearer".to_string(),
            expires_in: None,
            refresh_token: None,
            scope: None,
            id_token: None,
        };
        assert!(select_token(&flow, token).is_err());
    }

    #[test]
    fn a_blank_access_token_is_an_error() {
        for blank in ["", "   "] {
            let token = rocket_http::OAuthToken {
                access_token: blank.to_string(),
                token_type: "Bearer".to_string(),
                expires_in: None,
                refresh_token: None,
                scope: None,
                id_token: Some("id-1".to_string()),
            };
            let err = select_token(&client_credentials(), token).expect_err("blank token");
            assert!(
                matches!(&err, DomainError::InvalidInput(m) if m == "the token response has an empty access token"),
                "got: {err}"
            );
        }
    }
}
