# Environment Client Certificates Plan B: Model and Loading

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the serializable `ClientCertificate` that the HTTP layer carries today with a runtime-only `ResolvedClientCertificate` (key bytes and passphrase in `Zeroizing`, redacting `Debug`, not `Serialize`). Add optional vault reference fields to the persisted `ClientCertificate` with save-time validation. Load PEM and PKCS12 material held in memory (`CertificateSource::Inline`) in the executor and the OAuth2 token client.

**Architecture:** `rocket-http` gains `resolved_certificate.rs` (pure types, no I/O). `RequestOptions.client_certificates`, `find_certificate`, `certificate_covers` and `TokenClientProvider::client_for` switch to the resolved type. `rocket-app/src/client_certificates.rs` stays the single place that turns persisted entries into resolved ones (File sources only in this plan; Plan C adds vault lookup). `rocket-shared`'s persisted enum gains `certificateSecret`, `privateKeySecret` and `pkcs12Secret` references. `rocket-environment` gets `validate_client_certificates`, called by `EnvironmentService::save`. `rocket-infra`'s `load_identity` reads files or in-memory bytes and hands PEM to the existing `pem_key::unencrypted_key_pem`.

**Tech Stack:** Rust, serde, `zeroize`, `reqwest` 0.12 with `native-tls`, `pkcs8`, `wiremock`, `tempfile`, `serde_yaml`.

**Spec:** [`docs/superpowers/specs/2026-10-01-environment-client-certificates-design.md`](../../specs/2026-10-01-environment-client-certificates-design.md) (sections 4, 5, 7, 8, 13). Plan index and the shared interface contract: [`00-plan-index.md`](00-plan-index.md).

## Global Constraints

- Values fetched from RocketVault are never written to disk in any form: not the environment YAML, not history, not logs, not events, not the audit trail. Only references (`alias.secretName`) persist.
- Key bytes and passphrases held at runtime are `zeroize::Zeroizing` from resolution to use. The resolved certificate type is not `Serialize`, and its `Debug` never prints bytes or passphrases.
- A reference is `alias.secretName`, the same key RocketVault values already use in `VariableContext.external_secrets`. It is never a value.
- Each piece of material (certificate, private key, PKCS12 bundle) has exactly one source: a non-empty file path, or a reference.
- A file path or reference field must not hold key text. A value starting with `-----BEGIN` is rejected on save.
- An unresolved reference fails the request or token request only when that certificate is the one selected for the URL, with no fallback to another source.
- After decryption the PEM key must start exactly with `-----BEGIN PRIVATE KEY-----` (native TLS backend requirement).
- Serde: `#[serde(rename_all = "camelCase")]` on IPC DTOs only, never on persistence structs, except where `ClientCertificate` already has it on its variants. Persisted fields stay backward compatible (optional fields or defaults).
- Rust: never `unwrap()` in production paths. Always pass `-j4` to cargo. Do not run `cargo test --workspace`. Use targeted crate tests plus `cargo check -j4 --workspace`.
- Frontend: shadcn/ui primitives only (no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`), `lucide-react` icons only, `SingleLineEditor` for single-line variable-aware fields, Monaco only for multi-line editors, never fully destructure Zustand store state at component top level. Checks: `yarn tsc --noEmit` and `yarn check`.
- Commits: conventional commits, path-scoped (`git add <paths>` then `git commit -- <paths>`, never `git add -A` or `git commit -a`), because other sessions share this working tree. Before any commit invoke the `dev-workflow-skills:1-git-commit` skill. Commit messages end with `Relates to: #21`.
- Every task that touches collection, environment or certificate data models starts with: read `docs/superpowers/specs/opencollection-spec-reference.md`.

## Review Focus (items this plan owns)

2. **Vault values with CRLF line endings or trailing whitespace.** A PEM secret stored with `\r\n` or a trailing newline still loads. Owner: B3. Pinned by `inline_pem_with_crlf_line_endings_and_a_trailing_newline_loads` in `crates/rocket-infra/src/reqwest_executor.rs` (`mtls_tests`).
4. **Overlapping domains.** `*.example.com` listed before `api.example.com` wins by order. Owner: B1 (existing matcher kept). Pinned by the existing `first_matching_certificate_wins` in `crates/rocket-http/src/client_cert.rs`, migrated to the resolved type and still passing.
5. **A reference with no matching binding, or a pasted private key, in an environment file.** Rejected on save with a message that names the field. Owner: B2 (backend). Pinned by `rejects_a_reference_with_no_matching_binding` and `rejects_key_text_in_a_path_or_reference_field_and_names_the_field` in `crates/rocket-environment/src/client_certificate_validation.rs`, and `save_rejects_pasted_key_text_in_a_certificate` in `crates/rocket-app/src/environment_service.rs`.

## Test conventions

- New test code uses `.expect("message")` or `unwrap_err()` like the surrounding tests. Production code never calls `unwrap()`.
- `ResolvedClientCertificate` is deliberately not `PartialEq`. `rocket-app` tests compare it through the test-only helper `crate::client_certificates::describe_all`, added in Task B1.
- Line numbers below are from `main` at `c0ad93fb`. Line numbers shift after each task. Find code by the function or test name given.

---

## Task B1: `ResolvedClientCertificate` and the migration of the HTTP layer

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** sonnet.

**Files:**
- Create: `crates/rocket-http/src/resolved_certificate.rs`
- Modify: `crates/rocket-http/Cargo.toml` (add `zeroize = "1"`, same spec as `crates/rocket-infra/Cargo.toml:44`)
- Modify: `Cargo.lock` (updated by cargo for the new dependency)
- Modify: `crates/rocket-http/src/lib.rs` (module list lines 1-15, re-exports lines 17-34)
- Modify: `crates/rocket-http/src/request.rs` (import line 1, `RequestOptions.client_certificates` lines 31-34, tests lines 70-81)
- Modify: `crates/rocket-http/src/client_cert.rs` (import line 7, `certificate_domain` lines 9-14, `find_certificate` lines 22-32, `certificate_covers` lines 36-38, test helpers lines 90-100)
- Modify: `crates/rocket-http/src/token_client.rs` (import line 3, `client_for` lines 13-18)
- Modify: `crates/rocket-app/src/client_certificates.rs` (whole file, lines 1-114)
- Modify: `crates/rocket-app/src/execution_service.rs` (imports lines 13-16 and 21, `environment_client_certificates` lines 564-584, tests `resolve_request_carries_the_environment_client_certificates` lines 2857-2898, `cert_paths` lines 2929-2946)
- Modify: `crates/rocket-app/src/oauth2_service.rs` (imports lines 8-11, `ResolvedOAuth2Config.client_certificates` line 108, `client_certificates` lines 155-181, `token_client` lines 183-196, test imports lines 562-569, tests lines 909-1211)
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (import lines 8-9, `ClientIdentity`/`identity_for_url`/`ReqwestTokenClientProvider` lines 437-473, `load_identity` lines 516-560, `apply_auth` param line 603, `fetch_client_credentials_token` param line 839, `mtls_tests` helpers lines 1600-1619)

**Interfaces:**
- Consumes: `rocket_shared::certificate::ClientCertificate` as it is today (no reference fields yet), `pem_key::unencrypted_key_pem(key_pem: &[u8], passphrase: Option<&str>, path: &str) -> DomainResult<Zeroizing<Vec<u8>>>` (`crates/rocket-infra/src/pem_key.rs:23`).
- Produces (contract names): `CertificateSource { File(String), Inline(Zeroizing<Vec<u8>>) }`, `CertificateMaterial { Pem, Pkcs12, Unavailable }`, `ResolvedClientCertificate { domain, material }` with `pem`, `pkcs12`, `unavailable`; `find_certificate(&[ResolvedClientCertificate], &str)`, `certificate_covers(&ResolvedClientCertificate, &str)`, `TokenClientProvider::client_for(&self, &str, bool, &[ResolvedClientCertificate])`, `RequestOptions.client_certificates: Vec<ResolvedClientCertificate>` with `#[serde(skip)]`, `load_identity(&ResolvedClientCertificate)`, `identity_for_url(&[ResolvedClientCertificate], &str)`, and B1's `environment_client_certificates(repo, environment_name, collection_dir, vars) -> Vec<ResolvedClientCertificate>`. Consumed by B2, B3, C1, C2.

- [ ] **Step 1: Add the dependency and write the failing type tests**

In `crates/rocket-http/Cargo.toml`, add under `[dependencies]` after `urlencoding = "2"`:

```toml
zeroize = "1"
```

In `crates/rocket-http/src/lib.rs`, add `pub mod resolved_certificate;` between `pub mod request;` and `pub mod response;`.

Create `crates/rocket-http/src/resolved_certificate.rs` with only the tests for now:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const KEY: &[u8] =
        b"-----BEGIN PRIVATE KEY-----\nc2VjcmV0LWtleS1ieXRlcw==\n-----END PRIVATE KEY-----\n";

    fn inline(bytes: &[u8]) -> CertificateSource {
        CertificateSource::Inline(Zeroizing::new(bytes.to_vec()))
    }

    #[test]
    fn debug_shows_sources_but_never_key_bytes_or_the_passphrase() {
        let cert = ResolvedClientCertificate::pem(
            "api.example.com",
            CertificateSource::File("/certs/client.pem".into()),
            inline(KEY),
            Some("hunter2".into()),
        );
        let shown = format!("{cert:?}");
        assert!(shown.contains("api.example.com"), "{shown}");
        assert!(shown.contains("file /certs/client.pem"), "{shown}");
        assert!(
            shown.contains(&format!("inline {} bytes", KEY.len())),
            "{shown}"
        );
        assert!(shown.contains("<redacted>"), "{shown}");
        assert!(!shown.contains("hunter2"), "{shown}");
        assert!(!shown.contains("BEGIN"), "{shown}");
        assert!(!shown.contains("c2VjcmV0"), "{shown}");
    }

    #[test]
    fn debug_of_a_pkcs12_bundle_prints_only_its_size() {
        let cert = ResolvedClientCertificate::pkcs12(
            "*.example.com",
            inline(&[0x30, 0x82, 0x01, 0x02]),
            Some("changeit".into()),
        );
        let shown = format!("{cert:#?}");
        assert!(shown.contains("Pkcs12"), "{shown}");
        assert!(shown.contains("inline 4 bytes"), "{shown}");
        assert!(!shown.contains("changeit"), "{shown}");
        // A byte vector would print as `[48, 130, ...]`.
        assert!(!shown.contains('['), "{shown}");
    }

    #[test]
    fn an_unavailable_certificate_shows_its_domain_and_reason() {
        let cert = ResolvedClientCertificate::unavailable(
            "api.example.com",
            "Client certificate secret vault.clientCertPem was not found.",
        );
        let shown = format!("{cert:?}");
        assert!(shown.contains("Unavailable"), "{shown}");
        assert!(shown.contains("vault.clientCertPem"), "{shown}");
    }

    #[test]
    fn constructors_keep_the_domain_and_wrap_the_passphrase() {
        let cert = ResolvedClientCertificate::pkcs12(
            "api.example.com",
            CertificateSource::File("/c.p12".into()),
            Some("pw".into()),
        );
        assert_eq!(cert.domain, "api.example.com");
        match &cert.material {
            CertificateMaterial::Pkcs12 {
                bundle: CertificateSource::File(path),
                passphrase: Some(p),
            } => {
                assert_eq!(path, "/c.p12");
                assert_eq!(p.as_str(), "pw");
            }
            other => panic!("unexpected material {other:?}"),
        }
        let none = ResolvedClientCertificate::pem(
            "a",
            CertificateSource::File("/c.pem".into()),
            CertificateSource::File("/k.pem".into()),
            None,
        );
        assert!(matches!(
            none.material,
            CertificateMaterial::Pem { passphrase: None, .. }
        ));
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -j4 -p rocket-http resolved_certificate`
Expected: FAIL to compile with `cannot find type ResolvedClientCertificate in this scope` (and `CertificateSource`, `Zeroizing`).

- [ ] **Step 3: Implement the types**

Add above the tests module in `crates/rocket-http/src/resolved_certificate.rs`:

```rust
//! Client certificate material ready for the TLS layer.
//!
//! This is the runtime form of an environment's client certificate. It can hold key bytes and a
//! passphrase, so it is not `Serialize`, its secrets are wiped on drop, and its `Debug` shows
//! only the source kind and size.

use std::fmt;

use zeroize::Zeroizing;

/// Where one piece of material (certificate, private key or PKCS12 bundle) comes from.
#[derive(Clone)]
pub enum CertificateSource {
    /// An absolute path, or a `~/` path, read when the certificate is selected.
    File(String),
    /// The bytes themselves, for example PEM text or a PKCS12 bundle from a vault secret.
    Inline(Zeroizing<Vec<u8>>),
}

/// The material of one client certificate.
#[derive(Clone)]
pub enum CertificateMaterial {
    Pem {
        certificate: CertificateSource,
        private_key: CertificateSource,
        passphrase: Option<Zeroizing<String>>,
    },
    Pkcs12 {
        bundle: CertificateSource,
        passphrase: Option<Zeroizing<String>>,
    },
    /// The material could not be resolved. Selecting this certificate fails with `reason`.
    Unavailable { reason: String },
}

/// A client certificate with its domain, ready for the executor.
#[derive(Clone)]
pub struct ResolvedClientCertificate {
    pub domain: String,
    pub material: CertificateMaterial,
}

impl ResolvedClientCertificate {
    pub fn pem(
        domain: impl Into<String>,
        certificate: CertificateSource,
        private_key: CertificateSource,
        passphrase: Option<String>,
    ) -> Self {
        Self {
            domain: domain.into(),
            material: CertificateMaterial::Pem {
                certificate,
                private_key,
                passphrase: passphrase.map(Zeroizing::new),
            },
        }
    }

    pub fn pkcs12(
        domain: impl Into<String>,
        bundle: CertificateSource,
        passphrase: Option<String>,
    ) -> Self {
        Self {
            domain: domain.into(),
            material: CertificateMaterial::Pkcs12 {
                bundle,
                passphrase: passphrase.map(Zeroizing::new),
            },
        }
    }

    pub fn unavailable(domain: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            domain: domain.into(),
            material: CertificateMaterial::Unavailable {
                reason: reason.into(),
            },
        }
    }
}

/// Prints `<redacted>` in place of a passphrase.
struct Redacted;

impl fmt::Debug for Redacted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

// Hand-written so a `{:?}` of a request never prints key bytes.
impl fmt::Debug for CertificateSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CertificateSource::File(path) => write!(f, "file {path}"),
            CertificateSource::Inline(bytes) => write!(f, "inline {} bytes", bytes.len()),
        }
    }
}

