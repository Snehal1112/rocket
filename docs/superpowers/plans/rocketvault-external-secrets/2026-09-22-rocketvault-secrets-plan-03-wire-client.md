# RocketVault Secrets Plan 03: ReqwestVaultSecretFetcher (RocketVault Wire Client) — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Implement `VaultSecretFetcher` (the trait Plan 02 defines in
`rocket-environment`) for real, against RocketVault's actual REST API — OAuth2
client-credentials token issuance with caching, secret listing, secret value
retrieval, and connection testing. This is the concrete HTTP client that talks
to the user's self-hosted RocketVault server.

**Architecture:** One `ReqwestVaultSecretFetcher` struct in `rocket-infra`,
injected once at startup as `Arc<dyn VaultSecretFetcher>` and shared across
every configured RocketVault connection — mirrors how `ReqwestExecutor` serves
every HTTP request regardless of target host. Internally it holds a
`DashMap<String, TokenCache>` keyed by `connection.id` so each connection's
OAuth2 token is cached and refreshed independently, plus two pre-built
`reqwest::Client` instances (one verifying, one not) selected per call — see
Task 1 for why two clients exist rather than one.

**Tech Stack:** Rust, `reqwest`, `dashmap`, `wiremock` (tests).

**Spec:** `docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md`
(§4.1, §4.2). Wire contract verified directly against RocketVault's own
source, `~/data/rocket/rocketvault` (a separate repo, not this one):
`internal/vaultclient/client.go` for token issuance/caching (its `Get` method
covers secret-value retrieval by UUID, though this plan simplifies its
retry-then-fail-terminal behavior on `401` to a single attempt plus cache
invalidation — see the 401 bullet below), and `api/secrets.go` +
`internal/vaultapi/secrets.go` for the list-secrets response shape — see the
list-secrets bullet below for why `vaultclient/client.go` alone is
insufficient ground truth for that endpoint. Plan index (locked interface
contract): `docs/superpowers/plans/rocketvault-external-secrets/00-plan-index.md`.
Previous plans: [Plan 01: Domain Types](2026-09-22-rocketvault-secrets-plan-01-domain-types.md)
(produces `SecretManagerConnection`/`ExternalSecretRef`, consumed throughout
this plan) and [Plan 02: Fetcher Trait, VariableContext, rok.getSecretVar](2026-09-22-rocketvault-secrets-plan-02-fetcher-trait-and-context.md)
(produces the `VaultSecretFetcher` trait this plan implements).

## Global Constraints

