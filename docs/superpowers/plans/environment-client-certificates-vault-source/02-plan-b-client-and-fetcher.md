# RocketVault Certificate Source, Plan B: RocketVault Client and Fetcher

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let the shared `VaultSecretFetcher` list the certificates of a vault and export one by name. The RocketVault client resolves the name to an id through the paged list, caches the id in memory, retries once with a fresh id after a 404, exports PEM or PKCS12 (a fresh random password per PKCS12 export), reads at most 1 MiB, and maps every RocketVault error code to a clear message. Material is never cached.

**Architecture:** `rocket-environment` gains two domain types (`VaultCertificateSummary`, `VaultCertificateMaterial`) and two trait methods with default bodies, so the five test fakes compile unchanged. In `rocket-infra/src/rocketvault/`, a new `certificate_api.rs` holds the whole RocketVault certificate HTTP contract (routes, request and response shapes, error codes, messages) as pure functions, and a new `certificates.rs` holds the calls (list walk, id cache, export) as `impl ReqwestVaultSecretFetcher` blocks. Both are child modules of `rocketvault/mod.rs`, so they reuse its private `ensure_token`, `client_for`, `tokens` and `vault_api_url`. The trait impl in `mod.rs` only delegates.

**Tech Stack:** Rust, `async-trait`, `reqwest` 0.12, `serde`/`serde_json`, `base64`, `dashmap`, `zeroize`, `rand` 0.8 (`OsRng`), `wiremock`.

**Spec:** [`docs/superpowers/specs/2026-10-02-vault-certificate-source-design.md`](../../specs/2026-10-02-vault-certificate-source-design.md) (sections 1, 2, 5.3, 5.4, 8, 9, 10, 11.1, 11.3). Plan index and the shared interface contract: [`00-plan-index.md`](00-plan-index.md).

**Plan B of 4 (A, B, C, D).** Depends on A1 (`VaultCertificateFormat`).

---

## Global Constraints

- Values fetched from RocketVault are never written to disk in any form: not the environment YAML, not history, not logs, not events, not the audit trail. Only references persist (an External Secrets alias and a certificate name). Runtime material is `zeroize::Zeroizing` from fetch to use.
- An unresolved or failed vault certificate fails a request or token request only when that certificate is the one selected for the URL, with no fallback to another entry. An entry for another domain causes no RocketVault call.
- A `CertificateMaterial::Deferred` that reaches the executor is an `InvalidInput` error, never a silent skip.
- A path or reference field must not hold key text: a value starting with `-----BEGIN` is rejected on save. For a `vault` entry this covers every field.
- Error and log text never contains key bytes, PKCS12 bundle bytes or the one-time password.
- Serde: `#[serde(rename_all = "camelCase")]` on IPC DTOs only, plus the `ClientCertificate` variants, which already carry it. Persisted fields stay backward compatible (additive, with defaults).
- Rust: never call `unwrap` in production paths. Always pass `-j4` to cargo. Never run `cargo test --workspace`; use targeted crate tests plus `cargo check -j4 --workspace`.
- Frontend: shadcn/ui primitives only, `lucide-react` icons only, `SingleLineEditor` for single-line variable-aware fields, Monaco only for multi-line editors, never fully destructure Zustand store state at component top level.
- Commits: conventional commits, path-scoped (`git add <paths>` then `git commit -- <paths>`, never `git add -A` or `git commit -a`). Every commit goes through the `dev-workflow-skills:1-git-commit` skill. Commit messages end with `Relates to: #21`.
- Every task that touches collection, environment or certificate data models starts with: `📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.`

## Review Focus (items this plan owns)

1. **A name on page 3.** Owner B2. Pinned by `certificate_id_walks_pages_until_the_name_is_found_on_page_three` in `crates/rocket-infra/src/rocketvault/certificates.rs`.
2. **Deleted and created again under the same name.** Owner B3. Pinned by `export_refreshes_the_id_and_retries_once_after_a_404` and `a_certificate_that_stays_missing_is_not_found_after_one_retry` in `certificates.rs`.
4. **A 401 in the middle of a send.** Owner B3 (client half). Pinned by `export_401_evicts_the_token_and_fails` in `certificates.rs`.
5. **A non-exportable certificate.** Owner B3 (message half). Pinned by `export_403_not_exportable_says_so` in `certificates.rs`.

Also pinned here: `each_pkcs12_export_uses_a_new_random_password_with_legacy_compat`, `material_is_never_cached_each_fetch_exports_again`, `an_export_larger_than_1_mib_is_rejected`, `errors_and_debug_never_contain_key_bytes_or_the_password` (B3).

## Spec versus code (read before starting)

1. **The password is generated inside the client.** Spec 5.3 lists `fetch_certificate(..., name, format, password)`. Here `fetch_certificate` takes no password: `rocket-infra` makes one per PKCS12 export (`OsRng`, 32 letters and digits), sends it, and returns it inside `VaultCertificateMaterial::Pkcs12 { password }`, where Plan C moves it into the passphrase with no copy. The maker and the sender are one function, which is also where the "random per call" test sits.
2. **Default trait methods.** `VaultSecretFetcher` has seven implementors: `ReqwestVaultSecretFetcher`, `NullVaultSecretFetcher`, and test fakes in `crates/rocket-app/src/{agent_config_service,acp_session_service,vault_secret_resolution,secret_manager_service,test_doubles,execution_service}.rs`. The new methods get default bodies that return `Internal(...)`, so none of the fakes change. `NullVaultSecretFetcher` overrides them with its usual "no vault secret fetcher configured".
3. **The list envelope is assumed.** The spec says the list "returns id and name per certificate, has no name filter and is paged" but not the envelope. `certificate_api.rs` assumes `{"certificates": [...], "total": N}` with `id`, `name`, `exportable`, `enabled`, `key_algorithm`, `expires_at` per entry, mirroring `model.ListSecretsResponse` (`{"secrets": [...], "total": N}`) that `list_secrets` already decodes. `total` is optional; a short page also ends the walk. Confirm with the RocketVault session before release; a change is local to `certificate_api.rs`.
4. **The flat route is not used.** The spec mentions a flat export route for the default vault. A binding always names a vault, so only `/api/v1/vaults/{vault}/certificates/{id}/export` is used.
5. **"legacy" PKCS12 and OpenSSL 3.** Decision 3 sends `compat: "legacy"` so every platform TLS stack can open the bundle. On Linux, native-tls uses OpenSSL 3, which opens 3DES bundles but not RC2-40 ones without the legacy provider. Which ciphers RocketVault's "legacy" mode uses is not in the spec. The live test (B3 Step 13) loads a real legacy export through `reqwest::Identity::from_pkcs12_der` on the machine that runs it; if it fails on Linux, report it instead of changing the compat value.
6. **Wiping is best effort past our buffers.** Our buffers are `Zeroizing` and reserved up front so they do not reallocate. Copies we cannot reach stay: reqwest owns the request body after `send` (it holds the one-time password, which opens nothing once the response is read), hyper's receive buffers, and serde_json's scratch buffer when it unescapes the PEM's `\n`. This matches the parent design's stance on `reqwest::Identity`.

## Findings from reading the real code (do not re-derive)

- `crates/rocket-environment/src/vault_secret_fetcher.rs` (160 lines): trait lines 20-52, `NullVaultSecretFetcher` lines 54-97, tests lines 99-160 with `dummy_connection()`. Re-exports in `crates/rocket-environment/src/lib.rs` line 22. `rocket-environment` has no `zeroize` dependency yet (`crates/rocket-environment/Cargo.toml`), and already depends on `rand = "0.8"`.
- `crates/rocket-infra/src/rocketvault/mod.rs` (936 lines, the only file in the folder): `ReqwestVaultSecretFetcher { http, http_insecure, tokens: DashMap<String, TokenCache> }` lines 87-99, `new()` lines 101-132, `client_for` lines 136-142, `ensure_token` lines 150-179 (it also enforces `https://` for non-loopback hosts), `vault_api_url` lines 271-301 (percent-encodes each segment, rejects empty, `.` and `..`), the `use rocket_environment::{ExternalSecretRef, VaultSecretFetcher};` at line 336, the trait impl lines 363-485 (`list_secrets` asks for `per_page=200` and evicts the token on a 401). Tests use `wiremock` with a `test_connection(base_url)` helper (`allow_insecure_http: true`).
- `crates/rocket-infra/Cargo.toml` has `base64`, `dashmap`, `serde_json`, `url`, `zeroize`, `reqwest`, and `wiremock` as a dev-dependency. It has no `rand`.
- Child modules see their parent's private items, so `certificates.rs` can call `self.ensure_token`, `self.client_for`, `self.tokens` and `super::vault_api_url` without changing their visibility.

## Test conventions

- New tests use `.expect("message")` and `expect_err("message")`. Production code never calls `unwrap`.
- Wiremock tests mount the token endpoint with `mount_token`, then the list pages and export routes they need, with `.expect(n)` so call counts are part of the assertion (checked when the server drops).

---

## Task B1: Fetcher trait, domain types and the RocketVault contract module

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** sonnet.

**Files:**
- Modify: `crates/rocket-environment/Cargo.toml` (add `zeroize = "1"` under `[dependencies]`, after `rand = "0.8"`)
- Modify: `Cargo.lock` (updated by cargo)
- Modify: `crates/rocket-environment/src/vault_secret_fetcher.rs` (imports lines 1-3, new types before the trait, trait lines 20-52, `NullVaultSecretFetcher` lines 62-97, tests)
- Modify: `crates/rocket-environment/src/lib.rs` (line 22)
- Create: `crates/rocket-infra/src/rocketvault/certificate_api.rs`
- Modify: `crates/rocket-infra/src/rocketvault/mod.rs` (module declaration after the `use` lines 1-6)

**Interfaces:**
- Consumes: A1's `rocket_shared::certificate::VaultCertificateFormat`; `SecretManagerConnection`; `super::vault_api_url`.
- Produces (contract names): `VaultCertificateSummary`, `VaultCertificateMaterial` (`format()`, `key_algorithm()`), `VaultSecretFetcher::list_certificates` and `fetch_certificate` with default bodies; every `certificate_api` item in the contract. Consumed by B2, B3, C1, C3.

- [ ] **Step 1: Add the dependency and write the failing trait tests**

In `crates/rocket-environment/Cargo.toml`, add under `[dependencies]` after `rand = "0.8"`:

```toml
zeroize = "1"
```

In `crates/rocket-environment/src/vault_secret_fetcher.rs`, add at the end of `mod tests` (before its closing `}`):