// Hand-written so a `{:?}` of a request never prints a passphrase.
impl fmt::Debug for CertificateMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let redact = |p: &Option<Zeroizing<String>>| p.as_ref().map(|_| Redacted);
        match self {
            CertificateMaterial::Pem {
                certificate,
                private_key,
                passphrase,
            } => f
                .debug_struct("Pem")
                .field("certificate", certificate)
                .field("private_key", private_key)
                .field("passphrase", &redact(passphrase))
                .finish(),
            CertificateMaterial::Pkcs12 { bundle, passphrase } => f
                .debug_struct("Pkcs12")
                .field("bundle", bundle)
                .field("passphrase", &redact(passphrase))
                .finish(),
            CertificateMaterial::Unavailable { reason } => f
                .debug_struct("Unavailable")
                .field("reason", reason)
                .finish(),
        }
    }
}

impl fmt::Debug for ResolvedClientCertificate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ResolvedClientCertificate")
            .field("domain", &self.domain)
            .field("material", &self.material)
            .finish()
    }
}
```

In `crates/rocket-http/src/lib.rs`, add after `pub use response::HttpResponse;`:

```rust
pub use resolved_certificate::{CertificateMaterial, CertificateSource, ResolvedClientCertificate};
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-http resolved_certificate`
Expected: PASS, 4 tests.

- [ ] **Step 5: Write the failing `RequestOptions` serde test**

In `crates/rocket-http/src/request.rs`, add inside `mod tests` after `default_options`:

```rust
    #[test]
    fn client_certificates_never_cross_serde() {
        use crate::resolved_certificate::{CertificateSource, ResolvedClientCertificate};
        let options = RequestOptions {
            client_certificates: vec![ResolvedClientCertificate::pkcs12(
                "api.example.com",
                CertificateSource::File("/certs/client.p12".into()),
                Some("s3cret".into()),
            )],
            ..RequestOptions::default()
        };
        let json = serde_json::to_string(&options).expect("serialize options");
        assert!(!json.contains("clientCertificates"), "{json}");
        assert!(!json.contains("s3cret"), "{json}");

        // The IPC input cannot carry certificates: the environment is the only source.
        let back: RequestOptions = serde_json::from_str(
            r#"{"clientCertificates":[{"type":"pkcs12","domain":"evil.example.com","pkcs12FilePath":"/etc/shadow"}]}"#,
        )
        .expect("deserialize options");
        assert!(back.client_certificates.is_empty());
    }
```

- [ ] **Step 6: Run it to verify it fails**

Run: `cargo test -j4 -p rocket-http client_certificates_never_cross_serde`
Expected: FAIL to compile with `mismatched types: expected ClientCertificate, found ResolvedClientCertificate`.

- [ ] **Step 7: Migrate `request.rs`, `client_cert.rs` and `token_client.rs`**

`crates/rocket-http/src/request.rs` line 1, old:

```rust
use rocket_shared::certificate::ClientCertificate;
```

new:

```rust
use crate::resolved_certificate::ResolvedClientCertificate;
```

Lines 31-34, old:

```rust
    /// Client certificates of the active environment. The executor picks the one whose
    /// domain matches the request URL and presents it for mutual TLS.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub client_certificates: Vec<ClientCertificate>,
```

new:

```rust
    /// Client certificates of the active environment, resolved for this request. The executor
    /// picks the one whose domain matches the request URL and presents it for mutual TLS. It is
    /// never serialized: the environment is the only source, and the material can hold key bytes.
    #[serde(skip)]
    pub client_certificates: Vec<ResolvedClientCertificate>,
```

`crates/rocket-http/src/client_cert.rs` lines 1-14, old:

```rust
//! Client certificate selection for mutual TLS.
//!
//! Pure matching only: the executor in `rocket-infra` reads the files of the certificate
//! chosen here.

use reqwest::Url;
use rocket_shared::certificate::ClientCertificate;

/// Returns the `domain` a certificate is configured for.
pub fn certificate_domain(cert: &ClientCertificate) -> &str {
    match cert {
        ClientCertificate::Pem { domain, .. } | ClientCertificate::Pkcs12 { domain, .. } => domain,
    }
}
```

new (`certificate_domain` is removed, it has no other caller; `grep -rn certificate_domain crates src-tauri` finds only this file):

```rust
//! Client certificate selection for mutual TLS.
//!
//! Pure matching only: the executor in `rocket-infra` loads the material of the certificate
//! chosen here.

use reqwest::Url;

use crate::resolved_certificate::ResolvedClientCertificate;
```

Lines 22-38, old:

```rust
pub fn find_certificate<'a>(
    certs: &'a [ClientCertificate],
    url: &str,
) -> Option<&'a ClientCertificate> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let port = parsed.port_or_known_default();
    certs
        .iter()
        .find(|c| domain_matches(certificate_domain(c), &host, port))
}

/// Returns whether `cert` would be chosen for `url`, for checking a redirect target against the
/// certificate that was picked for the original request.
pub fn certificate_covers(cert: &ClientCertificate, url: &str) -> bool {
```

new:

```rust
pub fn find_certificate<'a>(
    certs: &'a [ResolvedClientCertificate],
    url: &str,
) -> Option<&'a ResolvedClientCertificate> {
    let parsed = Url::parse(url).ok()?;
    let host = parsed.host_str()?.to_ascii_lowercase();
    let port = parsed.port_or_known_default();
    certs
        .iter()
        .find(|c| domain_matches(&c.domain, &host, port))
}

