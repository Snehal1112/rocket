# RocketVault Certificate Source, Plan A: Model, Validation and Deferred Material

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the persisted `vault` client certificate entry (`domain`, `binding`, `certificate`, `format`), validate it on save, and resolve it into a runtime `CertificateMaterial::Deferred` that carries names only. Make the executor refuse a `Deferred` entry it is asked to load, and make load tests fail a selected vault certificate with a clear message. No RocketVault call is made in this plan.

**Architecture:** `rocket-shared` gains `VaultCertificateFormat` and the `ClientCertificate::Vault` variant (camelCase like its siblings, YAML round trip through the existing `OcEnvironment`, which embeds `ClientCertificate` directly). `rocket-http` gains `VaultCertificateBinding` and `CertificateMaterial::Deferred`. `rocket-app/src/client_certificates.rs` stays the single place that turns persisted entries into resolved ones: a `vault` entry becomes `Deferred` with the connection and vault copied from the environment's External Secrets binding, or `Unavailable` when that binding is missing. `rocket-infra`'s `load_identity` turns a `Deferred` into an `InvalidInput` error. `rocket-environment`'s `validate_client_certificates` learns the `vault` rules.

**Tech Stack:** Rust, serde, serde_yaml, `zeroize`, `reqwest` 0.12 with `native-tls`, `wiremock`, `tempfile`.

**Spec:** [`docs/superpowers/specs/2026-10-02-vault-certificate-source-design.md`](../../specs/2026-10-02-vault-certificate-source-design.md) (sections 4, 5.1, 5.6, 6, 10, 12). Plan index and the shared interface contract: [`00-plan-index.md`](00-plan-index.md).

**Plan A of 4 (A, B, C, D).** No dependencies.

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

- **Mutation check (spec section 10).** A `Deferred` entry must never be skipped silently by the executor. Owner A1. Pinned by `a_selected_deferred_certificate_fails_the_request_and_nothing_is_sent` and `load_identity_never_skips_a_deferred_certificate` in `crates/rocket-infra/src/reqwest_executor.rs` (`mtls_tests::deferred_certificates`).
- **Load tests (spec section 6).** A selected vault certificate in a load test fails with "not available in load tests". Owner A3. Pinned by `load_tests_turn_a_vault_certificate_into_a_clear_error` in `crates/rocket-app/src/client_certificates.rs`.

## Spec versus code (read before starting)

1. **"An older build ignores the unknown type" is not true.** `ClientCertificate` is `#[serde(tag = "type")]` with no catch-all variant, and `OcEnvironment.client_certificates` is `Vec<ClientCertificate>` (`crates/rocket-infra/src/oc/environment.rs:47-48`). A build without `Vault` fails to parse the whole environment: `FsEnvironmentRepo::list` logs "skipping corrupt environment YAML file" and drops it (`crates/rocket-infra/src/fs_environment_repo.rs:117-124`), and `get` returns `Internal("Failed to parse environment YAML ...")` (lines 140-146). Nothing in this plan can change older builds. A3 writes this into the spec reference note so the user can decide how to announce it. The change is still additive for this and later builds.
2. **`Deferred.binding` is a struct.** The spec's `Deferred { binding, certificate, format }` keeps its field names, but `binding` is `VaultCertificateBinding { alias, connection_id, vault_name }`, copied from the environment while it is already loaded in `environment_client_certificates`. Plan C then needs no second environment read.
3. **The resolution function keeps its signature.** `environment_client_certificates` already reads the environment, so the bindings come from `env.external_secrets`; no new parameter.

## Findings from reading the real code (do not re-derive)

- `crates/rocket-shared/src/certificate.rs` (252 lines): `ClientCertificate` lines 9-40, `domain()` lines 42-51, hand-written `Debug` lines 53-89, tests lines 91-252.
- `crates/rocket-http/src/resolved_certificate.rs` (230 lines): `CertificateMaterial` lines 20-34, constructors lines 43-82, `Debug for CertificateMaterial` lines 103-129, tests lines 140-230. Re-exports in `crates/rocket-http/src/lib.rs` line 33. `rocket-http` already depends on `rocket-shared`.
- `crates/rocket-infra/src/reqwest_executor.rs`: `load_identity` lines 528-565 matches `CertificateMaterial` exhaustively (the `Unavailable` arm is lines 561-563). `identity_for_url` (lines 450-465) is the only caller for requests and for `ReqwestTokenClientProvider`. Test helpers `fixture`, `p12`, `ok_server` live in `mod mtls_tests` (line 1648); `mod unavailable_certificates` (lines 2031-2099) is the model for the new test module.
- `crates/rocket-app/src/client_certificates.rs` (299 lines) has exhaustive matches on `ClientCertificate` in `resolve_client_certificate` (lines 52-88), `absolutize_certificate_paths` (lines 91-110) and `resolve_references` (lines 146-203), and on `CertificateMaterial` in the test-only `describe_all` (lines 265-299). It has no test module of its own today; its behaviour is tested from `execution_service.rs` and `oauth2_service.rs`. `crate::test_doubles::StaticEnvRepo` (`crates/rocket-app/src/test_doubles.rs:522`) answers one fixed environment for any name.
- `crates/rocket-environment/src/client_certificate_validation.rs` (320 lines): `validate_client_certificates` lines 16-63, `reject_key_text` lines 89-97, test helpers `bindings()` (alias `vault`) at lines 141-154.
- Load tests call `resolve_request` with an empty secrets map in two places: `RequestExecutionService::run_load_test` (`crates/rocket-app/src/execution_service.rs:1722-1731`) and `LoadTestService::run` (`crates/rocket-app/src/load_test_service.rs:30-33`).
- `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`: allowed keys per certificate type at lines 171-183, and `environment_client_certificates_use_schema_keys_plus_rocket_extensions` at lines 807-861 (its `match` on `type` reports any other type as a violation).

## Test conventions

- New tests use `.expect("message")` or `expect_err("message")`. Production code never calls `unwrap`.
- `ResolvedClientCertificate` is not `PartialEq`. `rocket-app` tests compare it through `crate::client_certificates::describe_all`, which A1 extends with a `deferred ...` line.

---

## Task A1: `Deferred` material and the executor guard

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** sonnet.

**Files:**
- Modify: `crates/rocket-shared/src/certificate.rs` (add `VaultCertificateFormat` after the `use` on line 1; add one test at the end of `mod tests`)
- Modify: `crates/rocket-http/src/resolved_certificate.rs` (imports lines 7-9, `CertificateMaterial` lines 20-34, constructors after line 81, `Debug for CertificateMaterial` lines 103-129, tests)
- Modify: `crates/rocket-http/src/lib.rs` (re-export line 33)
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (`load_identity` arm at lines 561-563; new test module after `mod unavailable_certificates`, which ends at line 2099)
- Modify: `crates/rocket-app/src/client_certificates.rs` (`describe_all` arm at lines 294-296)