```rust
    /// A fetcher written before certificates existed: only the three original methods.
    struct SecretsOnlyFetcher;

    #[async_trait::async_trait]
    impl VaultSecretFetcher for SecretsOnlyFetcher {
        async fn list_secrets(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<Vec<ExternalSecretRef>> {
            Ok(Vec::new())
        }
        async fn get_secret_value(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
            _secret_id: &str,
        ) -> DomainResult<Option<String>> {
            Ok(None)
        }
        async fn test_connection(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn a_fetcher_without_certificate_support_refuses_by_default() {
        let fetcher = SecretsOnlyFetcher;
        let conn = dummy_connection();
        let err = fetcher
            .list_certificates(&conn, "shh", "prod-vault")
            .await
            .expect_err("the default refuses");
        assert!(matches!(err, DomainError::Internal(_)), "{err:?}");
        let err = fetcher
            .fetch_certificate(&conn, "shh", "prod-vault", "client-a", VaultCertificateFormat::Pem)
            .await
            .expect_err("the default refuses");
        assert!(matches!(err, DomainError::Internal(_)), "{err:?}");
    }

    #[tokio::test]
    async fn null_fetcher_certificate_calls_error() {
        let fetcher = NullVaultSecretFetcher;
        let conn = dummy_connection();
        let expected = DomainError::Internal("no vault secret fetcher configured".to_string());
        assert_eq!(
            fetcher
                .list_certificates(&conn, "shh", "prod-vault")
                .await
                .expect_err("null fetcher must error"),
            expected
        );
        assert_eq!(
            fetcher
                .fetch_certificate(&conn, "shh", "prod-vault", "client-a", VaultCertificateFormat::Pkcs12)
                .await
                .expect_err("null fetcher must error"),
            expected
        );
    }

    #[test]
    fn material_debug_prints_sizes_and_never_bytes_or_the_password() {
        let pem = VaultCertificateMaterial::Pem {
            certificate: Zeroizing::new(b"-----BEGIN CERTIFICATE-----\nAAAA\n".to_vec()),
            private_key: Zeroizing::new(b"-----BEGIN PRIVATE KEY-----\nc2VjcmV0\n".to_vec()),
            key_algorithm: "RSA-2048".into(),
        };
        let shown = format!("{pem:?}");
        assert!(shown.contains("bytes") && shown.contains("RSA-2048"), "{shown}");
        assert!(!shown.contains("BEGIN") && !shown.contains("c2VjcmV0"), "{shown}");

        let p12 = VaultCertificateMaterial::Pkcs12 {
            bundle: Zeroizing::new(vec![0x30, 0x82, 0x01]),
            password: Zeroizing::new("one-time-pass-123".into()),
            key_algorithm: "EC-P256".into(),
        };
        let shown = format!("{p12:#?}");
        assert!(shown.contains("3 bytes") && shown.contains("<redacted>"), "{shown}");
        assert!(!shown.contains("one-time-pass-123"), "{shown}");
        // A byte vector would print as `[48, 130, 1]`.
        assert!(!shown.contains('['), "{shown}");
    }

    #[test]
    fn material_reports_its_format_and_key_algorithm() {
        let p12 = VaultCertificateMaterial::Pkcs12 {
            bundle: Zeroizing::new(vec![1]),
            password: Zeroizing::new("p".into()),
            key_algorithm: "EC-P256".into(),
        };
        assert_eq!(p12.format(), VaultCertificateFormat::Pkcs12);
        assert_eq!(p12.key_algorithm(), "EC-P256");
        let pem = VaultCertificateMaterial::Pem {
            certificate: Zeroizing::new(vec![1]),
            private_key: Zeroizing::new(vec![2]),
            key_algorithm: "RSA-2048".into(),
        };
        assert_eq!(pem.format(), VaultCertificateFormat::Pem);
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-environment vault_secret_fetcher`
Expected: FAIL to compile with `cannot find type VaultCertificateMaterial in this scope` and `no method named list_certificates`.

- [ ] **Step 3: Add the types, the trait methods and the null overrides**

In `crates/rocket-environment/src/vault_secret_fetcher.rs`, lines 1-3, old:

```rust
use crate::external_secret::ExternalSecretRef;
use crate::secret_manager::SecretManagerConnection;
use rocket_shared::error::{DomainError, DomainResult};
```

new:

```rust
use std::fmt;

use crate::external_secret::ExternalSecretRef;
use crate::secret_manager::SecretManagerConnection;
use rocket_shared::certificate::VaultCertificateFormat;
use rocket_shared::error::{DomainError, DomainResult};
use zeroize::Zeroizing;

/// One certificate in a vault, as the Certificates tab picker shows it. Names and metadata
/// only, never key material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultCertificateSummary {
    pub id: String,
    pub name: String,
    /// Created exportable over an exportable key. RocketVault never changes this flag.
    pub exportable: bool,
    pub enabled: bool,
    /// As RocketVault reports it, for example `RSA-2048` or `EC-P256`.
    pub key_algorithm: String,
    pub expires_at: Option<String>,
}

/// The exported material of one certificate. It lives in memory only and is wiped on drop. It
/// is deliberately not `Clone` or `Serialize`, and its `Debug` prints sizes only.
pub enum VaultCertificateMaterial {
    /// The certificate chain (leaf first, no root) and an unencrypted PKCS#8 private key, as PEM.
    Pem {
        certificate: Zeroizing<Vec<u8>>,
        private_key: Zeroizing<Vec<u8>>,
        key_algorithm: String,
    },
    /// A PKCS12 bundle and the one-time password it was exported with.
    Pkcs12 {
        bundle: Zeroizing<Vec<u8>>,
        password: Zeroizing<String>,
        key_algorithm: String,
    },
}

impl VaultCertificateMaterial {
    pub fn format(&self) -> VaultCertificateFormat {
        match self {
            VaultCertificateMaterial::Pem { .. } => VaultCertificateFormat::Pem,
            VaultCertificateMaterial::Pkcs12 { .. } => VaultCertificateFormat::Pkcs12,
        }
    }

    pub fn key_algorithm(&self) -> &str {
        match self {
            VaultCertificateMaterial::Pem { key_algorithm, .. }
            | VaultCertificateMaterial::Pkcs12 { key_algorithm, .. } => key_algorithm,
        }
    }
}

/// Prints `<n> bytes` in place of material.
struct ByteCount(usize);

impl fmt::Debug for ByteCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} bytes", self.0)
    }
}

// Hand-written so a `{:?}` never prints key bytes or the password.
impl fmt::Debug for VaultCertificateMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VaultCertificateMaterial::Pem {
                certificate,
                private_key,
                key_algorithm,
            } => f
                .debug_struct("Pem")
                .field("certificate", &ByteCount(certificate.len()))
                .field("private_key", &ByteCount(private_key.len()))
                .field("key_algorithm", key_algorithm)
                .finish(),
            VaultCertificateMaterial::Pkcs12 {
                bundle,
                key_algorithm,
                ..
            } => f
                .debug_struct("Pkcs12")
                .field("bundle", &ByteCount(bundle.len()))
                .field("password", &"<redacted>")
                .field("key_algorithm", key_algorithm)
                .finish(),
        }
    }
}
```

