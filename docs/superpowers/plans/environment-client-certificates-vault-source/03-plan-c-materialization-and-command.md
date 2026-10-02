# RocketVault Certificate Source, Plan C: Materialization and Command

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Right before a request or token request goes out, export the RocketVault certificate selected for its URL, and only that one, and hand the executor inline material. A failed export fails only that request, with a message that names the certificate and the binding. Add the `list_vault_certificates` command and its TypeScript binding for the picker.

**Architecture:** A new `rocket-app/src/vault_certificates.rs` holds the whole materialization step: which URLs a send presents a certificate to (`certificate_urls`), whether the entry selected for them is still `Deferred` (`needs_fetch`), and the fetch that replaces it with `Inline` material or `Unavailable` (`materialize_selected`). It borrows what it needs through `VaultCertificateAccess` (connection repo, keychain store, fetcher), so it works for both services. `RequestExecutionService::send_request`, the one dispatch point shared by `execute`, the Collection Runner and Flow runs, fetches into a per-send copy of the request; the phase state, history and scripts keep the names-only form. `OAuth2Service` gains `with_vault_access` and fetches for the token URL in its three token calls. `SecretManagerService` gains `list_certificates`, exposed as a Tauri command with a camelCase DTO.

**Tech Stack:** Rust, `async-trait`, `zeroize`, Tauri 2 commands, TypeScript.

**Spec:** [`docs/superpowers/specs/2026-10-02-vault-certificate-source-design.md`](../../specs/2026-10-02-vault-certificate-source-design.md) (sections 5, 7, 8, 9, 10, 11.4). Plan index and the shared interface contract: [`00-plan-index.md`](00-plan-index.md).

**Plan C of 4 (A, B, C, D).** Depends on Plan A and Plan B.

---

## Global Constraints

- Values fetched from RocketVault are never written to disk in any form: not the environment YAML, not history, not logs, not events, not the audit trail. Only references persist (an External Secrets alias and a certificate name). Runtime material is `zeroize::Zeroizing` from fetch to use.
- An unresolved or failed vault certificate fails a request or token request only when that certificate is the one selected for the URL, with no fallback to another entry. An entry for another domain causes no RocketVault call.
- A `CertificateMaterial::Deferred` that reaches the executor is an `InvalidInput` error, never a silent skip.
- A path or reference field must not hold key text: a value starting with `-----BEGIN` is rejected on save. For a `vault` entry this covers every field.
- Error and log text never contains key bytes, PKCS12 bundle bytes or the one-time password.
- Serde: `#[serde(rename_all = "camelCase")]` on IPC DTOs only, plus the `ClientCertificate` variants, which already carry it. Persisted fields stay backward compatible (additive, with defaults).
- Rust: never call `unwrap` in production paths. Always pass `-j4` to cargo. Never run `cargo test --workspace`; use targeted crate tests plus `cargo check -j4 --workspace`.
- Frontend: shadcn/ui primitives only, `lucide-react` icons only, `SingleLineEditor` for single-line variable-aware fields, Monaco only for multi-line editors, never fully destructure Zustand store state at component top level. Checks: `yarn tsc --noEmit`, `yarn check`.
- Commits: conventional commits, path-scoped (`git add <paths>` then `git commit -- <paths>`, never `git add -A` or `git commit -a`). Every commit goes through the `dev-workflow-skills:1-git-commit` skill. Commit messages end with `Relates to: #21`.
- Every task that touches collection, environment or certificate data models starts with: `📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.`

## Coordination (read before C2)

C2 edits `crates/rocket-app/src/execution_service.rs`, which the user's Flow auth work on another machine also touches. Run `git status --short` and `git pull --ff-only` first; stop and ask if the pull is not a fast-forward or the file has foreign uncommitted hunks. The C2 change is `send_request` plus one private helper next to it. Do not touch `crates/rocket-app/src/flow_*.rs`: Flow runs reach `send_request` through `execute_with_external_secrets`.

## Review Focus (items this plan owns)

3. **A vault entry for another domain makes zero RocketVault calls.** Owners C1 and C2. Pinned by `a_vault_certificate_for_another_domain_makes_no_vault_call` (C1) and `a_request_to_another_domain_never_calls_the_vault` (C2) in `crates/rocket-app/src/vault_certificates.rs`.
4. **A 401 in the middle of a send fails only that request.** Owner C1 (the app half; B3 owns the token eviction). Pinned by `a_failed_fetch_fails_only_the_selected_entry_with_no_fallback` in `vault_certificates.rs`.

Also pinned here: OAuth2 uses the token URL (`a_token_request_fetches_the_certificate_selected_for_the_token_url`, C2), the fetched copy never reaches the recorded request (`the_recorded_request_keeps_names_only`, C2), and `Debug` after a fetch shows no key bytes or password (`debug_after_a_fetch_never_shows_key_bytes_or_the_password`, C1).

## Spec versus code (read before starting)

1. **Selection happens in `send_request`, after the pre-request script** (spec 5.2 says "before dispatch"). A script can change the URL (`req.setUrl()`), and the certificate must match the URL that is actually sent. `send_request` (`crates/rocket-app/src/execution_service.rs:1414-1425`) is the one call into the executor for `execute_capturing`, the Collection Runner (`collection_runner_service.rs:436`) and, through `execute_with_external_secrets`, Flow runs (`flow_execution_service.rs:822`).
2. **The in-send OAuth2 token fetch.** `ReqwestExecutor::apply_auth` (`crates/rocket-infra/src/reqwest_executor.rs:672-697`) fetches a client-credentials token inside the send and presents the request's own certificate list to the token URL. `certificate_urls` therefore returns the request URL and, for `Auth::OAuth2(ClientCredentials { access_token_url, .. })`, that token URL too. The spec does not mention this path.
3. **`OAuth2Service` has no RocketVault access today** (`crates/rocket-app/src/oauth2_service.rs:116-125`). It gains `with_vault_access(connections, secret_store, fetcher)`, wired in `src-tauri/src/lib.rs` with a fourth `FsSecretManagerRepo` over the same file, like the three `SecretManagerService` instances already there. Without it, a selected vault certificate in a token request is `Unavailable` ("cannot be fetched here"), never skipped. The legacy `oauth2_auth_code_flow` command builds its own client and stays out of scope, as in Plans B to D.
4. **The 1 MiB cap and redaction forms** (spec 5.5). Exported material does not pass through `inline_from_secret`, so `MAX_INLINE_SECRET_BYTES` does not apply; the export was capped at read time in B3. Material never enters the external-secrets map, so `redaction_forms` and the script write guard need no change (spec section 9).
5. **Messages name the certificate and the binding** (spec section 8). The fetcher's message (B3) is the detail; C1 wraps it: `The RocketVault certificate client-a (binding prod) for api.example.com could not be fetched: Certificate is not marked exportable.`

## Findings from reading the real code (do not re-derive)

- `RequestExecutionService` holds `secret_manager_repo: Box<dyn SecretManagerRepository>`, `vault_connection_secret_store: Arc<dyn SecretStore>`, `vault_fetcher: Arc<dyn VaultSecretFetcher>` (`execution_service.rs:269-281`). Its fields are private to `execution_service`, so the access struct is built there and passed in.
- `crate::vault_secret_resolution::VAULT_CONNECTION_SCOPE` (`vault_secret_resolution.rs:11`) is the keychain scope of a connection's client secret, keyed by connection id.
- `PhaseState.http_request` is recorded into `sent` before `send_request` (`execution_service.rs:1713`) and is what history and scripts see, so the fetched copy must stay local to `send_request`.
- `OAuth2Service` token calls: `get_token_direct` (`oauth2_service.rs:336-349`), `refresh_token_with_secrets` (lines 358-428, certificates at 420-426), `exchange_code_for_token` (lines 467-491). Each calls the sync `token_client(&url, verify_ssl, certificates)`.
- `crate::test_doubles` (cfg(test), `crates/rocket-app/src/test_doubles.rs`) has `StaticEnvRepo`, `FakeSecretManagerRepo(SecretManagerConnection)`, `EmptySecretManagerRepo`, `FakeSecretStore(String)`, `InMemoryCollectionRepo::new(Collection) -> Arc<_>` with `SharedCollectionRepo`, `InMemoryHistoryRepo::new() -> Arc<_>` with `SharedHistoryRepo`, and `NullCookieRepo`. Its imports are at lines 7-24 and `FakeVaultSecretFetcher` ends before the "Callback listener" banner (around line 642).
- `src-tauri/src/commands/secret_managers.rs` (106 lines) holds the DTOs and commands; the package of `src-tauri` is `rocket`. Commands are registered in `src-tauri/src/lib.rs` lines 704-708. `src/lib/tauri-api.ts` has the secret-manager bindings at lines 1725-1742.