**Interfaces:**
- Consumes: `ResolvedClientCertificate`, `CertificateMaterial`, `find_certificate` as they are today.
- Produces (contract names): `VaultCertificateFormat` with `as_str`; `VaultCertificateBinding { alias, connection_id, vault_name }`; `CertificateMaterial::Deferred { binding, certificate, format }`; `ResolvedClientCertificate::deferred` and `is_deferred`; the executor's `Deferred` error. Consumed by A2, A3, B1, C1.

- [ ] **Step 1: Write the failing format test**

In `crates/rocket-shared/src/certificate.rs`, add at the end of `mod tests` (before the closing `}` on line 252):

```rust
    #[test]
    fn vault_certificate_format_uses_lowercase_names_and_defaults_to_pem() {
        assert_eq!(
            serde_json::to_string(&VaultCertificateFormat::Pkcs12).expect("serialize"),
            "\"pkcs12\""
        );
        assert_eq!(
            serde_json::from_str::<VaultCertificateFormat>("\"pem\"").expect("deserialize"),
            VaultCertificateFormat::Pem
        );
        assert_eq!(VaultCertificateFormat::default(), VaultCertificateFormat::Pem);
        assert!(serde_json::from_str::<VaultCertificateFormat>("\"der\"").is_err());
        assert_eq!(VaultCertificateFormat::Pem.as_str(), "pem");
        assert_eq!(VaultCertificateFormat::Pkcs12.as_str(), "pkcs12");
    }
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -j4 -p rocket-shared vault_certificate_format`
Expected: FAIL to compile with `cannot find type VaultCertificateFormat in this scope`.

- [ ] **Step 3: Add `VaultCertificateFormat`**

In `crates/rocket-shared/src/certificate.rs`, insert after line 1 (`use serde::{Deserialize, Serialize};`) and its blank line:

```rust
/// How a RocketVault certificate is exported: PEM text (the certificate chain and an
/// unencrypted PKCS#8 key) or a PKCS12 bundle. Persisted as `pem` or `pkcs12`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VaultCertificateFormat {
    #[default]
    Pem,
    Pkcs12,
}

impl VaultCertificateFormat {
    /// The persisted and wire name: `pem` or `pkcs12`.
    pub fn as_str(self) -> &'static str {
        match self {
            VaultCertificateFormat::Pem => "pem",
            VaultCertificateFormat::Pkcs12 => "pkcs12",
        }
    }
}

```

- [ ] **Step 4: Run it to verify it passes**

Run: `cargo test -j4 -p rocket-shared vault_certificate_format`
Expected: PASS, 1 test.

- [ ] **Step 5: Write the failing `Deferred` type tests**

In `crates/rocket-http/src/resolved_certificate.rs`, add at the end of `mod tests` (before its closing `}`):

```rust
    fn prod_binding() -> VaultCertificateBinding {
        VaultCertificateBinding {
            alias: "prod".into(),
            connection_id: "conn-1".into(),
            vault_name: "prod-vault".into(),
        }
    }

    #[test]
    fn a_deferred_certificate_shows_its_names_and_holds_no_material() {
        let cert = ResolvedClientCertificate::deferred(
            "api.example.com",
            prod_binding(),
            "client-a",
            VaultCertificateFormat::Pkcs12,
        );
        assert_eq!(cert.domain, "api.example.com");
        match &cert.material {
            CertificateMaterial::Deferred {
                binding,
                certificate,
                format,
            } => {
                assert_eq!(binding, &prod_binding());
                assert_eq!(certificate, "client-a");
                assert_eq!(*format, VaultCertificateFormat::Pkcs12);
            }
            other => panic!("unexpected material {other:?}"),
        }
        let shown = format!("{cert:?}");
        assert!(shown.contains("Deferred"), "{shown}");
        assert!(shown.contains("prod"), "{shown}");
        assert!(shown.contains("client-a"), "{shown}");
        assert!(shown.contains("pkcs12"), "{shown}");
    }

    #[test]
    fn is_deferred_is_true_only_for_deferred_material() {
        let deferred = ResolvedClientCertificate::deferred(
            "a",
            prod_binding(),
            "client-a",
            VaultCertificateFormat::Pem,
        );
        assert!(deferred.is_deferred());
        assert!(!ResolvedClientCertificate::unavailable("a", "gone").is_deferred());
        assert!(!ResolvedClientCertificate::pkcs12("a", inline(&[1, 2]), None).is_deferred());
    }
```

- [ ] **Step 6: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-http resolved_certificate`
Expected: FAIL to compile with `cannot find type VaultCertificateBinding in this scope` and `no variant named Deferred`.

- [ ] **Step 7: Add `VaultCertificateBinding`, `Deferred`, the constructor and `Debug`**

In `crates/rocket-http/src/resolved_certificate.rs`, lines 7-9, old:

```rust
use std::fmt;

use zeroize::Zeroizing;
```

new:

```rust
use std::fmt;

use rocket_shared::certificate::VaultCertificateFormat;
use zeroize::Zeroizing;

/// The RocketVault connection and vault behind an External Secrets binding, copied from the
/// environment when the certificate is resolved. Names and ids only, never a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultCertificateBinding {
    /// The binding alias, used in messages.
    pub alias: String,
    pub connection_id: String,
    pub vault_name: String,
}
```

Lines 32-34 (inside `CertificateMaterial`), old:

```rust
    /// The material could not be resolved. Selecting this certificate fails with `reason`.
    Unavailable { reason: String },
}
```

new:

```rust
    /// The material could not be resolved. Selecting this certificate fails with `reason`.
    Unavailable { reason: String },
    /// A RocketVault certificate that `rocket-app` exports only when it is selected for a URL.
    /// It holds names, never material. The executor does not load it: one that reaches the
    /// executor is an error.
    Deferred {
        binding: VaultCertificateBinding,
        certificate: String,
        format: VaultCertificateFormat,
    },
}
```

After `unavailable` (the constructor that ends on line 81, before the `impl` block's closing `}`), add:

```rust

    pub fn deferred(
        domain: impl Into<String>,
        binding: VaultCertificateBinding,
        certificate: impl Into<String>,
        format: VaultCertificateFormat,
    ) -> Self {
        Self {
            domain: domain.into(),
            material: CertificateMaterial::Deferred {
                binding,
                certificate: certificate.into(),
                format,
            },
        }
    }

    /// True while the material is a RocketVault certificate that has not been fetched yet.
    pub fn is_deferred(&self) -> bool {
        matches!(self.material, CertificateMaterial::Deferred { .. })
    }
```

In `impl fmt::Debug for CertificateMaterial`, lines 123-127, old:

```rust
            CertificateMaterial::Unavailable { reason } => f
                .debug_struct("Unavailable")
                .field("reason", reason)
                .finish(),
```

new:

```rust
            CertificateMaterial::Unavailable { reason } => f
                .debug_struct("Unavailable")
                .field("reason", reason)
                .finish(),
            CertificateMaterial::Deferred {
                binding,
                certificate,
                format,
            } => f
                .debug_struct("Deferred")
                .field("binding", &binding.alias)
                .field("certificate", certificate)
                .field("format", &format.as_str())
                .finish(),
