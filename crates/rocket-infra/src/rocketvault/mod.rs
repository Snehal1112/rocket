use std::time::{Duration, Instant};

use dashmap::DashMap;
use rocket_environment::SecretManagerConnection;
use rocket_shared::error::{DomainError, DomainResult};
use serde::Deserialize;

/// Minimum token TTL enforced client-side, even when the server reports
/// `expires_in: 0` — mirrors the reference Go client's floor in
/// `ensureToken` (rocketvault/internal/vaultclient/client.go).
const MIN_TOKEN_TTL: Duration = Duration::from_secs(30);

/// How long before a token's real expiry it is treated as stale, so a
/// request is never sent with a token that is about to expire mid-flight.
/// Mirrors the reference client's `tokenExpiryCutoff`.
const EARLY_REFRESH_CUTOFF: Duration = Duration::from_secs(60);

/// One cached OAuth2 access token for a single `SecretManagerConnection`,
/// keyed by `connection.id` in `ReqwestVaultSecretFetcher::tokens`.
struct TokenCache {
    token: String,
    expires_at: Instant,
}

/// Talks to a RocketVault server's REST API (token issuance, secret listing,
/// secret value retrieval) on behalf of every configured
/// `SecretManagerConnection`. One instance is injected as
/// `Arc<dyn VaultSecretFetcher>` at startup (Plan 05/08) and serves every
/// connection, exactly like `ReqwestExecutor` serves every outgoing HTTP
/// request regardless of target host — the connection and its resolved
/// client secret are passed per call rather than bound at construction.
pub struct ReqwestVaultSecretFetcher {
    /// Verifies TLS certificates normally. Used when `connection.verify_ssl`
    /// is `true` (the default).
    http: reqwest::Client,
    /// Built once with `.danger_accept_invalid_certs(true)`. Used only when
    /// `connection.verify_ssl` is `false`, for self-signed dev RocketVault
    /// servers. `reqwest` requires TLS verification to be decided at
    /// client-construction time rather than per request, so both clients are
    /// built eagerly here and selected per call by `client_for` — neither is
    /// ever rebuilt on a request path.
    http_insecure: reqwest::Client,
    tokens: DashMap<String, TokenCache>,
}

impl ReqwestVaultSecretFetcher {
    pub fn new() -> Self {
        let http_insecure = reqwest::Client::builder()
            .danger_accept_invalid_certs(true)
            .build()
            // Client::builder().build() only fails if the TLS backend itself
            // cannot initialize — the same failure mode reqwest::Client::new()
            // (used for `http` below) panics on internally. Falling back to
            // the verifying client here keeps this constructor's locked
            // `-> Self` signature infallible without ever panicking on our
            // own code path.
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            http: reqwest::Client::new(),
            http_insecure,
            tokens: DashMap::new(),
        }
    }

    /// Selects the client matching `connection.verify_ssl` — see the
    /// `http`/`http_insecure` field docs above for why two clients exist.
    fn client_for(&self, connection: &SecretManagerConnection) -> &reqwest::Client {
        if connection.verify_ssl {
            &self.http
        } else {
            &self.http_insecure
        }
    }

    /// Returns a valid cached token for `connection`, fetching and caching a
    /// fresh one via `fetch_token` when none is cached or the cached one is
    /// within the early-refresh window. Also enforces the `https://` scheme
    /// requirement on `connection.base_url` before issuing any request —
    /// the per-call equivalent of the reference client's constructor-time
    /// rejection, since one fetcher instance serves many connections.
    async fn ensure_token(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
    ) -> DomainResult<String> {
        validate_base_url(connection)?;

        if let Some(cached) = self.tokens.get(&connection.id) {
            if Instant::now() < token_expiry_cutoff(cached.expires_at) {
                return Ok(cached.token.clone());
            }
        }

        let (token, expires_in) = self.fetch_token(connection, client_secret).await?;
        let ttl = Duration::from_secs(expires_in).max(MIN_TOKEN_TTL);
        self.tokens.insert(
            connection.id.clone(),
            TokenCache {
                token: token.clone(),
                expires_at: Instant::now() + ttl,
            },
        );
        Ok(token)
    }

    /// Performs the actual `POST /api/v1/oauth2/token` call and decodes the
    /// response. Does not read or write the token cache — `ensure_token`
    /// owns that; a `401` here is returned as an error and, because this
    /// function never calls `self.tokens.insert`, the cache is left
    /// untouched on failure by construction.
    async fn fetch_token(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
    ) -> DomainResult<(String, u64)> {
        let client = self.client_for(connection);
        let url = format!(
            "{}/api/v1/oauth2/token",
            connection.base_url.trim_end_matches('/')
        );
        let form = [
            ("grant_type", "client_credentials"),
            ("client_id", connection.client_id.as_str()),
            ("client_secret", client_secret),
        ];

        let resp = client
            .post(&url)
            .form(&form)
            .send()
            .await
            .map_err(|e| DomainError::Http(format!("RocketVault token request failed: {e}")))?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            return Err(DomainError::Http(
                "RocketVault rejected the connection's client credentials (401)".to_string(),
            ));
        }
        if !resp.status().is_success() {
            let status = resp.status();
            return Err(DomainError::Http(format!(
                "RocketVault token endpoint returned unexpected status {status}"
            )));
        }

        #[derive(Deserialize)]
        struct TokenResponse {
            access_token: String,
            #[serde(default)]
            expires_in: u64,
        }
        let parsed: TokenResponse = resp.json().await.map_err(|e| {
            DomainError::Http(format!("failed to decode RocketVault token response: {e}"))
        })?;

        if parsed.access_token.is_empty() {
            return Err(DomainError::Http(
                "RocketVault token endpoint returned an empty access_token".to_string(),
            ));
        }

        Ok((parsed.access_token, parsed.expires_in))
    }
}

