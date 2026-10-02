# Flow Auth Node — Plan 2: Backend credential resolution

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Plan 2 of 7.** Previous plan: `docs/superpowers/plans/2026-10-02-flow-auth-node-01-domain.md` (must be merged and green).
**Next plan to run: `docs/superpowers/plans/2026-10-02-flow-auth-node-03-executor.md`**
**Recommended model: Opus** (token handling, async trait boundaries, error paths).

**Goal:** Before a flow run starts, turn every Auth node into a ready-to-use credential: use the token the UI supplied, or fetch a non-interactive OAuth2 token in the backend, or fail the run with a message naming the node.

**Architecture:** A new `rocket-app` module `flow_auth.rs` owns the resolution logic, a `FlowTokenFetcher` port (trait) and an adapter over the existing `OAuth2Service`. `FlowExecutionService` gains `with_token_fetcher` and `run_with_auth`; `run` delegates with no tokens. Resolved tokens are injected into the run's `external_secrets` map so the existing redaction (flow step output, executor history, vault write guard) masks them with no new code.

**Tech Stack:** Rust, `async-trait`, tokio tests.

## Global Constraints

- Spec: `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md`.
- Tokens are never written to disk, never cached beyond the run, and registered for redaction for the whole run.
- Interactive grants (authorization code, implicit) are never started by the backend; a missing token fails the run before the first event with `Auth node "<label>" needs you to authenticate first.`
- Non-interactive grants are client credentials and resource-owner password only.
- No `unwrap()` in production Rust paths; never shell out to `git`.
- `#[serde(rename_all = "camelCase")]` only on IPC DTOs.
- Conventional commits.
- Before each commit run `cargo fmt` (the plan's code is not rustfmt-checked) and re-run the task's tests.
- Verification per task: `cargo check -j4`, focused `cargo test -p rocket-app -j4 <name>`.

## Known hazard (read before Task 2)

`OAuth2Service::build_variable_context` only reads the **global** environment repo, not a collection's own environments. So `{{vars}}` in an Auth node's OAuth2 config would not resolve there. Task 2's fetcher therefore pre-resolves every string field with the full run variable map (`RequestExecutionService::build_variable_context`) before calling the service. Do not skip that step.

## File Structure

| File | Change |
|---|---|
| `crates/rocket-app/src/flow_auth.rs` | **Create**: types, port, resolution, `FlowCredentials`, `OAuth2ServiceFetcher`, `test_support` |
| `crates/rocket-app/src/lib.rs` | Declare module, re-export public types |
| `crates/rocket-app/src/flow_execution_service.rs` | `token_fetcher` field, `with_token_fetcher`, `run_with_auth`, secret injection, tests |

---

### Task 1: Types, port and credential resolution

**Files:**
- Create: `crates/rocket-app/src/flow_auth.rs`
- Modify: `crates/rocket-app/src/lib.rs`

**Interfaces:**
- Consumes: `FlowNodeKind::Auth { label, auth, apply_to_inherit }` (Plan 1).
- Produces (used by Tasks 2–3 and Plan 3):

```rust
pub struct SuppliedToken { pub access_token: String }
pub type FlowAuthTokens = HashMap<String, SuppliedToken>;          // node id -> token
pub struct FetchContext { pub collection: String, pub environment_name: Option<String>,
                          pub vars: HashMap<String, String>, pub external_secrets: HashMap<String, String> }
#[async_trait] pub trait FlowTokenFetcher: Send + Sync {
    async fn fetch_token(&self, flow: &OAuth2Flow, ctx: &FetchContext) -> DomainResult<String>;
}
pub struct NoTokenFetcher;
pub(crate) struct FlowCredentials { /* private */ }
impl FlowCredentials {
    pub(crate) fn auth_for_node(&self, node_id: &str) -> Option<&Auth>;
    pub(crate) fn apply_to_inherit(&self, auth: &mut Auth);
    pub(crate) fn wire_value(&self, node_id: &str) -> Option<&str>;
    pub(crate) fn secrets(&self) -> impl Iterator<Item = (&str, &str)>;   // (node_id, secret)
    pub(crate) fn secret_forms(&self) -> HashSet<String>;                 // for redaction
}
pub(crate) async fn resolve_flow_credentials(flow: &Flow, supplied: &FlowAuthTokens,
    fetcher: &dyn FlowTokenFetcher, ctx: &FetchContext) -> DomainResult<FlowCredentials>;
#[cfg(test)] pub(crate) mod test_support { client_credentials(), authorization_code(), FakeFetcher, SharedFetcher }
```

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (sections 3.9–3.12, OAuth2 flows).

- [ ] **Step 2: Create `flow_auth.rs` with the tests first**

Create `crates/rocket-app/src/flow_auth.rs` containing only the test module and `test_support` (the production code is added in Step 4, so the file does not compile yet — that is the failing state):

```rust
//! Run-start credentials for Flow Auth nodes. See
//! `docs/superpowers/specs/2026-10-02-flow-auth-node-design.md`.

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
            external_secrets: HashMap::from([("vault.key".to_string(), "vault-secret-555".to_string())]),
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
            assert!(!printed.contains(secret), "Debug leaked {secret}: {printed}");
        }
    }
}
```

- [ ] **Step 3: Declare the module and run to see it fail**

In `crates/rocket-app/src/lib.rs`, add `pub mod flow_auth;` after `pub(crate) mod flow_callbacks;` and add this line next to the other `pub use` lines:

```rust
pub use flow_auth::{FetchContext, FlowAuthTokens, FlowTokenFetcher, NoTokenFetcher, SuppliedToken};
```

Run: `cargo test -p rocket-app -j4 flow_auth`
Expected: compile errors (`cannot find FetchContext`, `resolve_flow_credentials`, ...).

- [ ] **Step 4: Add the production code**

Insert at the top of `flow_auth.rs`, directly below the module doc comment and above `#[cfg(test)] pub(crate) mod test_support`:

```rust
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
                    None if is_non_interactive(oauth) => fetcher
                        .fetch_token(oauth, ctx)
                        .await
                        .map_err(|e| {
                            DomainError::InvalidInput(format!("Auth node \"{label}\": {e}"))
                        })?,
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
```

- [ ] **Step 5: Run the tests**

Run: `cargo test -p rocket-app -j4 flow_auth`
Expected: all 11 tests PASS. Note: the test labels are `Auth a` because `auth_node` builds `label: format!("Auth {id}")`, so the asserted substring `Auth a` appears inside `Auth node "Auth a" ...`.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/flow_auth.rs crates/rocket-app/src/lib.rs
git commit -m "feat(flow): resolve Auth node credentials before a run"
```

---

### Task 2: OAuth2 token fetcher adapter

**Files:**
- Modify: `crates/rocket-app/src/flow_auth.rs`
- Modify: `crates/rocket-app/src/lib.rs`

**Interfaces:**
- Consumes: `OAuth2Service::{resolve_get_token_request_with_secrets, get_token_direct}`, `OAuth2GetTokenRequest`, `rocket_http::{AdditionalParam, OAuthToken}`.
- Produces:

```rust
pub struct OAuth2ServiceFetcher { /* wraps OAuth2Service */ }
impl OAuth2ServiceFetcher { pub fn new(service: OAuth2Service) -> Self }
impl FlowTokenFetcher for OAuth2ServiceFetcher
pub(crate) fn build_get_token_request(flow: &OAuth2Flow, ctx: &FetchContext) -> DomainResult<OAuth2GetTokenRequest>
```

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` (sections 3.9 and 3.10).

- [ ] **Step 2: Write the failing mapping tests**

Append inside `mod tests` of `flow_auth.rs`:

```rust
    use rocket_shared::oauth2::{
        OAuth2AdditionalParameter, OAuth2AdditionalParameters, OAuth2ClientCredentials,
        OAuth2Flow, OAuth2ResourceOwner, OAuth2Settings, OAuth2TokenConfig,
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
        let request =
            build_get_token_request(&client_credentials(), &vars_ctx()).expect("maps");

        assert_eq!(request.grant_type, "client_credentials");
        assert_eq!(request.client_id, "cid-1");
        assert_eq!(request.client_secret.as_deref(), Some("sec-1"));
        assert_eq!(request.token_url.as_deref(), Some("https://idp.example.com/token"));
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
        assert_eq!(select_token(&flow, token.clone()).expect("id token"), "id-1");
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
```

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test -p rocket-app -j4 flow_auth`
Expected: compile errors (`build_get_token_request`, `select_token` not found).

- [ ] **Step 4: Implement the adapter**

Add these imports to the top of `flow_auth.rs`:

```rust
use rocket_http::{AdditionalParam, OAuthToken};
use rocket_shared::oauth2::{
    OAuth2AdditionalParameter, OAuth2AdditionalParameters, OAuth2ClientCredentials,
    OAuth2Settings, OAuth2TokenConfig,
};

use crate::oauth2_service::{OAuth2GetTokenRequest, OAuth2Service};
```

Add after `resolve_flow_credentials`:

```rust
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
            token_params: params(
                additional.and_then(|a| a.access_token_request.as_ref()),
            ),
            refresh_params: params(
                additional.and_then(|a| a.refresh_token_request.as_ref()),
            ),
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
    let wants_id_token =
        token_config.and_then(|t| t.source.as_deref()) == Some("idToken");
    if wants_id_token {
        token
            .id_token
            .filter(|t| !t.is_empty())
            .ok_or_else(|| DomainError::InvalidInput("the token response has no id_token".into()))
    } else {
        Ok(token.access_token)
    }
}
```

Is `OAuthToken` `Clone`? The test calls `token.clone()`. If `cargo check` reports it is not, change the test to build the token twice with a small helper `fn token(id: Option<&str>) -> OAuthToken`.

In `lib.rs`, extend the `pub use flow_auth::{...}` line to include `OAuth2ServiceFetcher`.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p rocket-app -j4 flow_auth`
Expected: all PASS (17 tests). Run `cargo clippy -p rocket-app -j4 -- -D warnings` if clippy is available; fix lint findings (the `make` closure intentionally takes 7 arguments, which is under the `too_many_arguments` limit).

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-app/src/flow_auth.rs crates/rocket-app/src/lib.rs
git commit -m "feat(flow): fetch non-interactive OAuth2 tokens for Auth nodes"
```

---

### Task 3: Wire credentials into the run

**Files:**
- Modify: `crates/rocket-app/src/flow_execution_service.rs`

**Interfaces:**
- Consumes: `resolve_flow_credentials`, `FetchContext`, `FlowAuthTokens`, `FlowTokenFetcher`, `NoTokenFetcher`, `FlowCredentials` (Task 1).
- Produces (used by Plans 3–4):
  - `FlowExecutionService::with_token_fetcher(self, Box<dyn FlowTokenFetcher>) -> Self`
  - `FlowExecutionService::run_with_auth(&self, exec: &RequestExecutionService, input: RunFlowInput, auth_tokens: FlowAuthTokens) -> DomainResult<FlowRunSummary>`
  - Inside `run_with_auth`, a local `credentials: FlowCredentials` (Plan 3 passes it on to `execute_node`), and each credential secret present in `external_secrets` under the key `flow-auth.<node id>`.

- [ ] **Step 1: Read the OpenCollection reference**

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

- [ ] **Step 2: Write the failing tests**

In `flow_execution_service.rs`, inside `mod tests`, after the `run_input` helper (around the line `fn run_input(flow_name: &str) -> RunFlowInput`), add:

```rust
    fn auth_flow(auth: rocket_shared::types::Auth) -> Flow {
        Flow {
            name: "auth-flow".to_string(),
            nodes: vec![FlowNode {
                id: "a".to_string(),
                kind: FlowNodeKind::Auth {
                    label: "Sign in".to_string(),
                    auth,
                    apply_to_inherit: true,
                },
                position: NodePosition { x: 0.0, y: 0.0 },
            }],
            edges: Vec::new(),
            callback_host: None,
        }
    }

    #[tokio::test]
    async fn an_unauthenticated_interactive_auth_node_fails_the_run_before_any_event() {
        use crate::flow_auth::test_support::authorization_code;
        use rocket_shared::types::Auth;

        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(
            auth_flow(Auth::OAuth2(Box::new(authorization_code()))),
            &publisher,
        );
        let exec = exec_with_status(200);

        let err = service
            .run(&exec, run_input("auth-flow"))
            .await
            .expect_err("the run must not start");

        assert!(
            err.to_string().contains("needs you to authenticate first"),
            "got: {err}"
        );
        assert!(
            publisher.events().is_empty(),
            "a run that cannot start emits no events"
        );
    }

    #[tokio::test]
    async fn a_supplied_token_lets_the_run_start() {
        use crate::flow_auth::{FlowAuthTokens, SuppliedToken};
        use crate::flow_auth::test_support::authorization_code;
        use rocket_shared::types::Auth;

        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(
            auth_flow(Auth::OAuth2(Box::new(authorization_code()))),
            &publisher,
        );
        let exec = exec_with_status(200);
        let tokens: FlowAuthTokens = HashMap::from([(
            "a".to_string(),
            SuppliedToken {
                access_token: "supplied-token-123".to_string(),
            },
        )]);

        service
            .run_with_auth(&exec, run_input("auth-flow"), tokens)
            .await
            .expect("a supplied token lets the run start");

        assert!(publisher
            .events()
            .iter()
            .any(|e| matches!(e, DomainEvent::FlowRunStarted { .. })));
    }

    #[tokio::test]
    async fn a_non_interactive_auth_node_is_fetched_by_the_fetcher() {
        use crate::flow_auth::test_support::{client_credentials, FakeFetcher, SharedFetcher};
        use rocket_shared::types::Auth;

        let fetcher = FakeFetcher::ok("fetched-token-999");
        let publisher = RecordingPublisher::new();
        let service = service_with_publisher(
            auth_flow(Auth::OAuth2(Box::new(client_credentials()))),
            &publisher,
        )
        .with_token_fetcher(Box::new(SharedFetcher(Arc::clone(&fetcher))));
        let exec = exec_with_status(200);

        service
            .run(&exec, run_input("auth-flow"))
            .await
            .expect("the run starts");

        assert_eq!(fetcher.calls(), 1);
    }