/// Returns whether `cert` would be chosen for `url`, for checking a redirect target against the
/// certificate that was picked for the original request.
pub fn certificate_covers(cert: &ResolvedClientCertificate, url: &str) -> bool {
```

Test helpers, lines 88-100, old:

```rust
    use super::*;

    fn pkcs12(domain: &str) -> ClientCertificate {
        ClientCertificate::Pkcs12 {
            domain: domain.into(),
            pkcs12_file_path: format!("/certs/{domain}.p12"),
            passphrase: None,
        }
    }

    fn found(certs: &[ClientCertificate], url: &str) -> Option<String> {
        find_certificate(certs, url).map(|c| certificate_domain(c).to_string())
    }
```

new (every test body stays as it is, including `first_matching_certificate_wins`, Review Focus 4):

```rust
    use super::*;
    use crate::resolved_certificate::CertificateSource;

    fn pkcs12(domain: &str) -> ResolvedClientCertificate {
        ResolvedClientCertificate::pkcs12(
            domain,
            CertificateSource::File(format!("/certs/{domain}.p12")),
            None,
        )
    }

    fn found(certs: &[ResolvedClientCertificate], url: &str) -> Option<String> {
        find_certificate(certs, url).map(|c| c.domain.clone())
    }
```

`crates/rocket-http/src/token_client.rs` line 3, old `use rocket_shared::certificate::ClientCertificate;`, new `use crate::resolved_certificate::ResolvedClientCertificate;`. Line 17, old `        certificates: &[ClientCertificate],`, new `        certificates: &[ResolvedClientCertificate],`. In the doc comment line 9, replace `because loading a certificate reads` / `/// files.` with `because loading a certificate can read` / `/// files.`.

- [ ] **Step 8: Run the `rocket-http` tests**

Run: `cargo test -j4 -p rocket-http`
Expected: PASS, including `client_certificates_never_cross_serde`, the 4 `resolved_certificate` tests and all 10 `client_cert::tests` (among them `first_matching_certificate_wins`).

- [ ] **Step 9: Write the failing executor tests**

In `crates/rocket-infra/src/reqwest_executor.rs`, `mod mtls_tests`, replace the helpers `p12`, `pem` and `pem_with_passphrase` (lines 1600-1619) with:

```rust
    fn p12(domain: &str, path: String, passphrase: Option<&str>) -> ResolvedClientCertificate {
        ResolvedClientCertificate::pkcs12(
            domain,
            CertificateSource::File(path),
            passphrase.map(String::from),
        )
    }

    fn pem(domain: &str, key: &str) -> ResolvedClientCertificate {
        pem_with_passphrase(domain, key, None)
    }

    fn pem_with_passphrase(
        domain: &str,
        key: &str,
        passphrase: Option<&str>,
    ) -> ResolvedClientCertificate {
        ResolvedClientCertificate::pem(
            domain,
            CertificateSource::File(fixture("client.pem")),
            CertificateSource::File(fixture(key)),
            passphrase.map(String::from),
        )
    }
```

Add after `a_missing_file_is_an_error_that_names_the_path`:

```rust
    #[test]
    fn inline_material_is_not_supported_yet() {
        let cert = ResolvedClientCertificate::pkcs12(
            "x",
            CertificateSource::Inline(zeroize::Zeroizing::new(vec![1, 2, 3])),
            None,
        );
        let err = load_identity(&cert).unwrap_err().to_string();
        assert!(
            err.contains("Inline client certificate material is not supported yet"),
            "{err}"
        );
    }

    #[test]
    fn an_unavailable_certificate_fails_with_its_reason() {
        let reason = "Client certificate secret vault.clientCertPem was not found.";
        let err = load_identity(&ResolvedClientCertificate::unavailable("x", reason)).unwrap_err();
        assert!(
            matches!(&err, DomainError::InvalidInput(m) if m == reason),
            "{err:?}"
        );
    }
```

Add after `a_certificate_for_another_domain_is_not_loaded`:

```rust
    #[tokio::test]
    async fn an_unavailable_certificate_for_another_domain_does_not_block_the_request() {
        let server = ok_server().await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
        req.options.client_certificates = vec![ResolvedClientCertificate::unavailable(
            "other.example.com",
            "Client certificate secret vault.clientCertPem was not found.",
        )];
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();
        assert_eq!(resp.status, 200);
    }
```

- [ ] **Step 10: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-infra mtls_tests`
Expected: FAIL to compile with `cannot find type ResolvedClientCertificate in this scope` and `expected &[ClientCertificate], found &Vec<ResolvedClientCertificate>`.

- [ ] **Step 11: Migrate the executor**

`crates/rocket-infra/src/reqwest_executor.rs` lines 8-9, old:

```rust
use rocket_http::{HttpExecutor, HttpRequest, HttpResponse};
use rocket_shared::certificate::ClientCertificate;
```

new:

```rust
use rocket_http::{
    CertificateMaterial, CertificateSource, HttpExecutor, HttpRequest, HttpResponse,
    ResolvedClientCertificate,
};
```

Lines 437-473 (`ClientIdentity`, `identity_for_url`, `ReqwestTokenClientProvider`), old: from `/// A loaded TLS identity together with the certificate entry it came from, whose domain says` through the closing `}` of `impl rocket_http::TokenClientProvider for ReqwestTokenClientProvider`. New:

```rust
/// A loaded TLS identity together with the domain scope of the certificate it came from, which
/// says which hosts may see it.
struct ClientIdentity {
    identity: reqwest::Identity,
    /// Only the domain is used, to scope redirects. It carries no key material.
    certificate: ResolvedClientCertificate,
}

/// Loads the identity of the certificate chosen for `url`, if one matches. A certificate that
/// matches but cannot be loaded is an error, so the request is never sent without it.
fn identity_for_url(
    certificates: &[ResolvedClientCertificate],
    url: &str,
) -> DomainResult<Option<ClientIdentity>> {
    match rocket_http::client_cert::find_certificate(certificates, url) {
        Some(cert) => Ok(Some(ClientIdentity {
            identity: load_identity(cert)?,
            // The redirect policy keeps this for the life of the client, so no bytes are copied.
            certificate: ResolvedClientCertificate::unavailable(
                cert.domain.clone(),
                "redirect scope only",
            ),
        })),
        None => Ok(None),
    }
}

/// Builds the client for OAuth2 token requests. A token endpoint that requires mutual TLS
/// gets the matching environment certificate, like the request itself does.
pub struct ReqwestTokenClientProvider;

impl rocket_http::TokenClientProvider for ReqwestTokenClientProvider {
    fn client_for(
        &self,
        token_url: &str,
        verify_ssl: bool,
        certificates: &[ResolvedClientCertificate],
    ) -> DomainResult<Client> {
        let identity = identity_for_url(certificates, token_url)?;
        build_client_with_identity(true, verify_ssl, None, identity)
    }
}
```

`build_client_with_identity` stays as it is: `certificate_covers(&scope, ...)` now receives the domain-only copy.

Lines 516-560 (the doc comment of `load_identity` and the function), old: from `/// Reads a client certificate's files and turns them into a TLS identity.` through the closing `}` of `fn load_identity`. New:

```rust
/// Turns a client certificate's material into a TLS identity.
///
/// The TLS backend is the platform one (native-tls), which loads PKCS12 bundles and
/// unencrypted PKCS#8 PEM keys. An encrypted PKCS#8 key is decrypted in memory first.
/// `Unavailable` material fails with its reason, since this certificate was selected.
fn load_identity(cert: &ResolvedClientCertificate) -> DomainResult<reqwest::Identity> {
    match &cert.material {
        CertificateMaterial::Pkcs12 { bundle, passphrase } => {
            let der = read_der_source(bundle)?;
            let passphrase = passphrase.as_deref().map_or("", |p| p.as_str());
            reqwest::Identity::from_pkcs12_der(&der, passphrase).map_err(|e| {
                DomainError::InvalidInput(format!(
                    "Cannot load PKCS12 client certificate {}: {e}. \
                     Check the file and its passphrase.",
                    source_name(bundle, &cert.domain)
                ))
            })
        }
        CertificateMaterial::Pem {
            certificate,
            private_key,
            passphrase,
        } => {
            let cert_pem = read_pem_source(certificate)?;
            let key_file = read_pem_source(private_key)?;
            // An encrypted PKCS#8 key is decrypted in memory, and the key bytes are wiped on drop.
            let key_pem = crate::pem_key::unencrypted_key_pem(
                &key_file,
                passphrase.as_deref().map(|p| p.as_str()),
                &source_name(private_key, &cert.domain),
            )?;
            reqwest::Identity::from_pkcs8_pem(&cert_pem, &key_pem).map_err(|e| {
                DomainError::InvalidInput(format!(
                    "Cannot load PEM client certificate {}: {e}",
                    source_name(certificate, &cert.domain)
                ))
            })
        }
        CertificateMaterial::Unavailable { reason } => {
            Err(DomainError::InvalidInput(reason.clone()))
        }
    }
}

/// Reads binary material (a PKCS12 bundle). The bytes are wiped on drop.
fn read_der_source(source: &CertificateSource) -> DomainResult<zeroize::Zeroizing<Vec<u8>>> {
    match source {
        CertificateSource::File(path) => Ok(zeroize::Zeroizing::new(read_certificate_file(path)?)),
        CertificateSource::Inline(_) => Err(inline_not_supported()),
    }
}

/// Reads PEM text (a certificate or a private key). The bytes are wiped on drop.
fn read_pem_source(source: &CertificateSource) -> DomainResult<zeroize::Zeroizing<Vec<u8>>> {
    match source {
        CertificateSource::File(path) => Ok(zeroize::Zeroizing::new(read_certificate_file(path)?)),
        CertificateSource::Inline(_) => Err(inline_not_supported()),
    }
}

fn inline_not_supported() -> DomainError {
    DomainError::InvalidInput("Inline client certificate material is not supported yet".into())
}

/// Names a piece of material in an error message: its file path, never its bytes.
fn source_name(source: &CertificateSource, domain: &str) -> String {
    match source {
        CertificateSource::File(path) => path.clone(),
        CertificateSource::Inline(_) => format!("(inline, for {domain})"),
    }
}
```

`apply_auth`, line 603, old `    certificates: &[ClientCertificate],`, new `    certificates: &[ResolvedClientCertificate],`. `fetch_client_credentials_token`, line 839, the same change.

- [ ] **Step 12: Run the executor tests**

Run: `cargo test -j4 -p rocket-infra mtls_tests`
Expected: PASS, every existing `mtls_tests` test plus `inline_material_is_not_supported_yet`, `an_unavailable_certificate_fails_with_its_reason` and `an_unavailable_certificate_for_another_domain_does_not_block_the_request` (the handshake test stays ignored).

- [ ] **Step 13: Rewrite `rocket-app/src/client_certificates.rs`**

Replace the whole file with:

```rust
//! Client certificates of the selected environment, prepared for the HTTP layer.
//!
//! Shared by request execution and the OAuth2 token requests, so both resolve placeholders and
//! relative paths in exactly the same way.

use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};

use rocket_environment::{resolve, EnvironmentRepository};
use rocket_http::{CertificateSource, ResolvedClientCertificate};
use rocket_shared::certificate::ClientCertificate;

/// Returns the named environment's client certificates, ready for the executor: `{{placeholders}}`
/// resolved with `vars`, and relative file paths joined onto `collection_dir`.
///
/// No environment name, or an environment that cannot be read, means no certificates, like it
/// means no variables.
pub(crate) fn environment_client_certificates(
    repo: &dyn EnvironmentRepository,
    environment_name: Option<&str>,
    collection_dir: Option<&Path>,
    vars: &HashMap<String, String>,
) -> Vec<ResolvedClientCertificate> {
    let Some(name) = environment_name else {
        return Vec::new();
    };
    let Ok(env) = repo.get(name) else {
        return Vec::new();
    };
    env.client_certificates
        .into_iter()
        .map(|c| resolve_client_certificate(c, collection_dir, vars))
        .collect()
}

/// Resolves `{{placeholders}}` in a certificate's domain, file paths and passphrase, then joins
/// relative file paths onto the collection folder.
fn resolve_client_certificate(
    cert: ClientCertificate,
    base: Option<&Path>,
    vars: &HashMap<String, String>,
) -> ResolvedClientCertificate {
    let r = |s: String| resolve(&s, vars).output;
    let file = |p: String| CertificateSource::File(absolutize(r(p), base));
    match cert {
        ClientCertificate::Pem {
            domain,
            certificate_file_path,
            private_key_file_path,
            passphrase,
        } => ResolvedClientCertificate::pem(
            r(domain),
            file(certificate_file_path),
            file(private_key_file_path),
            passphrase.map(&r),
        ),
        ClientCertificate::Pkcs12 {
            domain,
            pkcs12_file_path,
            passphrase,
        } => ResolvedClientCertificate::pkcs12(
            r(domain),
            file(pkcs12_file_path),
            passphrase.map(&r),
        ),
    }
}

/// Joins a relative certificate file path onto the collection folder `base`.
///
/// Absolute paths and `~/` paths stay as written. So does a relative path with a `..` in it, so
/// an environment file cannot point outside the collection folder. The executor rejects any
/// path that is still relative, with a message that says what is allowed.
fn absolutize(p: String, base: Option<&Path>) -> String {
    let Some(base) = base else { return p };
    let path = Path::new(&p);
    let stays = p.is_empty()
        || p.starts_with("~/")
        || path.is_absolute()
        || path.components().any(|c| matches!(c, Component::ParentDir));
    if stays {
        return p;
    }
    // Drop `.` components so `./certs/a.pem` joins as `certs/a.pem`.
    let tidy: PathBuf = path
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect();
    base.join(tidy).to_string_lossy().into_owned()
}

/// One line per certificate, for test assertions: `pkcs12 <domain> file:<path> pass:<value>`.
/// It prints the passphrase, so it only exists in tests.
#[cfg(test)]
pub(crate) fn describe_all(certs: &[ResolvedClientCertificate]) -> Vec<String> {
    use rocket_http::CertificateMaterial;
    let source = |s: &CertificateSource| match s {
        CertificateSource::File(path) => format!("file:{path}"),
        CertificateSource::Inline(bytes) => format!("inline:{}", bytes.len()),
    };
    certs
        .iter()
        .map(|c| match &c.material {
            CertificateMaterial::Pem {
                certificate,
                private_key,
                passphrase,
            } => format!(
                "pem {} {} {} pass:{}",
                c.domain,
                source(certificate),
                source(private_key),
                passphrase.as_deref().map_or("-", |p| p.as_str())
            ),
            CertificateMaterial::Pkcs12 { bundle, passphrase } => format!(
                "pkcs12 {} {} pass:{}",
                c.domain,
                source(bundle),
                passphrase.as_deref().map_or("-", |p| p.as_str())
            ),
            CertificateMaterial::Unavailable { reason } => {
                format!("unavailable {} {reason}", c.domain)
            }
        })
        .collect()
}
```

- [ ] **Step 14: Migrate `execution_service.rs`**

Imports, lines 13-16, old:

```rust
use rocket_http::{
    run_load_test as http_run_load_test, CookieRepository, HttpExecutor, HttpRequest, HttpResponse,
    LoadTestConfig, LoadTestResult, RequestOptions,
};
```

new:

```rust
use rocket_http::{
    run_load_test as http_run_load_test, CookieRepository, HttpExecutor, HttpRequest, HttpResponse,
    LoadTestConfig, LoadTestResult, RequestOptions, ResolvedClientCertificate,
};
```

Delete line 21 `use rocket_shared::certificate::ClientCertificate;` (no production use is left). In `fn environment_client_certificates` (line 570), old `    ) -> Vec<ClientCertificate> {`, new `    ) -> Vec<ResolvedClientCertificate> {`.

In `mod tests` (line 1822), add after `use super::*;`:

```rust
    use crate::client_certificates::describe_all;
    use rocket_http::{CertificateMaterial, CertificateSource};
    use rocket_shared::certificate::ClientCertificate;
```

In `resolve_request_carries_the_environment_client_certificates`, old (from `input.options.client_certificates = vec![ClientCertificate::Pkcs12 {` to the end of the final `assert_eq!`):

```rust
        input.options.client_certificates = vec![ClientCertificate::Pkcs12 {
            domain: "evil.example.com".into(),
            pkcs12_file_path: "/etc/shadow".into(),
            passphrase: None,
        }];
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");

        assert_eq!(
            resolved.options.client_certificates,
            vec![ClientCertificate::Pkcs12 {
                domain: "api.example.com".into(),
                pkcs12_file_path: "/certs/client.p12".into(),
                passphrase: Some("s3cret".into()),
            }]
        );
```

new:

```rust
        input.options.client_certificates = vec![ResolvedClientCertificate::pkcs12(
            "evil.example.com",
            CertificateSource::File("/etc/shadow".into()),
            None,
        )];
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");

        assert_eq!(
            describe_all(&resolved.options.client_certificates),
            ["pkcs12 api.example.com file:/certs/client.p12 pass:s3cret"]
        );
```

`cert_paths` (lines 2929-2946), old: the whole function. New:

```rust
    fn cert_paths(certs: &[ResolvedClientCertificate]) -> Vec<String> {
        let path = |s: &CertificateSource| match s {
            CertificateSource::File(p) => p.clone(),
            CertificateSource::Inline(_) => "<inline>".to_string(),
        };
        certs
            .iter()
            .flat_map(|c| match &c.material {
                CertificateMaterial::Pem {
                    certificate,
                    private_key,
                    ..
                } => vec![path(certificate), path(private_key)],
                CertificateMaterial::Pkcs12 { bundle, .. } => vec![path(bundle)],
                CertificateMaterial::Unavailable { .. } => Vec::new(),
            })
            .collect()
    }
```

The environments built with `ClientCertificate::Pem`/`Pkcs12` in these tests stay as they are (they are the persisted input).

- [ ] **Step 15: Migrate `oauth2_service.rs`**

Imports, lines 8-11, old:

```rust
use rocket_http::{
    apply_params_to_body, apply_params_to_url, AdditionalParam, OAuthToken, TokenClientProvider,
};
use rocket_shared::certificate::ClientCertificate;
```

new:

```rust
use rocket_http::{
    apply_params_to_body, apply_params_to_url, AdditionalParam, OAuthToken,
    ResolvedClientCertificate, TokenClientProvider,
};
```

Line 108 `    pub client_certificates: Vec<ClientCertificate>,` becomes `    pub client_certificates: Vec<ResolvedClientCertificate>,`. Line 160 `    ) -> Vec<ClientCertificate> {` becomes `    ) -> Vec<ResolvedClientCertificate> {`. Line 187 `        certificates: &[ClientCertificate],` becomes `        certificates: &[ResolvedClientCertificate],`.

In `mod tests`, add after `use rocket_shared::error::{DomainError, DomainResult};` (line 569):

```rust
    use crate::client_certificates::describe_all;
    use rocket_shared::certificate::ClientCertificate;
```

`CapturingProvider`: `seen: std::sync::Mutex<Vec<(String, bool, Vec<ClientCertificate>)>>,` becomes `seen: std::sync::Mutex<Vec<(String, bool, Vec<ResolvedClientCertificate>)>>,`, and its `client_for` parameter `certificates: &[ClientCertificate],` becomes `certificates: &[ResolvedClientCertificate],`. The helpers `pkcs12`, `env_with_certificates` and `service_with_certificates` keep building persisted `ClientCertificate`s.

Assertion changes, test by test (old, then new):

`resolving_a_get_token_request_carries_the_environment_certificates`:

```rust
        assert_eq!(
            config.client_certificates,
            vec![pkcs12("idp.example.com", "/certs/client.p12")]
        );
```

```rust
        assert_eq!(
            describe_all(&config.client_certificates),
            ["pkcs12 idp.example.com file:/certs/client.p12 pass:-"]
        );
```

`a_direct_grant_asks_the_provider_for_a_client_with_the_certificates`:

```rust
        assert_eq!(seen[0].2, vec![pkcs12("idp.example.com", "/c.p12")]);
```

```rust
        assert_eq!(
            describe_all(&seen[0].2),
            ["pkcs12 idp.example.com file:/c.p12 pass:-"]
        );
```

`a_refresh_resolves_certificates_from_its_own_environment`:

```rust
        assert_eq!(
            seen[0].2,
            vec![pkcs12("idp.example.com", "/certs/client.p12")]
        );
```

```rust
        assert_eq!(
            describe_all(&seen[0].2),
            ["pkcs12 idp.example.com file:/certs/client.p12 pass:-"]
        );
```

`vault_references_resolve_in_a_get_token_request_when_secrets_are_given`:

```rust
        assert_eq!(
            config.client_certificates,
            vec![ClientCertificate::Pkcs12 {
                domain: "idp.example.com".into(),
                pkcs12_file_path: "/c.p12".into(),
                passphrase: Some("p4ss".into()),
            }]
        );
```

```rust
        assert_eq!(
            describe_all(&config.client_certificates),
            ["pkcs12 idp.example.com file:/c.p12 pass:p4ss"]
        );
```

`a_refresh_resolves_a_vault_passphrase_for_the_certificate`:

```rust
        assert!(matches!(
            &seen[0].2[0],
            ClientCertificate::Pkcs12 { passphrase: Some(p), .. } if p == "p4ss"
        ));
```

```rust
        assert_eq!(
            describe_all(&seen[0].2),
            ["pkcs12 idp.example.com file:/c.p12 pass:p4ss"]
        );
```

`a_collection_environment_is_used_and_relative_paths_join_its_folder`:

```rust
        assert_eq!(
            config.client_certificates,
            vec![pkcs12(
                "idp.example.com",
                "/ws/collections/api/certs/client.p12"
            )]
        );
```

```rust
        assert_eq!(
            describe_all(&config.client_certificates),
            ["pkcs12 idp.example.com file:/ws/collections/api/certs/client.p12 pass:-"]
        );
```

- [ ] **Step 16: Run the `rocket-app` tests**

Run: `cargo test -j4 -p rocket-app certificate` then `cargo test -j4 -p rocket-app oauth2_service`
Expected: PASS for both. The first covers `resolve_request_carries_the_environment_client_certificates`, `relative_certificate_paths_resolve_against_the_collection_folder`, `relative_certificate_paths_stay_as_written_without_a_known_collection_folder`, `resolve_request_has_no_client_certificates_without_an_environment` and the OAuth2 certificate tests; the second covers every OAuth2 test, including `a_collection_environment_is_used_and_relative_paths_join_its_folder` and `vault_references_resolve_in_a_get_token_request_when_secrets_are_given`.

- [ ] **Step 17: Workspace check, clippy and the real handshake**

Run: `cargo check -j4 --workspace`
Expected: `Finished`, no errors (`src-tauri` only uses `rocket_infra::ReqwestTokenClientProvider`, which keeps its name).

Run: `cargo clippy -j4 -p rocket-http --all-targets`, `cargo clippy -j4 -p rocket-infra --all-targets`, `cargo clippy -j4 -p rocket-app --all-targets`
Expected: no warnings in the changed files.

Run: `cargo test -j4 -p rocket-infra mutual_tls_handshake -- --ignored`
Expected: PASS (file-based PKCS12, PEM and encrypted PEM still complete a real handshake).

- [ ] **Step 18: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add Cargo.lock crates/rocket-http/Cargo.toml crates/rocket-http/src/resolved_certificate.rs crates/rocket-http/src/lib.rs crates/rocket-http/src/request.rs crates/rocket-http/src/client_cert.rs crates/rocket-http/src/token_client.rs crates/rocket-app/src/client_certificates.rs crates/rocket-app/src/execution_service.rs crates/rocket-app/src/oauth2_service.rs crates/rocket-infra/src/reqwest_executor.rs
git commit -- Cargo.lock crates/rocket-http/Cargo.toml crates/rocket-http/src/resolved_certificate.rs crates/rocket-http/src/lib.rs crates/rocket-http/src/request.rs crates/rocket-http/src/client_cert.rs crates/rocket-http/src/token_client.rs crates/rocket-app/src/client_certificates.rs crates/rocket-app/src/execution_service.rs crates/rocket-app/src/oauth2_service.rs crates/rocket-infra/src/reqwest_executor.rs
```

Suggested subject: `refactor(http): carry resolved client certificates at runtime`. The message ends with `Relates to: #21`.

---

## Task B2: Persisted vault references and save-time validation

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** sonnet.

**Files:**
- Modify: `crates/rocket-shared/src/certificate.rs` (enum lines 3-23, `Debug` lines 25-54, tests lines 56-133)
- Create: `crates/rocket-environment/src/client_certificate_validation.rs`
- Modify: `crates/rocket-environment/src/lib.rs` (module list lines 1-10, re-exports lines 12-21)
- Modify: `crates/rocket-app/src/environment_service.rs` (`save` lines 41-45, tests lines 74-199)
- Modify: `crates/rocket-app/src/client_certificates.rs` (`resolve_client_certificate`, as written in B1)
- Modify: `crates/rocket-app/src/execution_service.rs` (test fixtures in `resolve_request_carries_the_environment_client_certificates` and `relative_path_env`, one new test)
- Modify: `crates/rocket-app/src/oauth2_service.rs` (test fixtures `pkcs12`, `vault_references_resolve_in_a_get_token_request_when_secrets_are_given`, `a_refresh_resolves_a_vault_passphrase_for_the_certificate`)
- Modify: `crates/rocket-infra/src/conversions/tests.rs` (`environment_client_certificates_survive_oc_roundtrip`, lines 1331-1349)
- Modify: `crates/rocket-infra/src/fs_environment_repo.rs` (tests module, lines 207-713)
- Modify: `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs` (allow-lists after line 170, new test at the end of the file)

No change is needed in `crates/rocket-infra/src/oc/environment.rs` (line 47) or `crates/rocket-infra/src/conversions/environment.rs` (lines 75 and 111): `OcEnvironment.client_certificates` is `Vec<rocket_shared::certificate::ClientCertificate>` (imported as `OcClientCertificate`) and both conversions move the vector unchanged, so the new serde attributes on the shared enum are the YAML format. The tests in this task pin that.

**Interfaces:**
- Consumes: `ResolvedClientCertificate::unavailable` (B1), `ExternalSecretBinding { alias, connection_id, vault_name, secret_names: Vec<ExternalSecretRef { name, secret_id }> }` (`crates/rocket-environment/src/external_secret.rs:18-25`).
- Produces: the persisted `ClientCertificate` of the contract (with `certificate_secret`, `private_key_secret`, `pkcs12_secret`, empty-skipped paths) and `ClientCertificate::domain(&self) -> &str`; `rocket_environment::validate_client_certificates(certs: &[ClientCertificate], bindings: &[ExternalSecretBinding]) -> DomainResult<()>`. Consumed by C1 (resolution), D1 (frontend type mirror).

- [ ] **Step 1: Write the failing persisted-model tests**

In `crates/rocket-shared/src/certificate.rs`, add inside `mod tests` after `certificate_dispatch_on_type`:

```rust
    #[test]
    fn a_vault_sourced_pem_writes_only_its_references() {
        let cert = ClientCertificate::Pem {
            domain: "api.example.com".into(),
            certificate_file_path: String::new(),
            private_key_file_path: String::new(),
            certificate_secret: Some("vault.clientCertPem".into()),
            private_key_secret: Some("vault.clientKeyPem".into()),
            passphrase: Some("{{vault.clientKeyPass}}".into()),
        };
        let json = serde_json::to_string(&cert).expect("serialize");
        assert!(!json.contains("FilePath"), "{json}");
        assert!(
            json.contains("\"certificateSecret\":\"vault.clientCertPem\""),
            "{json}"
        );
        assert!(
            json.contains("\"privateKeySecret\":\"vault.clientKeyPem\""),
            "{json}"
        );
        let back: ClientCertificate = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cert, back);
    }

    #[test]
    fn a_vault_sourced_pkcs12_writes_only_its_reference() {
        let cert = ClientCertificate::Pkcs12 {
            domain: "*.internal.example.com".into(),
            pkcs12_file_path: String::new(),
            pkcs12_secret: Some("vault.clientBundleB64".into()),
            passphrase: None,
        };
        let json = serde_json::to_string(&cert).expect("serialize");
        assert!(!json.contains("pkcs12FilePath"), "{json}");
        assert!(
            json.contains("\"pkcs12Secret\":\"vault.clientBundleB64\""),
            "{json}"
        );
        let back: ClientCertificate = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cert, back);
    }

    #[test]
    fn an_old_file_only_entry_loads_and_round_trips_unchanged() {
        let old_entries = [
            r#"{"type":"pem","domain":"a.com","certificateFilePath":"/c.pem","privateKeyFilePath":"/k.pem"}"#,
            r#"{"type":"pkcs12","domain":"b.com","pkcs12FilePath":"/c.p12","passphrase":"{{p}}"}"#,
        ];
        for old in old_entries {
            let cert: ClientCertificate = serde_json::from_str(old).expect("old entry loads");
            let written = serde_json::to_value(&cert).expect("serialize");
            let original: serde_json::Value = serde_json::from_str(old).expect("parse");
            assert_eq!(written, original, "{old}");
        }
    }

    #[test]
    fn domain_returns_the_domain_of_either_variant() {
        let pem: ClientCertificate = serde_json::from_str(
            r#"{"type":"pem","domain":"a.com","certificateFilePath":"/c.pem","privateKeyFilePath":"/k.pem"}"#,
        )
        .expect("pem");
        let p12: ClientCertificate =
            serde_json::from_str(r#"{"type":"pkcs12","domain":"b.com","pkcs12Secret":"v.b"}"#)
                .expect("pkcs12");
        assert_eq!(pem.domain(), "a.com");
        assert_eq!(p12.domain(), "b.com");
    }

    #[test]
    fn debug_shows_references_but_never_the_passphrase() {
        let cert = ClientCertificate::Pem {
            domain: "api.example.com".into(),
            certificate_file_path: String::new(),
            private_key_file_path: String::new(),
            certificate_secret: Some("vault.clientCertPem".into()),
            private_key_secret: Some("vault.clientKeyPem".into()),
            passphrase: Some("hunter2".into()),
        };
        let shown = format!("{cert:?}");
        assert!(shown.contains("vault.clientCertPem"), "{shown}");
        assert!(shown.contains("<redacted>"), "{shown}");
        assert!(!shown.contains("hunter2"), "{shown}");
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-shared certificate`
Expected: FAIL to compile with `variant ClientCertificate::Pem has no field named certificate_secret` and `no method named domain found`.

- [ ] **Step 3: Extend the persisted enum, its `Debug` and add `domain()`**

Replace lines 3-54 of `crates/rocket-shared/src/certificate.rs` (the doc comment, the enum and the `Debug` impl) with:

```rust
/// Client certificate — PEM or PKCS12 format, discriminated by `type` field.
///
/// Each piece of material comes from a file path or from a RocketVault reference
/// (`alias.secretName`, never a value). A path that is not used is empty and is not written.
/// The `*Secret` keys are Rocket extensions outside the OpenCollection schema, like
/// `externalSecrets`.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ClientCertificate {
    #[serde(rename = "pem", rename_all = "camelCase")]
    Pem {
        domain: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        certificate_file_path: String,
        #[serde(default, skip_serializing_if = "String::is_empty")]
        private_key_file_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        certificate_secret: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        private_key_secret: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
    #[serde(rename = "pkcs12", rename_all = "camelCase")]
    Pkcs12 {
        domain: String,
        #[serde(
            rename = "pkcs12FilePath",
            default,
            skip_serializing_if = "String::is_empty"
        )]
        pkcs12_file_path: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pkcs12_secret: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
}

impl ClientCertificate {
    /// The domain this certificate is presented to.
    pub fn domain(&self) -> &str {
        match self {
            ClientCertificate::Pem { domain, .. } | ClientCertificate::Pkcs12 { domain, .. } => {
                domain
            }
        }
    }
}

// Hand-written so a `{:?}` of a request or environment never prints a passphrase. References
// are names, not values, so they are shown.
impl std::fmt::Debug for ClientCertificate {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let redact = |p: &Option<String>| p.as_ref().map(|_| "<redacted>");
        match self {
            ClientCertificate::Pem {
                domain,
                certificate_file_path,
                private_key_file_path,
                certificate_secret,
                private_key_secret,
                passphrase,
            } => f
                .debug_struct("Pem")
                .field("domain", domain)
                .field("certificate_file_path", certificate_file_path)
                .field("private_key_file_path", private_key_file_path)
                .field("certificate_secret", certificate_secret)
                .field("private_key_secret", private_key_secret)
                .field("passphrase", &redact(passphrase))
                .finish(),
            ClientCertificate::Pkcs12 {
                domain,
                pkcs12_file_path,
                pkcs12_secret,
                passphrase,
            } => f
                .debug_struct("Pkcs12")
                .field("domain", domain)
                .field("pkcs12_file_path", pkcs12_file_path)
                .field("pkcs12_secret", pkcs12_secret)
                .field("passphrase", &redact(passphrase))
                .finish(),
        }
    }
}
```

In the four existing tests of this file (`debug_output_never_contains_the_passphrase`, `pem_certificate_serde`, `pkcs12_certificate_serde`, `pem_with_passphrase`), add the new fields to each literal: after `pkcs12_file_path: ...,` add `pkcs12_secret: None,`; after `private_key_file_path: ...,` add `certificate_secret: None,` and `private_key_secret: None,`. For example, old:

```rust
        let cert = ClientCertificate::Pkcs12 {
            domain: "a.example.com".into(),
            pkcs12_file_path: "/c.p12".into(),
            passphrase: Some("hunter2".into()),
        };
```

new:

```rust
        let cert = ClientCertificate::Pkcs12 {
            domain: "a.example.com".into(),
            pkcs12_file_path: "/c.p12".into(),
            pkcs12_secret: None,
            passphrase: Some("hunter2".into()),
        };
```

- [ ] **Step 4: Run the model tests**

Run: `cargo test -j4 -p rocket-shared certificate`
Expected: PASS, the 10 tests of `certificate::tests` (5 existing, 5 new).

- [ ] **Step 5: Update every other constructor and match**

`grep -rn "ClientCertificate::\(Pem\|Pkcs12\)" --include='*.rs' crates src-tauri` lists them. After B1 they are:

1. `crates/rocket-app/src/client_certificates.rs`, `resolve_client_certificate`. Replace its `match cert { ... }` with the version below, and add the helper `not_resolved_yet` after the function. Until Plan C resolves references, an entry that names one is `Unavailable`, so it fails only when it is selected.

```rust
    match cert {
        ClientCertificate::Pem {
            domain,
            certificate_file_path,
            private_key_file_path,
            certificate_secret,
            private_key_secret,
            passphrase,
        } => {
            if let Some(reference) = certificate_secret.or(private_key_secret) {
                return ResolvedClientCertificate::unavailable(
                    r(domain),
                    not_resolved_yet(&reference),
                );
            }
            ResolvedClientCertificate::pem(
                r(domain),
                file(certificate_file_path),
                file(private_key_file_path),
                passphrase.map(&r),
            )
        }
        ClientCertificate::Pkcs12 {
            domain,
            pkcs12_file_path,
            pkcs12_secret,
            passphrase,
        } => {
            if let Some(reference) = pkcs12_secret {
                return ResolvedClientCertificate::unavailable(
                    r(domain),
                    not_resolved_yet(&reference),
                );
            }
            ResolvedClientCertificate::pkcs12(r(domain), file(pkcs12_file_path), passphrase.map(&r))
        }
    }
```

```rust
/// The error for a vault reference before references are resolved. It names the reference only.
fn not_resolved_yet(reference: &str) -> String {
    format!(
        "Client certificate secret {reference} cannot be used yet: certificate material from \
         RocketVault is not supported in this build."
    )
}
```

2. `crates/rocket-app/src/execution_service.rs` tests: the `ClientCertificate::Pkcs12` in `resolve_request_carries_the_environment_client_certificates` (`env.client_certificates = vec![...]`) and the four entries of `relative_path_env` (one `Pem`, three `Pkcs12`). Add `pkcs12_secret: None,` after each `pkcs12_file_path`, and `certificate_secret: None, private_key_secret: None,` after the `private_key_file_path` of the `Pem`.

3. `crates/rocket-app/src/oauth2_service.rs` tests: the `pkcs12` helper, and the `ClientCertificate::Pkcs12` literals in `vault_references_resolve_in_a_get_token_request_when_secrets_are_given` and `a_refresh_resolves_a_vault_passphrase_for_the_certificate`. Add `pkcs12_secret: None,` after `pkcs12_file_path`.

4. `crates/rocket-infra/src/conversions/tests.rs`, `environment_client_certificates_survive_oc_roundtrip`: add `certificate_secret: None, private_key_secret: None,` after `private_key_file_path`.

Then add this test to `mod tests` of `crates/rocket-app/src/execution_service.rs`, after `resolve_request_has_no_client_certificates_without_an_environment`:

```rust
    #[tokio::test]
    async fn a_vault_sourced_certificate_is_unavailable_until_references_are_resolved() {
        let mut env = Environment::new("dev");
        env.client_certificates = vec![ClientCertificate::Pkcs12 {
            domain: "api.example.com".into(),
            pkcs12_file_path: String::new(),
            pkcs12_secret: Some("vault.clientBundleB64".into()),
            passphrase: None,
        }];
        let svc = service_with(env, None);
        let input = sample_input("https://api.example.com/x", Some("dev"));
        let resolved = svc
            .resolve_request(&input, &std::collections::HashMap::new())
            .expect("resolve_request");
        assert_eq!(
            describe_all(&resolved.options.client_certificates),
            ["unavailable api.example.com Client certificate secret vault.clientBundleB64 \
              cannot be used yet: certificate material from RocketVault is not supported in \
              this build."]
        );
    }
```

- [ ] **Step 6: Run the migrated crates' tests**

Run: `cargo test -j4 -p rocket-app certificate`, `cargo test -j4 -p rocket-app oauth2_service`, `cargo test -j4 -p rocket-infra conversions`
Expected: PASS, including `a_vault_sourced_certificate_is_unavailable_until_references_are_resolved` and `environment_client_certificates_survive_oc_roundtrip`.

- [ ] **Step 7: Write the failing validation tests**

Create `crates/rocket-environment/src/client_certificate_validation.rs` with only the tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::external_secret::ExternalSecretRef;

    const KEY_TEXT: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvQsecret\n-----END PRIVATE KEY-----";

    fn bindings() -> Vec<ExternalSecretBinding> {
        vec![ExternalSecretBinding {
            alias: "vault".to_string(),
            connection_id: "conn-1".to_string(),
            vault_name: "prod-vault".to_string(),
            secret_names: ["clientCertPem", "clientKeyPem", "clientBundleB64"]
                .iter()
                .map(|name| ExternalSecretRef {
                    name: name.to_string(),
                    secret_id: format!("id-{name}"),
                })
                .collect(),
        }]
    }

    fn pem(
        domain: &str,
        cert_path: &str,
        key_path: &str,
        cert_secret: Option<&str>,
        key_secret: Option<&str>,
    ) -> ClientCertificate {
        ClientCertificate::Pem {
            domain: domain.to_string(),
            certificate_file_path: cert_path.to_string(),
            private_key_file_path: key_path.to_string(),
            certificate_secret: cert_secret.map(String::from),
            private_key_secret: key_secret.map(String::from),
            passphrase: Some("{{vault.clientKeyPass}}".to_string()),
        }
    }

    fn pkcs12(domain: &str, path: &str, secret: Option<&str>) -> ClientCertificate {
        ClientCertificate::Pkcs12 {
            domain: domain.to_string(),
            pkcs12_file_path: path.to_string(),
            pkcs12_secret: secret.map(String::from),
            passphrase: None,
        }
    }

    fn message(result: DomainResult<()>) -> String {
        match result.expect_err("must reject") {
            DomainError::InvalidInput(m) => m,
            other => panic!("expected InvalidInput, got {other:?}"),
        }
    }

    #[test]
    fn accepts_file_and_vault_sources() {
        let certs = [
            pem("api.example.com", "certs/client.pem", "/k.pem", None, None),
            pem(
                "api.example.com",
                "",
                "",
                Some("vault.clientCertPem"),
                Some("vault.clientKeyPem"),
            ),
            pkcs12("*.internal.example.com", "", Some("vault.clientBundleB64")),
            pkcs12("b.example.com", "{{certDir}}/client.p12", None),
        ];
        assert!(validate_client_certificates(&certs, &bindings()).is_ok());
        assert!(validate_client_certificates(&[], &[]).is_ok());
    }

    #[test]
    fn rejects_an_empty_domain() {
        let msg = message(validate_client_certificates(
            &[pem("  ", "/c.pem", "/k.pem", None, None)],
            &bindings(),
        ));
        assert!(msg.contains("domain"), "{msg}");
    }

    #[test]
    fn rejects_a_piece_with_no_source() {
        let msg = message(validate_client_certificates(
            &[pem("a.example.com", "/c.pem", "", None, None)],
            &bindings(),
        ));
        assert!(
            msg.contains("privateKeyFilePath") && msg.contains("privateKeySecret"),
            "{msg}"
        );
    }

    #[test]
    fn rejects_a_piece_with_two_sources() {
        let msg = message(validate_client_certificates(
            &[pkcs12("a.example.com", "/c.p12", Some("vault.clientBundleB64"))],
            &bindings(),
        ));
        assert!(
            msg.contains("pkcs12FilePath") && msg.contains("pkcs12Secret") && msg.contains("not both"),
            "{msg}"
        );
    }

    #[test]
    fn rejects_a_reference_that_is_not_alias_dot_secret_name() {
        for bad in ["clientCertPem", ".clientCertPem", "vault."] {
            let msg = message(validate_client_certificates(
                &[pem("a.example.com", "", "/k.pem", Some(bad), None)],
                &bindings(),
            ));
            assert!(
                msg.contains("certificateSecret") && msg.contains("alias.secretName"),
                "{bad}: {msg}"
            );
        }
    }

    // Review Focus 5.
    #[test]
    fn rejects_a_reference_with_no_matching_binding() {
        let msg = message(validate_client_certificates(
            &[pem("a.example.com", "", "/k.pem", Some("payments.clientCertPem"), None)],
            &bindings(),
        ));
        assert!(
            msg.contains("certificateSecret") && msg.contains("payments"),
            "{msg}"
        );

        let msg = message(validate_client_certificates(
            &[pkcs12("a.example.com", "", Some("vault.otherBundle"))],
            &bindings(),
        ));
        assert!(
            msg.contains("pkcs12Secret") && msg.contains("otherBundle") && msg.contains("Fetch"),
            "{msg}"
        );
    }

    // Review Focus 5.
    #[test]
    fn rejects_key_text_in_a_path_or_reference_field_and_names_the_field() {
        let indented = format!("  {KEY_TEXT}");
        let cases = [
            (pem("a.example.com", KEY_TEXT, "/k.pem", None, None), "certificateFilePath"),
            (pem("a.example.com", "/c.pem", &indented, None, None), "privateKeyFilePath"),
            (pem("a.example.com", "/c.pem", "", None, Some(KEY_TEXT)), "privateKeySecret"),
            (pkcs12("a.example.com", KEY_TEXT, None), "pkcs12FilePath"),
            (pkcs12("a.example.com", "", Some(KEY_TEXT)), "pkcs12Secret"),
        ];
        for (cert, field) in cases {
            let msg = message(validate_client_certificates(&[cert], &bindings()));
            assert!(msg.contains(field) && msg.contains("not key text"), "{field}: {msg}");
            assert!(!msg.contains("MIIEvQsecret"), "the key must not be echoed: {msg}");
        }
    }
}
```

In `crates/rocket-environment/src/lib.rs`, add `pub mod client_certificate_validation;` before `pub mod context;`, and `pub use client_certificate_validation::validate_client_certificates;` before `pub use context::VariableContext;`.

- [ ] **Step 8: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-environment client_certificate_validation`
Expected: FAIL to compile with `cannot find function validate_client_certificates in this scope`.