## Test conventions

- New tests use `.expect("message")` and `expect_err("message")`. Production code never calls `unwrap`.
- `ResolvedClientCertificate` is compared through `crate::client_certificates::describe_all`: `pem <domain> inline:<n> inline:<n> pass:-`, `pkcs12 <domain> inline:<n> pass:<passphrase>`, `unavailable <domain> <reason>`, `deferred <domain> <alias>:<name> <format> conn:<id> vault:<name>`.

---

## Task C1: Materialize the certificate selected for a URL

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** opus (the materialization step).

**Files:**
- Create: `crates/rocket-app/src/vault_certificates.rs`
- Modify: `crates/rocket-app/src/lib.rs` (module list, before `pub mod vault_secret_resolution;` on line 36)
- Modify: `crates/rocket-app/src/test_doubles.rs` (imports lines 17-24; new fake after `impl VaultSecretFetcher for FakeVaultSecretFetcher`)

**Interfaces:**
- Consumes: A1's `CertificateMaterial::Deferred`, `VaultCertificateBinding`, `is_deferred`; B1's `VaultCertificateMaterial` and `fetch_certificate`; `rocket_http::client_cert::find_certificate`; `VAULT_CONNECTION_SCOPE`.
- Produces (contract names): `VaultCertificateAccess`, `certificate_urls`, `needs_fetch`, `materialize_selected`; test doubles `FakeCertificateFetcher`, `FakeExport`, `FAKE_CERT_PEM`, `FAKE_KEY_PEM`, `FAKE_BUNDLE`, `FAKE_PASSWORD`. Consumed by C2 and C3.

- [ ] **Step 1: Add the certificate test double**

In `crates/rocket-app/src/test_doubles.rs`, imports lines 17-24, old:

```rust
use rocket_environment::{
    Environment, EnvironmentRepository, EnvironmentRepositoryFactory, ExternalSecretRef,
    SecretManagerConnection, SecretManagerRepository, SecretStore, VaultSecretFetcher,
};
use rocket_history::{HistoryEntry, HistoryFilter, HistoryRepository};
use rocket_http::{CookieJar, CookieRepository, HttpExecutor, HttpRequest, HttpResponse};
use rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult};
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};
```

new:

```rust
use rocket_environment::{
    Environment, EnvironmentRepository, EnvironmentRepositoryFactory, ExternalSecretRef,
    SecretManagerConnection, SecretManagerRepository, SecretStore, VaultCertificateMaterial,
    VaultCertificateSummary, VaultSecretFetcher,
};
use rocket_history::{HistoryEntry, HistoryFilter, HistoryRepository};
use rocket_http::{CookieJar, CookieRepository, HttpExecutor, HttpRequest, HttpResponse};
use rocket_scripting::{ScriptContext, ScriptEngine, ScriptResult};
use rocket_shared::certificate::VaultCertificateFormat;
use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};
use zeroize::Zeroizing;
```

After the closing `}` of `impl VaultSecretFetcher for FakeVaultSecretFetcher` (just before the `// Callback listener` banner), add:

```rust

/// What `FakeCertificateFetcher` answers for one certificate name.
#[derive(Clone, Copy)]
pub enum FakeExport {
    /// Exports fixed material in the asked format.
    Ok,
    /// Fails with `InvalidInput(message)`, as RocketVault's mapped errors do.
    Fail(&'static str),
}

pub const FAKE_CERT_PEM: &[u8] =
    b"-----BEGIN CERTIFICATE-----\nZmFrZS1jZXJ0\n-----END CERTIFICATE-----\n";
pub const FAKE_KEY_PEM: &[u8] =
    b"-----BEGIN PRIVATE KEY-----\nc2VjcmV0LWtleQ==\n-----END PRIVATE KEY-----\n";
pub const FAKE_BUNDLE: &[u8] = &[0x30, 0x82, 0x01, 0x02, 0x03];
pub const FAKE_PASSWORD: &str = "one-time-pass-123";

/// Vault fetcher that exports certificates from a script and records every export as
/// `vault/name/format`. A name with no script entry is "not found". `list_certificates` lists
/// the scripted names, with `exportable` true for `Ok`.
pub struct FakeCertificateFetcher {
    exports: HashMap<String, FakeExport>,
    calls: Mutex<Vec<String>>,
}

impl FakeCertificateFetcher {
    pub fn new(exports: &[(&str, FakeExport)]) -> Arc<Self> {
        Arc::new(Self {
            exports: exports
                .iter()
                .map(|(name, export)| (name.to_string(), *export))
                .collect(),
            calls: Mutex::new(Vec::new()),
        })
    }

    pub fn calls(&self) -> Vec<String> {
        self.calls.lock().expect("lock").clone()
    }
}

#[async_trait]
impl VaultSecretFetcher for FakeCertificateFetcher {
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

    async fn list_certificates(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<Vec<VaultCertificateSummary>> {
        let mut listed: Vec<VaultCertificateSummary> = self
            .exports
            .iter()
            .map(|(name, export)| VaultCertificateSummary {
                id: format!("id-{name}"),
                name: name.clone(),
                exportable: matches!(export, FakeExport::Ok),
                enabled: true,
                key_algorithm: "RSA-2048".into(),
                expires_at: None,
            })
            .collect();
        listed.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(listed)
    }

    async fn fetch_certificate(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        vault_name: &str,
        certificate_name: &str,
        export_format: VaultCertificateFormat,
    ) -> DomainResult<VaultCertificateMaterial> {
        self.calls.lock().expect("lock").push(format!(
            "{vault_name}/{certificate_name}/{}",
            export_format.as_str()
        ));
        match self.exports.get(certificate_name).copied() {
            Some(FakeExport::Ok) => Ok(match export_format {
                VaultCertificateFormat::Pem => VaultCertificateMaterial::Pem {
                    certificate: Zeroizing::new(FAKE_CERT_PEM.to_vec()),
                    private_key: Zeroizing::new(FAKE_KEY_PEM.to_vec()),
                    key_algorithm: "RSA-2048".into(),
                },
                VaultCertificateFormat::Pkcs12 => VaultCertificateMaterial::Pkcs12 {
                    bundle: Zeroizing::new(FAKE_BUNDLE.to_vec()),
                    password: Zeroizing::new(FAKE_PASSWORD.to_string()),
                    key_algorithm: "EC-P256".into(),
                },
            }),
            Some(FakeExport::Fail(message)) => Err(DomainError::InvalidInput(message.to_string())),
            None => Err(DomainError::NotFound(
                "Certificate not found in this vault.".into(),
            )),
        }
    }
}
```

Run: `cargo test -j4 -p rocket-app --no-run`
Expected: builds (the fake is unused for now, which is fine in a `cfg(test)` module).

- [ ] **Step 2: Write the failing materialization tests**

In `crates/rocket-app/src/lib.rs`, before `pub mod vault_secret_resolution;` (line 36), add:

```rust
pub(crate) mod vault_certificates;
```

Create `crates/rocket-app/src/vault_certificates.rs` with only the module doc and the tests:

```rust
//! Fetches the RocketVault certificate selected for a URL, right before a request or token
//! request is sent.
//!
//! Resolution leaves a `vault` entry as `CertificateMaterial::Deferred`, which holds names only.
//! Here the entry chosen for a URL, and only that one, is exported and replaced by inline
//! material, so an entry for another domain never causes a RocketVault call. A failed export
//! makes that entry `Unavailable` with the reason, and the executor then fails the request.
//! There is no fallback to another entry. The material is never cached and never leaves memory.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::client_certificates::describe_all;
    use crate::test_doubles::{
        EmptySecretManagerRepo, FakeCertificateFetcher, FakeExport, FakeSecretManagerRepo,
        FakeSecretStore, FAKE_BUNDLE, FAKE_PASSWORD,
    };
    use rocket_environment::SecretManagerConnection;
    use rocket_shared::types::HttpMethod;

    fn binding() -> VaultCertificateBinding {
        VaultCertificateBinding {
            alias: "prod".into(),
            connection_id: "conn-1".into(),
            vault_name: "prod-vault".into(),
        }
    }

    fn deferred(domain: &str, name: &str, format: VaultCertificateFormat) -> ResolvedClientCertificate {
        ResolvedClientCertificate::deferred(domain, binding(), name, format)
    }

    fn connection() -> SecretManagerConnection {
        SecretManagerConnection {
            id: "conn-1".into(),
            label: "Prod RocketVault".into(),
            base_url: "https://vault.internal:8774".into(),
            client_id: "rocketapi".into(),
            verify_ssl: true,
            allow_insecure_http: false,
        }
    }

    async fn run(
        certs: &mut [ResolvedClientCertificate],
        urls: &[&str],
        fetcher: &FakeCertificateFetcher,
    ) {
        let repo = FakeSecretManagerRepo(connection());
        let store = FakeSecretStore("client-secret".into());
        let access = VaultCertificateAccess {
            connections: &repo,
            secret_store: &store,
            fetcher,
        };
        materialize_selected(certs, urls, Some(&access)).await;
    }

    #[tokio::test]
    async fn only_the_certificate_selected_for_the_url_is_fetched() {
        let fetcher =
            FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok), ("client-b", FakeExport::Ok)]);
        let mut certs = vec![
            deferred("api.example.com", "client-a", VaultCertificateFormat::Pem),
            deferred("other.example.com", "client-b", VaultCertificateFormat::Pem),
        ];
        run(&mut certs, &["https://api.example.com/v1/users"], &fetcher).await;

        assert_eq!(fetcher.calls(), vec!["prod-vault/client-a/pem"]);
        let lines = describe_all(&certs);
        assert!(
            lines[0].starts_with("pem api.example.com inline:") && lines[0].ends_with("pass:-"),
            "{lines:?}"
        );
        assert!(lines[1].starts_with("deferred other.example.com"), "{lines:?}");
    }

    // Review Focus 3.
    #[tokio::test]
    async fn a_vault_certificate_for_another_domain_makes_no_vault_call() {
        let fetcher = FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok)]);
        let mut certs = vec![deferred("api.example.com", "client-a", VaultCertificateFormat::Pem)];
        run(&mut certs, &["https://unrelated.example.org/"], &fetcher).await;

        assert!(fetcher.calls().is_empty());
        assert!(certs.iter().all(|c| c.is_deferred()));
    }

    // Review Focus 4.
    #[tokio::test]
    async fn a_failed_fetch_fails_only_the_selected_entry_with_no_fallback() {
        let fetcher = FakeCertificateFetcher::new(&[(
            "client-a",
            FakeExport::Fail("RocketVault rejected the access token (401)."),
        )]);
        let mut certs = vec![
            deferred("api.example.com", "client-a", VaultCertificateFormat::Pem),
            ResolvedClientCertificate::pkcs12(
                "api.example.com",
                CertificateSource::File("/certs/fallback.p12".into()),
                None,
            ),
        ];
        run(&mut certs, &["https://api.example.com/"], &fetcher).await;

        let lines = describe_all(&certs);
        assert!(
            lines[0].starts_with("unavailable api.example.com")
                && lines[0].contains("client-a")
                && lines[0].contains("binding prod")
                && lines[0].contains("(401)"),
            "{lines:?}"
        );
        // The file entry for the same domain is untouched, and the first match still wins, so
        // the executor fails the request with the reason instead of using the file.
        assert_eq!(lines[1], "pkcs12 api.example.com file:/certs/fallback.p12 pass:-");
        assert!(matches!(
            find_certificate(&certs, "https://api.example.com/").map(|c| &c.material),
            Some(CertificateMaterial::Unavailable { .. })
        ));
    }

    #[tokio::test]
    async fn pem_material_becomes_inline_pem_with_no_passphrase() {
        let fetcher = FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok)]);
        let mut certs = vec![deferred("api.example.com", "client-a", VaultCertificateFormat::Pem)];
        run(&mut certs, &["https://api.example.com/"], &fetcher).await;

        match &certs[0].material {
            CertificateMaterial::Pem {
                certificate: CertificateSource::Inline(cert),
                private_key: CertificateSource::Inline(key),
                passphrase: None,
            } => {
                assert_eq!(cert.as_slice(), crate::test_doubles::FAKE_CERT_PEM);
                assert_eq!(key.as_slice(), crate::test_doubles::FAKE_KEY_PEM);
            }
            other => panic!("expected inline PEM, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn pkcs12_material_carries_its_one_time_password_as_the_passphrase() {
        let fetcher = FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok)]);
        let mut certs = vec![deferred("api.example.com", "client-a", VaultCertificateFormat::Pkcs12)];
        run(&mut certs, &["https://api.example.com/"], &fetcher).await;

        assert_eq!(fetcher.calls(), vec!["prod-vault/client-a/pkcs12"]);
        assert_eq!(
            describe_all(&certs),
            vec![format!(
                "pkcs12 api.example.com inline:{} pass:{FAKE_PASSWORD}",
                FAKE_BUNDLE.len()
            )]
        );
    }

    #[tokio::test]
    async fn two_urls_on_one_certificate_fetch_it_once() {
        let fetcher = FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok)]);
        let mut certs = vec![deferred("api.example.com", "client-a", VaultCertificateFormat::Pem)];
        run(
            &mut certs,
            &["https://api.example.com/v1", "https://api.example.com/oauth/token"],
            &fetcher,
        )
        .await;
        assert_eq!(fetcher.calls().len(), 1);
    }

    #[tokio::test]
    async fn a_request_url_and_a_token_url_fetch_their_own_certificates() {
        let fetcher =
            FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok), ("idp-cert", FakeExport::Ok)]);
        let mut certs = vec![
            deferred("api.example.com", "client-a", VaultCertificateFormat::Pem),
            deferred("idp.example.com", "idp-cert", VaultCertificateFormat::Pkcs12),
        ];
        run(
            &mut certs,
            &["https://api.example.com/v1", "https://idp.example.com/token"],
            &fetcher,
        )
        .await;
        assert_eq!(
            fetcher.calls(),
            vec!["prod-vault/client-a/pem", "prod-vault/idp-cert/pkcs12"]
        );
        assert!(certs.iter().all(|c| !c.is_deferred()));
    }

    #[tokio::test]
    async fn without_vault_access_a_selected_vault_certificate_is_unavailable() {
        let mut certs = vec![deferred("api.example.com", "client-a", VaultCertificateFormat::Pem)];
        materialize_selected(&mut certs, &["https://api.example.com/"], None).await;
        let lines = describe_all(&certs);
        assert!(
            lines[0].starts_with("unavailable api.example.com")
                && lines[0].contains("client-a")
                && lines[0].contains("cannot be fetched here"),
            "{lines:?}"
        );
    }

    #[tokio::test]
    async fn a_missing_connection_names_the_binding() {
        let fetcher = FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok)]);
        let store = FakeSecretStore("client-secret".into());
        let access = VaultCertificateAccess {
            connections: &EmptySecretManagerRepo,
            secret_store: &store,
            fetcher: &*fetcher,
        };
        let mut certs = vec![deferred("api.example.com", "client-a", VaultCertificateFormat::Pem)];
        materialize_selected(&mut certs, &["https://api.example.com/"], Some(&access)).await;

        let lines = describe_all(&certs);
        assert!(
            lines[0].contains("binding prod") && lines[0].contains("no longer exists"),
            "{lines:?}"
        );
        assert!(fetcher.calls().is_empty());
    }

    #[tokio::test]
    async fn debug_after_a_fetch_never_shows_key_bytes_or_the_password() {
        let fetcher =
            FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok), ("idp-cert", FakeExport::Ok)]);
        let mut certs = vec![
            deferred("api.example.com", "client-a", VaultCertificateFormat::Pem),
            deferred("idp.example.com", "idp-cert", VaultCertificateFormat::Pkcs12),
        ];
        run(
            &mut certs,
            &["https://api.example.com/", "https://idp.example.com/token"],
            &fetcher,
        )
        .await;
        let shown = format!("{certs:?}");
        assert!(!shown.contains(FAKE_PASSWORD), "{shown}");
        assert!(!shown.contains("BEGIN") && !shown.contains("c2VjcmV0"), "{shown}");
    }

    #[test]
    fn certificate_urls_adds_the_client_credentials_token_url() {
        let mut request = HttpRequest::new(HttpMethod::Get, "https://api.example.com/v1");
        assert_eq!(certificate_urls(&request), vec!["https://api.example.com/v1"]);

        request.auth = Auth::OAuth2(Box::new(
            serde_json::from_value(serde_json::json!({
                "flow": "client_credentials",
                "accessTokenUrl": "https://idp.example.com/token",
                "credentials": { "clientId": "id", "clientSecret": "s" }
            }))
            .expect("client credentials flow"),
        ));
        assert_eq!(
            certificate_urls(&request),
            vec!["https://api.example.com/v1", "https://idp.example.com/token"]
        );
    }

    #[test]
    fn needs_fetch_only_for_a_selected_deferred_entry() {
        let certs = vec![
            deferred("api.example.com", "client-a", VaultCertificateFormat::Pem),
            ResolvedClientCertificate::pkcs12(
                "files.example.com",
                CertificateSource::File("/c.p12".into()),
                None,
            ),
        ];
        assert!(needs_fetch(&certs, &["https://api.example.com/"]));
        assert!(!needs_fetch(&certs, &["https://files.example.com/"]));
        assert!(!needs_fetch(&certs, &["https://elsewhere.example.org/"]));
    }
}
```

