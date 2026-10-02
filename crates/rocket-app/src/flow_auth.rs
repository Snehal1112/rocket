//! Run-start credentials for Flow Auth nodes. See
//! `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md`.

use std::collections::{HashMap, HashSet};

use async_trait::async_trait;
use rocket_flow::{Flow, FlowNodeKind};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::oauth2::OAuth2Flow;
use rocket_shared::types::Auth;

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

    /// Replaces `auth` with the auto-apply credential when `auth` is `inherit`.
    /// Any other auth is left alone.
    pub(crate) fn apply_to_inherit(&self, auth: &mut Auth) {
        if matches!(auth, Auth::Inherit) {
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

fn is_non_interactive(flow: &OAuth2Flow) -> bool {
    matches!(
        flow,
        OAuth2Flow::ClientCredentials { .. } | OAuth2Flow::ResourceOwnerPassword { .. }
    )
}

/// The plain value an auth puts on a wire, with `{{variables}}` resolved.
fn wire_value_of(credential: &Auth, vars: &HashMap<String, String>) -> Option<String> {
    let resolved = match credential {
        Auth::Bearer { token } => rocket_environment::resolve(token, vars).output,
        Auth::ApiKey { value, .. } => rocket_environment::resolve(value, vars).output,
        _ => return None,
    };
    (!resolved.is_empty()).then_some(resolved)
}

/// Resolves every Auth node of `flow` into a credential, before the run starts.
///
/// Order per OAuth2 node: a supplied token wins; otherwise a non-interactive
/// grant is fetched; otherwise the run cannot start. Other auth types pass
/// through with their `{{variables}}` intact, because the request resolves
/// them at send time with its own scopes.
pub(crate) async fn resolve_flow_credentials(
    flow: &Flow,
    supplied: &FlowAuthTokens,
    fetcher: &dyn FlowTokenFetcher,
    ctx: &FetchContext,
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
        let credential = match auth {
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
                        fetcher.fetch_token(oauth, ctx).await.map_err(|e| {
                            DomainError::InvalidInput(format!("Auth node \"{label}\": {e}"))
                        })?
                    }
                    None => {
                        return Err(DomainError::InvalidInput(format!(
                            "Auth node \"{label}\" needs you to authenticate first."
                        )));
                    }
                };
                Auth::Bearer { token }
            }
            other => other.clone(),
        };
        if let Some(value) = wire_value_of(&credential, &ctx.vars) {
            creds.wire_values.push((node.id.clone(), value));
        }
        if *apply_to_inherit {
            creds.auto_apply = Some(credential.clone());
        }
        creds.by_node.insert(node.id.clone(), credential);
    }
    Ok(creds)
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

        let creds = resolve_flow_credentials(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
            .await
            .expect("static auth resolves");

        assert_eq!(
            creds.auth_for_node("a"),
            Some(&Auth::Bearer {
                token: "{{token}}".to_string()
            }),
            "variables stay unresolved in the credential; the request resolves them"
        );
        assert_eq!(creds.wire_value("a"), Some("resolved-token-123"));
        assert_eq!(fetcher.calls(), 0);
    }

    #[tokio::test]
    async fn a_supplied_token_beats_a_fetch() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::OAuth2(Box::new(client_credentials())),
            true,
        )]);
        let fetcher = FakeFetcher::ok("fetched-token-999");

        let creds = resolve_flow_credentials(
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

        let creds = resolve_flow_credentials(&flow, &supplied("a", "  "), fetcher.as_ref(), &ctx())
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

        let creds = resolve_flow_credentials(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
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

        let err = resolve_flow_credentials(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
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

        let err = resolve_flow_credentials(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
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
            let err = resolve_flow_credentials(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
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
        let creds = resolve_flow_credentials(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
            .await
            .expect("resolves");

        let mut inherited = Auth::Inherit;
        creds.apply_to_inherit(&mut inherited);
        assert_eq!(
            inherited,
            Auth::Bearer {
                token: "{{token}}".to_string()
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
    async fn no_flagged_node_leaves_inherit_alone() {
        let flow = flow_with(vec![auth_node(
            "a",
            Auth::Bearer {
                token: "t".to_string(),
            },
            false,
        )]);
        let fetcher = FakeFetcher::ok("x");
        let creds = resolve_flow_credentials(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
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
        let creds = resolve_flow_credentials(&flow, &HashMap::new(), fetcher.as_ref(), &ctx())
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
        let creds = resolve_flow_credentials(&flow, &HashMap::new(), fetcher.as_ref(), &context)
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
}