- [ ] **Step 9: Implement the validation**

Add above the tests module in `crates/rocket-environment/src/client_certificate_validation.rs`:

```rust
//! Save-time checks for an environment's client certificates.
//!
//! Every piece of material needs exactly one source, a reference must name a bound secret, and
//! no path or reference field may hold key text, so a private key never lands in the
//! environment file or in git.

use rocket_shared::certificate::ClientCertificate;
use rocket_shared::error::{DomainError, DomainResult};

use crate::external_secret::ExternalSecretBinding;

const PEM_MARKER: &str = "-----BEGIN";

/// Checks the rules of spec section 4. Certificates are numbered from 1 in messages, because
/// the domain can be the thing that is missing.
pub fn validate_client_certificates(
    certs: &[ClientCertificate],
    bindings: &[ExternalSecretBinding],
) -> DomainResult<()> {
    for (index, cert) in certs.iter().enumerate() {
        let entry = index + 1;
        if cert.domain().trim().is_empty() {
            return Err(invalid(format!("Client certificate {entry} needs a domain.")));
        }
        match cert {
            ClientCertificate::Pem {
                certificate_file_path,
                private_key_file_path,
                certificate_secret,
                private_key_secret,
                ..
            } => {
                check_piece(
                    entry,
                    ("certificateFilePath", certificate_file_path.as_str()),
                    ("certificateSecret", certificate_secret.as_deref()),
                    bindings,
                )?;
                check_piece(
                    entry,
                    ("privateKeyFilePath", private_key_file_path.as_str()),
                    ("privateKeySecret", private_key_secret.as_deref()),
                    bindings,
                )?;
            }
            ClientCertificate::Pkcs12 {
                pkcs12_file_path,
                pkcs12_secret,
                ..
            } => {
                check_piece(
                    entry,
                    ("pkcs12FilePath", pkcs12_file_path.as_str()),
                    ("pkcs12Secret", pkcs12_secret.as_deref()),
                    bindings,
                )?;
            }
        }
    }
    Ok(())
}

/// Checks one piece: no key text, then exactly one source, then the reference itself.
fn check_piece(
    entry: usize,
    (path_field, path): (&str, &str),
    (secret_field, secret): (&str, Option<&str>),
    bindings: &[ExternalSecretBinding],
) -> DomainResult<()> {
    reject_key_text(entry, path_field, path)?;
    if let Some(reference) = secret {
        reject_key_text(entry, secret_field, reference)?;
    }
    let secret = secret.map(str::trim).filter(|s| !s.is_empty());
    match (path.trim().is_empty(), secret) {
        (false, Some(_)) => Err(invalid(format!(
            "Client certificate {entry}: set either {path_field} or {secret_field}, not both."
        ))),
        (true, None) => Err(invalid(format!(
            "Client certificate {entry}: set {path_field} or {secret_field}."
        ))),
        (false, None) => Ok(()),
        (true, Some(reference)) => check_reference(entry, secret_field, reference, bindings),
    }
}

fn reject_key_text(entry: usize, field: &str, value: &str) -> DomainResult<()> {
    if value.trim_start().starts_with(PEM_MARKER) {
        return Err(invalid(format!(
            "Client certificate {entry}: field {field} must be a file path or a vault secret \
             reference, not key text."
        )));
    }
    Ok(())
}

/// A reference is `alias.secretName`: the alias is bound in this environment and the name is
/// one of the secret names fetched for that binding.
fn check_reference(
    entry: usize,
    field: &str,
    reference: &str,
    bindings: &[ExternalSecretBinding],
) -> DomainResult<()> {
    let (alias, name) = match reference.split_once('.') {
        Some((alias, name)) if !alias.is_empty() && !name.is_empty() => (alias, name),
        _ => {
            return Err(invalid(format!(
                "Client certificate {entry}: {field} must have the form alias.secretName."
            )))
        }
    };
    let Some(binding) = bindings.iter().find(|b| b.alias == alias) else {
        return Err(invalid(format!(
            "Client certificate {entry}: {field} uses the alias {alias}, which has no External \
             Secrets binding in this environment."
        )));
    };
    if !binding.secret_names.iter().any(|s| s.name == name) {
        return Err(invalid(format!(
            "Client certificate {entry}: {field} names the secret {name}, which is not in the \
             {alias} binding. Fetch the secret names first."
        )));
    }
    Ok(())
}

fn invalid(message: String) -> DomainError {
    DomainError::InvalidInput(message)
}
```