/// Mirrors the reference Go client's `tokenExpiryCutoff`: refresh 60s before
/// actual expiry, except when the token's remaining life is already at or
/// under that window, in which case the raw expiry is used — otherwise a
/// short-lived token (e.g. a 45s TTL) would be refetched on every call.
fn token_expiry_cutoff(expires_at: Instant) -> Instant {
    let remaining = expires_at.saturating_duration_since(Instant::now());
    if remaining <= EARLY_REFRESH_CUTOFF {
        expires_at
    } else {
        expires_at - EARLY_REFRESH_CUTOFF
    }
}

/// Enforces the `https://` requirement for non-loopback hosts, mirroring the
/// reference Go client's `isLoopbackHost` guard in `New()`. Loopback hosts
/// are always allowed over plain HTTP for local dev RocketVault instances,
/// regardless of `allow_insecure_http`.
fn validate_base_url(connection: &SecretManagerConnection) -> DomainResult<()> {
    let parsed = url::Url::parse(&connection.base_url).map_err(|e| {
        DomainError::InvalidInput(format!(
            "RocketVault connection {} has an invalid base_url: {e}",
            connection.id
        ))
    })?;
    let host = parsed.host_str().unwrap_or_default();
    let is_loopback = host == "localhost" || host == "127.0.0.1" || host == "::1";
    if parsed.scheme() != "https" && !is_loopback && !connection.allow_insecure_http {
        return Err(DomainError::InvalidInput(format!(
            "RocketVault connection {} must use https:// for non-loopback host {host} \
             unless allow_insecure_http is set",
            connection.id
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::SecretManagerConnection;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_connection(base_url: String) -> SecretManagerConnection {
        SecretManagerConnection {
            id: "conn-1".to_string(),
            label: "Test".to_string(),
            base_url,
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: true, // mock server is http://127.0.0.1:<port>
        }
    }

    #[tokio::test]
    async fn ensure_token_success_populates_cache_and_second_call_reuses_it() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok-1",
                "expires_in": 300
            })))
            .expect(1) // exactly one fetch across both ensure_token calls below
            .mount(&mock_server)
            .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let conn = test_connection(mock_server.uri());

        let token = fetcher
            .ensure_token(&conn, "shh")
            .await
            .expect("first token fetch");
        assert_eq!(token, "tok-1");
        assert!(fetcher.tokens.get(&conn.id).is_some());

        // Within the cached TTL window this must not issue a second HTTP
        // request — the mock's .expect(1) above fails the test on drop if
        // wiremock observes a second call.
        let token_again = fetcher
            .ensure_token(&conn, "shh")
            .await
            .expect("second token fetch reuses cache");
        assert_eq!(token_again, "tok-1");
    }

    #[tokio::test]
    async fn ensure_token_401_is_an_error_and_does_not_populate_cache() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth2/token"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let conn = test_connection(mock_server.uri());

        let err = fetcher
            .ensure_token(&conn, "wrong-secret")
            .await
            .expect_err("401 must error");
        assert!(matches!(err, rocket_shared::error::DomainError::Http(_)));
        assert!(fetcher.tokens.get(&conn.id).is_none());
    }
}