```

In `crates/rocket-http/src/lib.rs`, line 33, old:

```rust
pub use resolved_certificate::{CertificateMaterial, CertificateSource, ResolvedClientCertificate};
```

new:

```rust
pub use resolved_certificate::{
    CertificateMaterial, CertificateSource, ResolvedClientCertificate, VaultCertificateBinding,
};
```

- [ ] **Step 8: Run the type tests to verify they pass**

Run: `cargo test -j4 -p rocket-http resolved_certificate`
Expected: PASS, 6 tests (4 existing, 2 new).

- [ ] **Step 9: Write the failing executor mutation tests**

In `crates/rocket-infra/src/reqwest_executor.rs`, insert after the closing `}` of `mod unavailable_certificates` (line 2099), still inside `mod mtls_tests`:

```rust

    mod deferred_certificates {
        use super::*;
        use rocket_http::{ResolvedClientCertificate, TokenClientProvider, VaultCertificateBinding};
        use rocket_shared::certificate::VaultCertificateFormat;

        fn deferred(domain: &str) -> ResolvedClientCertificate {
            ResolvedClientCertificate::deferred(
                domain,
                VaultCertificateBinding {
                    alias: "prod".into(),
                    connection_id: "conn-1".into(),
                    vault_name: "prod-vault".into(),
                },
                "client-a",
                VaultCertificateFormat::Pem,
            )
        }

        // Spec section 10, mutation check: if the executor ever skipped a Deferred entry, the
        // request would go out, here with the valid certificate listed second.
        #[tokio::test]
        async fn a_selected_deferred_certificate_fails_the_request_and_nothing_is_sent() {
            let server = ok_server().await;
            let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
            req.options.client_certificates = vec![
                deferred("127.0.0.1"),
                p12("127.0.0.1", fixture("client.p12"), Some("changeit")),
            ];
            let err = ReqwestExecutor::new()
                .execute(&req)
                .await
                .expect_err("a selected deferred certificate is an error");
            assert!(
                matches!(&err, DomainError::InvalidInput(m)
                    if m.contains("client-a") && m.contains("binding prod") && m.contains("was not fetched")),
                "{err:?}"
            );
            assert!(server
                .received_requests()
                .await
                .expect("requests recorded")
                .is_empty());
        }

        #[tokio::test]
        async fn a_deferred_certificate_for_another_domain_does_not_block_the_request() {
            let server = ok_server().await;
            let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
            req.options.client_certificates = vec![deferred("other.example.com")];
            let resp = ReqwestExecutor::new()
                .execute(&req)
                .await
                .expect("an entry for another domain is not selected");
            assert_eq!(resp.status, 200);
        }

        #[test]
        fn the_token_client_fails_on_a_selected_deferred_certificate_only() {
            let provider = ReqwestTokenClientProvider;
            let certs = [deferred("idp.example.com")];
            assert!(provider
                .client_for("https://other.example.com/token", true, &certs)
                .is_ok());
            let err = provider
                .client_for("https://idp.example.com/token", true, &certs)
                .expect_err("a selected deferred certificate is an error")
                .to_string();
            assert!(err.contains("was not fetched"), "{err}");
        }

        #[test]
        fn load_identity_never_skips_a_deferred_certificate() {
            let err = load_identity(&deferred("x")).expect_err("deferred material cannot load");
            assert!(err.to_string().contains("was not fetched"), "{err}");
        }
    }
```

- [ ] **Step 10: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-infra deferred_certificates`
Expected: FAIL to compile with `non-exhaustive patterns: &CertificateMaterial::Deferred { .. } not covered` in `load_identity`.

- [ ] **Step 11: Add the executor guard**

In `crates/rocket-infra/src/reqwest_executor.rs`, `load_identity`, lines 561-563, old:

```rust
        CertificateMaterial::Unavailable { reason } => {
            Err(DomainError::InvalidInput(reason.clone()))
        }
```

new:

```rust
        CertificateMaterial::Unavailable { reason } => {
            Err(DomainError::InvalidInput(reason.clone()))
        }
        // rocket-app fetches a RocketVault certificate before the send. One that is still
        // deferred here was never fetched, so the request must fail instead of going out
        // without the certificate.
        CertificateMaterial::Deferred {
            binding,
            certificate,
            ..
        } => Err(DomainError::InvalidInput(format!(
            "The RocketVault certificate {certificate} (binding {}) for {} was not fetched \
             before the request was sent.",
            binding.alias, cert.domain
        ))),
```

Update the doc comment on `load_identity` (line 527), old:

```rust
/// `Unavailable` material fails with its reason, since this certificate was selected.
```

new:

```rust
/// `Unavailable` material fails with its reason, since this certificate was selected.
/// `Deferred` material fails too: rocket-app must have fetched it before the send.
```

- [ ] **Step 12: Run the executor tests to verify they pass**

Run: `cargo test -j4 -p rocket-infra deferred_certificates`
Expected: PASS, 4 tests.

Run: `cargo test -j4 -p rocket-infra mtls_tests`
Expected: PASS, every existing `mtls_tests` test still passes (the `#[ignore]` handshake test is skipped).

- [ ] **Step 13: Extend `describe_all` so rocket-app tests still compile**

In `crates/rocket-app/src/client_certificates.rs`, `describe_all`, lines 294-296, old:

```rust
            CertificateMaterial::Unavailable { reason } => {
                format!("unavailable {} {reason}", c.domain)
            }
```

new:

```rust
            CertificateMaterial::Unavailable { reason } => {
                format!("unavailable {} {reason}", c.domain)
            }
            CertificateMaterial::Deferred {
                binding,
                certificate,
                format: kind,
            } => format!(
                "deferred {} {}:{} {} conn:{} vault:{}",
                c.domain,
                binding.alias,
                certificate,
                kind.as_str(),
                binding.connection_id,
                binding.vault_name
            ),
```

Update the doc comment above `describe_all` (line 265), old:

```rust
/// One line per certificate, for test assertions: `pkcs12 <domain> file:<path> pass:<value>`.
```

new:

```rust
/// One line per certificate, for test assertions: `pkcs12 <domain> file:<path> pass:<value>`,
/// or `deferred <domain> <alias>:<name> <format> conn:<id> vault:<name>`.
```

- [ ] **Step 14: Run the rocket-app certificate tests**

Run: `cargo test -j4 -p rocket-app vault_certificates`
Expected: PASS, the existing `vault_certificates_*` tests in `execution_service.rs` and `oauth2_service.rs` still pass.

- [ ] **Step 15: Workspace check and clippy**

Run: `cargo check -j4 --workspace`
Expected: `Finished`, no errors.

Run: `cargo clippy -j4 -p rocket-http --all-targets` and `cargo clippy -j4 -p rocket-infra --all-targets`
Expected: no warnings in `resolved_certificate.rs` or `reqwest_executor.rs`.

