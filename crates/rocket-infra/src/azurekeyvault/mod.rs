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

        let (token, expires_in) = self
            .fetch_token(connection, &settings, client_secret)
            .await?;
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
                .filter(|c| {
                    c.len() <= 64 && c.chars().all(|ch| ch.is_ascii_alphanumeric() || ch == '_')
                })
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
        let parsed: TokenResponse = resp.json().await.map_err(|_| {
            DomainError::Http("failed to decode the Azure AD token response".to_string())
        })?;
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
        Mock::given(method("POST"))
            .and(path("/tenant-1/oauth2/v2.0/token"))
            .and(body_string_contains("grant_type=client_credentials"))
            .and(body_string_contains("client_id=app-id"))
            .and(body_string_contains("client_secret=shh"))
            .and(body_string_contains(
                "scope=https%3A%2F%2Fvault.azure.net%2F.default",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "token_type": "Bearer",
                "access_token": "tok-1",
                "expires_in": 3599
            })))
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
        token_mock("tenant-1", "tok-1")
            .expect(1)
            .mount(&server)
            .await;
        let fetcher = AzureKeyVaultFetcher::new();
        let conn = connection(&server.uri());

        fetcher.ensure_token(&conn, "shh").await.expect("first");
        fetcher.ensure_token(&conn, "shh").await.expect("second");
    }

    #[tokio::test]
    async fn changing_the_secret_forces_a_new_token() {
        let server = MockServer::start().await;
        token_mock("tenant-1", "tok-1")
            .expect(2)
            .mount(&server)
            .await;
        let fetcher = AzureKeyVaultFetcher::new();
        let conn = connection(&server.uri());

        fetcher.ensure_token(&conn, "old").await.expect("first");
        fetcher.ensure_token(&conn, "new").await.expect("second");
    }

    #[tokio::test]
    async fn changing_the_tenant_forces_a_new_token() {
        let server = MockServer::start().await;
        token_mock("tenant-1", "tok-1")
            .expect(1)
            .mount(&server)
            .await;
        token_mock("tenant-2", "tok-2")
            .expect(1)
            .mount(&server)
            .await;
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
        token_mock("tenant-1", "tok-1")
            .expect(2)
            .mount(&server)
            .await;
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
        for ok in [
            "72f988bf-86f1-41af-91ab-2d7cd011db47",
            "contoso.onmicrosoft.com",
        ] {
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
        let url =
            vault_endpoint(&vault, &["secrets", "a b"], &[("maxresults", "5")]).expect("endpoint");
        assert_eq!(
            url.as_str(),
            "https://kv.vault.azure.net/secrets/a%20b?api-version=7.4&maxresults=5"
        );
        for bad in ["", ".", "..", " "] {
            assert!(
                vault_endpoint(&vault, &["secrets", bad], &[]).is_err(),
                "{bad:?}"
            );
        }
        let slash = vault_endpoint(&vault, &["secrets", "a/b"], &[]).expect("encoded");
        assert!(slash.as_str().contains("a%2Fb"), "got: {slash}");
    }
}
