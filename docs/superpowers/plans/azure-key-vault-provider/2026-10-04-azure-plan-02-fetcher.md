# Azure Key Vault Plan 02: Fetcher and Wiring Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A working `AzureKeyVaultFetcher` registered with the dispatcher, so a saved Azure connection can test, list and resolve secrets.

**Architecture:** A new `azurekeyvault` module in `rocket-infra` implements `VaultSecretFetcher`. It authenticates with the Azure AD client-credentials flow, caches the token per connection, and calls the Key Vault REST API with `reqwest`. It reuses RocketVault's token-cache helpers (widened to `pub(crate)`). The dispatcher registers it, and `src-tauri` builds the dispatcher with `with_providers()`.

**Tech Stack:** Rust, reqwest 0.12, serde_json, dashmap, url 2, wiremock 0.6 (dev).

**Spec:** [../../specs/2026-10-04-azure-key-vault-provider-design.md](../../specs/2026-10-04-azure-key-vault-provider-design.md) (section 5, 9)

**Index:** [00-plan-index.md](00-plan-index.md) holds the locked interface contract and the global constraints. Read both first. Plan 01 must be done.

> 📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md`.

## Global Constraints

- No new dependencies. No Azure SDK.
- Redirects are disabled on the HTTP client. A followed redirect could carry the client secret to another host.
- Error text never includes a token, a secret value, or a response body. A JSON decode failure yields a generic message.
- Public cloud only: authority `https://login.microsoftonline.com`, scope `https://vault.azure.net/.default`, API version `7.4`.
- `cargo` commands take `-j4`. Never call `unwrap` in production code.

## Review Focus

- A `nextLink` pointing at another host must never receive the bearer token.
- A vault with more than one page of secrets lists all of them, and a runaway `nextLink` loop stops at 100 pages.
- A disabled secret is skipped in the list and reads as `None`, not as an error that aborts every send.
- A secret name holding `/`, `?`, `#` or `..` cannot change which endpoint is called.
- Editing the tenant, vault URL, client id or secret must not reuse a token minted under the old values.
- Azure AD error text must not leak the client secret, and a 403 must say what permission is missing.

> Open point to confirm against a real vault: Azure signals a disabled secret as HTTP 403. The tests below accept the code `SecretDisabled` in either the outer `error.code` or `error.innererror.code`, because the exact nesting is not confirmed here.

---

### Task 1: Shared helpers, validation and the token flow

**Files:**
- Modify: `crates/rocket-infra/src/rocketvault/mod.rs` (visibility, `is_loopback_url`)
- Create: `crates/rocket-infra/src/azurekeyvault/mod.rs`
- Modify: `crates/rocket-infra/src/lib.rs:26` and `:57`

**Interfaces:**
- Consumes: `ProviderConfig::Azure` (Plan 01).
- Produces: `AzureKeyVaultFetcher::new()`, private `azure_settings`, `vault_url`, `vault_endpoint`, `ensure_token`, used by Task 2 and 3.

- [ ] **Step 1: Widen the RocketVault helpers**

In `crates/rocket-infra/src/rocketvault/mod.rs` change these four declarations from private to `pub(crate)`:

```rust
pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) fn secret_fingerprint(secret: &str) -> String {
pub(crate) fn token_expiry_cutoff(expires_at: Instant) -> Instant {
pub(crate) fn compute_token_ttl(expires_in: u64) -> Duration {
```

Then add this helper above `validate_base_url`, and replace the `let is_loopback = match parsed.host() { ... };` statement inside `validate_base_url` (and the comment block directly above it) with `let is_loopback = is_loopback_url(&parsed);`:

```rust
/// True for `localhost` and every loopback IP. `url::Url::host()` is matched
/// structurally, because `host_str()` brackets IPv6 hosts ("[::1]") and would
/// never equal a bare "::1".
pub(crate) fn is_loopback_url(url: &url::Url) -> bool {
    match url.host() {
        Some(url::Host::Domain(d)) => d == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    }
}
```

Run: `cargo test -j4 -p rocket-infra rocketvault`
Expected: PASS, behavior is unchanged.

- [ ] **Step 2: Declare the module**

In `crates/rocket-infra/src/lib.rs` add `pub mod azurekeyvault;` next to `pub mod rocketvault;` (line 26) and `pub use azurekeyvault::AzureKeyVaultFetcher;` next to `pub use rocketvault::ReqwestVaultSecretFetcher;` (line 57).

- [ ] **Step 3: Write the failing tests**