- [ ] **Step 16: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add crates/rocket-shared/src/certificate.rs crates/rocket-http/src/resolved_certificate.rs crates/rocket-http/src/lib.rs crates/rocket-infra/src/reqwest_executor.rs crates/rocket-app/src/client_certificates.rs
git commit -- crates/rocket-shared/src/certificate.rs crates/rocket-http/src/resolved_certificate.rs crates/rocket-http/src/lib.rs crates/rocket-infra/src/reqwest_executor.rs crates/rocket-app/src/client_certificates.rs
```

Suggested subject: `feat(http): add deferred RocketVault certificate material`. The message ends with `Relates to: #21`.

---

## Task A2: Persisted `vault` entry: model, validation and resolution to `Deferred`

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** sonnet.

Adding a variant to `ClientCertificate` breaks every exhaustive `match` on it, so the variant, the save-time rules (`rocket-environment`) and the resolution (`rocket-app`) land together. Between Step 3 and Step 11 the workspace does not compile; the per-crate test commands below are ordered so each one builds.

**Files:**
- Modify: `crates/rocket-shared/src/certificate.rs` (doc comment above `ClientCertificate`, the enum, `domain()`, `Debug`, tests)
- Modify: `crates/rocket-environment/src/client_certificate_validation.rs` (module doc lines 1-5, `validate_client_certificates` lines 20-62, new helpers after `reject_key_text`, tests)
- Modify: `crates/rocket-app/src/client_certificates.rs` (imports lines 10-11, `environment_client_certificates` lines 19-48, `resolve_client_certificate` lines 50-88, `absolutize_certificate_paths` lines 91-110, `resolve_references` lines 145-203, new `mod tests` at the end)
- Modify: `crates/rocket-app/src/environment_service.rs` (test import line 84, new test at the end of `mod tests`)

**Interfaces:**
- Consumes: A1's `VaultCertificateFormat`, `VaultCertificateBinding`, `ResolvedClientCertificate::deferred`, `describe_all`'s `deferred` line; `rocket_environment::ExternalSecretBinding`; `crate::test_doubles::StaticEnvRepo`.
- Produces (contract names): `ClientCertificate::Vault { domain, binding, certificate, format }` with a `#[serde(default)]` format; `domain()` covering it; `validate_client_certificates` with the `vault` rules (signature unchanged); `environment_client_certificates` mapping a `vault` entry to `Deferred` (binding found) or `Unavailable` (binding missing), signature unchanged. Consumed by A3, C1, C2, D1.

- [ ] **Step 1: Write the failing persisted-model tests**

In `crates/rocket-shared/src/certificate.rs`, add at the end of `mod tests`:

```rust
    fn vault_entry() -> ClientCertificate {
        ClientCertificate::Vault {
            domain: "api.example.com".into(),
            binding: "prod".into(),
            certificate: "client-a".into(),
            format: VaultCertificateFormat::Pkcs12,
        }
    }

    #[test]
    fn a_vault_entry_round_trips_with_its_names_and_format() {
        let json = serde_json::to_string(&vault_entry()).expect("serialize");
        assert_eq!(
            json,
            r#"{"type":"vault","domain":"api.example.com","binding":"prod","certificate":"client-a","format":"pkcs12"}"#
        );
        let back: ClientCertificate = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back, vault_entry());
    }

    #[test]
    fn a_vault_entry_without_a_format_defaults_to_pem() {
        let cert: ClientCertificate = serde_json::from_str(
            r#"{"type":"vault","domain":"a.com","binding":"prod","certificate":"client-a"}"#,
        )
        .expect("format is optional");
        assert!(matches!(
            cert,
            ClientCertificate::Vault {
                format: VaultCertificateFormat::Pem,
                ..
            }
        ));
    }

    #[test]
    fn a_vault_entry_with_an_unknown_format_is_rejected() {
        let result = serde_json::from_str::<ClientCertificate>(
            r#"{"type":"vault","domain":"a.com","binding":"prod","certificate":"c","format":"der"}"#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn domain_and_debug_cover_a_vault_entry() {
        let cert = vault_entry();
        assert_eq!(cert.domain(), "api.example.com");
        let shown = format!("{cert:?}");
        assert!(shown.contains("Vault"), "{shown}");
        assert!(shown.contains("prod") && shown.contains("client-a"), "{shown}");
    }
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-shared certificate`
Expected: FAIL to compile with `no variant named Vault found for enum ClientCertificate`.

- [ ] **Step 3: Add the `Vault` variant, `domain()` and `Debug`**

In `crates/rocket-shared/src/certificate.rs`, the doc comment above `ClientCertificate`, old:

```rust
/// Client certificate — PEM or PKCS12 format, discriminated by `type` field.
///
/// Each piece of material comes from a file path or from a RocketVault reference
/// (`alias.secretName`, never a value). A path that is not used is empty and is not written.
/// The `*Secret` keys are Rocket extensions outside the OpenCollection schema, like
/// `externalSecrets`.
```

new:

```rust
/// Client certificate — PEM, PKCS12 or RocketVault, discriminated by the `type` field.
///
/// For PEM and PKCS12, each piece of material comes from a file path or from a RocketVault
/// reference (`alias.secretName`, never a value). A path that is not used is empty and is not
/// written. The `*Secret` keys are Rocket extensions outside the OpenCollection schema, like
/// `externalSecrets`. The whole `vault` type is a Rocket extension too: it names a certificate
/// that RocketVault exports at send time, and stores names only.
```

At the end of the enum, old:

```rust
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pkcs12_secret: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
}
```

new:

```rust
        #[serde(default, skip_serializing_if = "Option::is_none")]
        pkcs12_secret: Option<String>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        passphrase: Option<String>,
    },
    /// A certificate that RocketVault exports when it is selected. `binding` is an External
    /// Secrets alias of this environment (the connection and vault come from it) and
    /// `certificate` is the certificate name in that vault. No id, password, key or path is
    /// stored.
    #[serde(rename = "vault", rename_all = "camelCase")]
    Vault {
        domain: String,
        binding: String,
        certificate: String,
        #[serde(default)]
        format: VaultCertificateFormat,
    },
}
```

`domain()`, old:

```rust
        match self {
            ClientCertificate::Pem { domain, .. } | ClientCertificate::Pkcs12 { domain, .. } => {
                domain
            }
        }
```

new:

```rust
        match self {
            ClientCertificate::Pem { domain, .. }
            | ClientCertificate::Pkcs12 { domain, .. }
            | ClientCertificate::Vault { domain, .. } => domain,
        }
```

In `impl std::fmt::Debug for ClientCertificate`, after the `Pkcs12` arm (which ends with `.finish(),`), add:

```rust
            ClientCertificate::Vault {
                domain,
                binding,
                certificate,
                format,
            } => f
                .debug_struct("Vault")
                .field("domain", domain)
                .field("binding", binding)
                .field("certificate", certificate)
                .field("format", format)
                .finish(),
```

- [ ] **Step 4: Run the persisted-model tests to verify they pass**

Run: `cargo test -j4 -p rocket-shared certificate`
Expected: PASS, including the 4 new tests and `an_old_file_only_entry_loads_and_round_trips_unchanged`.

- [ ] **Step 5: Write the failing validation tests**

In `crates/rocket-environment/src/client_certificate_validation.rs`, in `mod tests`, after the `pkcs12` helper (line 180), add:

