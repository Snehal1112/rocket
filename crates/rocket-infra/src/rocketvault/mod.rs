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

/// Ceiling clamp applied to the server-supplied `expires_in` before it is
/// turned into a `Duration`. `expires_in` is untrusted input deserialized
/// straight from RocketVault's token response with no upper bound — without
/// this clamp, a malicious or buggy server could send a value large enough
/// that `Instant::now() + ttl` overflows and panics. A server claiming a
/// token lives longer than this is itself untrustworthy, so the value is
/// clamped rather than merely bounds-checked.
const MAX_TOKEN_TTL_SECS: u64 = 86_400; // 24 hours

/// Request timeout applied to both `reqwest::Client`s built in `new()`.
/// Matches `rocket_http::RequestOptions`'s own 30s default
/// (`default_timeout()` in `crates/rocket-http/src/request.rs`), so
/// RocketVault calls fail closed with the same ceiling the rest of the app
/// uses for outgoing HTTP requests, rather than hanging indefinitely.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// One cached OAuth2 access token for a single `SecretManagerConnection`,
/// keyed by `connection.id` in `ReqwestVaultSecretFetcher::tokens`.
///
/// `base_url` and `client_id` are stored alongside the token so
/// `ensure_token` can detect a stale entry: if a connection's `base_url` or
/// `client_id` changes while its `id` stays the same (the user edits the
/// connection), a cache hit keyed only on `id` would keep returning a token
/// minted under the old `base_url`/`client_id` until its TTL expires —
/// potentially sending a token to the wrong server, or one issued under a
/// now-rotated `client_id`.
///
/// `secret_fingerprint` exists for the same reason, covering `client_secret`:
/// without it, editing a connection's secret (fixing a typo, rotating a
/// leaked credential) while `base_url`/`client_id` stay the same would keep
/// returning a token minted under the OLD secret until the cache entry's TTL
/// expires (up to `MAX_TOKEN_TTL_SECS`) — a false-positive "Test Connection"
/// success, and a silent failure only much later when the stale token itself
/// finally expires. The raw `client_secret` is deliberately never stored
/// here — only a one-way SHA-256 fingerprint of it — so it never lives in
/// memory longer than the single `fetch_token` call that mints the token.
struct TokenCache {
    token: String,
    base_url: String,
    client_id: String,
    secret_fingerprint: String,
    expires_at: Instant,
}