Create `crates/rocket-infra/src/azurekeyvault/mod.rs` containing only the test module for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::SecretProviderKind;
    use wiremock::matchers::{body_string_contains, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    pub(super) fn connection(server_uri: &str) -> SecretManagerConnection {
        SecretManagerConnection {
            id: "az-1".to_string(),
            label: "Test".to_string(),
            base_url: server_uri.to_string(),
            client_id: "app-id".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: SecretProviderKind::Azure,
            config: Some(ProviderConfig::Azure {
                tenant_id: "tenant-1".to_string(),
                // The mock server stands in for both Azure AD and the vault.
                authority_host: Some(server_uri.to_string()),
            }),
        }
    }

    fn token_mock(tenant: &str, token: &str) -> Mock {
        Mock::given(method("POST"))
            .and(path(format!("/{tenant}/oauth2/v2.0/token")))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "token_type": "Bearer",
                "access_token": token,
                "expires_in": 3599
            })))
    }

    #[tokio::test]
    async fn token_request_uses_the_client_credentials_shape() {
        let server = MockServer::start().await;
        token_mock("tenant-1", "tok-1")
            .and(body_string_contains("grant_type=client_credentials"))
            .and(body_string_contains("client_id=app-id"))
            .and(body_string_contains("client_secret=shh"))
            .and(body_string_contains(
                "scope=https%3A%2F%2Fvault.azure.net%2F.default",
            ))
            .expect(1)
            .mount(&server)
            .await;
        let fetcher = AzureKeyVaultFetcher::new();

        let token = fetcher
            .ensure_token(&connection(&server.uri()), "shh")
            .await
            .expect("token");

        assert_eq!(token, "tok-1");
    }

    #[tokio::test]
    async fn a_second_call_reuses_the_cached_token() {
        let server = MockServer::start().await;
        token_mock("tenant-1", "tok-1").expect(1).mount(&server).await;
        let fetcher = AzureKeyVaultFetcher::new();
        let conn = connection(&server.uri());

        fetcher.ensure_token(&conn, "shh").await.expect("first");
        fetcher.ensure_token(&conn, "shh").await.expect("second");
    }

    #[tokio::test]
    async fn changing_the_secret_forces_a_new_token() {
        let server = MockServer::start().await;
        token_mock("tenant-1", "tok-1").expect(2).mount(&server).await;
        let fetcher = AzureKeyVaultFetcher::new();
        let conn = connection(&server.uri());

        fetcher.ensure_token(&conn, "old").await.expect("first");
        fetcher.ensure_token(&conn, "new").await.expect("second");
    }

    #[tokio::test]
    async fn changing_the_tenant_forces_a_new_token() {
        let server = MockServer::start().await;
        token_mock("tenant-1", "tok-1").expect(1).mount(&server).await;
        token_mock("tenant-2", "tok-2").expect(1).mount(&server).await;
        let fetcher = AzureKeyVaultFetcher::new();
        let mut conn = connection(&server.uri());

        fetcher.ensure_token(&conn, "shh").await.expect("first");
        conn.config = Some(ProviderConfig::Azure {
            tenant_id: "tenant-2".to_string(),
            authority_host: Some(server.uri()),
        });
        let token = fetcher.ensure_token(&conn, "shh").await.expect("second");

        assert_eq!(token, "tok-2");
    }

    #[tokio::test]
    async fn changing_the_vault_url_forces_a_new_token() {
        let server = MockServer::start().await;
        token_mock("tenant-1", "tok-1").expect(2).mount(&server).await;
        let fetcher = AzureKeyVaultFetcher::new();
        let mut conn = connection(&server.uri());

        fetcher.ensure_token(&conn, "shh").await.expect("first");
        conn.base_url = format!("{}/", server.uri().replace("127.0.0.1", "localhost"));
        fetcher.ensure_token(&conn, "shh").await.expect("second");
    }

    #[tokio::test]
    async fn an_azure_ad_rejection_names_the_error_but_never_the_secret() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/tenant-1/oauth2/v2.0/token"))
            .respond_with(ResponseTemplate::new(401).set_body_json(serde_json::json!({
                "error": "invalid_client",
                "error_description": "AADSTS7000215: Invalid client secret 'super-secret-value'."
            })))
            .mount(&server)
            .await;
        let fetcher = AzureKeyVaultFetcher::new();
        let conn = connection(&server.uri());

        let err = fetcher
            .ensure_token(&conn, "super-secret-value")
            .await
            .expect_err("must fail");

        let msg = err.to_string();
        assert!(msg.contains("invalid_client"), "got: {msg}");
        assert!(!msg.contains("super-secret-value"), "leaked: {msg}");
        assert!(fetcher.tokens.get(&conn.id).is_none());
    }

    #[tokio::test]
    async fn an_empty_access_token_is_rejected() {
        let server = MockServer::start().await;
        token_mock("tenant-1", "").mount(&server).await;

        let err = AzureKeyVaultFetcher::new()
            .ensure_token(&connection(&server.uri()), "shh")
            .await
            .expect_err("empty token");

        assert!(err.to_string().contains("empty"), "got: {err}");
    }

    #[test]
    fn tenant_must_be_a_guid_or_a_domain() {
        for ok in ["72f988bf-86f1-41af-91ab-2d7cd011db47", "contoso.onmicrosoft.com"] {
            assert!(validate_tenant(ok).is_ok(), "{ok}");
        }
        for bad in ["", "a/b", "a b", "..", ".x", "x..y", "t?x=1", "t#f"] {
            assert!(validate_tenant(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn vault_url_must_be_https_or_loopback_and_only_the_address() {
        let mut conn = connection("http://127.0.0.1:1");
        assert!(vault_url(&conn).is_ok(), "loopback http is allowed");
        conn.base_url = "https://kv.vault.azure.net".to_string();
        assert!(vault_url(&conn).is_ok());
        conn.base_url = "https://kv.vault.azure.net/".to_string();
        assert!(vault_url(&conn).is_ok(), "a trailing slash is fine");
        for bad in [
            "http://kv.vault.azure.net",
            "https://kv.vault.azure.net/secrets",
            "https://kv.vault.azure.net/?a=b",
            "https://user:pw@kv.vault.azure.net",
            "not a url",
            "",
        ] {
            conn.base_url = bad.to_string();
            assert!(vault_url(&conn).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_connection_without_azure_config_is_rejected() {
        let mut conn = connection("http://127.0.0.1:1");
        conn.config = None;
        assert!(azure_settings(&conn).is_err());
    }

    #[test]
    fn the_authority_host_must_be_https_or_loopback() {
        let mut conn = connection("http://127.0.0.1:1");
        conn.config = Some(ProviderConfig::Azure {
            tenant_id: "t".to_string(),
            authority_host: Some("http://login.example.com".to_string()),
        });
        assert!(azure_settings(&conn).is_err());
    }

    #[test]
    fn endpoints_encode_segments_and_carry_the_api_version() {
        let vault = url::Url::parse("https://kv.vault.azure.net").expect("url");
        let url = vault_endpoint(&vault, &["secrets", "a b"], &[("maxresults", "5")])
            .expect("endpoint");
        assert_eq!(
            url.as_str(),
            "https://kv.vault.azure.net/secrets/a%20b?api-version=7.4&maxresults=5"
        );
        for bad in ["", ".", "..", " "] {
            assert!(vault_endpoint(&vault, &["secrets", bad], &[]).is_err(), "{bad:?}");
        }
        let slash = vault_endpoint(&vault, &["secrets", "a/b"], &[]).expect("encoded");
        assert!(slash.as_str().contains("a%2Fb"), "got: {slash}");
    }
}
```

- [ ] **Step 4: Run to verify they fail**

Run: `cargo test -j4 -p rocket-infra azurekeyvault`
Expected: FAIL to compile, `AzureKeyVaultFetcher`, `validate_tenant` and the other items are not defined.

- [ ] **Step 5: Write the implementation**

Put this at the top of `crates/rocket-infra/src/azurekeyvault/mod.rs`, above the test module:

```rust
//! Azure Key Vault provider. Authenticates as an Azure AD service principal
//! (client credentials) and reads secrets through the Key Vault REST API.

use std::time::Instant;

use dashmap::DashMap;
use rocket_environment::{ProviderConfig, SecretManagerConnection};
use rocket_shared::error::{DomainError, DomainResult};
use serde::Deserialize;

use crate::rocketvault::{
    compute_token_ttl, is_loopback_url, secret_fingerprint, token_expiry_cutoff, REQUEST_TIMEOUT,
};

const DEFAULT_AUTHORITY: &str = "https://login.microsoftonline.com";
const VAULT_SCOPE: &str = "https://vault.azure.net/.default";
const API_VERSION: &str = "7.4";

/// One cached access token. Every input that went into minting it is kept
/// next to it, so editing a connection never reuses a token minted under the
/// old values. The raw client secret is never stored, only a fingerprint.
struct TokenCache {
    token: String,
    tenant_id: String,
    authority: String,
    vault_url: String,
    client_id: String,
    secret_fingerprint: String,
    expires_at: Instant,
}

/// Talks to Azure AD and Azure Key Vault for every Azure connection. One
/// instance serves all connections, the same way the RocketVault fetcher does.
pub struct AzureKeyVaultFetcher {
    http: reqwest::Client,
    tokens: DashMap<String, TokenCache>,
}

impl AzureKeyVaultFetcher {
    pub fn new() -> Self {
        // Redirects are off: a followed redirect could carry the client secret
        // form body, or the bearer token, to an unintended host.
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            http,
            tokens: DashMap::new(),
        }
    }

    /// Returns a valid cached token for `connection`, minting one when none is
    /// cached, an input changed, or the cached one is about to expire.
    async fn ensure_token(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
    ) -> DomainResult<String> {
        let settings = azure_settings(connection)?;
        let vault = vault_url(connection)?;
        let fingerprint = secret_fingerprint(client_secret);

        if let Some(cached) = self.tokens.get(&connection.id) {
            let still_matches = cached.tenant_id == settings.tenant_id
                && cached.authority == settings.authority.as_str()
                && cached.vault_url == vault.as_str()
                && cached.client_id == connection.client_id
                && cached.secret_fingerprint == fingerprint;
            if still_matches && Instant::now() < token_expiry_cutoff(cached.expires_at) {
                return Ok(cached.token.clone());
            }
        }

        let (token, expires_in) = self.fetch_token(connection, &settings, client_secret).await?;
        self.tokens.insert(
            connection.id.clone(),
            TokenCache {
                token: token.clone(),
                tenant_id: settings.tenant_id.to_string(),
                authority: settings.authority.to_string(),
                vault_url: vault.to_string(),
                client_id: connection.client_id.clone(),
                secret_fingerprint: fingerprint,
                expires_at: Instant::now() + compute_token_ttl(expires_in),
            },
        );
        Ok(token)
    }

    /// Posts the client-credentials grant. Never touches the cache, so a
    /// failure leaves it as it was.
    async fn fetch_token(
        &self,
        connection: &SecretManagerConnection,
        settings: &AzureSettings<'_>,
        client_secret: &str,
    ) -> DomainResult<(String, u64)> {
        let mut url = settings.authority.clone();
        url.path_segments_mut()
            .map_err(|()| {
                DomainError::InvalidInput("the Azure authority host cannot hold a path".to_string())
            })?
            .pop_if_empty()
            .extend([settings.tenant_id, "oauth2", "v2.0", "token"]);
        let form = [
            ("grant_type", "client_credentials"),
            ("client_id", connection.client_id.as_str()),
            ("client_secret", client_secret),
            ("scope", VAULT_SCOPE),
        ];

        let resp = self
            .http
            .post(url)
            .form(&form)
            .send()
            .await
            .map_err(|e| DomainError::Http(format!("Azure AD token request failed: {e}")))?;

        let status = resp.status();
        if status == reqwest::StatusCode::BAD_REQUEST || status == reqwest::StatusCode::UNAUTHORIZED
        {
            #[derive(Deserialize)]
            struct AadError {
                error: Option<String>,
            }
            // Only a short identifier such as `invalid_client` is shown. The
            // description text can quote request values, so it never is.
            let code = resp
                .json::<AadError>()
                .await
                .ok()
                .and_then(|b| b.error)
                .filter(|c| c.len() <= 64 && c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_'))
                .unwrap_or_else(|| "unknown".to_string());
            return Err(DomainError::Http(format!(
                "Azure AD rejected the connection's credentials ({status}, {code})"
            )));
        }
        if !status.is_success() {
            return Err(DomainError::Http(format!(
                "Azure AD token endpoint returned unexpected status {status}"
            )));
        }

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            #[serde(default)]
            expires_in: u64,
        }
        // The decode error is dropped on purpose, because it can quote the token.
        let parsed: TokenResponse = resp
            .json()
            .await
            .map_err(|_| DomainError::Http("failed to decode the Azure AD token response".to_string()))?;
        if parsed.access_token.is_empty() {
            return Err(DomainError::Http(
                "Azure AD returned an empty access_token".to_string(),
            ));
        }
        Ok((parsed.access_token, parsed.expires_in))
    }
}

impl Default for AzureKeyVaultFetcher {
    fn default() -> Self {
        Self::new()
    }
}

/// The tenant and token authority of an Azure connection.
struct AzureSettings<'a> {
    tenant_id: &'a str,
    authority: url::Url,
}

fn azure_settings(connection: &SecretManagerConnection) -> DomainResult<AzureSettings<'_>> {
    let Some(ProviderConfig::Azure {
        tenant_id,
        authority_host,
    }) = &connection.config
    else {
        return Err(DomainError::InvalidInput(format!(
            "Azure Key Vault connection {} has no tenant configured",
            connection.id
        )));
    };
    validate_tenant(tenant_id)?;
    let authority = parse_secure_url(
        authority_host.as_deref().unwrap_or(DEFAULT_AUTHORITY),
        "authority host",
    )?;
    Ok(AzureSettings {
        tenant_id,
        authority,
    })
}

/// A tenant is a GUID or a domain name. Only those characters are allowed, so
/// the value can never change the token URL's path or query.
fn validate_tenant(tenant: &str) -> DomainResult<()> {
    let ok = !tenant.is_empty()
        && tenant.len() <= 253
        && !tenant.starts_with('.')
        && !tenant.contains("..")
        && tenant
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.');
    if ok {
        Ok(())
    } else {
        Err(DomainError::InvalidInput(
            "the Azure tenant must be a tenant id or a domain name".to_string(),
        ))
    }
}

/// Parses a URL that must be https, with plain http allowed only for loopback
/// hosts (local tests). Embedded credentials are refused, so the value is safe
/// to show in an error.
fn parse_secure_url(raw: &str, what: &str) -> DomainResult<url::Url> {
    let parsed = url::Url::parse(raw.trim())
        .map_err(|_| DomainError::InvalidInput(format!("the Azure {what} is not a valid URL")))?;
    if parsed.host_str().is_none() {
        return Err(DomainError::InvalidInput(format!(
            "the Azure {what} has no host"
        )));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err(DomainError::InvalidInput(format!(
            "the Azure {what} must not contain credentials"
        )));
    }
    let https = parsed.scheme() == "https";
    let loopback_http = parsed.scheme() == "http" && is_loopback_url(&parsed);
    if !https && !loopback_http {
        return Err(DomainError::InvalidInput(format!(
            "the Azure {what} must use https:// (http is allowed only for loopback hosts)"
        )));
    }
    Ok(parsed)
}

/// The vault address from `connection.base_url`: scheme, host and port only.
fn vault_url(connection: &SecretManagerConnection) -> DomainResult<url::Url> {
    let url = parse_secure_url(&connection.base_url, "vault URL")?;
    if url.query().is_some() || url.fragment().is_some() || !matches!(url.path(), "" | "/") {
        return Err(DomainError::InvalidInput(
            "the Azure vault URL must be just the vault address, like https://name.vault.azure.net"
                .to_string(),
        ));
    }
    Ok(url)
}

/// Builds a Key Vault data-plane URL. Each segment is percent-encoded, so a
/// secret name holding `/`, `?` or `#` cannot change which endpoint is called.
/// Empty, `.` and `..` segments are rejected for the same reason.
fn vault_endpoint(
    vault: &url::Url,
    segments: &[&str],
    extra_query: &[(&str, &str)],
) -> DomainResult<url::Url> {
    if let Some(bad) = segments
        .iter()
        .find(|s| s.trim().is_empty() || **s == "." || **s == "..")
    {
        return Err(DomainError::InvalidInput(format!(
            "invalid Azure Key Vault path segment '{bad}'"
        )));
    }
    let mut url = vault.clone();
    url.path_segments_mut()
        .map_err(|()| {
            DomainError::InvalidInput("the Azure vault URL cannot hold a path".to_string())
        })?
        .pop_if_empty()
        .extend(segments);
    {
        let mut query = url.query_pairs_mut();
        query.append_pair("api-version", API_VERSION);
        for (key, value) in extra_query {
            query.append_pair(key, value);
        }
    }
    Ok(url)
}
```

- [ ] **Step 6: Run to verify they pass**

Run: `cargo test -j4 -p rocket-infra azurekeyvault`
Expected: PASS. A `dead_code` warning for `ensure_token` and `vault_endpoint` in the non-test build is expected until Task 2 uses them. Do not silence it.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-infra/src/azurekeyvault/mod.rs crates/rocket-infra/src/rocketvault/mod.rs crates/rocket-infra/src/lib.rs
```
Invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(secrets): add the Azure token flow and URL validation`.

---

### Task 2: Listing and reading secrets

**Files:**
- Modify: `crates/rocket-infra/src/azurekeyvault/mod.rs` (add types, helpers and the trait impl above the test module, tests inside it)

**Interfaces:**
- Consumes: `ensure_token`, `vault_url`, `vault_endpoint` from Task 1.
- Produces: `impl VaultSecretFetcher for AzureKeyVaultFetcher` with `list_secrets`, `get_secret_value`, `test_connection`.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module (the `connection` and `token_mock` helpers already exist). Extend the matcher import to `use wiremock::matchers::{body_string_contains, header, method, path, query_param};` and add `use rocket_environment::VaultSecretFetcher;`:

```rust
    fn secret_item(server: &str, name: &str, enabled: bool) -> serde_json::Value {
        serde_json::json!({
            "id": format!("{server}/secrets/{name}"),
            "attributes": { "enabled": enabled, "created": 1, "updated": 2 },
            "contentType": "text/plain"
        })
    }

    async fn server_with_token() -> MockServer {
        let server = MockServer::start().await;
        token_mock("tenant-1", "tok-1").mount(&server).await;
        server
    }

    #[tokio::test]
    async fn list_maps_names_and_skips_disabled_secrets() {
        let server = server_with_token().await;
        let uri = server.uri();
        Mock::given(method("GET"))
            .and(path("/secrets"))
            .and(query_param("api-version", "7.4"))
            .and(header("authorization", "Bearer tok-1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": [
                    secret_item(&uri, "stripe-key", true),
                    secret_item(&uri, "old-key", false),
                    secret_item(&uri, "db-password", true)
                ]
            })))
            .mount(&server)
            .await;

        let refs = AzureKeyVaultFetcher::new()
            .list_secrets(&connection(&uri), "shh", "ignored")
            .await
            .expect("list");

        let names: Vec<_> = refs.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["stripe-key", "db-password"]);
        assert!(refs.iter().all(|r| r.secret_id == r.name));
    }

    #[tokio::test]
    async fn list_follows_next_link_on_the_vault_host() {
        let server = server_with_token().await;
        let uri = server.uri();
        Mock::given(method("GET"))
            .and(path("/secrets"))
            .and(query_param("maxresults", "25"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": [secret_item(&uri, "one", true)],
                "nextLink": format!("{uri}/secrets?api-version=7.4&$skiptoken=abc")
            })))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/secrets"))
            .and(query_param("$skiptoken", "abc"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": [secret_item(&uri, "two", true)],
                "nextLink": null
            })))
            .mount(&server)
            .await;

        let refs = AzureKeyVaultFetcher::new()
            .list_secrets(&connection(&uri), "shh", "")
            .await
            .expect("list");

        let names: Vec<_> = refs.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["one", "two"]);
    }

    #[tokio::test]
    async fn list_never_follows_a_next_link_to_another_host() {
        let server = server_with_token().await;
        let foreign = MockServer::start().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&foreign)
            .await;
        Mock::given(method("GET"))
            .and(path("/secrets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": [],
                "nextLink": format!("{}/secrets?api-version=7.4", foreign.uri())
            })))
            .mount(&server)
            .await;

        let err = AzureKeyVaultFetcher::new()
            .list_secrets(&connection(&server.uri()), "shh", "")
            .await
            .expect_err("a foreign link must fail");

        assert!(err.to_string().contains("different host"), "got: {err}");
    }

    #[tokio::test]
    async fn list_stops_a_runaway_next_link_loop() {
        let server = server_with_token().await;
        let uri = server.uri();
        Mock::given(method("GET"))
            .and(path("/secrets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": [],
                "nextLink": format!("{uri}/secrets?api-version=7.4&$skiptoken=again")
            })))
            .expect(100)
            .mount(&server)
            .await;

        let err = AzureKeyVaultFetcher::new()
            .list_secrets(&connection(&uri), "shh", "")
            .await
            .expect_err("page cap");

        assert!(err.to_string().contains("more than 100 pages"), "got: {err}");
    }

    #[tokio::test]
    async fn list_401_clears_the_cached_token() {
        let server = server_with_token().await;
        Mock::given(method("GET"))
            .and(path("/secrets"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&server)
            .await;
        let fetcher = AzureKeyVaultFetcher::new();
        let conn = connection(&server.uri());

        let err = fetcher.list_secrets(&conn, "shh", "").await.expect_err("401");

        assert!(err.to_string().contains("401"), "got: {err}");
        assert!(fetcher.tokens.get(&conn.id).is_none());
    }

    #[tokio::test]
    async fn list_403_names_the_missing_permission() {
        let server = server_with_token().await;
        Mock::given(method("GET"))
            .and(path("/secrets"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&server)
            .await;

        let err = AzureKeyVaultFetcher::new()
            .list_secrets(&connection(&server.uri()), "shh", "")
            .await
            .expect_err("403");

        assert!(err.to_string().contains("Key Vault Secrets User"), "got: {err}");
    }

    #[tokio::test]
    async fn throttling_reports_retry_after_digits_only() {
        let server = server_with_token().await;
        Mock::given(method("GET"))
            .and(path("/secrets"))
            .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "7"))
            .mount(&server)
            .await;

        let err = AzureKeyVaultFetcher::new()
            .list_secrets(&connection(&server.uri()), "shh", "")
            .await
            .expect_err("429");

        assert!(err.to_string().contains("Retry after 7 seconds"), "got: {err}");
    }

    #[tokio::test]
    async fn an_error_body_never_reaches_the_message() {
        let server = server_with_token().await;
        Mock::given(method("GET"))
            .and(path("/secrets"))
            .respond_with(ResponseTemplate::new(500).set_body_string("leak-me-please"))
            .mount(&server)
            .await;

        let err = AzureKeyVaultFetcher::new()
            .list_secrets(&connection(&server.uri()), "shh", "")
            .await
            .expect_err("500");

        assert!(!err.to_string().contains("leak-me-please"), "got: {err}");
    }

    #[tokio::test]
    async fn get_returns_the_latest_value() {
        let server = server_with_token().await;
        Mock::given(method("GET"))
            .and(path("/secrets/stripe-key"))
            .and(query_param("api-version", "7.4"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": "sk_live_123",
                "id": format!("{}/secrets/stripe-key/v1", server.uri())
            })))
            .mount(&server)
            .await;

        let value = AzureKeyVaultFetcher::new()
            .get_secret_value(&connection(&server.uri()), "shh", "ignored", "stripe-key")
            .await
            .expect("get");

        assert_eq!(value, Some("sk_live_123".to_string()));
    }

    #[tokio::test]
    async fn get_treats_a_missing_secret_as_none() {
        let server = server_with_token().await;
        Mock::given(method("GET"))
            .and(path("/secrets/gone"))
            .respond_with(ResponseTemplate::new(404).set_body_json(serde_json::json!({
                "error": { "code": "SecretNotFound", "message": "not found" }
            })))
            .mount(&server)
            .await;

        let value = AzureKeyVaultFetcher::new()
            .get_secret_value(&connection(&server.uri()), "shh", "", "gone")
            .await
            .expect("404 is not an error");

        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn get_treats_a_disabled_secret_as_none_for_either_error_nesting() {
        for body in [
            serde_json::json!({ "error": { "code": "SecretDisabled", "message": "m" } }),
            serde_json::json!({ "error": { "code": "Forbidden", "message": "m",
                "innererror": { "code": "SecretDisabled" } } }),
        ] {
            let server = server_with_token().await;
            Mock::given(method("GET"))
                .and(path("/secrets/off"))
                .respond_with(ResponseTemplate::new(403).set_body_json(body))
                .mount(&server)
                .await;

            let value = AzureKeyVaultFetcher::new()
                .get_secret_value(&connection(&server.uri()), "shh", "", "off")
                .await
                .expect("disabled is not an error");

            assert_eq!(value, None);
        }
    }

    #[tokio::test]
    async fn get_403_for_any_other_reason_is_an_error() {
        let server = server_with_token().await;
        Mock::given(method("GET"))
            .and(path("/secrets/locked"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({
                "error": { "code": "Forbidden", "message": "m" }
            })))
            .mount(&server)
            .await;

        let err = AzureKeyVaultFetcher::new()
            .get_secret_value(&connection(&server.uri()), "shh", "", "locked")
            .await
            .expect_err("403");

        assert!(err.to_string().contains("Key Vault Secrets User"), "got: {err}");
    }

    #[tokio::test]
    async fn get_keeps_an_empty_value() {
        let server = server_with_token().await;
        Mock::given(method("GET"))
            .and(path("/secrets/blank"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "value": "" })))
            .mount(&server)
            .await;

        let value = AzureKeyVaultFetcher::new()
            .get_secret_value(&connection(&server.uri()), "shh", "", "blank")
            .await
            .expect("get");

        assert_eq!(value, Some(String::new()));
    }

    #[tokio::test]
    async fn get_refuses_a_dot_segment_without_calling_the_vault() {
        let server = server_with_token().await;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(200))
            .expect(0)
            .mount(&server)
            .await;

        let err = AzureKeyVaultFetcher::new()
            .get_secret_value(&connection(&server.uri()), "shh", "", "..")
            .await
            .expect_err("dot segment");

        assert!(matches!(err, DomainError::InvalidInput(_)), "got: {err:?}");
    }

    #[tokio::test]
    async fn test_connection_lists_a_single_secret() {
        let server = server_with_token().await;
        Mock::given(method("GET"))
            .and(path("/secrets"))
            .and(query_param("maxresults", "1"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "value": [] })))
            .expect(1)
            .mount(&server)
            .await;

        AzureKeyVaultFetcher::new()
            .test_connection(&connection(&server.uri()), "shh", "")
            .await
            .expect("test connection");
    }

    #[test]
    fn secret_names_come_from_the_last_id_segment() {
        assert_eq!(
            secret_name_from_id("https://kv.vault.azure.net/secrets/stripe-key"),
            Some("stripe-key".to_string())
        );
        assert_eq!(
            secret_name_from_id("https://kv.vault.azure.net/secrets/stripe-key/0123abcd"),
            Some("stripe-key".to_string())
        );
        assert_eq!(secret_name_from_id("https://kv.vault.azure.net/keys/k"), None);
        assert_eq!(secret_name_from_id("not a url"), None);
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-infra azurekeyvault`
Expected: FAIL to compile, `list_secrets`, `secret_name_from_id` and the trait are not implemented.

- [ ] **Step 3: Write the implementation**

Extend the imports at the top of the file:

```rust
use rocket_environment::{
    ExternalSecretRef, ProviderConfig, SecretManagerConnection, VaultSecretFetcher,
};
```
(replacing the existing `use rocket_environment::{ProviderConfig, SecretManagerConnection};`) and add the constants next to the others:

```rust
const PAGE_SIZE: &str = "25";
/// A guard against a vault, or a hostile server, that keeps returning a next link.
const MAX_PAGES: usize = 100;
```

Add this block above the `#[cfg(test)]` module:

```rust
/// One entry of a list response. There is deliberately no `value` field, so a
/// list can never read a secret's value into memory.
#[derive(Deserialize)]
struct RawSecretItem {
    id: String,
    #[serde(default)]
    attributes: RawAttributes,
}

#[derive(Deserialize)]
struct RawAttributes {
    #[serde(default = "enabled_by_default")]
    enabled: bool,
}

fn enabled_by_default() -> bool {
    true
}

impl Default for RawAttributes {
    fn default() -> Self {
        Self { enabled: true }
    }
}

#[derive(Deserialize)]
struct RawSecretPage {
    #[serde(default)]
    value: Vec<RawSecretItem>,
    #[serde(default, rename = "nextLink")]
    next_link: Option<String>,
}

/// Extracts `NAME` from `https://<vault>/secrets/NAME[/<version>]`.
fn secret_name_from_id(id: &str) -> Option<String> {
    let url = url::Url::parse(id).ok()?;
    let segments: Vec<&str> = url.path_segments()?.collect();
    match segments.as_slice() {
        ["secrets", name] | ["secrets", name, _] if !name.is_empty() => Some((*name).to_string()),
        _ => None,
    }
}

/// A next link is followed only when it stays on the vault's own origin.
/// Otherwise the bearer token would go to whatever host the response named.
fn follow_link(vault: &url::Url, link: &str) -> DomainResult<url::Url> {
    let parsed = url::Url::parse(link).map_err(|_| {
        DomainError::Http("Azure Key Vault returned an invalid next page link".to_string())
    })?;
    if parsed.origin() != vault.origin() {
        return Err(DomainError::Http(
            "Azure Key Vault returned a page link to a different host, so it was not followed"
                .to_string(),
        ));
    }
    Ok(parsed)
}

fn forbidden_error(op: &str) -> DomainError {
    DomainError::Http(format!(
        "Azure Key Vault denied access while {op} (403). Give the app the \
         Key Vault Secrets User role, or get and list permissions in the vault access policy."
    ))
}

/// True when a 403 body says the secret is disabled. Azure nests the code, so
/// both the outer and the inner code are checked.
async fn is_disabled_secret_error(resp: reqwest::Response) -> bool {
    #[derive(Deserialize)]
    struct Body {
        error: Option<Detail>,
    }
    #[derive(Deserialize)]
    struct Detail {
        code: Option<String>,
        innererror: Option<Box<Detail>>,
    }
    let Some(detail) = resp.json::<Body>().await.ok().and_then(|b| b.error) else {
        return false;
    };
    let is_disabled = |code: &Option<String>| code.as_deref() == Some("SecretDisabled");
    is_disabled(&detail.code) || detail.innererror.is_some_and(|inner| is_disabled(&inner.code))
}

impl AzureKeyVaultFetcher {
    /// Maps a non-success response to a safe error. A 401 also clears the
    /// cached token. The body is never read, so it can never reach the message.
    fn check_status(
        &self,
        resp: reqwest::Response,
        connection: &SecretManagerConnection,
        op: &str,
    ) -> DomainResult<reqwest::Response> {
        let status = resp.status();
        if status.is_success() {
            return Ok(resp);
        }
        if status == reqwest::StatusCode::UNAUTHORIZED {
            self.tokens.remove(&connection.id);
            return Err(DomainError::Http(format!(
                "Azure Key Vault rejected the token while {op} (401)"
            )));
        }
        if status == reqwest::StatusCode::FORBIDDEN {
            return Err(forbidden_error(op));
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            let wait = resp
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
                .map(|s| format!(" Retry after {s} seconds."))
                .unwrap_or_default();
            return Err(DomainError::Http(format!(
                "Azure Key Vault is throttling requests while {op} (429).{wait}"
            )));
        }
        Err(DomainError::Http(format!(
            "Azure Key Vault returned unexpected status {status} while {op}"
        )))
    }

    async fn get_page(
        &self,
        connection: &SecretManagerConnection,
        token: &str,
        url: url::Url,
    ) -> DomainResult<RawSecretPage> {
        let resp = self
            .http
            .get(url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| DomainError::Http(format!("Azure Key Vault list request failed: {e}")))?;
        let resp = self.check_status(resp, connection, "listing secrets")?;
        // The decoder's text can quote server strings, so it is not shown.
        resp.json::<RawSecretPage>().await.map_err(|_| {
            DomainError::Http(
                "Azure Key Vault returned a secret list that could not be decoded.".to_string(),
            )
        })
    }
}

#[async_trait::async_trait]
impl VaultSecretFetcher for AzureKeyVaultFetcher {
    async fn list_secrets(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<Vec<ExternalSecretRef>> {
        let token = self.ensure_token(connection, client_secret).await?;
        let vault = vault_url(connection)?;
        let mut next = Some(vault_endpoint(
            &vault,
            &["secrets"],
            &[("maxresults", PAGE_SIZE)],
        )?);
        let mut refs = Vec::new();
        let mut pages = 0usize;
        while let Some(url) = next.take() {
            pages += 1;
            if pages > MAX_PAGES {
                return Err(DomainError::Http(format!(
                    "Azure Key Vault returned more than {MAX_PAGES} pages of secrets"
                )));
            }
            let page = self.get_page(connection, &token, url).await?;
            for item in page.value {
                // A disabled secret cannot be read, so it is not offered.
                if !item.attributes.enabled {
                    continue;
                }
                if let Some(name) = secret_name_from_id(&item.id) {
                    refs.push(ExternalSecretRef {
                        secret_id: name.clone(),
                        name,
                    });
                }
            }
            next = match page.next_link.as_deref().filter(|l| !l.is_empty()) {
                Some(link) => Some(follow_link(&vault, link)?),
                None => None,
            };
        }
        Ok(refs)
    }

    async fn get_secret_value(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        _vault_name: &str,
        secret_id: &str,
    ) -> DomainResult<Option<String>> {
        let token = self.ensure_token(connection, client_secret).await?;
        let vault = vault_url(connection)?;
        let url = vault_endpoint(&vault, &["secrets", secret_id], &[])?;
        let resp = self
            .http
            .get(url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| DomainError::Http(format!("Azure Key Vault get request failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if resp.status() == reqwest::StatusCode::FORBIDDEN {
            if is_disabled_secret_error(resp).await {
                return Ok(None);
            }
            return Err(forbidden_error("fetching a secret value"));
        }
        let resp = self.check_status(resp, connection, "fetching a secret value")?;

        #[derive(Deserialize)]
        struct RawSecretValue {
            #[serde(default)]
            value: String,
        }
        // The decode error is dropped on purpose, because it can quote the secret value.
        let parsed: RawSecretValue = resp.json().await.map_err(|_| {
            DomainError::Http("failed to decode the Azure Key Vault secret value".to_string())
        })?;
        Ok(Some(parsed.value))
    }

    async fn test_connection(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<()> {
        let token = self.ensure_token(connection, client_secret).await?;
        let vault = vault_url(connection)?;
        let url = vault_endpoint(&vault, &["secrets"], &[("maxresults", "1")])?;
        self.get_page(connection, &token, url).await?;
        Ok(())
    }
}
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -j4 -p rocket-infra azurekeyvault`
Expected: PASS, and the Task 1 `dead_code` warnings are gone.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/src/azurekeyvault/mod.rs
```
Invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(secrets): list and read Azure Key Vault secrets`.

---

### Task 3: Capabilities, validation, registration and startup wiring

**Files:**
- Modify: `crates/rocket-infra/src/azurekeyvault/mod.rs` (extend the trait impl, add tests)
- Modify: `crates/rocket-infra/src/secret_providers.rs:29-37` and its tests (around line 315)
- Modify: `src-tauri/src/lib.rs:332`

**Interfaces:**
- Consumes: Task 2's trait impl.
- Produces: `DispatchingSecretFetcher::with_providers()`, replacing `with_rocketvault()`.

- [ ] **Step 1: Write the failing fetcher tests**

Add to the `tests` module in `azurekeyvault/mod.rs`:

```rust
    #[test]
    fn azure_offers_no_certificates_and_needs_a_credential() {
        let caps = AzureKeyVaultFetcher::new().capabilities(&connection("http://127.0.0.1:1"));
        assert!(!caps.certificates);
        assert!(!caps.credential_optional);
        assert!(!caps.fetch_on_reference);
    }

    #[test]
    fn validate_connection_accepts_a_complete_connection() {
        let mut conn = connection("https://kv.vault.azure.net");
        conn.config = Some(ProviderConfig::Azure {
            tenant_id: "contoso.onmicrosoft.com".to_string(),
            authority_host: None,
        });
        assert!(AzureKeyVaultFetcher::new().validate_connection(&conn).is_ok());
    }

    #[test]
    fn validate_connection_rejects_each_missing_piece() {
        let fetcher = AzureKeyVaultFetcher::new();
        let good = connection("https://kv.vault.azure.net");

        let mut no_client = good.clone();
        no_client.client_id = "  ".to_string();
        let mut no_config = good.clone();
        no_config.config = None;
        let mut bad_tenant = good.clone();
        bad_tenant.config = Some(ProviderConfig::Azure {
            tenant_id: "a/b".to_string(),
            authority_host: None,
        });
        let mut http_vault = good.clone();
        http_vault.base_url = "http://kv.vault.azure.net".to_string();
        let mut no_url = good;
        no_url.base_url = String::new();

        for bad in [no_client, no_config, bad_tenant, http_vault, no_url] {
            assert!(
                matches!(fetcher.validate_connection(&bad), Err(DomainError::InvalidInput(_))),
                "{bad:?}"
            );
        }
    }

    #[tokio::test]
    async fn forget_connection_drops_the_cached_token() {
        let server = server_with_token().await;
        let fetcher = AzureKeyVaultFetcher::new();
        let conn = connection(&server.uri());
        fetcher.ensure_token(&conn, "shh").await.expect("token");
        assert!(fetcher.tokens.get(&conn.id).is_some());

        fetcher.forget_connection(&conn.id);

        assert!(fetcher.tokens.get(&conn.id).is_none());
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `cargo test -j4 -p rocket-infra azurekeyvault`
Expected: `validate_connection_rejects_each_missing_piece` and `forget_connection_drops_the_cached_token` FAIL, because the trait defaults accept everything and forget nothing. `azure_offers_no_certificates_and_needs_a_credential` already passes, it pins the default.

- [ ] **Step 3: Implement the three methods**

Add inside `impl VaultSecretFetcher for AzureKeyVaultFetcher`, extending the import list with `ProviderCapabilities`:

```rust
    fn forget_connection(&self, connection_id: &str) {
        self.tokens.remove(connection_id);
    }

    fn capabilities(&self, _connection: &SecretManagerConnection) -> ProviderCapabilities {
        // Azure supplies secrets only. Certificates, a credential-free mode and
        // fetch-on-reference are all out of scope for this provider version.
        ProviderCapabilities::default()
    }

    fn validate_connection(&self, connection: &SecretManagerConnection) -> DomainResult<()> {
        if connection.client_id.trim().is_empty() {
            return Err(DomainError::InvalidInput(
                "Azure Key Vault connection client_id must not be empty".to_string(),
            ));
        }
        azure_settings(connection)?;
        vault_url(connection)?;
        Ok(())
    }
```

- [ ] **Step 4: Run to verify they pass**

Run: `cargo test -j4 -p rocket-infra azurekeyvault`
Expected: PASS.

- [ ] **Step 5: Register the provider and rename the constructor**

In `crates/rocket-infra/src/secret_providers.rs` replace `with_rocketvault` with:

```rust
    /// A dispatcher with every provider this build ships registered.
    pub fn with_providers() -> Self {
        let mut dispatcher = Self::new();
        dispatcher.register(
            SecretProviderKind::RocketVault,
            Arc::new(ReqwestVaultSecretFetcher::new()),
        );
        dispatcher.register(
            SecretProviderKind::Azure,
            Arc::new(AzureKeyVaultFetcher::new()),
        );
        dispatcher
    }
```

Add `use crate::azurekeyvault::AzureKeyVaultFetcher;` next to `use crate::rocketvault::ReqwestVaultSecretFetcher;`. In the same file's tests, update the existing test that calls `DispatchingSecretFetcher::with_rocketvault()` (around line 315) to call `with_providers()` and rename it to `with_providers_reports_certificate_support_for_rocketvault_only`. Add:

```rust
    fn azure_connection() -> SecretManagerConnection {
        SecretManagerConnection {
            id: "az-1".to_string(),
            label: "Azure".to_string(),
            base_url: "https://kv.vault.azure.net".to_string(),
            client_id: "app-id".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: SecretProviderKind::Azure,
            config: Some(rocket_environment::ProviderConfig::Azure {
                tenant_id: "tenant-1".to_string(),
                authority_host: None,
            }),
        }
    }

    #[test]
    fn with_providers_registers_azure() {
        let d = DispatchingSecretFetcher::with_providers();
        let conn = azure_connection();

        assert!(d.validate_connection(&conn).is_ok());
        assert!(!d.capabilities(&conn).certificates);
    }

    #[test]
    fn with_providers_still_refuses_providers_that_do_not_exist_yet() {
        let d = DispatchingSecretFetcher::with_providers();
        let mut conn = azure_connection();
        conn.provider = SecretProviderKind::Aws;
        conn.config = None;

        let err = d.validate_connection(&conn).expect_err("AWS is not built");

        assert!(err.to_string().contains("not available in this build"), "got: {err}");
    }
```

In `src-tauri/src/lib.rs:332` change `DispatchingSecretFetcher::with_rocketvault()` to `DispatchingSecretFetcher::with_providers()`.

- [ ] **Step 6: Verify**

Run:
```bash
cargo test -j4 -p rocket-infra secret_providers
cargo test -j4 -p rocket-infra azurekeyvault
cargo check -j4
```
Expected: all PASS, `cargo check` has no errors and no new warnings from `azurekeyvault`.

- [ ] **Step 7: Commit**

```bash
git add crates/rocket-infra/src/azurekeyvault/mod.rs crates/rocket-infra/src/secret_providers.rs src-tauri/src/lib.rs
```
Invoke the `dev-workflow-skills:1-git-commit` skill. Suggested subject: `feat(secrets): register the Azure Key Vault provider`.

---

## Next Plan

Plan 03: [Frontend](2026-10-04-azure-plan-03-frontend.md). Start it automatically once the three tasks above are committed and `cargo check -j4` is green.