- [ ] **Step 10: Run the validation tests**

Run: `cargo test -j4 -p rocket-environment client_certificate_validation`
Expected: PASS, 7 tests.

- [ ] **Step 11: Write the failing save tests**

In `crates/rocket-app/src/environment_service.rs`, `mod tests`, add after `use rocket_environment::Variable;`:

```rust
    use rocket_environment::{ExternalSecretBinding, ExternalSecretRef};
    use rocket_shared::certificate::ClientCertificate;
```

and add at the end of the module:

```rust
    // Review Focus 5.
    #[test]
    fn save_rejects_pasted_key_text_in_a_certificate() {
        let svc = make_service();
        let mut env = Environment::new("prod");
        env.client_certificates = vec![ClientCertificate::Pem {
            domain: "api.example.com".into(),
            certificate_file_path: "/certs/client.pem".into(),
            private_key_file_path:
                "-----BEGIN PRIVATE KEY-----\nMIIEvQsecret\n-----END PRIVATE KEY-----".into(),
            certificate_secret: None,
            private_key_secret: None,
            passphrase: None,
        }];
        let err = svc.save(&env).expect_err("pasted key text must be rejected");
        assert!(err.to_string().contains("privateKeyFilePath"), "{err}");
        assert!(svc.list().expect("list").is_empty(), "nothing may be written");
    }

    #[test]
    fn save_accepts_a_certificate_that_references_a_bound_secret() {
        let svc = make_service();
        let mut env = Environment::new("prod");
        env.external_secrets = vec![ExternalSecretBinding {
            alias: "vault".into(),
            connection_id: "conn-1".into(),
            vault_name: "prod-vault".into(),
            secret_names: vec![
                ExternalSecretRef {
                    name: "clientCertPem".into(),
                    secret_id: "id-1".into(),
                },
                ExternalSecretRef {
                    name: "clientKeyPem".into(),
                    secret_id: "id-2".into(),
                },
            ],
        }];
        env.client_certificates = vec![ClientCertificate::Pem {
            domain: "api.example.com".into(),
            certificate_file_path: String::new(),
            private_key_file_path: String::new(),
            certificate_secret: Some("vault.clientCertPem".into()),
            private_key_secret: Some("vault.clientKeyPem".into()),
            passphrase: Some("{{vault.clientKeyPass}}".into()),
        }];
        svc.save(&env).expect("a bound reference is valid");
        assert_eq!(svc.get("prod").expect("saved").client_certificates.len(), 1);
    }
```