In the trait, after `test_connection` (the method that ends at line 51, before the trait's closing `}`), add:

```rust

    /// Lists the certificates in `vault_name`: names and metadata, never key material. Backs
    /// the Certificates tab picker. The default refuses, for fetchers with no certificate
    /// support (test fakes written before certificates existed).
    async fn list_certificates(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<Vec<VaultCertificateSummary>> {
        Err(DomainError::Internal(
            "this vault fetcher cannot list certificates".to_string(),
        ))
    }

    /// Exports the certificate named `certificate_name` in `format`. A PKCS12 export uses a
    /// fresh random password, returned with the bundle and never stored. Nothing is cached.
    /// The default refuses, like `list_certificates`.
    async fn fetch_certificate(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
        _certificate_name: &str,
        _format: VaultCertificateFormat,
    ) -> DomainResult<VaultCertificateMaterial> {
        Err(DomainError::Internal(
            "this vault fetcher cannot export certificates".to_string(),
        ))
    }
```

In `impl VaultSecretFetcher for NullVaultSecretFetcher`, after `test_connection` (before the impl's closing `}` on line 97), add:

```rust

    async fn list_certificates(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<Vec<VaultCertificateSummary>> {
        Err(DomainError::Internal(
            "no vault secret fetcher configured".to_string(),
        ))
    }

    async fn fetch_certificate(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
        _certificate_name: &str,
        _format: VaultCertificateFormat,
    ) -> DomainResult<VaultCertificateMaterial> {
        Err(DomainError::Internal(
            "no vault secret fetcher configured".to_string(),
        ))
    }
```

In `crates/rocket-environment/src/lib.rs`, line 22, old:

```rust
pub use vault_secret_fetcher::{NullVaultSecretFetcher, VaultSecretFetcher};
```

new:

```rust
pub use vault_secret_fetcher::{
    NullVaultSecretFetcher, VaultCertificateMaterial, VaultCertificateSummary, VaultSecretFetcher,
};
```

- [ ] **Step 4: Run the trait tests to verify they pass**

Run: `cargo test -j4 -p rocket-environment vault_secret_fetcher`
Expected: PASS, 8 tests (4 existing, 4 new).

Run: `cargo check -j4 --workspace`
Expected: `Finished`. Every existing fake compiles unchanged thanks to the default bodies.

- [ ] **Step 5: Write the contract module with its tests first**

Create `crates/rocket-infra/src/rocketvault/certificate_api.rs` with only the module doc, the imports and the tests:

```rust
//! The RocketVault v4 certificate HTTP contract, in one place.
//!
//! Routes, request and response shapes, error codes and the user-facing messages for them live
//! here and nowhere else, so a change on the RocketVault side (its v-4.0.0 branch is not
//! published yet) is a change to this file only. The calls themselves are in `certificates.rs`.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use reqwest::StatusCode;
use rocket_environment::{SecretManagerConnection, VaultCertificateMaterial, VaultCertificateSummary};
use rocket_shared::certificate::VaultCertificateFormat;
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use super::vault_api_url;

#[cfg(test)]
mod tests {
    use super::*;

    fn connection() -> SecretManagerConnection {
        SecretManagerConnection {
            id: "conn-1".into(),
            label: "Test".into(),
            base_url: "https://vault.example.com/".into(),
            client_id: "rocketapi".into(),
            verify_ssl: true,
            allow_insecure_http: false,
        }
    }

    const CERT_PEM: &str = "-----BEGIN CERTIFICATE-----\nZmFrZS1jZXJ0\n-----END CERTIFICATE-----\n";
    const KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nZmFrZS1rZXk=\n-----END PRIVATE KEY-----\n";

    #[test]
    fn list_url_asks_for_one_page_of_200() {
        let url = list_url(&connection(), "prod-vault", 2).expect("url");
        assert_eq!(
            url.as_str(),
            "https://vault.example.com/api/v1/vaults/prod-vault/certificates?page=2&per_page=200"
        );
    }

    #[test]
    fn export_url_uses_the_id_and_percent_encodes_segments() {
        let url = export_url(&connection(), "a/b", "id?1").expect("url");
        assert_eq!(
            url.as_str(),
            "https://vault.example.com/api/v1/vaults/a%2Fb/certificates/id%3F1/export"
        );
        assert!(export_url(&connection(), "prod-vault", "..").is_err());
    }

    #[test]
    fn a_pem_export_body_sends_only_the_format() {
        let body = export_request_body(VaultCertificateFormat::Pem, None).expect("body");
        assert_eq!(body.as_slice(), br#"{"format":"pem"}"#);
    }

    #[test]
    fn a_pkcs12_export_body_sends_the_password_and_legacy_compat() {
        let body =
            export_request_body(VaultCertificateFormat::Pkcs12, Some("pw-123")).expect("body");
        assert_eq!(
            body.as_slice(),
            br#"{"format":"pkcs12","password":"pw-123","compat":"legacy"}"#
        );
        assert!(export_request_body(VaultCertificateFormat::Pkcs12, None).is_err());
    }

    #[test]
    fn parse_certificate_page_maps_fields_and_defaults() {
        let body = br#"{"certificates":[
            {"id":"id-1","name":"client-a","exportable":true,"enabled":false,
             "key_algorithm":"EC-P256","expires_at":"2027-01-01T00:00:00Z","not_before":"x"},
            {"id":"id-2","name":"bare"}],"total":2}"#;
        let page = parse_certificate_page(body).expect("page");
        assert_eq!(page.total, Some(2));
        assert_eq!(
            page.certificates,
            vec![
                VaultCertificateSummary {
                    id: "id-1".into(),
                    name: "client-a".into(),
                    exportable: true,
                    enabled: false,
                    key_algorithm: "EC-P256".into(),
                    expires_at: Some("2027-01-01T00:00:00Z".into()),
                },
                VaultCertificateSummary {
                    id: "id-2".into(),
                    name: "bare".into(),
                    exportable: false,
                    enabled: true,
                    key_algorithm: String::new(),
                    expires_at: None,
                },
            ]
        );
    }

    #[test]
    fn is_last_page_stops_on_a_short_page_or_the_total() {
        assert!(is_last_page(3, 3, None));
        assert!(is_last_page(0, 400, None));
        assert!(!is_last_page(LIST_PAGE_SIZE, LIST_PAGE_SIZE, None));
        assert!(is_last_page(LIST_PAGE_SIZE, LIST_PAGE_SIZE, Some(LIST_PAGE_SIZE)));
        assert!(!is_last_page(LIST_PAGE_SIZE, LIST_PAGE_SIZE, Some(LIST_PAGE_SIZE + 1)));
    }

    #[test]
    fn list_failed_names_the_problem() {
        assert!(list_failed(StatusCode::FORBIDDEN)
            .to_string()
            .contains("cannot list certificates"));
        assert!(matches!(
            list_failed(StatusCode::NOT_FOUND),
            DomainError::NotFound(_)
        ));
        assert!(list_failed(StatusCode::BAD_GATEWAY).to_string().contains("502"));
    }

    #[test]
    fn parse_export_pem_returns_both_pieces() {
        let body = serde_json::to_vec(&serde_json::json!({
            "id": "id-1", "name": "client-a", "version": 3, "key_algorithm": "RSA-2048",
            "certificate_pem": CERT_PEM, "private_key_pem": KEY_PEM
        }))
        .expect("json");
        match parse_export(&body, VaultCertificateFormat::Pem, None).expect("material") {
            VaultCertificateMaterial::Pem {
                certificate,
                private_key,
                key_algorithm,
            } => {
                assert_eq!(certificate.as_slice(), CERT_PEM.as_bytes());
                assert_eq!(private_key.as_slice(), KEY_PEM.as_bytes());
                assert_eq!(key_algorithm, "RSA-2048");
            }
            other => panic!("expected PEM, got {other:?}"),
        }
    }

    #[test]
    fn parse_export_pkcs12_decodes_wrapped_base64_and_keeps_the_password() {
        let body = serde_json::to_vec(&serde_json::json!({
            "key_algorithm": "EC-P256", "pkcs12_base64": "AQID\nBAU="
        }))
        .expect("json");
        let password = Zeroizing::new("one-time-pass-123".to_string());
        match parse_export(&body, VaultCertificateFormat::Pkcs12, Some(password))
            .expect("material")
        {
            VaultCertificateMaterial::Pkcs12 {
                bundle, password, ..
            } => {
                assert_eq!(bundle.as_slice(), &[1, 2, 3, 4, 5]);
                assert_eq!(password.as_str(), "one-time-pass-123");
            }
            other => panic!("expected PKCS12, got {other:?}"),
        }
    }

    #[test]
    fn parse_export_rejects_a_pem_export_without_the_key() {
        let body = serde_json::to_vec(&serde_json::json!({ "certificate_pem": CERT_PEM }))
            .expect("json");
        let err = parse_export(&body, VaultCertificateFormat::Pem, None)
            .expect_err("a PEM export needs both pieces");
        assert!(err.to_string().contains("private key"), "{err}");
    }

    #[test]
    fn parse_export_errors_never_quote_the_body() {
        let body = b"{\"private_key_pem\": \"-----BEGIN PRIVATE KEY-----\\nc2VjcmV0\" ,,, }";
        let err = parse_export(body, VaultCertificateFormat::Pem, None)
            .expect_err("malformed JSON")
            .to_string();
        assert!(!err.contains("c2VjcmV0") && !err.contains("BEGIN"), "{err}");
    }

    #[test]
    fn classify_maps_each_status_and_code() {
        let json = |code: &str| {
            format!(r#"{{"error":{{"code":"{code}","message":"from RocketVault"}}}}"#)
                .into_bytes()
        };
        let failed = |status: u16, body: &[u8]| {
            let status = StatusCode::from_u16(status).expect("status");
            match classify_export_error(status, body) {
                ExportFailure::Failed(err) => err.to_string(),
                _ => panic!("expected Failed for {status}"),
            }
        };
        assert!(matches!(
            classify_export_error(StatusCode::UNAUTHORIZED, b"Unauthorized"),
            ExportFailure::TokenRejected
        ));
        assert!(matches!(
            classify_export_error(StatusCode::NOT_FOUND, &json("not_found")),
            ExportFailure::NotFound
        ));
        assert!(failed(403, &json("certificate_not_exportable")).contains(NOT_EXPORTABLE));
        assert!(failed(403, b"Forbidden").contains(MISSING_ROLE));
        assert!(failed(403, &json("vault_forbidden")).contains("vault_forbidden"));
        assert!(failed(409, &json("certificate_disabled")).contains(DISABLED));
        assert!(failed(400, &json("bad_request")).contains("400"));
        assert!(failed(500, &json("internal_error")).contains("500"));
    }
}
```

In `crates/rocket-infra/src/rocketvault/mod.rs`, after line 6 (`use serde::Deserialize;`), add:

```rust

// The certificate contract. Its export half is first used in Task B3, which removes this allow.
#[allow(dead_code)]
mod certificate_api;
```

- [ ] **Step 6: Run the contract tests to verify they fail**

Run: `cargo test -j4 -p rocket-infra certificate_api`
Expected: FAIL to compile with `cannot find function list_url in this scope` (and the other contract items).

- [ ] **Step 7: Implement the contract**

In `crates/rocket-infra/src/rocketvault/certificate_api.rs`, insert between `use super::vault_api_url;` and `#[cfg(test)]`:

```rust
/// Page size asked for when listing certificates. RocketVault allows up to 200.
pub(super) const LIST_PAGE_SIZE: usize = 200;
/// The list walk stops here, so a server that never reports a last page cannot loop forever.
pub(super) const MAX_LIST_PAGES: usize = 100;
/// Largest export response read. A PEM chain with its key, or a PKCS12 bundle, is a few KiB.
pub(super) const MAX_EXPORT_BYTES: usize = 1024 * 1024;
/// Largest error response read. Only its error code is used.
pub(super) const MAX_ERROR_BYTES: usize = 64 * 1024;
/// Length of the one-time PKCS12 password.
pub(super) const PKCS12_PASSWORD_LEN: usize = 32;

pub(super) const NOT_FOUND: &str = "Certificate not found in this vault.";
pub(super) const NOT_EXPORTABLE: &str = "Certificate is not marked exportable.";
pub(super) const MISSING_ROLE: &str = "The service account lacks the Certificate Exporter role.";
pub(super) const DISABLED: &str = "Certificate is disabled.";
pub(super) const TOKEN_REJECTED: &str = "RocketVault rejected the access token (401).";

/// `GET /api/v1/vaults/{vault}/certificates?page={page}&per_page=200`. Pages count from 0.
/// The route has no name filter.
pub(super) fn list_url(
    connection: &SecretManagerConnection,
    vault_name: &str,
    page: usize,
) -> DomainResult<url::Url> {
    let mut url = vault_api_url(
        connection,
        &["api", "v1", "vaults", vault_name, "certificates"],
    )?;
    url.query_pairs_mut()
        .append_pair("page", &page.to_string())
        .append_pair("per_page", &LIST_PAGE_SIZE.to_string());
    Ok(url)
}

/// `POST /api/v1/vaults/{vault}/certificates/{id}/export`. The route takes the id, not the name.
pub(super) fn export_url(
    connection: &SecretManagerConnection,
    vault_name: &str,
    certificate_id: &str,
) -> DomainResult<url::Url> {
    vault_api_url(
        connection,
        &[
            "api",
            "v1",
            "vaults",
            vault_name,
            "certificates",
            certificate_id,
            "export",
        ],
    )
}

fn default_true() -> bool {
    true
}

/// One list entry. Assumed shape, mirroring the secret list; confirm against RocketVault
/// v-4.0.0. Unknown fields (`not_before`, `version`, ...) are ignored.
#[derive(Deserialize)]
struct RawCertificateSummary {
    id: String,
    name: String,
    #[serde(default)]
    exportable: bool,
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    key_algorithm: String,
    #[serde(default)]
    expires_at: Option<String>,
}

/// One list page: `{"certificates": [...], "total": N}`. Assumed like the secret list.
#[derive(Deserialize)]
struct RawCertificatePage {
    #[serde(default)]
    certificates: Vec<RawCertificateSummary>,
    #[serde(default)]
    total: Option<usize>,
}

/// One decoded list page.
pub(super) struct CertificatePage {
    pub certificates: Vec<VaultCertificateSummary>,
    pub total: Option<usize>,
}

/// Decodes a list page. A list never carries key material, so the decoder's message is kept.
pub(super) fn parse_certificate_page(body: &[u8]) -> DomainResult<CertificatePage> {
    let raw: RawCertificatePage = serde_json::from_slice(body).map_err(|e| {
        DomainError::Http(format!("failed to decode RocketVault certificate list: {e}"))
    })?;
    Ok(CertificatePage {
        certificates: raw
            .certificates
            .into_iter()
            .map(|c| VaultCertificateSummary {
                id: c.id,
                name: c.name,
                exportable: c.exportable,
                enabled: c.enabled,
                key_algorithm: c.key_algorithm,
                expires_at: c.expires_at,
            })
            .collect(),
        total: raw.total,
    })
}

/// True when the page just read is the last one: it was short, or `seen` reached `total`.
pub(super) fn is_last_page(page_len: usize, seen: usize, total: Option<usize>) -> bool {
    page_len < LIST_PAGE_SIZE || total.is_some_and(|t| seen >= t)
}

/// A failed list call as a user-facing error. A 401 is handled by the caller.
pub(super) fn list_failed(status: StatusCode) -> DomainError {
    match status.as_u16() {
        403 => DomainError::Http(
            "The service account cannot list certificates in this vault (403).".to_string(),
        ),
        404 => DomainError::NotFound("RocketVault has no vault with this name.".to_string()),
        other => DomainError::Http(format!(
            "RocketVault returned status {other} while listing certificates."
        )),
    }
}

/// The JSON body of an export request.
#[derive(Serialize)]
struct ExportRequest<'a> {
    format: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    compat: Option<&'static str>,
}

/// The body of an export. PEM sends only the format. PKCS12 sends the one-time password and
/// `compat: legacy`, which every platform TLS stack can open. The bytes are wiped on drop.
pub(super) fn export_request_body(
    format: VaultCertificateFormat,
    password: Option<&str>,
) -> DomainResult<Zeroizing<Vec<u8>>> {
    let request = match format {
        VaultCertificateFormat::Pem => ExportRequest {
            format: "pem",
            password: None,
            compat: None,
        },
        VaultCertificateFormat::Pkcs12 => ExportRequest {
            format: "pkcs12",
            password: Some(password.ok_or_else(|| {
                DomainError::Internal("a PKCS12 export needs a password".to_string())
            })?),
            compat: Some("legacy"),
        },
    };
    serde_json::to_vec(&request)
        .map(Zeroizing::new)
        .map_err(|_| DomainError::Internal("could not build the export request".to_string()))
}

/// A successful export response. Every secret string moves into `Zeroizing` right after
/// parsing. Unknown fields (`id`, `name`, `version`, `not_before`, `expires_at`) are ignored.
#[derive(Deserialize)]
struct RawExport {
    #[serde(default)]
    key_algorithm: String,
    #[serde(default)]
    certificate_pem: Option<String>,
    #[serde(default)]
    private_key_pem: Option<String>,
    #[serde(default)]
    pkcs12_base64: Option<String>,
}

/// Turns a successful export response into material. A decoder message can quote the input,
/// so every parse error is replaced by a fixed message.
pub(super) fn parse_export(
    body: &[u8],
    format: VaultCertificateFormat,
    password: Option<Zeroizing<String>>,
) -> DomainResult<VaultCertificateMaterial> {
    let RawExport {
        key_algorithm,
        certificate_pem,
        private_key_pem,
        pkcs12_base64,
    } = serde_json::from_slice::<RawExport>(body).map_err(|_| {
        DomainError::Http("RocketVault returned a certificate export Rocket cannot read.".into())
    })?;
    let certificate_pem = certificate_pem.map(Zeroizing::new);
    let private_key_pem = private_key_pem.map(Zeroizing::new);
    let pkcs12_base64 = pkcs12_base64.map(Zeroizing::new);

    match format {
        VaultCertificateFormat::Pem => {
            let certificate = certificate_pem.filter(|c| !c.trim().is_empty());
            let private_key = private_key_pem.filter(|k| !k.trim().is_empty());
            let (Some(certificate), Some(private_key)) = (certificate, private_key) else {
                return Err(DomainError::Http(
                    "RocketVault returned a PEM export without the certificate or the private key."
                        .into(),
                ));
            };
            Ok(VaultCertificateMaterial::Pem {
                certificate: Zeroizing::new(certificate.as_bytes().to_vec()),
                private_key: Zeroizing::new(private_key.as_bytes().to_vec()),
                key_algorithm,
            })
        }
        VaultCertificateFormat::Pkcs12 => {
            let Some(encoded) = pkcs12_base64 else {
                return Err(DomainError::Http(
                    "RocketVault returned a PKCS12 export without the bundle.".into(),
                ));
            };
            let Some(password) = password else {
                return Err(DomainError::Internal(
                    "a PKCS12 export needs its password".into(),
                ));
            };
            // Whitespace, including line breaks, is ignored, like for vault-secret bundles.
            let compact: Zeroizing<String> =
                Zeroizing::new(encoded.chars().filter(|c| !c.is_whitespace()).collect());
            let bundle = STANDARD
                .decode(compact.as_bytes())
                .map(Zeroizing::new)
                .map_err(|_| {
                    DomainError::Http(
                        "RocketVault returned a PKCS12 bundle that is not valid base64.".into(),
                    )
                })?;
            Ok(VaultCertificateMaterial::Pkcs12 {
                bundle,
                password,
                key_algorithm,
            })
        }
    }
}

/// `{"error": {"code": ..., "message": ...}}`. Only the code is read.
#[derive(Deserialize)]
struct RawErrorEnvelope {
    error: RawErrorBody,
}

#[derive(Deserialize)]
struct RawErrorBody {
    code: String,
}

/// What a failed export means for the caller.
pub(super) enum ExportFailure {
    /// 401: the cached token is stale. The caller evicts it and fails the request.
    TokenRejected,
    /// 404: the cached id may be stale. The caller looks the name up again and retries once.
    NotFound,
    /// Anything else, as the user-facing error.
    Failed(DomainError),
}

/// Maps a failed export to its meaning. A 401 and a missing-role 403 come from middleware with
/// no JSON body, so they are read by status alone.
pub(super) fn classify_export_error(status: StatusCode, body: &[u8]) -> ExportFailure {
    let code = serde_json::from_slice::<RawErrorEnvelope>(body)
        .ok()
        .map(|e| e.error.code.chars().take(64).collect::<String>());
    match (status.as_u16(), code.as_deref()) {
        (401, _) => ExportFailure::TokenRejected,
        (404, _) => ExportFailure::NotFound,
        (403, Some("certificate_not_exportable")) => {
            ExportFailure::Failed(DomainError::InvalidInput(NOT_EXPORTABLE.to_string()))
        }
        (403, None) => ExportFailure::Failed(DomainError::Http(MISSING_ROLE.to_string())),
        (403, Some(other)) => ExportFailure::Failed(DomainError::Http(format!(
            "RocketVault refused the export (403, {other})."
        ))),
        (409, _) => ExportFailure::Failed(DomainError::InvalidInput(DISABLED.to_string())),
        (other, _) => ExportFailure::Failed(DomainError::Http(format!(
            "RocketVault returned status {other} for the certificate export."
        ))),
    }
}

```

- [ ] **Step 8: Run the contract tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra certificate_api`
Expected: PASS, 12 tests.

- [ ] **Step 9: Workspace check and clippy**

Run: `cargo check -j4 --workspace`
Expected: `Finished`, no errors.

Run: `cargo clippy -j4 -p rocket-environment --all-targets` and `cargo clippy -j4 -p rocket-infra --all-targets`
Expected: no warnings in `vault_secret_fetcher.rs` or `rocketvault/`.

- [ ] **Step 10: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add crates/rocket-environment/Cargo.toml Cargo.lock crates/rocket-environment/src/vault_secret_fetcher.rs crates/rocket-environment/src/lib.rs crates/rocket-infra/src/rocketvault/certificate_api.rs crates/rocket-infra/src/rocketvault/mod.rs
git commit -- crates/rocket-environment/Cargo.toml Cargo.lock crates/rocket-environment/src/vault_secret_fetcher.rs crates/rocket-environment/src/lib.rs crates/rocket-infra/src/rocketvault/certificate_api.rs crates/rocket-infra/src/rocketvault/mod.rs
```

Suggested subject: `feat(vault): add the RocketVault certificate contract and fetcher methods`. The message ends with `Relates to: #21`.

---

## Task B2: Paged certificate list and the name-to-id cache

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** sonnet.

**Files:**
- Create: `crates/rocket-infra/src/rocketvault/certificates.rs`
- Modify: `crates/rocket-infra/src/rocketvault/mod.rs` (module declaration next to B1's; `ReqwestVaultSecretFetcher` fields lines 87-99 and `new()` lines 127-131; the `use` at line 336; trait impl lines 363-485)

**Interfaces:**
- Consumes: B1's `certificate_api::{list_url, parse_certificate_page, is_last_page, list_failed, LIST_PAGE_SIZE, MAX_LIST_PAGES, NOT_FOUND, TOKEN_REJECTED}`; `ensure_token`, `client_for`, `tokens`.
- Produces (contract names): field `certificate_ids: DashMap<String, String>`; `list_certificate_pages`, `remember_certificate_ids`, `certificate_id`, `forget_certificate_id`; `VaultSecretFetcher::list_certificates` for `ReqwestVaultSecretFetcher`. Consumed by B3, C3.

- [ ] **Step 1: Write the failing list and cache tests**

Create `crates/rocket-infra/src/rocketvault/certificates.rs` with only the module doc and the tests:

```rust
//! Certificate calls on top of the contract in `certificate_api.rs`: the paged list walk, the
//! name-to-id cache and the export. Only ids are cached. Material never is.

#[cfg(test)]
mod tests {
    use super::super::ReqwestVaultSecretFetcher;
    use rocket_environment::{SecretManagerConnection, VaultCertificateSummary, VaultSecretFetcher};
    use rocket_shared::error::DomainError;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    const LIST_PATH: &str = "/api/v1/vaults/prod-vault/certificates";

    fn conn(base_url: String) -> SecretManagerConnection {
        SecretManagerConnection {
            id: "conn-1".into(),
            label: "Test".into(),
            base_url,
            client_id: "rocketapi".into(),
            verify_ssl: true,
            allow_insecure_http: true, // the mock server is http://127.0.0.1:<port>
        }
    }

    async fn mount_token(server: &MockServer) {
        Mock::given(method("POST"))
            .and(path("/api/v1/oauth2/token"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({
                "access_token": "tok-1",
                "expires_in": 300
            })))
            .mount(server)
            .await;
    }

    fn entry(id: &str, name: &str) -> serde_json::Value {
        json!({
            "id": id, "name": name, "exportable": true, "enabled": true,
            "key_algorithm": "RSA-2048"
        })
    }

    fn filler(prefix: &str, count: usize) -> Vec<serde_json::Value> {
        (0..count)
            .map(|i| entry(&format!("{prefix}-id-{i}"), &format!("{prefix}-{i}")))
            .collect()
    }

    fn page_of(entries: Vec<serde_json::Value>, total: Option<usize>) -> ResponseTemplate {
        let mut body = json!({ "certificates": entries });
        if let Some(total) = total {
            body["total"] = json!(total);
        }
        ResponseTemplate::new(200).set_body_json(body)
    }

    async fn mount_page(server: &MockServer, page: usize, response: ResponseTemplate, calls: u64) {
        Mock::given(method("GET"))
            .and(path(LIST_PATH))
            .and(query_param("page", page.to_string()))
            .and(query_param("per_page", "200"))
            .respond_with(response)
            .expect(calls)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn list_certificates_maps_fields_and_reads_one_short_page() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        let body = json!({
            "certificates": [{
                "id": "id-1", "name": "client-a", "exportable": false, "enabled": true,
                "key_algorithm": "EC-P256", "expires_at": "2027-01-01T00:00:00Z"
            }],
            "total": 1
        });
        mount_page(&server, 0, ResponseTemplate::new(200).set_body_json(body), 1).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let listed = fetcher
            .list_certificates(&conn(server.uri()), "shh", "prod-vault")
            .await
            .expect("list");
        assert_eq!(
            listed,
            vec![VaultCertificateSummary {
                id: "id-1".into(),
                name: "client-a".into(),
                exportable: false,
                enabled: true,
                key_algorithm: "EC-P256".into(),
                expires_at: Some("2027-01-01T00:00:00Z".into()),
            }]
        );
    }

    #[tokio::test]
    async fn list_certificates_walks_every_page_until_a_short_one() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_page(&server, 0, page_of(filler("a", 200), None), 1).await;
        mount_page(&server, 1, page_of(filler("b", 3), None), 1).await;
        mount_page(&server, 2, page_of(Vec::new(), None), 0).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let listed = fetcher
            .list_certificates(&conn(server.uri()), "shh", "prod-vault")
            .await
            .expect("list");
        assert_eq!(listed.len(), 203);
    }

    // Review Focus 1.
    #[tokio::test]
    async fn certificate_id_walks_pages_until_the_name_is_found_on_page_three() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_page(&server, 0, page_of(filler("a", 200), Some(650)), 1).await;
        mount_page(&server, 1, page_of(filler("b", 200), Some(650)), 1).await;
        let mut third = filler("c", 200);
        third[150] = entry("wanted-id", "client-a");
        mount_page(&server, 2, page_of(third, Some(650)), 1).await;
        // The walk stops at the page that holds the name.
        mount_page(&server, 3, page_of(filler("d", 50), Some(650)), 0).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let id = fetcher
            .certificate_id(&conn(server.uri()), "shh", "prod-vault", "client-a", false)
            .await
            .expect("found on page three");
        assert_eq!(id, "wanted-id");
    }

    #[tokio::test]
    async fn a_cached_id_makes_no_list_call() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_page(&server, 0, page_of(vec![entry("id-1", "client-a")], Some(1)), 1).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let c = conn(server.uri());
        for _ in 0..2 {
            let id = fetcher
                .certificate_id(&c, "shh", "prod-vault", "client-a", false)
                .await
                .expect("id");
            assert_eq!(id, "id-1");
        }
    }

    #[tokio::test]
    async fn a_fresh_lookup_replaces_the_cached_id() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path(LIST_PATH))
            .respond_with(page_of(vec![entry("old-id", "client-a")], Some(1)))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(LIST_PATH))
            .respond_with(page_of(vec![entry("new-id", "client-a")], Some(1)))
            .expect(1)
            .mount(&server)
            .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let c = conn(server.uri());
        let first = fetcher
            .certificate_id(&c, "shh", "prod-vault", "client-a", false)
            .await
            .expect("first");
        assert_eq!(first, "old-id");
        let fresh = fetcher
            .certificate_id(&c, "shh", "prod-vault", "client-a", true)
            .await
            .expect("fresh");
        assert_eq!(fresh, "new-id");
        let cached = fetcher
            .certificate_id(&c, "shh", "prod-vault", "client-a", false)
            .await
            .expect("cached");
        assert_eq!(cached, "new-id");
    }

    #[tokio::test]
    async fn a_name_missing_from_the_vault_is_not_found_and_not_cached() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_page(&server, 0, page_of(vec![entry("id-1", "other")], Some(1)), 2).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let c = conn(server.uri());
        for _ in 0..2 {
            let err = fetcher
                .certificate_id(&c, "shh", "prod-vault", "client-a", false)
                .await
                .expect_err("not in the vault");
            assert_eq!(
                err,
                DomainError::NotFound("Certificate not found in this vault.".into())
            );
        }
    }

    #[tokio::test]
    async fn listing_certificates_fills_the_id_cache() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_page(&server, 0, page_of(vec![entry("id-1", "client-a")], Some(1)), 1).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let c = conn(server.uri());
        fetcher
            .list_certificates(&c, "shh", "prod-vault")
            .await
            .expect("list");
        let id = fetcher
            .certificate_id(&c, "shh", "prod-vault", "client-a", false)
            .await
            .expect("cached by the list");
        assert_eq!(id, "id-1");
    }

    // Spec risk 3: the cache is cleared when a connection changes.
    #[tokio::test]
    async fn a_changed_base_url_does_not_reuse_a_cached_id() {
        let first = MockServer::start().await;
        mount_token(&first).await;
        mount_page(&first, 0, page_of(vec![entry("id-on-first", "client-a")], Some(1)), 1).await;
        let second = MockServer::start().await;
        mount_token(&second).await;
        mount_page(&second, 0, page_of(vec![entry("id-on-second", "client-a")], Some(1)), 1).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let before = conn(first.uri());
        let mut after = before.clone();
        after.base_url = second.uri();
        assert_eq!(
            fetcher
                .certificate_id(&before, "shh", "prod-vault", "client-a", false)
                .await
                .expect("first"),
            "id-on-first"
        );
        assert_eq!(
            fetcher
                .certificate_id(&after, "shh", "prod-vault", "client-a", false)
                .await
                .expect("second"),
            "id-on-second"
        );
    }

    #[tokio::test]
    async fn list_401_evicts_the_token() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_page(&server, 0, ResponseTemplate::new(401), 1).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let c = conn(server.uri());
        let err = fetcher
            .list_certificates(&c, "shh", "prod-vault")
            .await
            .expect_err("401");
        assert!(err.to_string().contains("401"), "{err}");
        assert!(fetcher.tokens.get(&c.id).is_none());
    }

    #[tokio::test]
    async fn list_403_says_the_account_cannot_list() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_page(&server, 0, ResponseTemplate::new(403), 1).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let err = fetcher
            .list_certificates(&conn(server.uri()), "shh", "prod-vault")
            .await
            .expect_err("403");
        assert!(err.to_string().contains("cannot list certificates"), "{err}");
    }
}
```

In `crates/rocket-infra/src/rocketvault/mod.rs`, after B1's `mod certificate_api;`, add:

```rust
// `certificate_id` and `forget_certificate_id` are first used outside tests in Task B3, which
// removes this allow.
#[allow(dead_code)]
mod certificates;
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-infra rocketvault::certificates`
Expected: FAIL to compile with `no method named certificate_id found for struct ReqwestVaultSecretFetcher` (and `list_certificates` uses the trait default, which is not reached because compilation stops first).

- [ ] **Step 3: Add the id cache field**

In `crates/rocket-infra/src/rocketvault/mod.rs`, the struct (line 98), old:

```rust
    http_insecure: reqwest::Client,
    tokens: DashMap<String, TokenCache>,
}
```

new:

```rust
    http_insecure: reqwest::Client,
    tokens: DashMap<String, TokenCache>,
    /// Certificate name to id, per connection and vault (see `certificates.rs`). An id is not a
    /// secret. Certificate material is never cached.
    certificate_ids: DashMap<String, String>,
}
```

and in `new()`, old:

```rust
        Self {
            http,
            http_insecure,
            tokens: DashMap::new(),
        }
```

new:

```rust
        Self {
            http,
            http_insecure,
            tokens: DashMap::new(),
            certificate_ids: DashMap::new(),
        }
```

- [ ] **Step 4: Implement the list walk and the id cache**

In `crates/rocket-infra/src/rocketvault/certificates.rs`, insert between the module doc and `#[cfg(test)]`:

```rust

use rocket_environment::{SecretManagerConnection, VaultCertificateSummary};
use rocket_shared::error::{DomainError, DomainResult};

use super::certificate_api::{self, LIST_PAGE_SIZE, MAX_LIST_PAGES};
use super::ReqwestVaultSecretFetcher;

impl ReqwestVaultSecretFetcher {
    /// Walks the certificate list page by page. With `until`, the walk stops after the page that
    /// holds that name, because the route has no name filter.
    pub(super) async fn list_certificate_pages(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
        until: Option<&str>,
    ) -> DomainResult<Vec<VaultCertificateSummary>> {
        let token = self.ensure_token(connection, client_secret).await?;
        let client = self.client_for(connection);
        let mut all: Vec<VaultCertificateSummary> = Vec::new();
        for page in 0..MAX_LIST_PAGES {
            let url = certificate_api::list_url(connection, vault_name, page)?;
            let resp = client
                .get(url)
                .bearer_auth(&token)
                .send()
                .await
                .map_err(|e| {
                    DomainError::Http(format!("RocketVault certificate list request failed: {e}"))
                })?;
            let status = resp.status();
            if status == reqwest::StatusCode::UNAUTHORIZED {
                self.tokens.remove(&connection.id);
                return Err(DomainError::Http(certificate_api::TOKEN_REJECTED.to_string()));
            }
            if !status.is_success() {
                return Err(certificate_api::list_failed(status));
            }
            let body = resp.bytes().await.map_err(|e| {
                DomainError::Http(format!("RocketVault certificate list could not be read: {e}"))
            })?;
            let parsed = certificate_api::parse_certificate_page(&body)?;
            let page_len = parsed.certificates.len();
            let found = until.is_some_and(|name| parsed.certificates.iter().any(|c| c.name == name));
            all.extend(parsed.certificates);
            if found || certificate_api::is_last_page(page_len, all.len(), parsed.total) {
                return Ok(all);
            }
        }
        Err(DomainError::Http(format!(
            "RocketVault listed more than {} certificates; Rocket stopped reading.",
            MAX_LIST_PAGES * LIST_PAGE_SIZE
        )))
    }

    /// Caches the id of every listed certificate, so a send right after the picker loaded needs
    /// no list call.
    pub(super) fn remember_certificate_ids(
        &self,
        connection: &SecretManagerConnection,
        vault_name: &str,
        listed: &[VaultCertificateSummary],
    ) {
        for cert in listed {
            self.certificate_ids.insert(
                certificate_id_key(connection, vault_name, &cert.name),
                cert.id.clone(),
            );
        }
    }

    /// The id of the certificate called `name`: from the cache, unless `fresh` is set or there
    /// is none, and then from the list. A name that is not listed is "not found" and is not
    /// cached.
    pub(super) async fn certificate_id(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
        name: &str,
        fresh: bool,
    ) -> DomainResult<String> {
        let key = certificate_id_key(connection, vault_name, name);
        if fresh {
            self.certificate_ids.remove(&key);
        } else if let Some(id) = self.certificate_ids.get(&key) {
            return Ok(id.value().clone());
        }
        let listed = self
            .list_certificate_pages(connection, client_secret, vault_name, Some(name))
            .await?;
        let Some(found) = listed.into_iter().find(|c| c.name == name) else {
            return Err(DomainError::NotFound(certificate_api::NOT_FOUND.to_string()));
        };
        self.certificate_ids.insert(key, found.id.clone());
        Ok(found.id)
    }

    /// Drops the cached id of `name`, for a certificate that is gone.
    pub(super) fn forget_certificate_id(
        &self,
        connection: &SecretManagerConnection,
        vault_name: &str,
        name: &str,
    ) {
        self.certificate_ids
            .remove(&certificate_id_key(connection, vault_name, name));
    }
}

/// Cache key for a certificate id. The connection's id and base URL are part of it, so an
/// edited connection never reuses an id read through the old one.
fn certificate_id_key(connection: &SecretManagerConnection, vault_name: &str, name: &str) -> String {
    [
        connection.id.as_str(),
        connection.base_url.as_str(),
        vault_name,
        name,
    ]
    .join("\u{1f}")
}

```

In `crates/rocket-infra/src/rocketvault/mod.rs`, line 336, old:

```rust
use rocket_environment::{ExternalSecretRef, VaultSecretFetcher};
```

new:

```rust
use rocket_environment::{ExternalSecretRef, VaultCertificateSummary, VaultSecretFetcher};
```

In `impl VaultSecretFetcher for ReqwestVaultSecretFetcher`, after `test_connection` (before the impl's closing `}` on line 485), add:

```rust

    async fn list_certificates(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<Vec<VaultCertificateSummary>> {
        let listed = self
            .list_certificate_pages(connection, client_secret, vault_name, None)
            .await?;
        self.remember_certificate_ids(connection, vault_name, &listed);
        Ok(listed)
    }
```

- [ ] **Step 5: Run the list and cache tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra rocketvault::certificates`
Expected: PASS, 10 tests.

Run: `cargo test -j4 -p rocket-infra rocketvault`
Expected: PASS, every existing `rocketvault::tests` test still passes.

- [ ] **Step 6: Workspace check and clippy**

Run: `cargo check -j4 --workspace`
Expected: `Finished`, no errors.

Run: `cargo clippy -j4 -p rocket-infra --all-targets`
Expected: no warnings in `rocketvault/`.

- [ ] **Step 7: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add crates/rocket-infra/src/rocketvault/certificates.rs crates/rocket-infra/src/rocketvault/mod.rs
git commit -- crates/rocket-infra/src/rocketvault/certificates.rs crates/rocket-infra/src/rocketvault/mod.rs
```

Suggested subject: `feat(vault): list RocketVault certificates and cache their ids`. The message ends with `Relates to: #21`.

---

## Task B3: Certificate export: one-time password, size cap, 404 retry, error mapping

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** opus (fetch, zeroize and password handling).

**Files:**
- Modify: `crates/rocket-infra/Cargo.toml` (add `rand = "0.8"` after `zeroize = "1"` on line 44)
- Modify: `Cargo.lock` (updated by cargo)
- Modify: `crates/rocket-infra/src/rocketvault/certificates.rs` (imports, new `impl` block and helpers, tests)
- Modify: `crates/rocket-infra/src/rocketvault/mod.rs` (remove both `#[allow(dead_code)]` lines from B1 and B2; imports at line 336; trait impl)

**Interfaces:**
- Consumes: B1's `certificate_api::{export_url, export_request_body, parse_export, classify_export_error, ExportFailure, MAX_EXPORT_BYTES, MAX_ERROR_BYTES, PKCS12_PASSWORD_LEN, NOT_FOUND, TOKEN_REJECTED}`; B2's `certificate_id`, `forget_certificate_id`.
- Produces (contract names): `export_certificate`; `VaultSecretFetcher::fetch_certificate` for `ReqwestVaultSecretFetcher`. Consumed by C1.

- [ ] **Step 1: Write the failing export tests**

In `crates/rocket-infra/src/rocketvault/certificates.rs`, `mod tests`, extend the imports, old:

```rust
    use super::super::ReqwestVaultSecretFetcher;
    use rocket_environment::{SecretManagerConnection, VaultCertificateSummary, VaultSecretFetcher};
    use rocket_shared::error::DomainError;
    use serde_json::json;
    use wiremock::matchers::{method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};
```

new:

```rust
    use super::super::certificate_api::{MAX_EXPORT_BYTES, PKCS12_PASSWORD_LEN};
    use super::super::ReqwestVaultSecretFetcher;
    use rocket_environment::{
        SecretManagerConnection, VaultCertificateMaterial, VaultCertificateSummary,
        VaultSecretFetcher,
    };
    use rocket_shared::certificate::VaultCertificateFormat;
    use rocket_shared::error::DomainError;
    use serde_json::json;
    use wiremock::matchers::{header, method, path, query_param};
    use wiremock::{Mock, MockServer, ResponseTemplate};
```

Add at the end of `mod tests`:

```rust

    const CERT_PEM: &str = "-----BEGIN CERTIFICATE-----\nZmFrZS1jZXJ0\n-----END CERTIFICATE-----\n";
    const KEY_PEM: &str = "-----BEGIN PRIVATE KEY-----\nZmFrZS1rZXk=\n-----END PRIVATE KEY-----\n";

    fn export_path(id: &str) -> String {
        format!("{LIST_PATH}/{id}/export")
    }

    fn pem_export() -> ResponseTemplate {
        ResponseTemplate::new(200)
            .insert_header("Cache-Control", "no-store")
            .set_body_json(json!({
                "id": "id-1", "name": "client-a", "version": 1,
                "not_before": "2026-01-01T00:00:00Z", "expires_at": "2027-01-01T00:00:00Z",
                "key_algorithm": "RSA-2048",
                "certificate_pem": CERT_PEM, "private_key_pem": KEY_PEM
            }))
    }

    fn pkcs12_export() -> ResponseTemplate {
        ResponseTemplate::new(200)
            .insert_header("Cache-Control", "no-store")
            .set_body_json(json!({
                "id": "id-1", "name": "client-a", "version": 1,
                "key_algorithm": "EC-P256", "pkcs12_base64": "AQID\nBAU="
            }))
    }

    fn rocketvault_error(status: u16, code: &str) -> ResponseTemplate {
        ResponseTemplate::new(status).set_body_json(json!({
            "error": { "code": code, "message": "from RocketVault" }
        }))
    }

    /// The token endpoint plus a one-entry list (`client-a` is `id-1`), answered `list_calls`
    /// times.
    async fn with_client_a(server: &MockServer, list_calls: u64) {
        mount_token(server).await;
        mount_page(server, 0, page_of(vec![entry("id-1", "client-a")], Some(1)), list_calls).await;
    }

    async fn mount_export(server: &MockServer, id: &str, response: ResponseTemplate, calls: u64) {
        Mock::given(method("POST"))
            .and(path(export_path(id)))
            .and(header("authorization", "Bearer tok-1"))
            .respond_with(response)
            .expect(calls)
            .mount(server)
            .await;
    }

    async fn export_bodies(server: &MockServer) -> Vec<serde_json::Value> {
        server
            .received_requests()
            .await
            .expect("requests recorded")
            .iter()
            .filter(|r| r.url.path().ends_with("/export"))
            .map(|r| serde_json::from_slice(&r.body).expect("JSON export body"))
            .collect()
    }

    async fn fetch(
        fetcher: &ReqwestVaultSecretFetcher,
        server: &MockServer,
        format: VaultCertificateFormat,
    ) -> Result<VaultCertificateMaterial, DomainError> {
        fetcher
            .fetch_certificate(&conn(server.uri()), "shh", "prod-vault", "client-a", format)
            .await
    }

    #[tokio::test]
    async fn a_pem_export_returns_the_chain_and_key_and_sends_no_password() {
        let server = MockServer::start().await;
        with_client_a(&server, 1).await;
        mount_export(&server, "id-1", pem_export(), 1).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        match fetch(&fetcher, &server, VaultCertificateFormat::Pem).await.expect("export") {
            VaultCertificateMaterial::Pem {
                certificate,
                private_key,
                key_algorithm,
            } => {
                assert_eq!(certificate.as_slice(), CERT_PEM.as_bytes());
                assert_eq!(private_key.as_slice(), KEY_PEM.as_bytes());
                assert_eq!(key_algorithm, "RSA-2048");
            }
            other => panic!("expected PEM, got {other:?}"),
        }
        assert_eq!(export_bodies(&server).await, vec![json!({ "format": "pem" })]);
    }

    // Spec decision 3 and section 10.
    #[tokio::test]
    async fn each_pkcs12_export_uses_a_new_random_password_with_legacy_compat() {
        let server = MockServer::start().await;
        with_client_a(&server, 1).await;
        mount_export(&server, "id-1", pkcs12_export(), 2).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let first = fetch(&fetcher, &server, VaultCertificateFormat::Pkcs12)
            .await
            .expect("first export");
        let second = fetch(&fetcher, &server, VaultCertificateFormat::Pkcs12)
            .await
            .expect("second export");

        let sent: Vec<String> = export_bodies(&server)
            .await
            .iter()
            .map(|body| {
                assert_eq!(body["format"], "pkcs12");
                assert_eq!(body["compat"], "legacy");
                body["password"].as_str().expect("a password").to_string()
            })
            .collect();
        assert_eq!(sent.len(), 2);
        assert_ne!(sent[0], sent[1], "a password is never reused");
        for password in &sent {
            assert_eq!(password.len(), PKCS12_PASSWORD_LEN);
            assert!(password.chars().all(|c| c.is_ascii_alphanumeric()), "{password}");
        }
        for (material, password) in [(&first, &sent[0]), (&second, &sent[1])] {
            match material {
                VaultCertificateMaterial::Pkcs12 {
                    bundle,
                    password: kept,
                    key_algorithm,
                } => {
                    assert_eq!(bundle.as_slice(), &[1, 2, 3, 4, 5]);
                    assert_eq!(kept.as_str(), password.as_str());
                    assert_eq!(key_algorithm, "EC-P256");
                }
                other => panic!("expected PKCS12, got {other:?}"),
            }
        }
    }

    // Spec decision 4: only the id is cached.
    #[tokio::test]
    async fn material_is_never_cached_each_fetch_exports_again() {
        let server = MockServer::start().await;
        with_client_a(&server, 1).await;
        mount_export(&server, "id-1", pem_export(), 2).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        for _ in 0..2 {
            fetch(&fetcher, &server, VaultCertificateFormat::Pem)
                .await
                .expect("export");
        }
    }

    // Review Focus 2.
    #[tokio::test]
    async fn export_refreshes_the_id_and_retries_once_after_a_404() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        // The certificate was deleted and created again under the same name: the first lookup
        // still sees the old id, the next one the new id.
        Mock::given(method("GET"))
            .and(path(LIST_PATH))
            .respond_with(page_of(vec![entry("old-id", "client-a")], Some(1)))
            .up_to_n_times(1)
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(LIST_PATH))
            .respond_with(page_of(vec![entry("new-id", "client-a")], Some(1)))
            .expect(1)
            .mount(&server)
            .await;
        mount_export(&server, "old-id", rocketvault_error(404, "not_found"), 1).await;
        mount_export(&server, "new-id", pem_export(), 1).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let material = fetch(&fetcher, &server, VaultCertificateFormat::Pem)
            .await
            .expect("the retry with the fresh id succeeds");
        assert_eq!(material.format(), VaultCertificateFormat::Pem);
        let cached = fetcher
            .certificate_id(&conn(server.uri()), "shh", "prod-vault", "client-a", false)
            .await
            .expect("cached");
        assert_eq!(cached, "new-id");
    }

    // Review Focus 2.
    #[tokio::test]
    async fn a_certificate_that_stays_missing_is_not_found_after_one_retry() {
        let server = MockServer::start().await;
        with_client_a(&server, 2).await;
        mount_export(&server, "id-1", rocketvault_error(404, "not_found"), 2).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let err = fetch(&fetcher, &server, VaultCertificateFormat::Pem)
            .await
            .expect_err("still missing after one retry");
        assert_eq!(
            err,
            DomainError::NotFound("Certificate not found in this vault.".into())
        );
        assert!(fetcher.certificate_ids.is_empty(), "a missing id is forgotten");
    }

    // Review Focus 4.
    #[tokio::test]
    async fn export_401_evicts_the_token_and_fails() {
        let server = MockServer::start().await;
        with_client_a(&server, 1).await;
        // No retry on a 401: the request fails, and the next send signs in again.
        mount_export(&server, "id-1", ResponseTemplate::new(401), 1).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let err = fetch(&fetcher, &server, VaultCertificateFormat::Pem)
            .await
            .expect_err("401");
        assert_eq!(
            err,
            DomainError::Http("RocketVault rejected the access token (401).".into())
        );
        assert!(fetcher.tokens.get("conn-1").is_none());
    }

    // Review Focus 5.
    #[tokio::test]
    async fn export_403_not_exportable_says_so() {
        let server = MockServer::start().await;
        with_client_a(&server, 1).await;
        mount_export(
            &server,
            "id-1",
            rocketvault_error(403, "certificate_not_exportable"),
            1,
        )
        .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let err = fetch(&fetcher, &server, VaultCertificateFormat::Pkcs12)
            .await
            .expect_err("403");
        assert_eq!(
            err,
            DomainError::InvalidInput("Certificate is not marked exportable.".into())
        );
    }

    #[tokio::test]
    async fn export_403_without_a_json_body_names_the_missing_role() {
        let server = MockServer::start().await;
        with_client_a(&server, 1).await;
        mount_export(&server, "id-1", ResponseTemplate::new(403).set_body_string("Forbidden"), 1)
            .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let err = fetch(&fetcher, &server, VaultCertificateFormat::Pem)
            .await
            .expect_err("403");
        assert_eq!(
            err,
            DomainError::Http("The service account lacks the Certificate Exporter role.".into())
        );
    }

    #[tokio::test]
    async fn export_409_says_the_certificate_is_disabled() {
        let server = MockServer::start().await;
        with_client_a(&server, 1).await;
        mount_export(&server, "id-1", rocketvault_error(409, "certificate_disabled"), 1).await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let err = fetch(&fetcher, &server, VaultCertificateFormat::Pem)
            .await
            .expect_err("409");
        assert_eq!(err, DomainError::InvalidInput("Certificate is disabled.".into()));
    }

    #[tokio::test]
    async fn export_400_and_500_name_the_status() {
        for (status, code) in [(400, "bad_request"), (500, "internal_error")] {
            let server = MockServer::start().await;
            with_client_a(&server, 1).await;
            mount_export(&server, "id-1", rocketvault_error(status, code), 1).await;

            let fetcher = ReqwestVaultSecretFetcher::new();
            let err = fetch(&fetcher, &server, VaultCertificateFormat::Pem)
                .await
                .expect_err("error status");
            assert!(err.to_string().contains(&status.to_string()), "{err}");
        }
    }

    // Spec section 9.
    #[tokio::test]
    async fn an_export_larger_than_1_mib_is_rejected() {
        let server = MockServer::start().await;
        with_client_a(&server, 1).await;
        mount_export(
            &server,
            "id-1",
            ResponseTemplate::new(200).set_body_string("x".repeat(MAX_EXPORT_BYTES + 1)),
            1,
        )
        .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let err = fetch(&fetcher, &server, VaultCertificateFormat::Pem)
            .await
            .expect_err("too large");
        assert!(err.to_string().contains("larger than"), "{err}");
    }

    // Spec section 9.
    #[tokio::test]
    async fn errors_and_debug_never_contain_key_bytes_or_the_password() {
        let server = MockServer::start().await;
        with_client_a(&server, 1).await;
        mount_export(&server, "id-1", pkcs12_export(), 1).await;
        let fetcher = ReqwestVaultSecretFetcher::new();
        let material = fetch(&fetcher, &server, VaultCertificateFormat::Pkcs12)
            .await
            .expect("export");
        let sent = export_bodies(&server).await[0]["password"]
            .as_str()
            .expect("password")
            .to_string();
        let shown = format!("{material:?}");
        assert!(!shown.contains(&sent) && !shown.contains("AQID"), "{shown}");

        // A broken success body that holds key text must not be echoed in the error.
        let broken = MockServer::start().await;
        with_client_a(&broken, 1).await;
        mount_export(
            &broken,
            "id-1",
            ResponseTemplate::new(200)
                .set_body_string(format!("{{\"private_key_pem\": \"{KEY_PEM}\" ,,, }}")),
            1,
        )
        .await;
        let err = fetch(&fetcher, &broken, VaultCertificateFormat::Pem)
            .await
            .expect_err("unreadable")
            .to_string();
        assert!(!err.contains("ZmFrZS1rZXk") && !err.contains("BEGIN"), "{err}");
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-infra rocketvault::certificates`
Expected: the 10 B2 tests pass; the 12 new tests FAIL with `DomainError::Internal("this vault fetcher cannot export certificates")` from the trait default (the client does not override `fetch_certificate` yet).

- [ ] **Step 3: Add the dependency**

In `crates/rocket-infra/Cargo.toml`, after `zeroize = "1"` (line 44), add:

```toml
# The one-time PKCS12 export password comes from the OS random generator.
rand = "0.8"
```

- [ ] **Step 4: Implement the export**

In `crates/rocket-infra/src/rocketvault/certificates.rs`, imports (added in B2), old:

```rust
use rocket_environment::{SecretManagerConnection, VaultCertificateSummary};
use rocket_shared::error::{DomainError, DomainResult};

use super::certificate_api::{self, LIST_PAGE_SIZE, MAX_LIST_PAGES};
use super::ReqwestVaultSecretFetcher;
```

new:

```rust
use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use rocket_environment::{SecretManagerConnection, VaultCertificateMaterial, VaultCertificateSummary};
use rocket_shared::certificate::VaultCertificateFormat;
use rocket_shared::error::{DomainError, DomainResult};
use zeroize::Zeroizing;

use super::certificate_api::{
    self, ExportFailure, LIST_PAGE_SIZE, MAX_ERROR_BYTES, MAX_EXPORT_BYTES, MAX_LIST_PAGES,
    PKCS12_PASSWORD_LEN,
};
use super::ReqwestVaultSecretFetcher;

/// The outcome of one export call.
enum Export {
    Material(VaultCertificateMaterial),
    /// 404: the id is unknown, perhaps because the certificate was created again.
    NotFound,
}
```

Add after the B2 `impl ReqwestVaultSecretFetcher` block (before `fn certificate_id_key`):

```rust
impl ReqwestVaultSecretFetcher {
    /// Exports the certificate called `name`. The id comes from the cache or the list. After a
    /// 404 the name is looked up again and the export is retried once, because a certificate
    /// deleted and created again under the same name has a new id.
    pub(super) async fn export_certificate(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
        name: &str,
        format: VaultCertificateFormat,
    ) -> DomainResult<VaultCertificateMaterial> {
        let id = self
            .certificate_id(connection, client_secret, vault_name, name, false)
            .await?;
        if let Export::Material(material) = self
            .export_once(connection, client_secret, vault_name, &id, format)
            .await?
        {
            return Ok(material);
        }
        let fresh = self
            .certificate_id(connection, client_secret, vault_name, name, true)
            .await?;
        match self
            .export_once(connection, client_secret, vault_name, &fresh, format)
            .await?
        {
            Export::Material(material) => Ok(material),
            Export::NotFound => {
                self.forget_certificate_id(connection, vault_name, name);
                Err(DomainError::NotFound(certificate_api::NOT_FOUND.to_string()))
            }
        }
    }

    /// One export call. A PKCS12 export gets a fresh password, which goes to RocketVault in
    /// the body and comes back inside the material. It is never logged or put in an error.
    async fn export_once(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
        certificate_id: &str,
        format: VaultCertificateFormat,
    ) -> DomainResult<Export> {
        let token = self.ensure_token(connection, client_secret).await?;
        let password = match format {
            VaultCertificateFormat::Pem => None,
            VaultCertificateFormat::Pkcs12 => Some(one_time_password()),
        };
        let mut body = certificate_api::export_request_body(
            format,
            password.as_ref().map(|p| p.as_str()),
        )?;
        let url = certificate_api::export_url(connection, vault_name, certificate_id)?;
        let resp = self
            .client_for(connection)
            .post(url)
            .bearer_auth(&token)
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            // reqwest owns the body from here and does not wipe it. The password is single-use,
            // so a copy left behind opens nothing once this export is read.
            .body(std::mem::take(&mut *body))
            .send()
            .await
            .map_err(|e| {
                DomainError::Http(format!("RocketVault certificate export request failed: {e}"))
            })?;
        let status = resp.status();
        if status.is_success() {
            let bytes = read_capped(resp, MAX_EXPORT_BYTES).await?;
            return certificate_api::parse_export(&bytes, format, password).map(Export::Material);
        }
        // An error body that cannot be read is classified by its status alone.
        let error_body = read_capped(resp, MAX_ERROR_BYTES)
            .await
            .unwrap_or_else(|_| Zeroizing::new(Vec::new()));
        match certificate_api::classify_export_error(status, &error_body) {
            ExportFailure::TokenRejected => {
                self.tokens.remove(&connection.id);
                Err(DomainError::Http(certificate_api::TOKEN_REJECTED.to_string()))
            }
            ExportFailure::NotFound => Ok(Export::NotFound),
            ExportFailure::Failed(err) => Err(err),
        }
    }
}

/// A fresh random password for one PKCS12 export: letters and digits from the OS generator.
/// The buffer is reserved up front, so it never reallocates and leaves a copy behind.
fn one_time_password() -> Zeroizing<String> {
    let mut password = Zeroizing::new(String::with_capacity(PKCS12_PASSWORD_LEN));
    for byte in OsRng.sample_iter(&Alphanumeric).take(PKCS12_PASSWORD_LEN) {
        password.push(char::from(byte));
    }
    password
}

/// Reads a response body of at most `cap` bytes into memory that is wiped on drop. The buffer
/// is reserved up front, so it never reallocates and leaves a copy behind.
async fn read_capped(
    mut resp: reqwest::Response,
    cap: usize,
) -> DomainResult<Zeroizing<Vec<u8>>> {
    let too_large =
        || DomainError::Http(format!("RocketVault sent a response larger than {cap} bytes."));
    if resp.content_length().is_some_and(|n| n > cap as u64) {
        return Err(too_large());
    }
    let mut body = Zeroizing::new(Vec::with_capacity(cap));
    while let Some(chunk) = resp.chunk().await.map_err(|e| {
        DomainError::Http(format!("RocketVault response could not be read: {e}"))
    })? {
        if body.len() + chunk.len() > cap {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

```

In `crates/rocket-infra/src/rocketvault/mod.rs`, remove both allows. Old:

```rust
// The certificate contract. Its export half is first used in Task B3, which removes this allow.
#[allow(dead_code)]
mod certificate_api;
// `certificate_id` and `forget_certificate_id` are first used outside tests in Task B3, which
// removes this allow.
#[allow(dead_code)]
mod certificates;
```

new:

```rust
/// The RocketVault certificate contract: routes, shapes, codes and messages.
mod certificate_api;
/// Certificate calls: the paged list, the name-to-id cache and the export.
mod certificates;
```

The `use` (line 336 before B2), old:

```rust
use rocket_environment::{ExternalSecretRef, VaultCertificateSummary, VaultSecretFetcher};
```

new:

```rust
use rocket_environment::{
    ExternalSecretRef, VaultCertificateMaterial, VaultCertificateSummary, VaultSecretFetcher,
};
use rocket_shared::certificate::VaultCertificateFormat;
```

In `impl VaultSecretFetcher for ReqwestVaultSecretFetcher`, after B2's `list_certificates`, add:

```rust

    async fn fetch_certificate(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
        certificate_name: &str,
        format: VaultCertificateFormat,
    ) -> DomainResult<VaultCertificateMaterial> {
        self.export_certificate(connection, client_secret, vault_name, certificate_name, format)
            .await
    }
```

- [ ] **Step 5: Run the export tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra rocketvault`
Expected: PASS: the existing `rocketvault::tests`, 12 `certificate_api` tests and 22 `certificates` tests.

- [ ] **Step 6: Add the live export test**

In `crates/rocket-infra/src/rocketvault/certificates.rs`, add at the end of `mod tests`:

```rust

    /// Exports a real certificate from a live RocketVault v4 in both formats, checks that the
    /// PEM key is unencrypted PKCS#8, and loads each export as a native-TLS identity (this is
    /// where a "legacy" PKCS12 that OpenSSL 3 cannot open would show up). The rocketvault-4b
    /// session provides the instance; nothing is written to it. Set ROCKETVAULT_URL,
    /// ROCKETVAULT_CLIENT_ID, ROCKETVAULT_CLIENT_SECRET, ROCKETVAULT_VAULT and
    /// ROCKETVAULT_CERTIFICATE (an exportable certificate). Set ROCKETVAULT_INSECURE for a
    /// self-signed RocketVault. With ROCKETVAULT_MTLS_URL set, it also sends a request there
    /// with each identity, so a server that requires the certificate completes a handshake.
    #[tokio::test]
    #[ignore = "needs a live RocketVault v4 instance; see the doc comment"]
    async fn live_rocketvault_export_loads_as_a_tls_identity() {
        let var = |name: &str| {
            std::env::var(name).unwrap_or_else(|_| panic!("set {name} to run this test"))
        };
        let connection = SecretManagerConnection {
            id: "live".into(),
            label: "live".into(),
            base_url: var("ROCKETVAULT_URL"),
            client_id: var("ROCKETVAULT_CLIENT_ID"),
            verify_ssl: std::env::var("ROCKETVAULT_INSECURE").is_err(),
            allow_insecure_http: true,
        };
        let secret = var("ROCKETVAULT_CLIENT_SECRET");
        let vault = var("ROCKETVAULT_VAULT");
        let name = var("ROCKETVAULT_CERTIFICATE");

        let fetcher = ReqwestVaultSecretFetcher::new();
        let listed = fetcher
            .list_certificates(&connection, &secret, &vault)
            .await
            .expect("list");
        assert!(listed.iter().any(|c| c.name == name), "{name} is not listed");

        for format in [VaultCertificateFormat::Pem, VaultCertificateFormat::Pkcs12] {
            let material = fetcher
                .fetch_certificate(&connection, &secret, &vault, &name, format)
                .await
                .expect("export");
            let identity = match &material {
                VaultCertificateMaterial::Pem {
                    certificate,
                    private_key,
                    ..
                } => {
                    assert!(
                        private_key.starts_with(b"-----BEGIN PRIVATE KEY-----"),
                        "the key must be unencrypted PKCS#8"
                    );
                    reqwest::Identity::from_pkcs8_pem(certificate, private_key)
                }
                VaultCertificateMaterial::Pkcs12 {
                    bundle, password, ..
                } => reqwest::Identity::from_pkcs12_der(bundle, password),
            }
            .expect("the platform TLS stack loads the export");

            if let Ok(target) = std::env::var("ROCKETVAULT_MTLS_URL") {
                let client = reqwest::Client::builder()
                    .identity(identity)
                    .danger_accept_invalid_certs(true)
                    .build()
                    .expect("client");
                let status = client
                    .get(&target)
                    .send()
                    .await
                    .expect("handshake")
                    .status();
                assert!(!status.is_server_error(), "{format:?}: {status}");
            }
        }
    }
```

Run: `cargo test -j4 -p rocket-infra live_rocketvault_export`
Expected: `1 ignored`, nothing fails. Run it with `-- --ignored` only once the live instance and the variables are in place; record the result in the task report.

- [ ] **Step 7: Run the existing handshake test**

Run: `cargo test -j4 -p rocket-infra mutual_tls_handshake -- --ignored`
Expected: PASS (needs the `openssl` CLI). Nothing in the executor changed in this task; this guards the shared `reqwest` build after the new dependency.

- [ ] **Step 8: Workspace check and clippy**

Run: `cargo check -j4 --workspace`
Expected: `Finished`, no errors.

Run: `cargo clippy -j4 -p rocket-infra --all-targets`
Expected: no warnings in `rocketvault/`, and no `dead_code` warning now that both allows are gone.

- [ ] **Step 9: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add crates/rocket-infra/Cargo.toml Cargo.lock crates/rocket-infra/src/rocketvault/certificates.rs crates/rocket-infra/src/rocketvault/mod.rs
git commit -- crates/rocket-infra/Cargo.toml Cargo.lock crates/rocket-infra/src/rocketvault/certificates.rs crates/rocket-infra/src/rocketvault/mod.rs
```

Suggested subject: `feat(vault): export RocketVault certificates with a one-time password`. The message ends with `Relates to: #21`.

---

## Milestone Checklist: Plan B

- [ ] `VaultCertificateSummary` and `VaultCertificateMaterial` (sizes-only `Debug`) in `rocket-environment`; `list_certificates` and `fetch_certificate` on the trait with refusing defaults; `NullVaultSecretFetcher` overrides them
- [ ] The RocketVault certificate contract lives only in `rocketvault/certificate_api.rs`
- [ ] The list walk pages until a short page, the total, or the wanted name; a name on page 3 is found and page 4 is not read
- [ ] Ids are cached per connection id, base URL and vault; a 404 refreshes the id and retries once; material is never cached
- [ ] PKCS12 exports use a fresh 32-character `OsRng` password with `compat: legacy`; the password is returned in the material, never logged
- [ ] Responses are read with a 1 MiB cap into `Zeroizing` buffers
- [ ] 401 evicts the token; 403 not exportable, 403 role, 404, 409, 400 and 500 map to the spec's messages
- [ ] The live export test exists and is `#[ignore]`
- [ ] `cargo check -j4 --workspace` and clippy for each touched crate are clean

## Next Plan

[Plan C: Materialization and command](03-plan-c-materialization-and-command.md): fetches the vault certificate selected for a request URL, an in-send OAuth2 client-credentials token URL, or a direct OAuth2 token URL, right before the send, and adds the `list_vault_certificates` command with its TypeScript binding.