- [ ] **Step 3: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app vault_certificates::tests`
Expected: FAIL to compile with `cannot find struct VaultCertificateAccess in this scope` (and the other items).

- [ ] **Step 4: Implement the materialization**

In `crates/rocket-app/src/vault_certificates.rs`, insert between the module doc and `#[cfg(test)]`:

```rust

use rocket_environment::{
    SecretManagerRepository, SecretStore, VaultCertificateMaterial, VaultSecretFetcher,
};
use rocket_http::client_cert::find_certificate;
use rocket_http::{
    CertificateMaterial, CertificateSource, HttpRequest, ResolvedClientCertificate,
    VaultCertificateBinding,
};
use rocket_shared::certificate::VaultCertificateFormat;
use rocket_shared::error::DomainError;
use rocket_shared::oauth2::OAuth2Flow;
use rocket_shared::types::Auth;
use zeroize::Zeroizing;

use crate::vault_secret_resolution::VAULT_CONNECTION_SCOPE;

/// What a fetch needs, borrowed from the calling service.
pub(crate) struct VaultCertificateAccess<'a> {
    pub connections: &'a dyn SecretManagerRepository,
    pub secret_store: &'a dyn SecretStore,
    pub fetcher: &'a dyn VaultSecretFetcher,
}

/// The URLs a send may present a certificate to: the request URL, and the token URL of an
/// OAuth2 client-credentials auth, which the executor fetches inside the same send with the
/// request's certificates.
pub(crate) fn certificate_urls(request: &HttpRequest) -> Vec<&str> {
    let mut urls = vec![request.url.as_str()];
    if let Auth::OAuth2(flow) = &request.auth {
        if let OAuth2Flow::ClientCredentials {
            access_token_url, ..
        } = flow.as_ref()
        {
            urls.push(access_token_url.as_str());
        }
    }
    urls
}

/// Index of the entry selected for `url`: the first whose domain matches, as in the executor.
fn selected_index(certificates: &[ResolvedClientCertificate], url: &str) -> Option<usize> {
    let chosen = find_certificate(certificates, url)?;
    certificates.iter().position(|c| std::ptr::eq(c, chosen))
}

/// True when an entry selected for one of `urls` is still a RocketVault certificate to fetch.
/// Callers use it to skip the copy when there is nothing to fetch.
pub(crate) fn needs_fetch(certificates: &[ResolvedClientCertificate], urls: &[&str]) -> bool {
    urls.iter()
        .filter_map(|url| selected_index(certificates, url))
        .any(|i| certificates[i].is_deferred())
}

/// Replaces each entry selected for one of `urls` that is still `Deferred` by exported inline
/// material, or by `Unavailable` with the reason when the export fails. Other entries are not
/// touched and cause no RocketVault call. Without `access`, a selected vault certificate is
/// `Unavailable`. An entry selected for two URLs is fetched once.
pub(crate) async fn materialize_selected(
    certificates: &mut [ResolvedClientCertificate],
    urls: &[&str],
    access: Option<&VaultCertificateAccess<'_>>,
) {
    for url in urls {
        let Some(index) = selected_index(certificates, url) else {
            continue;
        };
        let CertificateMaterial::Deferred {
            binding,
            certificate,
            format: export_format,
        } = &certificates[index].material
        else {
            continue;
        };
        let (binding, certificate, export_format) =
            (binding.clone(), certificate.clone(), *export_format);
        let domain = certificates[index].domain.clone();
        let label = format!(
            "The RocketVault certificate {certificate} (binding {}) for {domain}",
            binding.alias
        );
        certificates[index] = match access {
            None => ResolvedClientCertificate::unavailable(
                domain,
                format!("{label} cannot be fetched here."),
            ),
            Some(access) => {
                match fetch_material(access, &binding, &certificate, export_format).await {
                    Ok(material) => into_inline(domain, material),
                    Err(err) => ResolvedClientCertificate::unavailable(
                        domain,
                        format!("{label} could not be fetched: {}", detail(&err)),
                    ),
                }
            }
        };
    }
}

/// Looks up the binding's connection and its client secret, then exports the certificate.
async fn fetch_material(
    access: &VaultCertificateAccess<'_>,
    binding: &VaultCertificateBinding,
    certificate: &str,
    export_format: VaultCertificateFormat,
) -> Result<VaultCertificateMaterial, DomainError> {
    let connection = access
        .connections
        .get(&binding.connection_id)?
        .ok_or_else(|| {
            DomainError::NotFound(format!(
                "The RocketVault connection of binding {} no longer exists. Pick another one \
                 on the External Secrets tab.",
                binding.alias
            ))
        })?;
    let client_secret = Zeroizing::new(
        access
            .secret_store
            .get(VAULT_CONNECTION_SCOPE, &binding.connection_id)?
            .ok_or_else(|| {
                DomainError::Internal(
                    "No client secret is stored for this RocketVault connection, or the OS \
                     keychain is locked."
                        .to_string(),
                )
            })?,
    );
    access
        .fetcher
        .fetch_certificate(
            &connection,
            &client_secret,
            &binding.vault_name,
            certificate,
            export_format,
        )
        .await
}

/// Turns exported material into the executor's inline form. Nothing is copied: the buffers and
/// the one-time PKCS12 password move as they are, and stay `Zeroizing`.
fn into_inline(domain: String, material: VaultCertificateMaterial) -> ResolvedClientCertificate {
    match material {
        VaultCertificateMaterial::Pem {
            certificate,
            private_key,
            ..
        } => ResolvedClientCertificate::pem(
            domain,
            CertificateSource::Inline(certificate),
            CertificateSource::Inline(private_key),
            None,
        ),
        VaultCertificateMaterial::Pkcs12 {
            bundle, password, ..
        } => ResolvedClientCertificate {
            domain,
            material: CertificateMaterial::Pkcs12 {
                bundle: CertificateSource::Inline(bundle),
                passphrase: Some(password),
            },
        },
    }
}

/// The message inside an error, without the `Display` prefix such as "HTTP error: ".
fn detail(err: &DomainError) -> String {
    match err {
        DomainError::NotFound(message)
        | DomainError::InvalidInput(message)
        | DomainError::Http(message)
        | DomainError::Internal(message) => message.clone(),
        other => other.to_string(),
    }
}

```

- [ ] **Step 5: Run the materialization tests to verify they pass**

Run: `cargo test -j4 -p rocket-app vault_certificates::tests`
Expected: PASS, 12 tests.

- [ ] **Step 6: Workspace check and clippy**

Run: `cargo check -j4 --workspace`
Expected: `Finished`. `cargo check` may warn that `materialize_selected`, `certificate_urls`, `needs_fetch` and `VaultCertificateAccess` are never used outside tests; C2 uses them. If the warning shows, leave it for C2 rather than adding an allow.

Run: `cargo clippy -j4 -p rocket-app --all-targets`
Expected: no warnings in `vault_certificates.rs` or `test_doubles.rs` other than the `dead_code` note above.