- [ ] **Step 12: Run them to verify the first one fails**

Run: `cargo test -j4 -p rocket-app environment_service`
Expected: FAIL. `save_rejects_pasted_key_text_in_a_certificate` panics in `expect_err` with `pasted key text must be rejected`, because `save` does not validate certificates yet. `save_accepts_a_certificate_that_references_a_bound_secret` passes.

- [ ] **Step 13: Call the validation on save**

In `crates/rocket-app/src/environment_service.rs`, `save`, old:

```rust
        rocket_environment::external_secret::validate_external_secret_bindings(
            &env.external_secrets,
        )?;
```

new:

```rust
        rocket_environment::external_secret::validate_external_secret_bindings(
            &env.external_secrets,
        )?;
        rocket_environment::validate_client_certificates(
            &env.client_certificates,
            &env.external_secrets,
        )?;
```

- [ ] **Step 14: Run the save tests**

Run: `cargo test -j4 -p rocket-app environment_service`
Expected: PASS, including both new tests.

- [ ] **Step 15: Pin the YAML format (backward compatibility and the schema shape)**

These tests pin behaviour that Step 3 already produces through the shared enum, so they pass on first run. Their job is to fail if a later change breaks the file format.

In `crates/rocket-infra/src/fs_environment_repo.rs`, `mod tests`, add after `use rocket_environment::Variable;`:

```rust
    use rocket_shared::certificate::ClientCertificate;
```

and add at the end of the module:

```rust
    #[test]
    fn an_old_client_certificate_entry_loads_and_round_trips_unchanged() {
        let (dir, repo) = setup();
        let old_yaml = "name: prod\nclientCertificates:\n\
            - type: pem\n  domain: api.example.com\n  certificateFilePath: certs/client.pem\n  privateKeyFilePath: certs/client-key.pem\n\
            - type: pkcs12\n  domain: '*.internal.example.com'\n  pkcs12FilePath: /certs/client.p12\n  passphrase: '{{vault.bundlePass}}'\n";
        std::fs::write(dir.path().join("prod.yml"), old_yaml).expect("write prod.yml");

        let env = repo.get("prod").expect("an old file still loads");
        repo.save(&env).expect("save");

        let raw = std::fs::read_to_string(dir.path().join("prod.yml")).expect("read prod.yml");
        let saved: serde_yaml::Value = serde_yaml::from_str(&raw).expect("parse saved");
        let original: serde_yaml::Value = serde_yaml::from_str(old_yaml).expect("parse original");
        assert_eq!(
            saved["clientCertificates"], original["clientCertificates"],
            "{raw}"
        );
    }

    #[test]
    fn a_vault_sourced_certificate_is_saved_as_references_only() {
        let (dir, repo) = setup();
        let mut env = Environment::new("prod");
        env.client_certificates = vec![ClientCertificate::Pem {
            domain: "api.example.com".into(),
            certificate_file_path: String::new(),
            private_key_file_path: String::new(),
            certificate_secret: Some("vault.clientCertPem".into()),
            private_key_secret: Some("vault.clientKeyPem".into()),
            passphrase: Some("{{vault.clientKeyPass}}".into()),
        }];
        repo.save(&env).expect("save");

        let raw = std::fs::read_to_string(dir.path().join("prod.yml")).expect("read prod.yml");
        assert!(raw.contains("certificateSecret: vault.clientCertPem"), "{raw}");
        assert!(raw.contains("privateKeySecret: vault.clientKeyPem"), "{raw}");
        assert!(!raw.contains("FilePath"), "{raw}");
        assert_eq!(
            repo.get("prod").expect("load").client_certificates,
            env.client_certificates
        );
    }
```

In `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`, add after `const OAUTH2_PARAMS_IMPLICIT: &[&str] = &["authorizationRequest"];` (line 170):

```rust
const CLIENT_CERT_PEM: &[&str] = &[
    "type",
    "domain",
    "certificateFilePath",
    "privateKeyFilePath",
    "passphrase",
];
const CLIENT_CERT_PKCS12: &[&str] = &["type", "domain", "pkcs12FilePath", "passphrase"];
/// Rocket extensions outside the OpenCollection `ClientCertificate` schema, like
/// `externalSecrets`: vault references (`alias.secretName`) for the certificate material.
/// Other OpenCollection tools do not know them.
const ROCKET_CLIENT_CERT_PEM_EXTENSIONS: &[&str] = &["certificateSecret", "privateKeySecret"];
const ROCKET_CLIENT_CERT_PKCS12_EXTENSIONS: &[&str] = &["pkcs12Secret"];
```

and add at the end of the file:

```rust
#[test]
fn environment_client_certificates_use_schema_keys_plus_rocket_extensions() {
    use rocket_environment::{Environment, EnvironmentRepository};
    use rocket_shared::certificate::ClientCertificate;

    let dir = TempDir::new().expect("tempdir");
    let repo = crate::fs_environment_repo::FsEnvironmentRepo::new(dir.path().to_path_buf());
    let mut env = Environment::new("prod");
    env.client_certificates = vec![
        ClientCertificate::Pem {
            domain: "a.example.com".into(),
            certificate_file_path: "certs/client.pem".into(),
            private_key_file_path: "certs/client-key.pem".into(),
            certificate_secret: None,
            private_key_secret: None,
            passphrase: Some("{{pass}}".into()),
        },
        ClientCertificate::Pem {
            domain: "b.example.com".into(),
            certificate_file_path: String::new(),
            private_key_file_path: String::new(),
            certificate_secret: Some("vault.clientCertPem".into()),
            private_key_secret: Some("vault.clientKeyPem".into()),
            passphrase: None,
        },
        ClientCertificate::Pkcs12 {
            domain: "c.example.com".into(),
            pkcs12_file_path: "/certs/client.p12".into(),
            pkcs12_secret: None,
            passphrase: None,
        },
        ClientCertificate::Pkcs12 {
            domain: "d.example.com".into(),
            pkcs12_file_path: String::new(),
            pkcs12_secret: Some("vault.clientBundleB64".into()),
            passphrase: Some("{{vault.bundlePass}}".into()),
        },
    ];
    repo.save(&env).expect("save environment");

    let doc = read_yaml(&dir.path().join("prod.yml"));
    assert_eq!(seq(doc.get("clientCertificates")).count(), 4);
    let mut v = Violations::default();
    for (i, cert) in seq(doc.get("clientCertificates")).enumerate() {
        let at = format!("prod.yml clientCertificates[{i}]");
        let allowed: Vec<&str> = match cert.get("type").and_then(Value::as_str) {
            Some("pem") => [CLIENT_CERT_PEM, ROCKET_CLIENT_CERT_PEM_EXTENSIONS].concat(),
            Some("pkcs12") => [CLIENT_CERT_PKCS12, ROCKET_CLIENT_CERT_PKCS12_EXTENSIONS].concat(),
            other => {
                v.0.push(format!("{at}: unknown certificate type {other:?}"));
                continue;
            }
        };
        v.keys("ClientCertificate", &at, cert, &allowed);
    }
    assert!(v.0.is_empty(), "{:#?}", v.0);
}
```

Run: `cargo test -j4 -p rocket-infra client_certificate` then `cargo test -j4 -p rocket-infra schema_shape`
Expected: PASS, including `an_old_client_certificate_entry_loads_and_round_trips_unchanged`, `a_vault_sourced_certificate_is_saved_as_references_only` and `environment_client_certificates_use_schema_keys_plus_rocket_extensions`.

- [ ] **Step 16: Workspace check and clippy**

Run: `cargo check -j4 --workspace`
Expected: `Finished`, no errors.

Run: `cargo test -j4 -p rocket-environment` and `cargo test -j4 -p rocket-shared`
Expected: PASS.

Run: `cargo clippy -j4 -p rocket-shared --all-targets`, `cargo clippy -j4 -p rocket-environment --all-targets`, `cargo clippy -j4 -p rocket-app --all-targets`, `cargo clippy -j4 -p rocket-infra --all-targets`
Expected: no warnings in the changed files.

- [ ] **Step 17: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add crates/rocket-shared/src/certificate.rs crates/rocket-environment/src/client_certificate_validation.rs crates/rocket-environment/src/lib.rs crates/rocket-app/src/environment_service.rs crates/rocket-app/src/client_certificates.rs crates/rocket-app/src/execution_service.rs crates/rocket-app/src/oauth2_service.rs crates/rocket-infra/src/conversions/tests.rs crates/rocket-infra/src/fs_environment_repo.rs crates/rocket-infra/src/fs_collection/schema_shape_tests.rs
git commit -- crates/rocket-shared/src/certificate.rs crates/rocket-environment/src/client_certificate_validation.rs crates/rocket-environment/src/lib.rs crates/rocket-app/src/environment_service.rs crates/rocket-app/src/client_certificates.rs crates/rocket-app/src/execution_service.rs crates/rocket-app/src/oauth2_service.rs crates/rocket-infra/src/conversions/tests.rs crates/rocket-infra/src/fs_environment_repo.rs crates/rocket-infra/src/fs_collection/schema_shape_tests.rs
```

Suggested subject: `feat(environment): persist vault references for client certificates`. The message ends with `Relates to: #21`.

---

## Task B3: Load inline PEM and PKCS12 material

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** sonnet.

**Files:**
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (`read_der_source`, `read_pem_source`, `inline_not_supported` as written in B1; `mtls_tests`, including `inline_material_is_not_supported_yet` from B1 and `mutual_tls_handshake_against_openssl_s_server`, today lines 1893-1969)

Fixtures used, unchanged: `crates/rocket-infra/test-fixtures/mtls/client.pem`, `client-key.pem`, `client-key-encrypted.pem` (password `changeit`), `client.p12` (password `changeit`), `server.pem`, `server-key.pem`.