```rust
    fn vault(domain: &str, binding: &str, certificate: &str) -> ClientCertificate {
        ClientCertificate::Vault {
            domain: domain.to_string(),
            binding: binding.to_string(),
            certificate: certificate.to_string(),
            format: rocket_shared::certificate::VaultCertificateFormat::Pem,
        }
    }
```

and at the end of `mod tests`:

```rust
    #[test]
    fn accepts_a_vault_entry_with_a_bound_alias_and_a_certificate_name() {
        let certs = [vault("api.example.com", "vault", "client-a")];
        assert!(validate_client_certificates(&certs, &bindings()).is_ok());
    }

    #[test]
    fn rejects_a_vault_entry_whose_binding_is_not_in_the_environment() {
        let msg = message(validate_client_certificates(
            &[vault("api.example.com", "payments", "client-a")],
            &bindings(),
        ));
        assert!(
            msg.contains("binding payments") && msg.contains("no External Secrets binding"),
            "{msg}"
        );
    }

    #[test]
    fn rejects_a_vault_entry_with_no_binding_or_no_certificate_name() {
        let msg = message(validate_client_certificates(
            &[vault("api.example.com", "  ", "client-a")],
            &bindings(),
        ));
        assert!(msg.contains("set binding"), "{msg}");

        let msg = message(validate_client_certificates(
            &[vault("api.example.com", "vault", " ")],
            &bindings(),
        ));
        assert!(msg.contains("set certificate"), "{msg}");
    }

    #[test]
    fn rejects_key_text_in_any_vault_field_without_echoing_it() {
        let cases = [
            (vault(KEY_TEXT, "vault", "client-a"), "domain"),
            (vault("a.example.com", KEY_TEXT, "client-a"), "binding"),
            (vault("a.example.com", "vault", KEY_TEXT), "certificate"),
        ];
        for (cert, field) in cases {
            let msg = message(validate_client_certificates(&[cert], &bindings()));
            assert!(
                msg.contains(&format!("field {field}")) && msg.contains("not key text"),
                "{field}: {msg}"
            );
            assert!(!msg.contains("MIIEvQsecret"), "the key must not be echoed: {msg}");
        }
    }
```

- [ ] **Step 6: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-environment client_certificate_validation`
Expected: FAIL to compile with `non-exhaustive patterns: &ClientCertificate::Vault { .. } not covered` in `validate_client_certificates`.

- [ ] **Step 7: Add the `vault` rules**

In `crates/rocket-environment/src/client_certificate_validation.rs`, the module doc (lines 1-5), old:

```rust
//! Save-time checks for an environment's client certificates.
//!
//! Every piece of material needs exactly one source, a reference must name a bound secret, and
//! no path or reference field may hold key text, so a private key never lands in the
//! environment file or in git.
```

new:

```rust
//! Save-time checks for an environment's client certificates.
//!
//! Every piece of material needs exactly one source, a reference must name a bound secret, and
//! no path or reference field may hold key text, so a private key never lands in the
//! environment file or in git. A `vault` entry must name one of the environment's External
//! Secrets aliases and a certificate, and none of its fields may hold key text either.
```

After the `Pkcs12` arm in `validate_client_certificates` (the arm that ends at line 59 with `}`), add:

```rust
            ClientCertificate::Vault {
                domain,
                binding,
                certificate,
                ..
            } => {
                check_vault_entry(entry, domain, binding, certificate, bindings)?;
            }
```

After `reject_key_text` (which ends at line 97), add:

```rust

/// A `vault` entry names a bound alias and a certificate. The format needs no check: serde
/// already rejects anything but `pem` and `pkcs12`. The alias is compared exactly, like the
/// lookup at send time, so a value that passes here also resolves there.
fn check_vault_entry(
    entry: usize,
    domain: &str,
    binding: &str,
    certificate: &str,
    bindings: &[ExternalSecretBinding],
) -> DomainResult<()> {
    reject_key_text_in_name(entry, "domain", domain)?;
    reject_key_text_in_name(entry, "binding", binding)?;
    reject_key_text_in_name(entry, "certificate", certificate)?;
    if binding.trim().is_empty() {
        return Err(invalid(format!(
            "Client certificate {entry}: set binding to an External Secrets alias of this \
             environment."
        )));
    }
    if !bindings.iter().any(|b| b.alias == binding) {
        return Err(invalid(format!(
            "Client certificate {entry}: binding {binding} has no External Secrets binding in \
             this environment."
        )));
    }
    if certificate.trim().is_empty() {
        return Err(invalid(format!(
            "Client certificate {entry}: set certificate to the certificate name in the vault."
        )));
    }
    Ok(())
}