- [ ] **Step 7: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add crates/rocket-app/src/vault_certificates.rs crates/rocket-app/src/lib.rs crates/rocket-app/src/test_doubles.rs
git commit -- crates/rocket-app/src/vault_certificates.rs crates/rocket-app/src/lib.rs crates/rocket-app/src/test_doubles.rs
```

Suggested subject: `feat(app): fetch the RocketVault certificate selected for a URL`. The message ends with `Relates to: #21`.

---

## Task C2: Wire materialization into the send path and OAuth2 token requests

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** sonnet.

**Before starting:** run `git status --short` and `git pull --ff-only`. Stop and ask if the pull is not a fast-forward or `crates/rocket-app/src/execution_service.rs` has foreign uncommitted hunks.

**Files:**
- Modify: `crates/rocket-app/src/execution_service.rs` (`send_request`, lines 1413-1425 only, plus one new private method right after it)
- Modify: `crates/rocket-app/src/oauth2_service.rs` (imports lines 1-13, struct lines 116-125, `new` lines 128-138, a builder after `with_token_client_provider` lines 148-151, a helper after `token_client` lines 185-199, the three token calls at lines 346-348, 420-427 and 489-490)
- Modify: `crates/rocket-app/src/vault_certificates.rs` (new `mod wiring_tests` at the end)
- Modify: `src-tauri/src/lib.rs` (the `OAuth2Service` builder, lines 418-423)

**Interfaces:**
- Consumes: C1's `VaultCertificateAccess`, `certificate_urls`, `needs_fetch`, `materialize_selected`; C1's test doubles.
- Produces (contract names): `RequestExecutionService::with_vault_certificates` (private) called by `send_request`; `OAuth2Service::with_vault_access` and `certificates_for` (private). Consumed by the app wiring only.

- [ ] **Step 1: Write the failing wiring tests**

In `crates/rocket-app/src/vault_certificates.rs`, add at the end of the file:

```rust

#[cfg(test)]
mod wiring_tests {
    use super::*;
    use crate::client_certificates::describe_all;
    use crate::execution_service::{ExecuteRequestInput, RequestExecutionService};
    use crate::oauth2_service::{OAuth2GetTokenRequest, OAuth2Service};
    use crate::test_doubles::{
        FakeCertificateFetcher, FakeExport, FakeSecretManagerRepo, FakeSecretStore,
        InMemoryCollectionRepo, InMemoryHistoryRepo, NullCookieRepo, SharedCollectionRepo,
        SharedHistoryRepo, StaticEnvRepo, FAKE_BUNDLE, FAKE_PASSWORD,
    };
    use async_trait::async_trait;
    use rocket_collection::Collection;
    use rocket_environment::{Environment, ExternalSecretBinding, SecretManagerConnection};
    use rocket_http::{HttpExecutor, HttpResponse, RequestOptions, TokenClientProvider};
    use rocket_shared::certificate::ClientCertificate;
    use rocket_shared::error::DomainResult;
    use rocket_shared::events::NullEventPublisher;
    use rocket_shared::types::HttpMethod;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    /// Records the certificates of every request it sends, as `describe_all` lines.
    #[derive(Default)]
    struct CertificateRecordingExecutor {
        seen: Mutex<Vec<Vec<String>>>,
    }

    impl CertificateRecordingExecutor {
        fn seen(&self) -> Vec<Vec<String>> {
            self.seen.lock().expect("lock").clone()
        }
    }

    #[async_trait]
    impl HttpExecutor for CertificateRecordingExecutor {
        async fn execute(&self, req: &HttpRequest) -> DomainResult<HttpResponse> {
            self.seen
                .lock()
                .expect("lock")
                .push(describe_all(&req.options.client_certificates));
            Ok(HttpResponse {
                status: 200,
                status_text: "OK".into(),
                headers: vec![],
                body: "{}".into(),
                duration_ms: 1,
                ttfb_ms: 1,
                size_bytes: 2,
            })
        }
    }

    /// Records the certificates each token request would present, then stops before the network.
    #[derive(Default)]
    struct RecordingTokenClientProvider {
        seen: Mutex<Vec<Vec<String>>>,
    }

    impl TokenClientProvider for RecordingTokenClientProvider {
        fn client_for(
            &self,
            _token_url: &str,
            _verify_ssl: bool,
            certificates: &[ResolvedClientCertificate],
        ) -> DomainResult<reqwest::Client> {
            self.seen.lock().expect("lock").push(describe_all(certificates));
            Err(DomainError::Internal("stopped before the network".into()))
        }
    }

    fn connection() -> SecretManagerConnection {
        SecretManagerConnection {
            id: "conn-1".into(),
            label: "Prod RocketVault".into(),
            base_url: "https://vault.internal:8774".into(),
            client_id: "rocketapi".into(),
            verify_ssl: true,
            allow_insecure_http: false,
        }
    }

    /// One binding with no secret names (so no secret fetch), a PEM vault certificate for the
    /// API host and a PKCS12 one for the identity provider.
    fn environment() -> Environment {
        let mut env = Environment::new("prod");
        env.external_secrets = vec![ExternalSecretBinding {
            alias: "prod".into(),
            connection_id: "conn-1".into(),
            vault_name: "prod-vault".into(),
            secret_names: Vec::new(),
        }];
        env.client_certificates = vec![
            ClientCertificate::Vault {
                domain: "api.example.com".into(),
                binding: "prod".into(),
                certificate: "client-a".into(),
                format: VaultCertificateFormat::Pem,
            },
            ClientCertificate::Vault {
                domain: "idp.example.com".into(),
                binding: "prod".into(),
                certificate: "idp-cert".into(),
                format: VaultCertificateFormat::Pkcs12,
            },
        ];
        env
    }

    fn service(
        executor: Arc<CertificateRecordingExecutor>,
        fetcher: Arc<FakeCertificateFetcher>,
    ) -> RequestExecutionService {
        RequestExecutionService::new(
            Box::new(StaticEnvRepo(environment())),
            executor,
            Box::new(SharedHistoryRepo(InMemoryHistoryRepo::new())),
            Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(Collection::new("c")))),
            Box::new(NullCookieRepo),
            Box::new(NullEventPublisher),
            Box::new(FakeSecretManagerRepo(connection())),
            Arc::new(FakeSecretStore("client-secret".into())),
            fetcher,
        )
    }

    fn input(url: &str, auth: Auth) -> ExecuteRequestInput {
        ExecuteRequestInput {
            skip_history: false,
            flow_vars: HashMap::new(),
            method: HttpMethod::Get,
            url: url.into(),
            headers: vec![],
            query_params: vec![],
            body: None,
            auth,
            options: RequestOptions::default(),
            environment_name: Some("prod".into()),
            collection: None,
            request_name: None,
            pre_request_script: None,
            post_response_script: None,
            tests_script: None,
            request_path: None,
            global_env_name: None,
            assertions: vec![],
            tags: vec![],
            path_params: vec![],
            actions: vec![],
            request_guard_policy: rocket_workspace::RequestGuardPolicy::default(),
        }
    }

    fn client_credentials(token_url: &str) -> Auth {
        Auth::OAuth2(Box::new(
            serde_json::from_value(serde_json::json!({
                "flow": "client_credentials",
                "accessTokenUrl": token_url,
                "credentials": { "clientId": "id", "clientSecret": "s" }
            }))
            .expect("client credentials flow"),
        ))
    }

    fn pkcs12_line(domain: &str) -> String {
        format!("pkcs12 {domain} inline:{} pass:{FAKE_PASSWORD}", FAKE_BUNDLE.len())
    }

    #[tokio::test]
    async fn a_send_fetches_the_vault_certificate_selected_for_the_request_url() {
        let executor = Arc::new(CertificateRecordingExecutor::default());
        let fetcher =
            FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok), ("idp-cert", FakeExport::Ok)]);
        let svc = service(executor.clone(), fetcher.clone());

        svc.execute(input("https://api.example.com/v1", Auth::None))
            .await
            .expect("send");

        assert_eq!(fetcher.calls(), vec!["prod-vault/client-a/pem"]);
        let seen = executor.seen();
        assert_eq!(seen.len(), 1);
        assert!(seen[0][0].starts_with("pem api.example.com inline:"), "{seen:?}");
        assert!(seen[0][1].starts_with("deferred idp.example.com"), "{seen:?}");
    }

    // Review Focus 3.
    #[tokio::test]
    async fn a_request_to_another_domain_never_calls_the_vault() {
        let executor = Arc::new(CertificateRecordingExecutor::default());
        let fetcher =
            FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok), ("idp-cert", FakeExport::Ok)]);
        let svc = service(executor.clone(), fetcher.clone());

        svc.execute(input("https://elsewhere.example.org/", Auth::None))
            .await
            .expect("send");

        assert!(fetcher.calls().is_empty());
        let seen = executor.seen();
        assert!(seen[0].iter().all(|line| line.starts_with("deferred ")), "{seen:?}");
    }

    #[tokio::test]
    async fn a_client_credentials_send_also_fetches_the_certificate_for_the_token_url() {
        let executor = Arc::new(CertificateRecordingExecutor::default());
        let fetcher =
            FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok), ("idp-cert", FakeExport::Ok)]);
        let svc = service(executor.clone(), fetcher.clone());

        svc.execute(input(
            "https://api.example.com/v1",
            client_credentials("https://idp.example.com/token"),
        ))
        .await
        .expect("send");

        assert_eq!(
            fetcher.calls(),
            vec!["prod-vault/client-a/pem", "prod-vault/idp-cert/pkcs12"]
        );
        let seen = executor.seen();
        assert_eq!(seen[0][1], pkcs12_line("idp.example.com"));
    }

    #[tokio::test]
    async fn a_failed_fetch_reaches_the_executor_as_unavailable_with_the_reason() {
        let executor = Arc::new(CertificateRecordingExecutor::default());
        let fetcher = FakeCertificateFetcher::new(&[(
            "client-a",
            FakeExport::Fail("Certificate is not marked exportable."),
        )]);
        let svc = service(executor.clone(), fetcher.clone());

        // The recording executor sends anyway; the real one fails on the Unavailable entry
        // (pinned in rocket-infra by A1 and the existing Unavailable tests).
        svc.execute(input("https://api.example.com/v1", Auth::None))
            .await
            .expect("send");

        let seen = executor.seen();
        assert_eq!(
            seen[0][0],
            "unavailable api.example.com The RocketVault certificate client-a (binding prod) \
             for api.example.com could not be fetched: Certificate is not marked exportable."
        );
    }

    #[tokio::test]
    async fn the_recorded_request_keeps_names_only() {
        let executor = Arc::new(CertificateRecordingExecutor::default());
        let fetcher = FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok)]);
        let svc = service(executor.clone(), fetcher.clone());

        let mut sent = None;
        svc.execute_capturing(
            input("https://api.example.com/v1", Auth::None),
            &HashMap::new(),
            &mut sent,
        )
        .await
        .expect("send");

        let sent = sent.expect("the request is recorded");
        assert!(
            describe_all(&sent.options.client_certificates)[0].starts_with("deferred api.example.com"),
            "history and scripts see names only"
        );
        assert!(executor.seen()[0][0].starts_with("pem api.example.com inline:"));
    }

    fn oauth2_service(provider: Arc<RecordingTokenClientProvider>) -> OAuth2Service {
        OAuth2Service::new(
            Box::new(StaticEnvRepo(environment())),
            Box::new(SharedCollectionRepo(InMemoryCollectionRepo::new(Collection::new("c")))),
        )
        .with_token_client_provider(provider)
    }

    fn token_request() -> OAuth2GetTokenRequest {
        serde_json::from_value(serde_json::json!({
            "grantType": "client_credentials",
            "tokenUrl": "https://idp.example.com/token",
            "clientId": "id",
            "environmentName": "prod"
        }))
        .expect("token request")
    }

    #[tokio::test]
    async fn a_token_request_fetches_the_certificate_selected_for_the_token_url() {
        let provider = Arc::new(RecordingTokenClientProvider::default());
        let fetcher = FakeCertificateFetcher::new(&[("idp-cert", FakeExport::Ok)]);
        let svc = oauth2_service(provider.clone()).with_vault_access(
            Box::new(FakeSecretManagerRepo(connection())),
            Arc::new(FakeSecretStore("client-secret".into())),
            fetcher.clone(),
        );

        let config = svc.resolve_get_token_request(&token_request());
        let err = svc
            .get_token_direct(&config)
            .await
            .expect_err("the provider stops before the network");
        assert!(err.to_string().contains("stopped before the network"), "{err}");

        assert_eq!(fetcher.calls(), vec!["prod-vault/idp-cert/pkcs12"]);
        let seen = provider.seen.lock().expect("lock").clone();
        // The API entry is for another domain and stays deferred.
        assert!(seen[0][0].starts_with("deferred api.example.com"), "{seen:?}");
        assert_eq!(seen[0][1], pkcs12_line("idp.example.com"));
        // The resolved config still holds names only.
        assert!(describe_all(&config.client_certificates)[1].starts_with("deferred idp.example.com"));
    }

    #[tokio::test]
    async fn without_vault_access_a_token_request_fails_on_a_vault_certificate() {
        let provider = Arc::new(RecordingTokenClientProvider::default());
        let svc = oauth2_service(provider.clone());

        let config = svc.resolve_get_token_request(&token_request());
        let _ = svc.get_token_direct(&config).await;

        let seen = provider.seen.lock().expect("lock").clone();
        assert!(
            seen[0][1].starts_with("unavailable idp.example.com")
                && seen[0][1].contains("cannot be fetched here"),
            "{seen:?}"
        );
    }
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app vault_certificates::wiring_tests`
Expected: FAIL to compile with `no method named with_vault_access found for struct OAuth2Service`.