- **Shapes consumed from Plan 01** (`rocket_environment::SecretManagerConnection`,
  confirmed from that plan's file, plain field names, no camelCase):
  `{ id: String, label: String, base_url: String, client_id: String, verify_ssl: bool, allow_insecure_http: bool }`.
  `client_secret` is never a field on this struct — every method below takes
  it as a separate `&str` parameter, resolved from the OS keychain by the
  caller (Plan 05), never persisted by this crate.
- **Trait consumed from Plan 02** (`rocket_environment::VaultSecretFetcher`,
  confirmed from that plan's file):
  ```rust
  #[async_trait::async_trait]
  pub trait VaultSecretFetcher: Send + Sync {
      async fn list_secrets(&self, connection: &SecretManagerConnection, client_secret: &str, vault_name: &str) -> DomainResult<Vec<ExternalSecretRef>>;
      async fn get_secret_value(&self, connection: &SecretManagerConnection, client_secret: &str, vault_name: &str, secret_id: &str) -> DomainResult<Option<String>>;
      async fn test_connection(&self, connection: &SecretManagerConnection, client_secret: &str, vault_name: &str) -> DomainResult<()>;
  }
  ```
  and `ExternalSecretRef { name: String, secret_id: String }` (camelCase on
  the wire via serde, plain snake_case Rust field names).
- **Wire contract**:
  - **Token:** `POST {base_url}/api/v1/oauth2/token`,
    `Content-Type: application/x-www-form-urlencoded`, body
    `grant_type=client_credentials&client_id=<id>&client_secret=<secret>`.
    `200` body: `{"access_token": "...", "expires_in": <seconds>}`. `401` =
    bad credentials — terminal, never retried with the same credentials.
    Confirmed from `internal/vaultclient/client.go`'s `fetchToken`.
  - **List — IMPORTANT correction versus a plausible first read of the spec:**
    `GET {base_url}/api/v1/vaults/{vault_name}/secrets`,
    `Authorization: Bearer <token>`. The response is **not** a bare JSON
    array — it is the `model.ListSecretsResponse` envelope,
    `{"secrets": [...], "total": <count>}`. `internal/vaultclient/client.go`
    (the reference Go client cited by the spec for the wire contract) has no
    List method at all, so it cannot confirm this endpoint's shape; the
    actual ground truth is the HTTP handler itself
    (`~/data/rocket/rocketvault/api/secrets.go:202`, which builds and returns
    exactly `model.ListSecretsResponse{Secrets: secretResponses, Total: len(secretsList)}`)
    and RocketVault's own internal `vaultapi` client
    (`~/data/rocket/rocketvault/internal/vaultapi/secrets.go:36-40`, whose
    `secretsListResponse{ Secrets []secretWire; Total int }` mirrors the same
    envelope with `json:"secrets"`/`json:"total"` tags). Each entry inside
    `secrets` carries at least `id` (string UUID) and `name` (string), plus
    other fields (`value`, `tags`, `version`, `created_at`, `enabled`, ...)
    that this client must never read into a value-bearing type — map only
    `id`/`name` into `ExternalSecretRef { name, secret_id: id }`. Never
    decode a `value` field even when the server includes one in a list entry
    (spec §4.1/§4.4 — values must never be captured at list time).
  - **Get secret value:** `GET {base_url}/api/v1/vaults/{vault_name}/secrets/{secret_id}`,
    same Bearer token. `200` body: `{"value": "..."}`. `404` → the secret
    doesn't exist, returns `Ok(None)` (not an error — mirrors
    `SecretStore::get`'s existing `Option<String>` contract,
    `crates/rocket-environment/src/secret_store.rs:9`). `401` → invalidate
    the cached token for that connection and return an error for this call —
    never silently retried with the same stale token within the same call.
    (The reference Go client's own `Get` does retry once via
    `retry.WithExponentialBackoff` before surfacing `ErrAuthFailed`; this
    plan intentionally simplifies that to a single attempt plus cache
    invalidation, so the *next* call re-authenticates instead.)
  - **Token caching**, mirroring the reference client's `ensureToken`/
    `tokenExpiryCutoff` exactly: a minimum 30-second TTL is enforced
    client-side even if the server returns `expires_in: 0`; the cached token
    is treated as stale and refreshed 60 seconds before its real expiry
    (early-refresh cutoff), except when the token's total remaining life is
    already at or under that 60s window, in which case the raw expiry is used
    (otherwise a token with a 45s TTL would be refetched on every call).
  - **Security default:** `connection.base_url` must use `https://` for any
    non-loopback host unless `connection.allow_insecure_http` is `true`.
    Loopback (`localhost`/`127.0.0.1`/`::1`) is always allowed over plain HTTP
    regardless of that flag, for local dev RocketVault instances — mirrors
    `isLoopbackHost` in the reference client. This check runs before every
    token fetch (see Task 1's `validate_base_url`, called from
    `ensure_token`) rather than once at `ReqwestVaultSecretFetcher`
    construction time, because one fetcher instance serves every configured
    connection rather than being constructed per connection — this is the
    per-call equivalent of the reference client's `New()` rejecting
    construction.
  - **TLS verification is a `reqwest::Client`-construction-time setting, not
    a per-request one.** `connection.verify_ssl == false` must skip
    certificate verification (self-signed dev certs) for that connection's
    requests. This plan resolves that by building **two** internal
    `reqwest::Client` instances once, in `ReqwestVaultSecretFetcher::new()` —
    one with default (verifying) TLS behavior, one built with
    `.danger_accept_invalid_certs(true)` — and picking whichever one matches
    `connection.verify_ssl` on every call (Task 1's `client_for`). Neither
    client is ever rebuilt on a request path.
- **`DomainError` variant mapping** (`rocket_shared::error::DomainError`,
  confirmed variants: `NotFound`, `InvalidInput`, `AlreadyExists`, `Io`,
  `Serialization`, `Http`, `Internal`, `Conflict`, three `Ssh*` variants,
  `TlsCertificateInvalid` — no variant exists for "unauthorized" or
  "RocketVault" specifically, so pick the closest existing fit rather than
  inventing one, matching this repo's DDD rules):
  - `base_url` scheme/parse validation failures → `DomainError::InvalidInput`
    (a connection configuration problem, not a network failure — same
    category as any other bad-input rejection).
  - Every network/transport failure, non-2xx response status (including
    `401` on the token/list/get endpoints), and response-decode failure
    (missing/empty `access_token`, malformed JSON) → `DomainError::Http`.
    This matches the exact precedent already established for OAuth2 token
    fetches in this same crate — `fetch_client_credentials_token` in
    `crates/rocket-infra/src/reqwest_executor.rs:466-528` uses
    `DomainError::Http` uniformly for request failures, non-success status
    (its own `401` test asserts a `DomainError::Http` whose message contains
    `"401"`), and missing-`access_token` decode failures. This plan follows
    that same convention rather than introducing a second way to represent
    an auth failure.
  - `404` on `get_secret_value` is not an error at all — `Ok(None)`.
- `dashmap = { workspace = true }` (`crates/rocket-infra/Cargo.toml:28`),
  `async-trait.workspace = true` (`Cargo.toml:17`), `reqwest.workspace = true`
  (`Cargo.toml:19`), `url = "2"` (`Cargo.toml:33`), and `wiremock = "0.6"`
  under `[dev-dependencies]` (`Cargo.toml:41`) are all already dependencies of
  `rocket-infra` — no `Cargo.toml` changes needed anywhere in this plan.
- Test style: match `crates/rocket-infra/src/reqwest_executor.rs`'s existing
  `oauth2_tests` module exactly — `#[tokio::test]`,
  `wiremock::{Mock, MockServer, ResponseTemplate}`,
  `wiremock::matchers::{method, path, header_exists}`, `MockServer::start().await`
  then `Mock::given(...).respond_with(...).mount(&mock_server).await`. No real
  RocketVault server is started in any test.
- Test code uses `.expect("message")` for fallible calls rather than the bare
  panicking shorthand, matching this repository's stricter Rust safety
  convention even in test paths. Production code never uses that shorthand
  either — every fallible call here is handled with `?`/`map_err`, and the
  one call that cannot return a `Result` (`ReqwestVaultSecretFetcher::new()`,
  whose signature is locked to `-> Self`) falls back with `.unwrap_or_else(...)`
  rather than ever panicking.
- All three tasks below modify a single new file,
  `crates/rocket-infra/src/rocketvault/mod.rs` — later tasks add to the same
  `#[cfg(test)] mod tests` block and the same `impl VaultSecretFetcher for
  ReqwestVaultSecretFetcher` block that earlier tasks create, rather than
  creating new ones.

---

## Task 1: Struct skeleton + token fetch/cache

**Files:**
- Create: `crates/rocket-infra/src/rocketvault/mod.rs`
- Modify: `crates/rocket-infra/src/lib.rs`

**Interfaces:**
- Consumes: `rocket_environment::SecretManagerConnection` (Plan 01).
- Produces: `ReqwestVaultSecretFetcher::new() -> Self`, private `ensure_token`
  and `fetch_token` methods — consumed internally by Tasks 2 and 3 of this
  plan, and by Plan 05 (`SecretManagerService` holds this type behind
  `Arc<dyn VaultSecretFetcher>`).

- [ ] **Step 1: Write the failing tests**

```rust
// crates/rocket-infra/src/rocketvault/mod.rs
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-infra rocketvault::tests`
Expected: FAIL — `rocketvault` module doesn't exist yet.

- [ ] **Step 3: Implement the struct, token fetch, and cache**

```rust
// crates/rocket-infra/src/rocketvault/mod.rs (add above the tests module)
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
```

- [ ] **Step 4: Register the module**

In `crates/rocket-infra/src/lib.rs`, add alongside the existing module
declarations:

```rust
pub mod rocketvault;
pub use rocketvault::ReqwestVaultSecretFetcher;
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rocket-infra rocketvault::tests`
Expected: PASS — 2 tests.

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-infra/src/rocketvault/mod.rs crates/rocket-infra/src/lib.rs
git commit -m "feat(infra): add ReqwestVaultSecretFetcher token fetch and cache"
```

---

## Task 2: `list_secrets`

**Files:**
- Modify: `crates/rocket-infra/src/rocketvault/mod.rs`

**Interfaces:**
- Consumes: `ensure_token`/`client_for` from Task 1;
  `rocket_environment::VaultSecretFetcher` trait and
  `rocket_environment::ExternalSecretRef` (Plans 01/02).
- Produces: `VaultSecretFetcher::list_secrets` impl on
  `ReqwestVaultSecretFetcher` — consumed by Plan 05
  (`SecretManagerService::fetch_secret_names`).

- [ ] **Step 1: Write the failing tests**

Add to the existing `#[cfg(test)] mod tests` block from Task 1:

```rust
use wiremock::matchers::{header_exists, query_param};

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
```

(The `use wiremock::matchers::header_exists;` line above only needs to be
added once to the test module's imports, alongside the `method`/`path`
import already added in Task 1.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-infra rocketvault::tests -- list_secrets`
Expected: FAIL — `list_secrets` is not implemented (the `VaultSecretFetcher`
trait is not yet implemented for `ReqwestVaultSecretFetcher`).

- [ ] **Step 3: Implement `list_secrets`**

```rust
// crates/rocket-infra/src/rocketvault/mod.rs (add below the impl block from Task 1)
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
            .map_err(|e| DomainError::Http(format!("RocketVault list_secrets request failed: {e}")))?;

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

    // get_secret_value / test_connection: implemented in Task 3. This
    // scaffolding exists only so the trait compiles with all three methods
    // present after this task — it is replaced with a real body in Task 3
    // below, and by the end of Task 3 no `unimplemented!` remains anywhere
    // in this file.
    async fn get_secret_value(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
        _secret_id: &str,
    ) -> DomainResult<Option<String>> {
        unimplemented!("implemented in Task 3")
    }

    async fn test_connection(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<()> {
        unimplemented!("implemented in Task 3")
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-infra rocketvault::tests -- list_secrets`
Expected: PASS — 2 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/src/rocketvault/mod.rs
git commit -m "feat(infra): implement VaultSecretFetcher::list_secrets"
```

---

## Task 3: `get_secret_value` + `test_connection`, full wiremock coverage

**Files:**
- Modify: `crates/rocket-infra/src/rocketvault/mod.rs`

**Interfaces:**
- Produces: complete `VaultSecretFetcher for ReqwestVaultSecretFetcher` impl,
  importable as `rocket_infra::ReqwestVaultSecretFetcher` — consumed by Plan
  05 (`SecretManagerService`), Plan 06 (`RequestExecutionService`), Plan 08
  (`src-tauri` startup wiring, as `Arc<dyn VaultSecretFetcher>`).

- [ ] **Step 1: Write the failing tests**

Add to the existing `#[cfg(test)] mod tests` block:

```rust
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
        .and(path("/api/v1/vaults/prod-vault/secrets/b6f1c2e0-1234-4a5b-9abc-000000000001"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "value": "sk-live-abc123"
        })))
        .mount(&mock_server)
        .await;

    let fetcher = ReqwestVaultSecretFetcher::new();
    let conn = test_connection(mock_server.uri());
    let value = fetcher
        .get_secret_value(&conn, "shh", "prod-vault", "b6f1c2e0-1234-4a5b-9abc-000000000001")
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
        .and(path("/api/v1/vaults/prod-vault/secrets/00000000-0000-0000-0000-000000000000"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let fetcher = ReqwestVaultSecretFetcher::new();
    let conn = test_connection(mock_server.uri());
    let value = fetcher
        .get_secret_value(&conn, "shh", "prod-vault", "00000000-0000-0000-0000-000000000000")
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
        .and(path("/api/v1/vaults/prod-vault/secrets/b6f1c2e0-1234-4a5b-9abc-000000000001"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&mock_server)
        .await;

    let fetcher = ReqwestVaultSecretFetcher::new();
    let conn = test_connection(mock_server.uri());

    let err = fetcher
        .get_secret_value(&conn, "shh", "prod-vault", "b6f1c2e0-1234-4a5b-9abc-000000000001")
        .await
        .expect_err("401 must error");
    assert!(matches!(err, rocket_shared::error::DomainError::Http(_)));
    assert!(fetcher.tokens.get(&conn.id).is_none());

    // A second call must re-authenticate because the cache entry was
    // cleared — the token mock's .expect(2) above fails the test on drop
    // if only one token request ever fires.
    let _ = fetcher
        .get_secret_value(&conn, "shh", "prod-vault", "b6f1c2e0-1234-4a5b-9abc-000000000001")
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
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p rocket-infra rocketvault::tests`
Expected: FAIL — the two `unimplemented!` bodies from Task 2 panic when
`get_secret_value`/`test_connection` are called.

- [ ] **Step 3: Implement `get_secret_value` and `test_connection`**

Replace the two `unimplemented!(...)` method bodies added in Task 2 with:

```rust
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
        .map_err(|e| DomainError::Http(format!("RocketVault get_secret_value request failed: {e}")))?;

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
    self.list_secrets(connection, client_secret, vault_name).await?;
    Ok(())
}
```

`test_connection` deliberately discards `list_secrets`'s `Ok` value — it only
needs to confirm authentication succeeds and the vault is readable, not
return any secret names, per the "Test Connection" UI action's contract
(spec §4.3).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rocket-infra rocketvault::tests`
Expected: PASS — all 9 tests across Tasks 1–3 (2 + 2 + 5).

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-infra/src/rocketvault/mod.rs
git commit -m "feat(infra): implement get_secret_value and test_connection"
```

---

## Milestone Checklist — Plan 03

- [ ] `ReqwestVaultSecretFetcher::new()` — builds both `http` and
  `http_insecure` clients eagerly, never rebuilds either on a request path
- [ ] `ensure_token`/`fetch_token` — fetches, caches, enforces 30s minimum
  TTL and 60s early-refresh cutoff, `401` never populates the cache
- [ ] `validate_base_url` — `https://` required for non-loopback hosts unless
  `allow_insecure_http` is set; loopback always allowed over plain HTTP
- [ ] `client_for` — selects `http` vs `http_insecure` based on
  `connection.verify_ssl`
- [ ] `list_secrets` — decodes the `{"secrets": [...], "total": N}` envelope
  (not a bare array), maps only `id`/`name` into `ExternalSecretRef`, never
  reads or persists a `value` field from any list entry
- [ ] `get_secret_value` — `200` → `Some(value)`, `404` → `Ok(None)`, `401` →
  invalidates the cached token for that connection and errors
- [ ] `test_connection` — `ensure_token` + `list_secrets`, propagates the
  first failure, discards the secret list on success
- [ ] No `unimplemented!` remains anywhere in the file by the end of Task 3
- [ ] `cargo test -p rocket-infra rocketvault::tests` — all pass (9 tests)

## Next Plan

[Plan 04: FsSecretManagerRepo + keychain + Oc persistence layer](2026-09-22-rocketvault-secrets-plan-04-persistence.md) —
independent of this plan (both depend only on Plan 01's domain types), but
next in build order per the index. Plan 05 then injects this plan's
`ReqwestVaultSecretFetcher` as `Arc<dyn VaultSecretFetcher>` into
`SecretManagerService` and the shared `resolve_vault_secret_value` helper,
alongside Plan 04's `FsSecretManagerRepo`.