/// Rejects key text in a field that holds a name, without echoing the value.
fn reject_key_text_in_name(entry: usize, field: &str, value: &str) -> DomainResult<()> {
    if value.trim_start().starts_with(PEM_MARKER) {
        return Err(invalid(format!(
            "Client certificate {entry}: field {field} must be a name, not key text."
        )));
    }
    Ok(())
}
```

- [ ] **Step 8: Run the validation tests to verify they pass**

Run: `cargo test -j4 -p rocket-environment client_certificate_validation`
Expected: PASS, the 7 existing tests and 4 new ones.

- [ ] **Step 9: Write the failing resolution tests**

In `crates/rocket-app/src/client_certificates.rs`, append at the end of the file:

```rust

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::StaticEnvRepo;
    use rocket_environment::Environment;
    use rocket_shared::certificate::VaultCertificateFormat;

    fn binding(alias: &str) -> ExternalSecretBinding {
        ExternalSecretBinding {
            alias: alias.into(),
            connection_id: "conn-1".into(),
            vault_name: "prod-vault".into(),
            secret_names: Vec::new(),
        }
    }

    fn vault_entry(domain: &str, alias: &str) -> ClientCertificate {
        ClientCertificate::Vault {
            domain: domain.into(),
            binding: alias.into(),
            certificate: "client-a".into(),
            format: VaultCertificateFormat::Pkcs12,
        }
    }

    fn resolve_env(env: Environment, vars: &[(&str, &str)]) -> Vec<String> {
        let vars: HashMap<String, String> = vars
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        let certs = environment_client_certificates(
            &StaticEnvRepo(env),
            Some("prod"),
            None,
            &vars,
            &HashMap::new(),
        );
        describe_all(&certs)
    }

    #[test]
    fn a_vault_entry_becomes_deferred_with_the_connection_and_vault_of_its_binding() {
        let mut env = Environment::new("prod");
        env.external_secrets = vec![binding("payments"), binding("prod")];
        env.client_certificates = vec![vault_entry("api.example.com", "prod")];
        assert_eq!(
            resolve_env(env, &[]),
            vec!["deferred api.example.com prod:client-a pkcs12 conn:conn-1 vault:prod-vault"]
        );
    }

    #[test]
    fn a_vault_entry_with_a_binding_the_environment_lacks_is_unavailable_and_names_it() {
        let mut env = Environment::new("prod");
        env.external_secrets = vec![binding("prod")];
        env.client_certificates = vec![vault_entry("api.example.com", "payments")];
        let lines = resolve_env(env, &[]);
        assert_eq!(lines.len(), 1);
        assert!(
            lines[0].starts_with("unavailable api.example.com")
                && lines[0].contains("payments"),
            "{lines:?}"
        );
    }

    #[test]
    fn a_placeholder_in_a_vault_entry_domain_is_resolved() {
        let mut env = Environment::new("prod");
        env.external_secrets = vec![binding("prod")];
        env.client_certificates = vec![vault_entry("{{apiHost}}", "prod")];
        let lines = resolve_env(env, &[("apiHost", "api.example.com")]);
        assert!(lines[0].starts_with("deferred api.example.com "), "{lines:?}");
    }
}
```

- [ ] **Step 10: Run them to verify they fail**

Run: `cargo test -j4 -p rocket-app client_certificates::tests`
Expected: FAIL to compile with `non-exhaustive patterns: ClientCertificate::Vault { .. } not covered` in `resolve_client_certificate`, `absolutize_certificate_paths` and `resolve_references`.

- [ ] **Step 11: Resolve a `vault` entry to `Deferred`**

In `crates/rocket-app/src/client_certificates.rs`, imports lines 10-11, old:

```rust
use rocket_environment::{resolve, EnvironmentRepository};
use rocket_http::{CertificateSource, ResolvedClientCertificate};
```

new:

```rust
use rocket_environment::{resolve, EnvironmentRepository, ExternalSecretBinding};
use rocket_http::{CertificateSource, ResolvedClientCertificate, VaultCertificateBinding};
```

The doc comment of `environment_client_certificates` (lines 19-25), old:

```rust
/// Returns the named environment's client certificates, ready for the executor: `{{placeholders}}`
/// resolved with `vars`, relative file paths joined onto `collection_dir`, and each RocketVault
/// reference replaced by the bytes found under `alias.secretName` in `external_secrets`.
///
/// A reference that cannot be resolved does not fail here. The entry becomes
/// `CertificateMaterial::Unavailable`, and the executor fails the request only when that entry is
/// the one selected for the URL.
```

new:

```rust
/// Returns the named environment's client certificates, ready for the executor: `{{placeholders}}`
/// resolved with `vars`, relative file paths joined onto `collection_dir`, and each RocketVault
/// reference replaced by the bytes found under `alias.secretName` in `external_secrets`.
/// A `vault` entry becomes `CertificateMaterial::Deferred` with the connection and vault of its
/// External Secrets binding. It holds names only, and is exported later, only when selected.
///
/// A reference or binding that cannot be resolved does not fail here. The entry becomes
/// `CertificateMaterial::Unavailable`, and the executor fails the request only when that entry is
/// the one selected for the URL.
```

The body (lines 39-47), old:

```rust
    let Ok(env) = repo.get(name) else {
        return Vec::new();
    };
    env.client_certificates
        .into_iter()
        .map(|c| resolve_client_certificate(c, vars))
        .map(|c| absolutize_certificate_paths(c, collection_dir))
        .map(|c| resolve_references(c, external_secrets))
        .collect()
```

new:

```rust
    let Ok(env) = repo.get(name) else {
        return Vec::new();
    };
    let bindings = env.external_secrets;
    env.client_certificates
        .into_iter()
        .map(|c| resolve_client_certificate(c, vars))
        .map(|c| absolutize_certificate_paths(c, collection_dir))
        .map(|c| resolve_references(c, external_secrets, &bindings))
        .collect()
```

`resolve_client_certificate`: the doc comment (lines 50-51), old:

```rust
/// Resolves `{{placeholders}}` in a certificate's domain, file paths and passphrase.
/// A reference (`certificateSecret` and the like) is a key, never a template, so it stays as is.
```

new:

```rust
/// Resolves `{{placeholders}}` in a certificate's domain, file paths and passphrase.
/// A reference (`certificateSecret` and the like) is a key, never a template, so it stays as is.
/// So do a `vault` entry's binding and certificate name; only its domain is resolved.
```

and after the `Pkcs12` arm of its `match` (the arm that ends at line 85), add:

```rust
        ClientCertificate::Vault { domain, .. } => r(domain),
```

`absolutize_certificate_paths`: after the `Pkcs12` arm (line 107), add:

```rust
        // A vault entry has no file path.
        ClientCertificate::Vault { .. } => {}