/// Computes a one-way fingerprint of a `client_secret` for staleness
/// detection in `TokenCache`. The raw secret must never be stored in the
/// cache — it would then persist in memory for as long as the cached token
/// does (up to `MAX_TOKEN_TTL_SECS`), well past the single `fetch_token` call
/// that actually needs it. A fingerprint lets `ensure_token` notice the
/// secret passed into a later call differs from the one that minted the
/// cached token, without ever holding onto the secret itself.
fn secret_fingerprint(secret: &str) -> String {
    use sha2::{Digest, Sha256};
    use std::fmt::Write;
    Sha256::digest(secret.as_bytes())
        .iter()
        .fold(String::with_capacity(64), |mut acc, b| {
            let _ = write!(acc, "{b:02x}");
            acc
        })
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
        // Client::builder().build() only fails if the TLS backend itself
        // cannot initialize — the same failure mode the panicking
        // reqwest::Client::new() hits internally. Both clients below are
        // built via the non-panicking builder path and fall back to
        // reqwest::Client::new() only as a last resort, keeping this
        // constructor's locked `-> Self` signature infallible without ever
        // panicking on our own code path.
        //
        // Both clients disable redirects and set a request timeout, matching
        // the style of this crate's own `reqwest_executor::build_client_impl`.
        // RocketVault's API never redirects, so following one is never
        // correct here — and a followed redirect could carry the request's
        // `client_secret` form body to an unintended host.
        let http = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        let http_insecure = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(REQUEST_TIMEOUT)
            .danger_accept_invalid_certs(true)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            http,
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
            let still_matches_connection = cached.base_url == connection.base_url
                && cached.client_id == connection.client_id
                && cached.secret_fingerprint == secret_fingerprint(client_secret);
            if still_matches_connection && Instant::now() < token_expiry_cutoff(cached.expires_at) {
                return Ok(cached.token.clone());
            }
        }

        let (token, expires_in) = self.fetch_token(connection, client_secret).await?;
        let ttl = compute_token_ttl(expires_in);
        self.tokens.insert(
            connection.id.clone(),
            TokenCache {
                token: token.clone(),
                base_url: connection.base_url.clone(),
                client_id: connection.client_id.clone(),
                secret_fingerprint: secret_fingerprint(client_secret),
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

impl Default for ReqwestVaultSecretFetcher {
    fn default() -> Self {
        Self::new()
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

/// Turns a server-reported `expires_in` (seconds) into the `Duration` cached
/// alongside the token. Clamps to `MAX_TOKEN_TTL_SECS` first — before the
/// `Duration` is ever constructed — so `Instant::now() + ttl` in
/// `ensure_token` can never overflow, then floors to `MIN_TOKEN_TTL` so a
/// server reporting `expires_in: 0` (or another degenerate small value)
/// doesn't force a refetch on every call.
fn compute_token_ttl(expires_in: u64) -> Duration {
    Duration::from_secs(expires_in.min(MAX_TOKEN_TTL_SECS)).max(MIN_TOKEN_TTL)
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
    // `url::Url::host()` (unlike `host_str()`, which brackets IPv6 hosts as
    // "[::1]" and would never match a bare "::1" comparison) returns a
    // `url::Host` we can match structurally, so `Host::Ipv6(ip).is_loopback()`
    // correctly recognizes "::1" and every other loopback form (e.g. also
    // `Host::Ipv4(ip).is_loopback()` covers 127.0.0.2, not just 127.0.0.1).
    let is_loopback = match parsed.host() {
        Some(url::Host::Domain(d)) => d == "localhost",
        Some(url::Host::Ipv4(ip)) => ip.is_loopback(),
        Some(url::Host::Ipv6(ip)) => ip.is_loopback(),
        None => false,
    };
    if parsed.scheme() != "https" && !is_loopback && !connection.allow_insecure_http {
        return Err(DomainError::InvalidInput(format!(
            "RocketVault connection {} must use https:// for non-loopback host {host} \
             unless allow_insecure_http is set",
            connection.id
        )));
    }
    Ok(())
}

use rocket_environment::{ExternalSecretRef, VaultSecretFetcher};

/// Deserialization target for one entry inside a RocketVault list-secrets
/// response's `secrets` array. Deliberately has no `value` field —
/// RocketVault's per-entry response includes one, but this struct must
/// never be able to read it, so a list response can never leak a value into
/// memory even though the server includes one (spec §4.1/§4.4). Every other
/// field (`tags`, `created_at`, `enabled`, `version`, ...) is silently
/// ignored by serde's default "unknown fields are skipped" deserialization
/// behavior — no `deny_unknown_fields`.
#[derive(Deserialize)]
struct RawSecretSummary {
    id: String,
    name: String,
}

/// Deserialization target for RocketVault's list-secrets envelope,
/// `{"secrets": [...], "total": N}` — confirmed from
/// `model.ListSecretsResponse` (ground truth cited in Global Constraints).
/// `total` is intentionally not declared as a field: it isn't needed here,
/// and omitting it is safe because serde ignores unknown JSON fields by
/// default (the same behavior `RawSecretSummary` relies on above).
#[derive(Deserialize)]
struct RawSecretListResponse {
    secrets: Vec<RawSecretSummary>,
}

#[async_trait::async_trait]
impl VaultSecretFetcher for ReqwestVaultSecretFetcher {
    async fn list_secrets(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<Vec<ExternalSecretRef>> {
        let token = self.ensure_token(connection, client_secret).await?;
        let client = self.client_for(connection);
        // RocketVault paginates this endpoint (default per_page=60, max 200 —
        // spec §4.1). Requesting the max page size is a pragmatic mitigation
        // for v1: it covers any vault with up to 200 secrets in one call
        // without implementing cursor-based pagination. A vault with more
        // than 200 secrets will still silently show only the first 200 in
        // "Fetch Secrets" — a documented v1 limitation, not a bug to fix
        // here.
        let url = format!(
            "{}/api/v1/vaults/{}/secrets?per_page=200",
            connection.base_url.trim_end_matches('/'),
            vault_name
        );

        let resp = client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| {
                DomainError::Http(format!("RocketVault list_secrets request failed: {e}"))
            })?;

        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.tokens.remove(&connection.id);
            return Err(DomainError::Http(
                "RocketVault rejected the token while listing secrets (401)".to_string(),
            ));
        }
        if !resp.status().is_success() {
            let status = resp.status();
            return Err(DomainError::Http(format!(
                "RocketVault list_secrets returned unexpected status {status}"
            )));
        }

        let raw: RawSecretListResponse = resp.json().await.map_err(|e| {
            DomainError::Http(format!("failed to decode RocketVault secret list: {e}"))
        })?;

        Ok(raw
            .secrets
            .into_iter()
            .map(|s| ExternalSecretRef {
                name: s.name,
                secret_id: s.id,
            })
            .collect())
    }

    async fn get_secret_value(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
        secret_id: &str,
    ) -> DomainResult<Option<String>> {
        let token = self.ensure_token(connection, client_secret).await?;
        let client = self.client_for(connection);
        let url = format!(
            "{}/api/v1/vaults/{}/secrets/{}",
            connection.base_url.trim_end_matches('/'),
            vault_name,
            secret_id
        );

        let resp = client
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .map_err(|e| {
                DomainError::Http(format!("RocketVault get_secret_value request failed: {e}"))
            })?;

        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
            self.tokens.remove(&connection.id);
            return Err(DomainError::Http(
                "RocketVault rejected the token while fetching a secret value (401)".to_string(),
            ));
        }
        if !resp.status().is_success() {
            let status = resp.status();
            return Err(DomainError::Http(format!(
                "RocketVault get_secret_value returned unexpected status {status}"
            )));
        }

        #[derive(Deserialize)]
        struct RawSecretValue {
            // RocketVault's server omits the `value` key entirely (via an
            // `omitempty` tag) when a secret's stored value is the empty
            // string, rather than sending `"value": ""`. Without
            // `#[serde(default)]` that legitimately-empty case fails to
            // decode as an error instead of yielding `Ok(Some(String::new()))`.
            #[serde(default)]
            value: String,
        }
        let parsed: RawSecretValue = resp.json().await.map_err(|e| {
            DomainError::Http(format!("failed to decode RocketVault secret value: {e}"))
        })?;
        Ok(Some(parsed.value))
    }

    async fn test_connection(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<()> {
        self.ensure_token(connection, client_secret).await?;
        self.list_secrets(connection, client_secret, vault_name)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::SecretManagerConnection;
    use wiremock::matchers::{body_string_contains, header_exists, method, path, query_param};
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
    async fn ensure_token_refetches_when_client_secret_changes_even_within_ttl() {
        // Inverse of the test above: the SAME connection id, base_url, and
        // client_id, but a DIFFERENT client_secret on the second call, must
        // NOT reuse the cached token — it must re-authenticate and pick up
        // the token minted for the new secret. This is the regression test
        // for the bug where `still_matches_connection` compared only
        // `base_url`/`client_id` and never noticed `client_secret` changed,
        // so a cache hit returned success even though the fresh secret was
        // never actually validated against the server.
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth2/token"))
            .and(body_string_contains("client_secret=secret-a"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok-for-secret-a",
                "expires_in": 300
            })))
            .expect(1)
            .mount(&mock_server)
            .await;
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth2/token"))
            .and(body_string_contains("client_secret=secret-b"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok-for-secret-b",
                "expires_in": 300
            })))
            .expect(1)
            .mount(&mock_server)
            .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let conn = test_connection(mock_server.uri());

        let token_a = fetcher
            .ensure_token(&conn, "secret-a")
            .await
            .expect("first token fetch with secret-a");
        assert_eq!(token_a, "tok-for-secret-a");

        // Still within the TTL window (300s), but the secret changed — this
        // must issue a fresh HTTP call rather than reuse the cached token.
        // Each mock's .expect(1) above fails the test on drop unless exactly
        // one request matching its own client_secret is observed.
        let token_b = fetcher
            .ensure_token(&conn, "secret-b")
            .await
            .expect("second token fetch with secret-b must re-authenticate");
        assert_eq!(token_b, "tok-for-secret-b");
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

    #[tokio::test]
    async fn list_secrets_maps_id_and_name_ignoring_other_fields() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok-1",
                "expires_in": 300
            })))
            .mount(&mock_server)
            .await;
        // RocketVault's real response is the model.ListSecretsResponse envelope
        // ({"secrets": [...], "total": N}), not a bare array — see the Global
        // Constraints "List" bullet for the ground-truth source. The
        // query_param assertion below is the pagination-mitigation check: it
        // fails the test if the implementation ever drops the ?per_page=200
        // query string, which would otherwise silently truncate any vault with
        // more than RocketVault's default 60-per-page limit.
        Mock::given(method("GET"))
            .and(path("/api/v1/vaults/prod-vault/secrets"))
            .and(query_param("per_page", "200"))
            .and(header_exists("Authorization"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "secrets": [
                    {
                        "id": "b6f1c2e0-1234-4a5b-9abc-000000000001",
                        "name": "stripe-key",
                        "value": "sk-live-should-never-be-read",
                        "tags": ["payments"],
                        "created_at": "2026-01-01T00:00:00Z"
                    },
                    {
                        "id": "c7a2d3f1-5678-4b6c-9def-000000000002",
                        "name": "sendgrid-key",
                        "value": "sg-should-never-be-read",
                        "tags": []
                    }
                ],
                "total": 2
            })))
            .mount(&mock_server)
            .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let conn = test_connection(mock_server.uri());

        let refs = fetcher
            .list_secrets(&conn, "shh", "prod-vault")
            .await
            .expect("list_secrets");
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].name, "stripe-key");
        assert_eq!(refs[0].secret_id, "b6f1c2e0-1234-4a5b-9abc-000000000001");
        assert_eq!(refs[1].name, "sendgrid-key");
        assert_eq!(refs[1].secret_id, "c7a2d3f1-5678-4b6c-9def-000000000002");
    }

    #[tokio::test]
    async fn list_secrets_401_is_an_error_and_clears_cached_token() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok-1",
                "expires_in": 300
            })))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/vaults/prod-vault/secrets"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let conn = test_connection(mock_server.uri());

        let err = fetcher
            .list_secrets(&conn, "shh", "prod-vault")
            .await
            .expect_err("401 must error");
        assert!(matches!(err, rocket_shared::error::DomainError::Http(_)));
        assert!(fetcher.tokens.get(&conn.id).is_none());
    }

    #[tokio::test]
    async fn get_secret_value_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok-1",
                "expires_in": 300
            })))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/api/v1/vaults/prod-vault/secrets/b6f1c2e0-1234-4a5b-9abc-000000000001",
            ))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "value": "sk-live-abc123"
            })))
            .mount(&mock_server)
            .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let conn = test_connection(mock_server.uri());
        let value = fetcher
            .get_secret_value(
                &conn,
                "shh",
                "prod-vault",
                "b6f1c2e0-1234-4a5b-9abc-000000000001",
            )
            .await
            .expect("get_secret_value");
        assert_eq!(value, Some("sk-live-abc123".to_string()));
    }

    #[tokio::test]
    async fn get_secret_value_404_is_none_not_error() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok-1",
                "expires_in": 300
            })))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/api/v1/vaults/prod-vault/secrets/00000000-0000-0000-0000-000000000000",
            ))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let conn = test_connection(mock_server.uri());
        let value = fetcher
            .get_secret_value(
                &conn,
                "shh",
                "prod-vault",
                "00000000-0000-0000-0000-000000000000",
            )
            .await
            .expect("get_secret_value on missing id");
        assert_eq!(value, None);
    }

    #[tokio::test]
    async fn get_secret_value_401_clears_cached_token_and_forces_refetch() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok-1",
                "expires_in": 300
            })))
            .expect(2) // one for the initial fetch, one forced by the 401 below
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path(
                "/api/v1/vaults/prod-vault/secrets/b6f1c2e0-1234-4a5b-9abc-000000000001",
            ))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let conn = test_connection(mock_server.uri());

        let err = fetcher
            .get_secret_value(
                &conn,
                "shh",
                "prod-vault",
                "b6f1c2e0-1234-4a5b-9abc-000000000001",
            )
            .await
            .expect_err("401 must error");
        assert!(matches!(err, rocket_shared::error::DomainError::Http(_)));
        assert!(fetcher.tokens.get(&conn.id).is_none());

        // A second call must re-authenticate because the cache entry was
        // cleared — the token mock's .expect(2) above fails the test on drop
        // if only one token request ever fires.
        let _ = fetcher
            .get_secret_value(
                &conn,
                "shh",
                "prod-vault",
                "b6f1c2e0-1234-4a5b-9abc-000000000001",
            )
            .await;
    }

    #[tokio::test]
    async fn test_connection_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "access_token": "tok-1",
                "expires_in": 300
            })))
            .mount(&mock_server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/vaults/prod-vault/secrets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "secrets": [],
                "total": 0
            })))
            .mount(&mock_server)
            .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let conn = test_connection(mock_server.uri());
        fetcher
            .test_connection(&conn, "shh", "prod-vault")
            .await
            .expect("test_connection should succeed against a healthy mocked vault");
    }

    #[tokio::test]
    async fn test_connection_fails_on_bad_credentials() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth2/token"))
            .respond_with(ResponseTemplate::new(401))
            .mount(&mock_server)
            .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let conn = test_connection(mock_server.uri());
        let result = fetcher.test_connection(&conn, "wrong", "prod-vault").await;
        assert!(result.is_err());
    }

    #[test]
    fn validate_base_url_https_is_always_ok() {
        let mut conn = test_connection("https://example.com".to_string());
        conn.allow_insecure_http = false;
        assert!(validate_base_url(&conn).is_ok());
        conn.allow_insecure_http = true;
        assert!(validate_base_url(&conn).is_ok());
    }

    #[test]
    fn validate_base_url_http_non_loopback_rejected_unless_allowed() {
        let mut conn = test_connection("http://example.com".to_string());
        conn.allow_insecure_http = false;
        let err = validate_base_url(&conn)
            .expect_err("plain http to a non-loopback host must be rejected");
        assert!(matches!(err, DomainError::InvalidInput(_)));

        conn.allow_insecure_http = true;
        assert!(validate_base_url(&conn).is_ok());
    }

    #[test]
    fn validate_base_url_loopback_always_allowed_over_http() {
        let mut conn = test_connection("http://localhost:8200".to_string());
        conn.allow_insecure_http = false;
        assert!(validate_base_url(&conn).is_ok());

        conn.base_url = "http://127.0.0.1:8200".to_string();
        assert!(validate_base_url(&conn).is_ok());

        // IPv6 loopback ("::1") is also always allowed — regression test for
        // the bracketed-host mismatch between `Url::host_str()` (returns
        // "[::1]") and a bare "::1" comparison.
        conn.base_url = "http://[::1]:8200".to_string();
        assert!(validate_base_url(&conn).is_ok());
    }

    #[test]
    fn compute_token_ttl_floors_zero_expires_in_to_min_ttl() {
        assert_eq!(compute_token_ttl(0), MIN_TOKEN_TTL);
    }

    #[test]
    fn compute_token_ttl_clamps_huge_expires_in_to_max_ttl() {
        // A malicious/buggy server reporting an absurd expires_in (e.g. near
        // u64::MAX) must not be able to overflow `Instant::now() + ttl` in
        // `ensure_token` — the clamp caps the resulting Duration well below
        // any value that could cause that.
        assert_eq!(
            compute_token_ttl(u64::MAX),
            Duration::from_secs(MAX_TOKEN_TTL_SECS)
        );
        assert_eq!(
            compute_token_ttl(MAX_TOKEN_TTL_SECS + 1),
            Duration::from_secs(MAX_TOKEN_TTL_SECS)
        );
    }

    #[test]
    fn compute_token_ttl_passes_through_normal_values() {
        assert_eq!(compute_token_ttl(300), Duration::from_secs(300));
    }
}