- [ ] **Step 3: Fetch in `send_request`**

In `crates/rocket-app/src/execution_service.rs`, lines 1413-1425, old:

```rust
    /// Dispatches the (possibly script-mutated) request.
    pub(crate) async fn send_request(&self, state: &PhaseState) -> DomainResult<HttpResponse> {
        let response = self.executor.execute(&state.http_request).await?;
```

new:

```rust
    /// Dispatches the (possibly script-mutated) request. A RocketVault certificate selected for
    /// its URL, or for its OAuth2 client-credentials token URL, is fetched first.
    pub(crate) async fn send_request(&self, state: &PhaseState) -> DomainResult<HttpResponse> {
        let request = self.with_vault_certificates(&state.http_request).await;
        let response = self.executor.execute(&request).await?;
```

After the closing `}` of `send_request` (the `Ok(response)` line and its `}`), add:

```rust

    /// The request with its selected RocketVault certificates fetched. The fetched copy lives
    /// only for this send: `state.http_request`, which history and scripts see, keeps the
    /// names-only form. A request with nothing to fetch is not copied.
    async fn with_vault_certificates<'r>(
        &self,
        request: &'r HttpRequest,
    ) -> std::borrow::Cow<'r, HttpRequest> {
        let urls = crate::vault_certificates::certificate_urls(request);
        if !crate::vault_certificates::needs_fetch(&request.options.client_certificates, &urls) {
            return std::borrow::Cow::Borrowed(request);
        }
        let access = crate::vault_certificates::VaultCertificateAccess {
            connections: self.secret_manager_repo.as_ref(),
            secret_store: self.vault_connection_secret_store.as_ref(),
            fetcher: self.vault_fetcher.as_ref(),
        };
        let mut fetched = request.clone();
        crate::vault_certificates::materialize_selected(
            &mut fetched.options.client_certificates,
            &urls,
            Some(&access),
        )
        .await;
        std::borrow::Cow::Owned(fetched)
    }
```

- [ ] **Step 4: Give `OAuth2Service` vault access**

In `crates/rocket-app/src/oauth2_service.rs`, lines 1-11, old:

```rust
use std::collections::HashMap;
use std::sync::Arc;

use rocket_collection::CollectionRepository;
use rocket_environment::{
    resolve, EnvironmentRepository, EnvironmentRepositoryFactory, VariableContext,
};
```

new:

```rust
use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;

use rocket_collection::CollectionRepository;
use rocket_environment::{
    resolve, EnvironmentRepository, EnvironmentRepositoryFactory, SecretManagerRepository,
    SecretStore, VariableContext, VaultSecretFetcher,
};
```

The struct (lines 116-125), old:

```rust
    /// Builds the client for token requests. Without it, a plain client is used and client
    /// certificates are not presented.
    token_client_provider: Option<Arc<dyn TokenClientProvider>>,
}
```

new:

```rust
    /// Builds the client for token requests. Without it, a plain client is used and client
    /// certificates are not presented.
    token_client_provider: Option<Arc<dyn TokenClientProvider>>,
    /// Lets a token request fetch a RocketVault certificate selected for the token URL.
    /// Without it, such a certificate fails the token request.
    vault_access: Option<OAuth2VaultAccess>,
}

/// The RocketVault pieces `OAuth2Service` needs to fetch a certificate.
struct OAuth2VaultAccess {
    connections: Box<dyn SecretManagerRepository>,
    secret_store: Arc<dyn SecretStore>,
    fetcher: Arc<dyn VaultSecretFetcher>,
}
```

In `new` (lines 132-137), old:

```rust
        Self {
            env_repo,
            collection_repo,
            collection_env_repo_factory: None,
            token_client_provider: None,
        }
```

new:

```rust
        Self {
            env_repo,
            collection_repo,
            collection_env_repo_factory: None,
            token_client_provider: None,
            vault_access: None,
        }
```

After `with_token_client_provider` (lines 148-151), add:

```rust

    /// Lets token requests fetch a RocketVault certificate selected for the token URL.
    pub fn with_vault_access(
        mut self,
        connections: Box<dyn SecretManagerRepository>,
        secret_store: Arc<dyn SecretStore>,
        fetcher: Arc<dyn VaultSecretFetcher>,
    ) -> Self {
        self.vault_access = Some(OAuth2VaultAccess {
            connections,
            secret_store,
            fetcher,
        });
        self
    }
```

After `token_client` (the method that ends at line 199), add:

```rust

    /// `certificates` with the entry selected for `url` fetched when it is a RocketVault
    /// certificate. A list with nothing to fetch is not copied.
    async fn certificates_for<'c>(
        &self,
        url: &str,
        certificates: &'c [ResolvedClientCertificate],
    ) -> Cow<'c, [ResolvedClientCertificate]> {
        if !crate::vault_certificates::needs_fetch(certificates, &[url]) {
            return Cow::Borrowed(certificates);
        }
        let access = self.vault_access.as_ref().map(|v| {
            crate::vault_certificates::VaultCertificateAccess {
                connections: v.connections.as_ref(),
                secret_store: v.secret_store.as_ref(),
                fetcher: v.fetcher.as_ref(),
            }
        });
        let mut fetched = certificates.to_vec();
        crate::vault_certificates::materialize_selected(&mut fetched, &[url], access.as_ref())
            .await;
        Cow::Owned(fetched)
    }
```

`get_token_direct`, lines 346-348, old:

```rust
        let url = apply_params_to_url(&config.token_url, &config.token_params);
        let client = self.token_client(&url, config.verify_ssl, &config.client_certificates)?;
        Self::post_token_request(&client, &url, &form, &extra_headers).await
    }

    /// Refreshes an OAuth2 token.
```

new:

```rust
        let url = apply_params_to_url(&config.token_url, &config.token_params);
        let certificates = self.certificates_for(&url, &config.client_certificates).await;
        let client = self.token_client(&url, config.verify_ssl, &certificates)?;
        Self::post_token_request(&client, &url, &form, &extra_headers).await
    }

    /// Refreshes an OAuth2 token.
```

`refresh_token_with_secrets`, lines 420-427, old:

```rust
        let certificates = self.client_certificates(
            req.collection.as_deref(),
            req.environment_name.as_deref(),
            &vars,
            external_secrets,
        );
        let client = self.token_client(&url, verify_ssl, &certificates)?;
```

new:

```rust
        let environment_certificates = self.client_certificates(
            req.collection.as_deref(),
            req.environment_name.as_deref(),
            &vars,
            external_secrets,
        );
        let certificates = self.certificates_for(&url, &environment_certificates).await;
        let client = self.token_client(&url, verify_ssl, &certificates)?;
```

`exchange_code_for_token`, lines 489-491, old:

```rust
        let client = self.token_client(&url, config.verify_ssl, &config.client_certificates)?;
        Self::post_token_request(&client, &url, &form, &extra_headers).await
    }

    /// Resolves all {{variables}} in the get-token request fields.
```

new:

```rust
        let certificates = self.certificates_for(&url, &config.client_certificates).await;
        let client = self.token_client(&url, config.verify_ssl, &certificates)?;
        Self::post_token_request(&client, &url, &form, &extra_headers).await
    }

    /// Resolves all {{variables}} in the get-token request fields.
```

- [ ] **Step 5: Run the wiring tests to verify they pass**

Run: `cargo test -j4 -p rocket-app vault_certificates`
Expected: PASS, 12 `tests` and 7 `wiring_tests`.

Run: `cargo test -j4 -p rocket-app execution_service` and `cargo test -j4 -p rocket-app oauth2_service` and `cargo test -j4 -p rocket-app collection_runner_service`
Expected: PASS, no existing test changes behaviour (a request with no selected vault certificate is not copied).

- [ ] **Step 6: Wire the OAuth2 vault access at startup**

In `src-tauri/src/lib.rs`, lines 418-423, old:

```rust
            // Client certificates live on a collection's own environment, and a token
            // endpoint that needs mutual TLS gets the matching one.
            .with_collection_env_repo_factory(Box::new(SharedCollectionEnvironmentRepo::new(
                Arc::clone(&active_workspace_path),
            )))
            .with_token_client_provider(Arc::new(rocket_infra::ReqwestTokenClientProvider));
```

new:

```rust
            // Client certificates live on a collection's own environment, and a token
            // endpoint that needs mutual TLS gets the matching one.
            .with_collection_env_repo_factory(Box::new(SharedCollectionEnvironmentRepo::new(
                Arc::clone(&active_workspace_path),
            )))
            .with_token_client_provider(Arc::new(rocket_infra::ReqwestTokenClientProvider))
            // A RocketVault certificate selected for a token URL is fetched at send time. It
            // shares the connection store and fetcher (and so the token and id caches) above.
            .with_vault_access(
                Box::new(rocket_infra::FsSecretManagerRepo::new(
                    data_dir.join("secret_managers.yml"),
                )),
                Arc::clone(&vault_connection_secret_store),
                Arc::clone(&vault_fetcher),
            );
```

- [ ] **Step 7: Workspace check and clippy**

Run: `cargo check -j4 --workspace`
Expected: `Finished`, no errors, and no `dead_code` warning for `vault_certificates.rs` any more.

Run: `cargo clippy -j4 -p rocket-app --all-targets` and `cargo clippy -j4 -p rocket --all-targets`
Expected: no warnings in the touched files.

- [ ] **Step 8: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add crates/rocket-app/src/execution_service.rs crates/rocket-app/src/oauth2_service.rs crates/rocket-app/src/vault_certificates.rs src-tauri/src/lib.rs
git commit -- crates/rocket-app/src/execution_service.rs crates/rocket-app/src/oauth2_service.rs crates/rocket-app/src/vault_certificates.rs src-tauri/src/lib.rs
```

Suggested subject: `feat(app): present RocketVault certificates on requests and token calls`. The message ends with `Relates to: #21`.

---

## Task C3: `list_vault_certificates` command and its TypeScript binding

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** sonnet.

**Files:**
- Modify: `crates/rocket-app/src/secret_manager_service.rs` (imports lines 1-7, new method after `fetch_secret_names` lines 114-123, tests)
- Modify: `src-tauri/src/commands/secret_managers.rs` (imports lines 1-6, new DTO after `ExternalSecretRefDto` line 59, new command at the end, new test module)
- Modify: `src-tauri/src/lib.rs` (command registration after line 708)
- Modify: `src/lib/tauri-api.ts` (new interface after `SecretManagerConnection` at line 203, new binding after `fetchExternalSecretNames` at line 1742)

**Interfaces:**
- Consumes: B2's `list_certificates`; C1's `FakeCertificateFetcher` in tests.
- Produces (contract names): `SecretManagerService::list_certificates`, `VaultCertificateSummaryDto`, `list_vault_certificates`, TypeScript `VaultCertificateSummary` and `listVaultCertificates`. Consumed by Plan D.

- [ ] **Step 1: Write the failing service tests**

In `crates/rocket-app/src/secret_manager_service.rs`, add at the end of `mod tests`:

```rust

    #[tokio::test]
    async fn list_certificates_returns_the_fetcher_output_unchanged() {
        let repo = FakeRepo::new();
        repo.save(&sample_connection("conn-1"))
            .expect("seed connection");
        let store = FakeSecretStore::new();
        store
            .set(VAULT_CONNECTION_SCOPE, "conn-1", "shh-its-a-secret")
            .expect("seed keychain entry");
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(store),
            crate::test_doubles::FakeCertificateFetcher::new(&[
                ("client-a", crate::test_doubles::FakeExport::Ok),
                ("locked", crate::test_doubles::FakeExport::Fail("not exportable")),
            ]),
        );

        let listed = service
            .list_certificates("conn-1", "prod-vault")
            .await
            .expect("list_certificates should succeed");

        let names: Vec<(&str, bool)> = listed
            .iter()
            .map(|c| (c.name.as_str(), c.exportable))
            .collect();
        assert_eq!(names, vec![("client-a", true), ("locked", false)]);
    }

    #[tokio::test]
    async fn list_certificates_needs_a_stored_client_secret() {
        let repo = FakeRepo::new();
        repo.save(&sample_connection("conn-1"))
            .expect("seed connection");
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(FakeSecretStore::new()), // no keychain entry seeded
            crate::test_doubles::FakeCertificateFetcher::new(&[]),
        );

        let result = service.list_certificates("conn-1", "prod-vault").await;

        assert!(
            matches!(result, Err(DomainError::Internal(_))),
            "expected DomainError::Internal for a connection with no stored secret, got {result:?}"
        );
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app secret_manager_service::tests::list_certificates`
Expected: FAIL to compile with `no method named list_certificates found for struct SecretManagerService`.

- [ ] **Step 3: Add `list_certificates`**

In `crates/rocket-app/src/secret_manager_service.rs`, line 2, old:

```rust
use rocket_environment::external_secret::ExternalSecretRef;
```

new:

```rust
use rocket_environment::external_secret::ExternalSecretRef;
use rocket_environment::VaultCertificateSummary;
```

After `fetch_secret_names` (the method that ends at line 123), add:

```rust

    /// Lists the certificates in `vault_name` through the connection `id`, for the Certificates
    /// tab picker. Names and metadata only, never key material.
    pub async fn list_certificates(
        &self,
        id: &str,
        vault_name: &str,
    ) -> DomainResult<Vec<VaultCertificateSummary>> {
        let (connection, secret) = self.connection_and_secret(id)?;
        self.fetcher
            .list_certificates(&connection, &secret, vault_name)
            .await
    }
```

- [ ] **Step 4: Run the service tests to verify they pass**

Run: `cargo test -j4 -p rocket-app secret_manager_service`
Expected: PASS, the existing tests and the 2 new ones.

- [ ] **Step 5: Write the failing DTO test**

In `src-tauri/src/commands/secret_managers.rs`, add at the end of the file:

```rust

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vault_certificate_dto_is_camel_case_and_carries_no_material() {
        let dto = VaultCertificateSummaryDto::from(VaultCertificateSummary {
            id: "id-1".into(),
            name: "client-a".into(),
            exportable: true,
            enabled: false,
            key_algorithm: "EC-P256".into(),
            expires_at: None,
        });
        let json = serde_json::to_value(&dto).expect("serialize");
        assert_eq!(
            json,
            serde_json::json!({
                "id": "id-1",
                "name": "client-a",
                "exportable": true,
                "enabled": false,
                "keyAlgorithm": "EC-P256",
                "expiresAt": null
            })
        );
    }
}
```

Run: `cargo test -j4 -p rocket vault_certificate_dto`
Expected: FAIL to compile with `cannot find type VaultCertificateSummaryDto in this scope`.

- [ ] **Step 6: Add the DTO, the command and the registration**

In `src-tauri/src/commands/secret_managers.rs`, lines 1-6, old:

```rust
use rocket_app::SecretManagerService;
use rocket_environment::external_secret::ExternalSecretRef;
use rocket_environment::secret_manager::SecretManagerConnection;
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;
```

new:

```rust
use rocket_app::SecretManagerService;
use rocket_environment::external_secret::ExternalSecretRef;
use rocket_environment::secret_manager::SecretManagerConnection;
use rocket_environment::VaultCertificateSummary;
use rocket_shared::error::DomainError;
use serde::{Deserialize, Serialize};
use tauri::State;
```

After `impl From<ExternalSecretRef> for ExternalSecretRefDto` (which ends at line 59), add:

```rust

/// One certificate for the Certificates tab picker. Names and metadata only, never key
/// material, so the IPC payload cannot carry a key.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VaultCertificateSummaryDto {
    pub id: String,
    pub name: String,
    pub exportable: bool,
    pub enabled: bool,
    pub key_algorithm: String,
    pub expires_at: Option<String>,
}

impl From<VaultCertificateSummary> for VaultCertificateSummaryDto {
    fn from(c: VaultCertificateSummary) -> Self {
        Self {
            id: c.id,
            name: c.name,
            exportable: c.exportable,
            enabled: c.enabled,
            key_algorithm: c.key_algorithm,
            expires_at: c.expires_at,
        }
    }
}
```

After `fetch_external_secret_names` (the last command, before the new test module), add:

```rust

#[tauri::command]
pub async fn list_vault_certificates(
    connection_id: String,
    vault_name: String,
    svc: State<'_, SecretManagerService>,
) -> Result<Vec<VaultCertificateSummaryDto>, DomainError> {
    Ok(svc
        .list_certificates(&connection_id, &vault_name)
        .await?
        .into_iter()
        .map(Into::into)
        .collect())
}
```

In `src-tauri/src/lib.rs`, after line 708 (`commands::secret_managers::fetch_external_secret_names,`), add:

```rust
            commands::secret_managers::list_vault_certificates,
```

- [ ] **Step 7: Run the DTO test to verify it passes**

Run: `cargo test -j4 -p rocket vault_certificate_dto`
Expected: PASS, 1 test.

- [ ] **Step 8: Add the TypeScript binding**

In `src/lib/tauri-api.ts`, after the `SecretManagerConnection` interface (which ends at line 203), add:

```ts

// A certificate in a RocketVault vault, for the Certificates tab picker. Never key material.
export interface VaultCertificateSummary {
  id: string;
  name: string;
  exportable: boolean;
  enabled: boolean;
  keyAlgorithm: string;
  expiresAt?: string | null;
}
```

After `fetchExternalSecretNames` (lines 1741-1742), add:

```ts

export const listVaultCertificates = (connectionId: string, vaultName: string) =>
  invoke<VaultCertificateSummary[]>('list_vault_certificates', { connectionId, vaultName });
```

Run: `yarn tsc --noEmit`
Expected: no errors.

Run: `yarn check`
Expected: no Biome errors in `src/lib/tauri-api.ts`.

- [ ] **Step 9: Workspace check and clippy**

Run: `cargo check -j4 --workspace`
Expected: `Finished`, no errors.

Run: `cargo clippy -j4 -p rocket-app --all-targets` and `cargo clippy -j4 -p rocket --all-targets`
Expected: no warnings in the touched files.

- [ ] **Step 10: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add crates/rocket-app/src/secret_manager_service.rs src-tauri/src/commands/secret_managers.rs src-tauri/src/lib.rs src/lib/tauri-api.ts
git commit -- crates/rocket-app/src/secret_manager_service.rs src-tauri/src/commands/secret_managers.rs src-tauri/src/lib.rs src/lib/tauri-api.ts
```

Suggested subject: `feat(vault): add the list_vault_certificates command`. The message ends with `Relates to: #21`.

---

## Milestone Checklist: Plan C

- [ ] `materialize_selected` fetches only the entries selected for the given URLs, once each, and turns a failure into `Unavailable` naming the certificate and the binding
- [ ] An entry for another domain causes no fetch; no fallback to another entry
- [ ] `send_request` fetches for the request URL and an in-send client-credentials token URL into a per-send copy; the recorded request keeps names only
- [ ] `OAuth2Service` fetches for the token URL in `get_token_direct`, `refresh_token_with_secrets` and `exchange_code_for_token`; without vault access the certificate fails the token request
- [ ] `list_vault_certificates(connectionId, vaultName)` returns camelCase summaries with no key material; `listVaultCertificates` exists in `tauri-api.ts`
- [ ] `cargo check -j4 --workspace`, clippy for each touched crate, `yarn tsc --noEmit` and `yarn check` are clean

## Next Plan

[Plan D: Certificates UI](04-plan-d-certificates-ui.md): adds the `vault` type to the frontend, mirrors the save rules, and gives the Certificates tab an "Add RocketVault certificate" entry with binding, certificate and format pickers, disabled non-exportable certificates, stale names shown as "not found", and the EC-with-PEM-on-Windows warning.