```

If `HashMap`, `Arc`, `DomainEvent` or `RecordingPublisher` are not already in scope in `mod tests`, add the missing `use` (they are used by neighboring tests: `HashMap` and `Arc` come from `use super::*;`, `RecordingPublisher` from the `crate::test_doubles` import above `wire`).

- [ ] **Step 3: Run to verify they fail**

Run: `cargo test -p rocket-app -j4 auth_node`
Expected: compile errors (`no method run_with_auth`, `with_token_fetcher`).

- [ ] **Step 4: Add the field, builder and `run_with_auth`**

At the top of the file, add to the `use crate::...` block:

```rust
use crate::flow_auth::{
    resolve_flow_credentials, FetchContext, FlowAuthTokens, FlowTokenFetcher, NoTokenFetcher,
};
```

In `struct FlowExecutionService`, add the field after `callback_listener`:

```rust
    /// Fetches non-interactive OAuth2 tokens for Auth nodes at run start.
    token_fetcher: Box<dyn FlowTokenFetcher>,
```

In `FlowExecutionService::new`, add `token_fetcher: Box::new(NoTokenFetcher),` after `callback_listener: ...`.

After `with_callback_listener`, add:

```rust
    /// Replaces the default `NoTokenFetcher`. `src-tauri` passes an
    /// `OAuth2ServiceFetcher`; tests pass a `FakeFetcher`.
    pub fn with_token_fetcher(mut self, fetcher: Box<dyn FlowTokenFetcher>) -> Self {
        self.token_fetcher = fetcher;
        self
    }