```

`resolve_references`: the doc comment and signature (lines 145-149), old:

```rust
/// Turns one persisted entry into the runtime form, looking each reference up in `secrets`.
fn resolve_references(
    cert: ClientCertificate,
    secrets: &HashMap<String, String>,
) -> ResolvedClientCertificate {
```

new:

```rust
/// Turns one persisted entry into the runtime form, looking each reference up in `secrets`
/// and each `vault` binding up in `bindings`.
fn resolve_references(
    cert: ClientCertificate,
    secrets: &HashMap<String, String>,
    bindings: &[ExternalSecretBinding],
) -> ResolvedClientCertificate {
```

and after its `Pkcs12` arm (the arm that ends at line 201), add:

```rust
        ClientCertificate::Vault {
            domain,
            binding,
            certificate,
            format: export_format,
        } => match bindings.iter().find(|b| b.alias == binding) {
            Some(found) => ResolvedClientCertificate::deferred(
                domain,
                VaultCertificateBinding {
                    alias: found.alias.clone(),
                    connection_id: found.connection_id.clone(),
                    vault_name: found.vault_name.clone(),
                },
                certificate,
                export_format,
            ),
            None => {
                let reason = format!(
                    "Client certificate for {domain} uses the External Secrets binding \
                     {binding}, which this environment does not have."
                );
                ResolvedClientCertificate::unavailable(domain, reason)
            }
        },
```

- [ ] **Step 12: Run the resolution tests to verify they pass**

Run: `cargo test -j4 -p rocket-app client_certificates::tests`
Expected: PASS, 3 tests.

Run: `cargo test -j4 -p rocket-app vault_certificates`
Expected: PASS, the existing vault-secret certificate tests are unchanged.

- [ ] **Step 13: Pin the save wiring for the new type**

In `crates/rocket-app/src/environment_service.rs`, test import line 84, old:

```rust
    use rocket_shared::certificate::ClientCertificate;
```

new:

```rust
    use rocket_shared::certificate::{ClientCertificate, VaultCertificateFormat};
```

Add at the end of `mod tests` (before its closing `}`):

```rust

    #[test]
    fn save_rejects_a_vault_certificate_whose_binding_is_missing() {
        let svc = make_service();
        let mut env = Environment::new("prod");
        env.client_certificates = vec![ClientCertificate::Vault {
            domain: "api.example.com".into(),
            binding: "prod".into(),
            certificate: "client-a".into(),
            format: VaultCertificateFormat::Pem,
        }];
        let err = svc
            .save(&env)
            .expect_err("a vault entry needs a bound alias");
        assert!(err.to_string().contains("binding prod"), "{err}");
        assert!(svc.list().expect("list").is_empty(), "nothing may be written");
    }
```

Run: `cargo test -j4 -p rocket-app save_rejects_a_vault_certificate`
Expected: PASS, 1 test. `EnvironmentService::save` already calls `validate_client_certificates` (`environment_service.rs:46-49`); this pins it for the new type.

- [ ] **Step 14: Workspace check and clippy**

Run: `cargo check -j4 --workspace`
Expected: `Finished`, no errors.

Run: `cargo clippy -j4 -p rocket-shared --all-targets`, `cargo clippy -j4 -p rocket-environment --all-targets`, `cargo clippy -j4 -p rocket-app --all-targets`
Expected: no warnings in the touched files.

- [ ] **Step 15: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add crates/rocket-shared/src/certificate.rs crates/rocket-environment/src/client_certificate_validation.rs crates/rocket-app/src/client_certificates.rs crates/rocket-app/src/environment_service.rs
git commit -- crates/rocket-shared/src/certificate.rs crates/rocket-environment/src/client_certificate_validation.rs crates/rocket-app/src/client_certificates.rs crates/rocket-app/src/environment_service.rs
```

Suggested subject: `feat(environment): add the vault client certificate entry`. The message ends with `Relates to: #21`.

---

## Task A3: YAML round trip, load-test message, spec reference note

📖 Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Model:** sonnet.

**Before starting:** this task edits `crates/rocket-app/src/execution_service.rs`. Run `git status --short` and `git pull --ff-only`. If the pull is not a fast-forward, or `execution_service.rs` shows foreign uncommitted hunks, stop and ask.

**Files:**
- Modify: `crates/rocket-infra/src/fs_environment_repo.rs` (test import line 212, new test at the end of `mod tests`, before line 764)
- Modify: `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs` (constants after line 183, test lines 807-861)
- Modify: `crates/rocket-app/src/client_certificates.rs` (new `unavailable_in_load_tests` before `describe_all`, new test in A2's `mod tests`)
- Modify: `crates/rocket-app/src/execution_service.rs` (`run_load_test`, lines 1727-1728 only)
- Modify: `crates/rocket-app/src/load_test_service.rs` (lines 30-32 only)
- Modify: `docs/superpowers/specs/opencollection-spec-reference.md` (new paragraph after line 508)

**Interfaces:**
- Consumes: A2's `ClientCertificate::Vault`, A1's `is_deferred`.
- Produces (contract names): `unavailable_in_load_tests(&mut [ResolvedClientCertificate])`; a pinned YAML shape for the `vault` entry.

- [ ] **Step 1: Write the YAML round-trip test**

In `crates/rocket-infra/src/fs_environment_repo.rs`, test import line 212, old:

```rust
    use rocket_shared::certificate::ClientCertificate;
```

new:

```rust
    use rocket_shared::certificate::{ClientCertificate, VaultCertificateFormat};
```

Add at the end of `mod tests` (before its closing `}` on line 764):

```rust

    #[test]
    fn a_vault_certificate_entry_round_trips_through_yaml_with_names_only() {
        let (dir, repo) = setup();
        let yaml = "name: prod\nclientCertificates:\n\
            - type: vault\n  domain: api.example.com\n  binding: prod\n  certificate: client-a\n  format: pkcs12\n";
        std::fs::write(dir.path().join("prod.yml"), yaml).expect("write prod.yml");

        let env = repo.get("prod").expect("a vault entry loads");
        assert_eq!(
            env.client_certificates,
            vec![ClientCertificate::Vault {
                domain: "api.example.com".into(),
                binding: "prod".into(),
                certificate: "client-a".into(),
                format: VaultCertificateFormat::Pkcs12,
            }]
        );

        repo.save(&env).expect("save");
        let raw = std::fs::read_to_string(dir.path().join("prod.yml")).expect("read prod.yml");
        let saved: serde_yaml::Value = serde_yaml::from_str(&raw).expect("parse saved");
        let original: serde_yaml::Value = serde_yaml::from_str(yaml).expect("parse original");
        assert_eq!(
            saved["clientCertificates"], original["clientCertificates"],
            "{raw}"
        );
    }
```

- [ ] **Step 2: Run it**

Run: `cargo test -j4 -p rocket-infra a_vault_certificate_entry_round_trips`
Expected: PASS, 1 test. `OcEnvironment` embeds `ClientCertificate` directly (`crates/rocket-infra/src/oc/environment.rs:47-48`), so no conversion code changes; this test pins that the shape survives a load and a save.

- [ ] **Step 3: Teach the schema-shape test the `vault` type**

In `crates/rocket-infra/src/fs_collection/schema_shape_tests.rs`, after line 183 (`const ROCKET_CLIENT_CERT_PKCS12_EXTENSIONS: &[&str] = &["pkcs12Secret"];`), add:

```rust
/// Rocket extension outside the OpenCollection schema: a certificate that RocketVault exports
/// when it is selected. The whole entry type is an extension, and it stores names only.
const ROCKET_CLIENT_CERT_VAULT: &[&str] = &["type", "domain", "binding", "certificate", "format"];
```

In `environment_client_certificates_use_schema_keys_plus_rocket_extensions`, the end of the certificate list, old:

```rust
        ClientCertificate::Pkcs12 {
            domain: "d.example.com".into(),
            pkcs12_file_path: String::new(),
            pkcs12_secret: Some("vault.clientBundleB64".into()),
            passphrase: Some("{{vault.bundlePass}}".into()),
        },
    ];
```

new:

```rust
        ClientCertificate::Pkcs12 {
            domain: "d.example.com".into(),
            pkcs12_file_path: String::new(),
            pkcs12_secret: Some("vault.clientBundleB64".into()),
            passphrase: Some("{{vault.bundlePass}}".into()),
        },
        ClientCertificate::Vault {
            domain: "e.example.com".into(),
            binding: "vault".into(),
            certificate: "client-e".into(),
            format: rocket_shared::certificate::VaultCertificateFormat::Pem,
        },
    ];
```

Then, old:

```rust
    assert_eq!(seq(doc.get("clientCertificates")).count(), 4);
```

new:

```rust
    assert_eq!(seq(doc.get("clientCertificates")).count(), 5);
```

and in the type `match`, old:

```rust
            Some("pkcs12") => [CLIENT_CERT_PKCS12, ROCKET_CLIENT_CERT_PKCS12_EXTENSIONS].concat(),
```

new:

```rust
            Some("pkcs12") => [CLIENT_CERT_PKCS12, ROCKET_CLIENT_CERT_PKCS12_EXTENSIONS].concat(),
            Some("vault") => ROCKET_CLIENT_CERT_VAULT.to_vec(),
```

- [ ] **Step 4: Run it**

Run: `cargo test -j4 -p rocket-infra environment_client_certificates_use_schema_keys`
Expected: PASS, 1 test.

- [ ] **Step 5: Write the failing load-test guard test**

In `crates/rocket-app/src/client_certificates.rs`, add at the end of A2's `mod tests`:

```rust

    // Spec section 6.
    #[test]
    fn load_tests_turn_a_vault_certificate_into_a_clear_error() {
        let mut certs = vec![
            ResolvedClientCertificate::deferred(
                "api.example.com",
                VaultCertificateBinding {
                    alias: "prod".into(),
                    connection_id: "conn-1".into(),
                    vault_name: "prod-vault".into(),
                },
                "client-a",
                VaultCertificateFormat::Pem,
            ),
            ResolvedClientCertificate::pkcs12(
                "files.example.com",
                CertificateSource::File("/certs/client.p12".into()),
                None,
            ),
        ];
        unavailable_in_load_tests(&mut certs);
        let lines = describe_all(&certs);
        assert!(
            lines[0].starts_with("unavailable api.example.com")
                && lines[0].contains("not available in load tests"),
            "{lines:?}"
        );
        assert_eq!(lines[1], "pkcs12 files.example.com file:/certs/client.p12 pass:-");
    }
```

- [ ] **Step 6: Run it to verify it fails**

Run: `cargo test -j4 -p rocket-app load_tests_turn_a_vault_certificate`
Expected: FAIL to compile with `cannot find function unavailable_in_load_tests in this scope`.

- [ ] **Step 7: Add `unavailable_in_load_tests` and call it from both load-test paths**

In `crates/rocket-app/src/client_certificates.rs`, before the `describe_all` doc comment, add:

```rust
/// Load tests send without RocketVault access, so each vault certificate becomes
/// `Unavailable` with a clear reason. As everywhere else, it fails a request only when it is
/// the one selected for the URL.
pub(crate) fn unavailable_in_load_tests(certificates: &mut [ResolvedClientCertificate]) {
    for cert in certificates.iter_mut().filter(|c| c.is_deferred()) {
        let reason = format!(
            "RocketVault certificates are not available in load tests. Use a file or a vault \
             secret certificate for {}.",
            cert.domain
        );
        *cert = ResolvedClientCertificate::unavailable(cert.domain.clone(), reason);
    }
}

```

In `crates/rocket-app/src/execution_service.rs`, `run_load_test`, lines 1727-1728, old:

```rust
        // Load testing is out of scope for external-secrets resolution.
        let resolved = self.resolve_request(&input, &std::collections::HashMap::new())?;
```

new:

```rust
        // Load testing is out of scope for external-secrets resolution and RocketVault
        // certificates, which fail with a clear message when selected.
        let mut resolved = self.resolve_request(&input, &std::collections::HashMap::new())?;
        crate::client_certificates::unavailable_in_load_tests(
            &mut resolved.options.client_certificates,
        );
```

In `crates/rocket-app/src/load_test_service.rs`, lines 30-32, old:

```rust
        // Load testing is out of scope for external-secrets resolution.
        let resolved =
            execution_service.resolve_request(&input, &std::collections::HashMap::new())?;
```

new:

```rust
        // Load testing is out of scope for external-secrets resolution and RocketVault
        // certificates, which fail with a clear message when selected.
        let mut resolved =
            execution_service.resolve_request(&input, &std::collections::HashMap::new())?;
        crate::client_certificates::unavailable_in_load_tests(
            &mut resolved.options.client_certificates,
        );
```

- [ ] **Step 8: Run the load-test tests**

Run: `cargo test -j4 -p rocket-app load_tests_turn_a_vault_certificate`
Expected: PASS, 1 test.

Run: `cargo test -j4 -p rocket-app load_test`
Expected: PASS, the existing `load_test_service` and `run_load_test` tests are unchanged.

- [ ] **Step 9: Add the spec reference note**

In `docs/superpowers/specs/opencollection-spec-reference.md`, after the paragraph that ends on line 508 ("... only when that certificate is the one selected for the URL."), insert a blank line and:

````markdown
**Rocket extension entry type `vault` (not in the OpenCollection schema).** An entry may name a certificate that RocketVault exports at send time, instead of naming material:

```yaml
- type: vault
  domain: api.example.com
  binding: prod          # External Secrets alias of this environment; the connection and vault come from it
  certificate: client-a  # certificate name in that vault
  format: pem            # pem (default) or pkcs12
```

Only names are stored: no id, version, password, key text or path. On save the binding must be one of the environment's `externalSecrets` aliases, the certificate name must be set, and no field may start with `-----BEGIN`. At send time Rocket exports the certificate only when it is the one selected for the URL (requests and OAuth2 token URLs alike), keeps the material in memory, and fails that request with a clear message if the export fails, with no fallback to another entry. Load tests cannot use it. Other OpenCollection tools do not know this type, and Rocket builds from before it fail to parse an environment file that holds it, so they hide that environment.
````

- [ ] **Step 10: Workspace check and clippy**

Run: `cargo check -j4 --workspace`
Expected: `Finished`, no errors.

Run: `cargo clippy -j4 -p rocket-app --all-targets` and `cargo clippy -j4 -p rocket-infra --all-targets`
Expected: no warnings in the touched files.

- [ ] **Step 11: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill. Stage and commit only these paths:

```bash
git add crates/rocket-infra/src/fs_environment_repo.rs crates/rocket-infra/src/fs_collection/schema_shape_tests.rs crates/rocket-app/src/client_certificates.rs crates/rocket-app/src/execution_service.rs crates/rocket-app/src/load_test_service.rs docs/superpowers/specs/opencollection-spec-reference.md
git commit -- crates/rocket-infra/src/fs_environment_repo.rs crates/rocket-infra/src/fs_collection/schema_shape_tests.rs crates/rocket-app/src/client_certificates.rs crates/rocket-app/src/execution_service.rs crates/rocket-app/src/load_test_service.rs docs/superpowers/specs/opencollection-spec-reference.md
```

Suggested subject: `feat(environment): pin the vault entry YAML and fail it in load tests`. The message ends with `Relates to: #21`.

---

## Milestone Checklist: Plan A

- [ ] `VaultCertificateFormat` (`pem` default, `pkcs12`) in `rocket-shared`; `ClientCertificate::Vault` serializes as `{"type":"vault","domain","binding","certificate","format"}` and round-trips through YAML
- [ ] `CertificateMaterial::Deferred` and `VaultCertificateBinding` exported from `rocket-http`; `Debug` shows names only
- [ ] A `vault` entry resolves to `Deferred` with the binding's connection and vault, or to `Unavailable` naming a missing binding
- [ ] The executor fails a selected `Deferred` entry with `InvalidInput` and sends nothing; an entry for another domain does not block the request
- [ ] Save rejects an unbound alias, an empty binding or certificate, and key text in any `vault` field
- [ ] Both load-test paths turn vault certificates into "not available in load tests"
- [ ] The spec reference note documents the `vault` type and the older-build behaviour
- [ ] `cargo check -j4 --workspace` and clippy for each touched crate are clean

## Next Plan

[Plan B: RocketVault client and fetcher](02-plan-b-client-and-fetcher.md): adds `list_certificates` and `fetch_certificate` to `VaultSecretFetcher`, puts the RocketVault certificate contract in `rocketvault/certificate_api.rs`, and implements the paged list, the name-to-id cache with one retry on a 404, and the export with a one-time PKCS12 password.