**Interfaces:**
- Consumes: `CertificateSource::Inline(Zeroizing<Vec<u8>>)` and `load_identity` from B1; `pem_key::unencrypted_key_pem`.
- Produces: `load_identity` and `ReqwestTokenClientProvider::client_for` accept `Inline` material (PEM text, encrypted PKCS#8 PEM with its passphrase, PKCS12 DER). Consumed by C1, which fills `Inline` from vault values.

- [ ] **Step 1: Write the failing tests**

In `crates/rocket-infra/src/reqwest_executor.rs`, `mod mtls_tests`, delete the B1 test `inline_material_is_not_supported_yet`. Add after `fn fixture`:

```rust
    fn fixture_bytes(name: &str) -> Vec<u8> {
        std::fs::read(fixture(name)).expect("read fixture")
    }

    /// Fixture bytes held in memory, as a vault secret delivers them.
    fn inline(name: &str) -> CertificateSource {
        CertificateSource::Inline(zeroize::Zeroizing::new(fixture_bytes(name)))
    }

    /// Fixture text with CRLF line endings and blank lines around it, like a secret that went
    /// through a Windows editor or a CSV export.
    fn inline_crlf(name: &str) -> CertificateSource {
        let text = String::from_utf8(fixture_bytes(name)).expect("fixture is text");
        let crlf = format!("\r\n{}\r\n\r\n", text.trim_end().replace('\n', "\r\n"));
        CertificateSource::Inline(zeroize::Zeroizing::new(crlf.into_bytes()))
    }
```

Add after `an_unavailable_certificate_fails_with_its_reason`:

```rust
    #[test]
    fn loads_an_inline_pem_identity() {
        let cert = ResolvedClientCertificate::pem(
            "x",
            inline("client.pem"),
            inline("client-key.pem"),
            None,
        );
        load_identity(&cert).expect("inline PEM loads");
    }

    #[test]
    fn loads_an_inline_encrypted_pem_identity_with_its_passphrase() {
        let cert = ResolvedClientCertificate::pem(
            "x",
            inline("client.pem"),
            inline("client-key-encrypted.pem"),
            Some("changeit".into()),
        );
        load_identity(&cert).expect("inline encrypted PEM loads");
    }

    #[test]
    fn a_wrong_inline_pem_passphrase_names_the_domain_and_never_the_key_or_passphrase() {
        let cert = ResolvedClientCertificate::pem(
            "api.example.com",
            inline("client.pem"),
            inline("client-key-encrypted.pem"),
            Some("nope-nope".into()),
        );
        let err = load_identity(&cert).unwrap_err().to_string();
        assert!(
            err.contains("wrong passphrase") && err.contains("(inline, for api.example.com)"),
            "{err}"
        );
        assert!(!err.contains("nope-nope") && !err.contains("BEGIN"), "{err}");
    }

    #[test]
    fn loads_an_inline_pkcs12_identity() {
        let cert =
            ResolvedClientCertificate::pkcs12("x", inline("client.p12"), Some("changeit".into()));
        load_identity(&cert).expect("inline PKCS12 loads");
    }

    #[test]
    fn a_wrong_inline_pkcs12_passphrase_names_the_domain() {
        let cert = ResolvedClientCertificate::pkcs12(
            "api.example.com",
            inline("client.p12"),
            Some("nope".into()),
        );
        let err = load_identity(&cert).unwrap_err().to_string();
        assert!(
            err.contains("PKCS12") && err.contains("(inline, for api.example.com)"),
            "{err}"
        );
    }

    // Review Focus 2.
    #[test]
    fn inline_pem_with_crlf_line_endings_and_a_trailing_newline_loads() {
        let plain = ResolvedClientCertificate::pem(
            "x",
            inline_crlf("client.pem"),
            inline_crlf("client-key.pem"),
            None,
        );
        load_identity(&plain).expect("CRLF PEM loads");
        let encrypted = ResolvedClientCertificate::pem(
            "x",
            inline_crlf("client.pem"),
            inline_crlf("client-key-encrypted.pem"),
            Some("changeit".into()),
        );
        load_identity(&encrypted).expect("CRLF encrypted PEM loads");
    }

    #[test]
    fn normalise_pem_turns_crlf_into_lf_and_trims_surrounding_whitespace() {
        let out = normalise_pem(b"\r\n  -----BEGIN X-----\r\nAB\r\n-----END X-----\r\n\r\n");
        assert_eq!(out.as_slice(), b"-----BEGIN X-----\nAB\n-----END X-----\n");
    }

    #[tokio::test]
    async fn a_matching_inline_certificate_builds_a_client_and_sends_the_request() {
        let server = ok_server().await;
        let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
        req.options.client_certificates = vec![ResolvedClientCertificate::pkcs12(
            "127.0.0.1",
            inline("client.p12"),
            Some("changeit".into()),
        )];
        let resp = ReqwestExecutor::new().execute(&req).await.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[test]
    fn the_token_client_provider_presents_inline_material() {
        use rocket_http::TokenClientProvider;
        let provider = ReqwestTokenClientProvider;
        let certs = [
            ResolvedClientCertificate::pkcs12(
                "idp.example.com",
                inline("client.p12"),
                Some("changeit".into()),
            ),
            ResolvedClientCertificate::pem(
                "pem-idp.example.com",
                inline("client.pem"),
                inline("client-key-encrypted.pem"),
                Some("changeit".into()),
            ),
        ];
        assert!(provider
            .client_for("https://idp.example.com/token", true, &certs)
            .is_ok());
        assert!(provider
            .client_for("https://pem-idp.example.com/token", true, &certs)
            .is_ok());
    }
```

(`ok_server` is defined further down in the same module; Rust allows the forward use.)

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-infra mtls_tests`
Expected: FAIL to compile with `cannot find function normalise_pem in this scope`. With that test commented out, the inline tests fail with `Inline client certificate material is not supported yet`.

- [ ] **Step 3: Load inline material**

In `crates/rocket-infra/src/reqwest_executor.rs`, replace `read_der_source`, `read_pem_source` and `inline_not_supported` (added in B1) with:

```rust
/// Reads binary material (a PKCS12 bundle). The bytes are wiped on drop.
fn read_der_source(source: &CertificateSource) -> DomainResult<zeroize::Zeroizing<Vec<u8>>> {
    match source {
        CertificateSource::File(path) => Ok(zeroize::Zeroizing::new(read_certificate_file(path)?)),
        CertificateSource::Inline(bytes) => Ok(zeroize::Zeroizing::new(bytes.to_vec())),
    }
}

/// Reads PEM text (a certificate or a private key). The bytes are wiped on drop.
///
/// Inline text from a vault secret is normalised first, because a secret can come back with
/// CRLF line endings or blank lines, and the TLS backend needs the key to start exactly with
/// its `-----BEGIN` line.
fn read_pem_source(source: &CertificateSource) -> DomainResult<zeroize::Zeroizing<Vec<u8>>> {
    match source {
        CertificateSource::File(path) => Ok(zeroize::Zeroizing::new(read_certificate_file(path)?)),
        CertificateSource::Inline(bytes) => Ok(normalise_pem(bytes)),
    }
}

/// Turns CRLF line endings into LF, trims whitespace around the text and ends it with one LF.
fn normalise_pem(text: &[u8]) -> zeroize::Zeroizing<Vec<u8>> {
    let trimmed = text.trim_ascii();
    // Sized up front so the vector never reallocates and leaves a copy behind.
    let mut out = zeroize::Zeroizing::new(Vec::with_capacity(trimmed.len() + 1));
    out.extend(trimmed.iter().copied().filter(|b| *b != b'\r'));
    out.push(b'\n');
    out
}
```

Update the doc comment of `load_identity` (first two lines from B1) to:

```rust
/// Turns a client certificate's material, from a file or held in memory, into a TLS identity.
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra mtls_tests`
Expected: PASS, including the nine new tests and every file-based test from before.

- [ ] **Step 5: Extend the real handshake test**

In `mutual_tls_handshake_against_openssl_s_server`, add after the `allowed_encrypted_pem` block (after `let allowed_encrypted_pem = exec.execute(&with_encrypted_pem).await;`):

```rust
        let mut with_inline_pem =
            HttpRequest::new(HttpMethod::Get, format!("https://127.0.0.1:{port}/"));
        with_inline_pem.options.verify_ssl = false;
        with_inline_pem.options.client_certificates = vec![ResolvedClientCertificate::pem(
            "127.0.0.1",
            inline("client.pem"),
            inline("client-key.pem"),
            None,
        )];
        let allowed_inline_pem = exec.execute(&with_inline_pem).await;

        let mut with_inline_p12 =
            HttpRequest::new(HttpMethod::Get, format!("https://127.0.0.1:{port}/"));
        with_inline_p12.options.verify_ssl = false;
        with_inline_p12.options.client_certificates = vec![ResolvedClientCertificate::pkcs12(
            "127.0.0.1",
            inline("client.p12"),
            Some("changeit".into()),
        )];
        let allowed_inline_p12 = exec.execute(&with_inline_p12).await;
```

and add at the end of the test, after the `allowed_encrypted_pem` assertion:

```rust
        assert_eq!(
            allowed_inline_pem
                .expect("inline PEM identity accepted")
                .status,
            200
        );
        assert_eq!(
            allowed_inline_p12
                .expect("inline PKCS12 identity accepted")
                .status,
            200
        );
```

Run: `cargo test -j4 -p rocket-infra mutual_tls_handshake -- --ignored`
Expected: PASS (needs the `openssl` CLI). The server rejects the request without a certificate and accepts the file PKCS12, file PEM, file encrypted PEM, inline PEM and inline PKCS12 identities.

- [ ] **Step 6: Workspace check and clippy**

Run: `cargo check -j4 --workspace`
Expected: `Finished`, no errors.

Run: `cargo clippy -j4 -p rocket-infra --all-targets`
Expected: no warnings in `reqwest_executor.rs`.

- [ ] **Step 7: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only this path:

```bash
git add crates/rocket-infra/src/reqwest_executor.rs
git commit -- crates/rocket-infra/src/reqwest_executor.rs
```

Suggested subject: `feat(http): load client certificate material held in memory`. The message ends with `Relates to: #21`.

---

## Milestone Checklist: Plan B

- [ ] `ResolvedClientCertificate`, `CertificateMaterial`, `CertificateSource` exported from `rocket-http`, not `Serialize`, redacting `Debug`, secrets in `Zeroizing`
- [ ] `RequestOptions.client_certificates` is `#[serde(skip)]`; the IPC input cannot carry certificates
- [ ] `find_certificate`, `certificate_covers`, `TokenClientProvider::client_for`, `load_identity`, `identity_for_url` take the resolved type; `first_matching_certificate_wins` still passes
- [ ] `Unavailable` fails only when selected, with its reason
- [ ] Persisted `ClientCertificate` has `certificateSecret`, `privateKeySecret`, `pkcs12Secret`; empty paths are not written; old files load and round-trip
- [ ] `validate_client_certificates` runs on `EnvironmentService::save`; missing source, two sources, bad reference, unbound alias or name, key text and empty domain are rejected with the field named
- [ ] Inline PEM, inline encrypted PEM, inline PKCS12 and CRLF PEM load; the real handshake passes with inline material
- [ ] `cargo check -j4 --workspace` and clippy for each touched crate are clean

## Next Plan

[Plan C: Resolution and hygiene](02-plan-c-resolution-and-hygiene.md): resolves `certificateSecret`, `privateKeySecret` and `pkcs12Secret` against the RocketVault values into `CertificateSource::Inline` (replacing B2's `not_resolved_yet` placeholder entry), fails only when the certificate is selected, and runs the security pass on redaction, logs, events and history.