```

Rename the existing `pub async fn run(` to `pub async fn run_with_auth(` and give it a third parameter, so the signature becomes:

```rust
    pub async fn run_with_auth(
        &self,
        exec: &RequestExecutionService,
        input: RunFlowInput,
        auth_tokens: FlowAuthTokens,
    ) -> DomainResult<FlowRunSummary> {
```

Directly above it, add the delegating `run` and move the existing doc comment of `run` onto `run_with_auth`:

```rust
    /// Runs a flow with no UI-supplied tokens. Same as `run_with_auth` with an empty map.
    pub async fn run(
        &self,
        exec: &RequestExecutionService,
        input: RunFlowInput,
    ) -> DomainResult<FlowRunSummary> {
        self.run_with_auth(exec, input, FlowAuthTokens::new()).await
    }

```

Inside `run_with_auth`, change `let external_secrets = exec.resolve_external_secrets(...).await?;` to `let mut external_secrets = ...` and, immediately after that statement (before `let nodes_by_id`), insert:

```rust
        // Resolve every Auth node before anything is announced, so a run that
        // cannot authenticate fails with no events, like a callback that
        // cannot open. Every credential secret joins `external_secrets`, so the
        // existing redaction masks it in step output, history and logs.
        let fetch_ctx = FetchContext {
            collection: input.collection.clone(),
            environment_name: input.environment_name.clone(),
            vars: exec.build_variable_context(
                input.global_env_name.as_deref(),
                Some(&input.collection),
                input.environment_name.as_deref(),
                None,
                &external_secrets,
            ),
            external_secrets: external_secrets.clone(),
        };
        let credentials = resolve_flow_credentials(
            &flow,
            &auth_tokens,
            self.token_fetcher.as_ref(),
            &fetch_ctx,
        )
        .await?;
        for (node_id, secret) in credentials.secrets() {
            external_secrets.insert(format!("flow-auth.{node_id}"), secret.to_string());
        }
```

`credentials` is used here and by Plan 3; there is no unused-variable warning.

- [ ] **Step 5: Run the tests**

Run: `cargo test -p rocket-app -j4 auth_node && cargo test -p rocket-app -j4 flow_execution_service`
Expected: the three new tests PASS and every existing flow-execution test still PASSES. (The Auth node's own step still fails with "cannot run yet" because Plan 3 replaces that arm; the tests above only assert that the run starts.)

- [ ] **Step 6: Check the whole workspace**

Run: `cargo check -j4`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-app/src/flow_execution_service.rs
git commit -m "feat(flow): resolve Auth credentials at run start and mask their secrets"
```

---

**End of Plan 2.** Verify: `cargo test -p rocket-app -j4 flow_auth flow_execution_service` and `cargo check -j4`.

**Next plan to run: `docs/superpowers/plans/2026-10-02-flow-auth-node-03-executor.md`** (Auth node execution, inherit substitution, `auth` wire target).
