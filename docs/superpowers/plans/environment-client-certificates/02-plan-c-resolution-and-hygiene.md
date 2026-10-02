# Environment Client Certificates, Plan C: Resolution and Hygiene

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Turn the persisted certificate references (`certificateSecret`, `privateKeySecret`, `pkcs12Secret`) into in-memory `CertificateSource::Inline` material for both request execution and OAuth2 token requests, fail only when the unresolved entry is the one selected for the URL, prove that key material and vault values never leak (redaction, `Debug`, serialization, logs, history, events, audit), and bring the docs in line.

**Architecture:** One function in `rocket-app` (`client_certificates::environment_client_certificates`) gains an `external_secrets` parameter and does all resolution, so `RequestExecutionService::resolve_request` and `OAuth2Service` behave identically. A reference that cannot be resolved becomes `CertificateMaterial::Unavailable { reason }`. `rocket-infra`'s `identity_for_url` already selects the entry by domain before loading anything, so an `Unavailable` entry only fails when it is selected; Plan C pins that with tests instead of changing it. The security pass adds a `redaction_forms()` helper so multi-line vault values are masked line by line, a 1 MiB cap on inline secrets, and leak tests.

**Tech Stack:** Rust (`rocket-app`, `rocket-http`, `rocket-infra`), `zeroize`, `base64`, `wiremock`, Markdown docs.

**Spec:** [../../specs/2026-10-01-environment-client-certificates-design.md](../../specs/2026-10-01-environment-client-certificates-design.md) (sections 6, 8, 11, 12, 13). Plan index and shared interface contract: [00-plan-index.md](00-plan-index.md). Depends on Plan B (`01-plan-b-model-and-loading.md`): `ResolvedClientCertificate`, `CertificateSource`, `CertificateMaterial`, the persisted reference fields, `validate_client_certificates`, and inline loading in `rocket-infra`. This plan uses those names exactly as in the contract.

## Global Constraints (every task includes these)

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

## Review Focus (owned by Plan C)

1. **A reference to a missing secret must not break unrelated requests.** Two certificates, the second has an unresolved reference and a different domain. A request to the first domain succeeds. Pinned by:
   - `vault_certificates::vault_certificates_missing_secret_on_another_domain_leaves_the_selected_certificate_usable` (`rocket-app`, `execution_service.rs`, Task C1),
   - `vault_certificates::vault_certificates_oauth_missing_secret_on_another_domain_leaves_the_selected_certificate_usable` (`rocket-app`, `oauth2_service.rs`, Task C1),
   - `unavailable_certificates::unavailable_certificate_for_another_domain_does_not_affect_the_request` (`rocket-infra`, `reqwest_executor.rs`, Task C1), which sends a real request.
2. (Owned by Plan B3, not here: CRLF and trailing whitespace in PEM vault values.)
3. **A base64 PKCS12 secret wrapped across lines.** Whitespace inside the base64, including newlines, is ignored when decoding. Pinned by `vault_certificates::vault_certificates_wrapped_base64_pkcs12_secret_decodes` (`rocket-app`, `execution_service.rs`, Task C1).

## Facts checked against the code on 2026-10-01 (before Plan B lands)

| Fact | Where |
|---|---|
| `environment_client_certificates(repo, environment_name, collection_dir, vars)` returns `Vec<ClientCertificate>` today, file is 114 lines. Plan B1 changes the return type, C1 adds the last parameter. | `crates/rocket-app/src/client_certificates.rs` |
| `resolve_request(&self, input, external_secrets)` already receives the RocketVault map and calls the private `environment_client_certificates(input, &vars)` at line 551. | `crates/rocket-app/src/execution_service.rs` 489-560, 564-583 |
| `OAuth2Service::client_certificates(&self, collection, environment_name, vars)` is called from `refresh_token_with_secrets` (line 417) and `resolve_get_token_request_with_secrets` (line 552); both already hold `external_secrets`. | `crates/rocket-app/src/oauth2_service.rs` 153-181 |
| `identity_for_url` calls `find_certificate` first and only then `load_identity`, so a certificate that is not selected is never loaded. The token client provider and `fetch_client_credentials_token` use the same function. | `crates/rocket-infra/src/reqwest_executor.rs` 446-458, 464-472, 844 |
| External secret values enter `VariableContext.secret_values` whole, only when `len >= MIN_REDACTION_LEN` (6), in `build_variable_scopes`. Nothing adds individual lines. | `crates/rocket-app/src/execution_service.rs` 435-440, `crates/rocket-app/src/redaction.rs` 9-23 |
| Script console masking is `redact()` in `ops/mod.rs` (`console.rs` calls it). It replaces every member of `secret_values` longest first, with no length floor of its own. It does not split values. | `crates/rocket-infra/src/scripting/ops/mod.rs` 31-47, `ops/console.rs` |
| `HttpRequest` and `RequestOptions` derive `Debug`, `Serialize`, `Deserialize`. `RequestOptions.client_certificates` is `Vec<ClientCertificate>` today (Plan B1 makes it `Vec<ResolvedClientCertificate>` with `#[serde(skip)]`). | `crates/rocket-http/src/request.rs` 7-35 |
| `rocket-app` has no `zeroize` dependency (`rocket-infra` has `zeroize = "1"`). `base64` is already a `rocket-app` dependency. | `crates/rocket-app/Cargo.toml` |
| `crates/rocket-app/src/redaction.rs` has uncommitted formatting-only edits from another session (`git status` at plan time). | working tree |
| Both load-test paths call `resolve_request(&input, &HashMap::new())`, so they never see vault values. | `execution_service.rs` 1566, `load_test_service.rs` 32 |

---

## Task C1: Resolve references for requests and OAuth2, fail only when selected

**Model:** sonnet.

Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Files:**
- Modify: `crates/rocket-app/Cargo.toml` (add `zeroize = "1"` after `base64.workspace = true`, line 26)
- Modify: `Cargo.lock` (only the `rocket-app` dependency list gains `zeroize`; check with `git diff Cargo.lock`)
- Modify: `crates/rocket-app/src/client_certificates.rs` (whole file, 114 lines today)
- Modify: `crates/rocket-app/src/execution_service.rs` (call at line ~551, private `environment_client_certificates` at ~564-583, new test module inserted before `resolve_request_resolves_placeholders_in_inherited_collection_auth`, ~line 3033)
- Modify: `crates/rocket-app/src/oauth2_service.rs` (`client_certificates` at 153-181, callers at ~417 and ~552, new test module inserted before `/// A factory that knows where the collection lives`, ~line 1208)
- Modify: `crates/rocket-infra/src/reqwest_executor.rs` (tests only, new module inserted before `async fn redirecting_to`, ~line 1806)

**Interfaces:**
- Consumes (Plan B, contract): `rocket_http::{ResolvedClientCertificate, CertificateSource, CertificateMaterial}` with `ResolvedClientCertificate::{pem, pkcs12, unavailable}`, `rocket_http::client_cert::find_certificate`, the persisted `ClientCertificate` with `certificate_secret`, `private_key_secret`, `pkcs12_secret`, and the `external_secrets` map (keyed `"alias.secretName"`) that `resolve_request`, `resolve_get_token_request_with_secrets` and `refresh_token_with_secrets` already hold.
- Produces (contract C1):
  ```rust
  pub(crate) fn environment_client_certificates(
      repo: &dyn EnvironmentRepository,
      environment_name: Option<&str>,
      collection_dir: Option<&Path>,
      vars: &HashMap<String, String>,
      external_secrets: &HashMap<String, String>,
  ) -> Vec<ResolvedClientCertificate>;
  ```
  Also `pub(crate) const MAX_INLINE_SECRET_BYTES` is added in Task C2, not here.

Resolution rules, per piece (certificate, private key, PKCS12 bundle), after placeholders and relative paths:

| File path | Reference | Result |
|---|---|---|
| non-empty | none | `CertificateSource::File(path)` |
| empty | found | `Inline` (PEM: the value's bytes untouched; PKCS12: base64 decoded, all whitespace ignored) |
| empty | not found | the entry is `Unavailable`: "Client certificate secret vault.clientCertPem was not found. Check the External Secrets binding and fetch the secret names." |
| empty | found, not base64 (PKCS12) | `Unavailable`: "Client certificate secret vault.bundle is not valid base64." (never the value, never the decoder's message) |
| empty | found, empty value | `Unavailable`: "Client certificate secret vault.x is empty." |
| non-empty | present | `Unavailable`: names the domain, says to use only one (the save-time validator rejects this, so it only happens for a hand-edited file) |
| empty | none | `Unavailable`: names the domain, says there is neither a file path nor a secret reference |

- [ ] **Step 1: Add the `zeroize` dependency to `rocket-app`**

In `crates/rocket-app/Cargo.toml` replace:

```toml
base64.workspace = true
regex = "1"
```

with:

```toml
base64.workspace = true
zeroize = "1"
regex = "1"
```

Run: `cargo check -j4 -p rocket-app`
Expected: compiles (Plan B state). `zeroize` was already resolved for `rocket-infra`, so `Cargo.lock` only gains a dependency entry on `rocket-app`.

- [ ] **Step 2: Write the failing resolution tests in `execution_service.rs`**

Insert the following module into the `tests` module of `crates/rocket-app/src/execution_service.rs`, directly before this existing test (use Edit with this anchor, it is unique):

```rust
    #[tokio::test]
    async fn resolve_request_resolves_placeholders_in_inherited_collection_auth() {
```

Code to insert (the anchor line stays after it):

```rust
    /// Tests for vault-backed client certificate material (Plan C). The helpers are `pub(super)`
    /// so the hygiene module of Task C2 can reuse them.
    mod vault_certificates {
        use super::*;
        use base64::Engine as _;
        use rocket_http::{CertificateMaterial, CertificateSource, ResolvedClientCertificate};

        pub(super) const CERT_PEM: &str =
            "-----BEGIN CERTIFICATE-----\r\nMIIBcertbody0123\r\n-----END CERTIFICATE-----\r\n";
        pub(super) const KEY_PEM: &str =
            "-----BEGIN PRIVATE KEY-----\nMIIEkeybody0123\n-----END PRIVATE KEY-----\n";

        pub(super) fn vault_pem(
            domain: &str,
            cert_ref: &str,
            key_ref: &str,
            passphrase: Option<&str>,
        ) -> ClientCertificate {
            ClientCertificate::Pem {
                domain: domain.into(),
                certificate_file_path: String::new(),
                private_key_file_path: String::new(),
                certificate_secret: Some(cert_ref.into()),
                private_key_secret: Some(key_ref.into()),
                passphrase: passphrase.map(String::from),
            }
        }

        pub(super) fn vault_p12(domain: &str, reference: &str) -> ClientCertificate {
            ClientCertificate::Pkcs12 {
                domain: domain.into(),
                pkcs12_file_path: String::new(),
                pkcs12_secret: Some(reference.into()),
                passphrase: None,
            }
        }

        pub(super) fn secrets(
            pairs: &[(&str, &str)],
        ) -> std::collections::HashMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        }

        /// Resolves `certs` through `resolve_request` for an environment named `dev`.
        pub(super) fn resolve_certificates(
            certs: Vec<ClientCertificate>,
            secrets: &std::collections::HashMap<String, String>,
        ) -> Vec<ResolvedClientCertificate> {
            let mut env = Environment::new("dev");
            env.client_certificates = certs;
            let svc = service_with(env, None);
            svc.resolve_request(&sample_input("https://a.example.com/x", Some("dev")), secrets)
                .expect("resolve_request")
                .options
                .client_certificates
        }

        pub(super) fn source_bytes(source: &CertificateSource) -> Vec<u8> {
            match source {
                CertificateSource::Inline(bytes) => bytes.to_vec(),
                CertificateSource::File(path) => {
                    panic!("expected inline material, got file {path}")
                }
            }
        }

        pub(super) fn unavailable_reason(cert: &ResolvedClientCertificate) -> String {
            match &cert.material {
                CertificateMaterial::Unavailable { reason } => reason.clone(),
                _ => panic!("expected an unavailable certificate for {}", cert.domain),
            }
        }

        #[tokio::test]
        async fn vault_certificates_pem_references_resolve_to_inline_bytes_unchanged() {
            let certs = resolve_certificates(
                vec![vault_pem(
                    "a.example.com",
                    "vault.certPem",
                    "vault.keyPem",
                    Some("{{vault.keyPass}}"),
                )],
                &secrets(&[
                    ("vault.certPem", CERT_PEM),
                    ("vault.keyPem", KEY_PEM),
                    ("vault.keyPass", "p4ss-word"),
                ]),
            );
            assert_eq!(certs.len(), 1);
            assert_eq!(certs[0].domain, "a.example.com");
            let CertificateMaterial::Pem {
                certificate,
                private_key,
                passphrase,
            } = &certs[0].material
            else {
                panic!("expected a PEM certificate");
            };
            // Byte for byte, including the CRLF line endings.
            assert_eq!(source_bytes(certificate), CERT_PEM.as_bytes());
            assert_eq!(source_bytes(private_key), KEY_PEM.as_bytes());
            assert_eq!(passphrase.as_ref().map(|p| p.as_str()), Some("p4ss-word"));
        }

        #[tokio::test]
        async fn vault_certificates_pkcs12_secret_is_base64_decoded() {
            let certs = resolve_certificates(
                vec![vault_p12("a.example.com", "vault.bundle")],
                &secrets(&[("vault.bundle", "AQIDBAU=")]),
            );
            let CertificateMaterial::Pkcs12 { bundle, .. } = &certs[0].material else {
                panic!("expected a PKCS12 certificate");
            };
            assert_eq!(source_bytes(bundle), vec![1u8, 2, 3, 4, 5]);
        }

        // Review Focus item 3.
        #[tokio::test]
        async fn vault_certificates_wrapped_base64_pkcs12_secret_decodes() {
            let bundle: Vec<u8> = (0u8..=200).collect();
            let encoded = base64::engine::general_purpose::STANDARD.encode(&bundle);
            // Wrapped at 64 columns with CRLF and an indent on every line, plus a trailing newline.
            let wrapped = format!(
                "{}\r\n",
                encoded
                    .as_bytes()
                    .chunks(64)
                    .map(|c| format!("  {}", std::str::from_utf8(c).expect("ascii")))
                    .collect::<Vec<_>>()
                    .join("\r\n")
            );
            assert!(wrapped.matches("\r\n").count() > 2, "the fixture must wrap");

            let certs = resolve_certificates(
                vec![vault_p12("a.example.com", "vault.bundle")],
                &secrets(&[("vault.bundle", wrapped.as_str())]),
            );
            let CertificateMaterial::Pkcs12 { bundle: got, .. } = &certs[0].material else {
                panic!("expected a PKCS12 certificate");
            };
            assert_eq!(source_bytes(got), bundle);
        }

        #[tokio::test]
        async fn vault_certificates_bad_base64_is_unavailable_and_never_echoes_the_value() {
            let certs = resolve_certificates(
                vec![vault_p12("a.example.com", "vault.bundle")],
                &secrets(&[("vault.bundle", "this is !!! not base64")]),
            );
            let reason = unavailable_reason(&certs[0]);
            assert_eq!(
                reason,
                "Client certificate secret vault.bundle is not valid base64."
            );
            assert!(!reason.contains("!!!"), "{reason}");
        }

        #[tokio::test]
        async fn vault_certificates_missing_secret_uses_the_spec_message() {
            let certs = resolve_certificates(
                vec![vault_pem("a.example.com", "vault.clientCertPem", "vault.k", None)],
                &secrets(&[("vault.k", KEY_PEM)]),
            );
            assert_eq!(
                unavailable_reason(&certs[0]),
                "Client certificate secret vault.clientCertPem was not found. \
                 Check the External Secrets binding and fetch the secret names."
            );
        }

        #[tokio::test]
        async fn vault_certificates_empty_secret_is_unavailable() {
            let certs = resolve_certificates(
                vec![vault_p12("a.example.com", "vault.bundle")],
                &secrets(&[("vault.bundle", "  \r\n")]),
            );
            assert_eq!(
                unavailable_reason(&certs[0]),
                "Client certificate secret vault.bundle is empty."
            );
        }

        // Review Focus item 1, at the resolution level.
        #[tokio::test]
        async fn vault_certificates_missing_secret_on_another_domain_leaves_the_selected_certificate_usable(
        ) {
            let certs = resolve_certificates(
                vec![
                    vault_p12("a.example.com", "vault.ok"),
                    vault_p12("b.example.com", "vault.missing"),
                ],
                &secrets(&[("vault.ok", "AQIDBAU=")]),
            );
            let first = rocket_http::client_cert::find_certificate(&certs, "https://a.example.com/x")
                .expect("the first certificate is selected");
            assert!(matches!(
                first.material,
                CertificateMaterial::Pkcs12 {
                    bundle: CertificateSource::Inline(_),
                    ..
                }
            ));
            let second =
                rocket_http::client_cert::find_certificate(&certs, "https://b.example.com/x")
                    .expect("the second certificate is selected");
            assert!(unavailable_reason(second).contains("vault.missing"));
            assert!(
                rocket_http::client_cert::find_certificate(&certs, "https://c.example.com/x")
                    .is_none()
            );
        }

        #[tokio::test]
        async fn vault_certificates_a_piece_needs_exactly_one_source() {
            let both = ClientCertificate::Pkcs12 {
                domain: "a.example.com".into(),
                pkcs12_file_path: "/abs/client.p12".into(),
                pkcs12_secret: Some("vault.bundle".into()),
                passphrase: None,
            };
            let neither = ClientCertificate::Pkcs12 {
                domain: "b.example.com".into(),
                pkcs12_file_path: String::new(),
                pkcs12_secret: None,
                passphrase: None,
            };
            let certs =
                resolve_certificates(vec![both, neither], &secrets(&[("vault.bundle", "AQIDBAU=")]));
            let both_reason = unavailable_reason(&certs[0]);
            assert!(
                both_reason.contains("a.example.com") && both_reason.contains("only one"),
                "{both_reason}"
            );
            let neither_reason = unavailable_reason(&certs[1]);
            assert!(
                neither_reason.contains("b.example.com") && neither_reason.contains("neither"),
                "{neither_reason}"
            );
        }

        #[tokio::test]
        async fn vault_certificates_file_paths_stay_file_sources() {
            let file = ClientCertificate::Pkcs12 {
                domain: "a.example.com".into(),
                pkcs12_file_path: "/abs/client.p12".into(),
                pkcs12_secret: None,
                passphrase: Some("changeit".into()),
            };
            let certs = resolve_certificates(vec![file], &secrets(&[]));
            assert!(matches!(
                &certs[0].material,
                CertificateMaterial::Pkcs12 {
                    bundle: CertificateSource::File(p),
                    ..
                } if p == "/abs/client.p12"
            ));
        }
    }

```

Run: `cargo test -j4 -p rocket-app vault_certificates`
Expected: the module compiles (Plan B types exist) and the tests FAIL, for example `vault_certificates_pem_references_resolve_to_inline_bytes_unchanged` panics with `expected inline material, got file ` and `vault_certificates_missing_secret_uses_the_spec_message` panics with `expected an unavailable certificate for a.example.com`. `vault_certificates_file_paths_stay_file_sources` may already pass. If the module does not compile because a Plan B name differs from the contract, stop and report it (the contract must not change silently).

- [ ] **Step 3: Write the failing OAuth2 tests in `oauth2_service.rs`**

Insert into the `tests` module of `crates/rocket-app/src/oauth2_service.rs`, directly before this existing comment (unique anchor):

```rust
    /// A factory that knows where the collection lives, like the real workspace one.
```

Code to insert:

```rust
    mod vault_certificates {
        use super::*;
        use rocket_http::{CertificateMaterial, CertificateSource};

        fn vault_p12(domain: &str, reference: &str) -> ClientCertificate {
            ClientCertificate::Pkcs12 {
                domain: domain.into(),
                pkcs12_file_path: String::new(),
                pkcs12_secret: Some(reference.into()),
                passphrase: None,
            }
        }

        fn secrets(pairs: &[(&str, &str)]) -> HashMap<String, String> {
            pairs
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect()
        }

        #[test]
        fn vault_certificates_a_get_token_request_carries_inline_material_from_the_vault() {
            let provider = CapturingProvider::new();
            let svc = service_with_certificates(
                vec![vault_p12("idp.example.com", "vault.bundle")],
                &provider,
            );
            let config = svc.resolve_get_token_request_with_secrets(
                &get_token_request(),
                &secrets(&[("vault.bundle", "AQIDBAU=")]),
            );
            let CertificateMaterial::Pkcs12 {
                bundle: CertificateSource::Inline(bytes),
                ..
            } = &config.client_certificates[0].material
            else {
                panic!("expected inline PKCS12 material");
            };
            assert_eq!(&bytes[..], &[1u8, 2, 3, 4, 5][..]);
        }

        #[tokio::test]
        async fn vault_certificates_a_refresh_resolves_inline_material_for_the_provider() {
            let provider = CapturingProvider::new();
            let svc = service_with_certificates(
                vec![vault_p12("idp.example.com", "vault.bundle")],
                &provider,
            );
            let req = OAuth2RefreshRequest {
                refresh_token: "r".into(),
                token_url: "https://idp.example.com/token".into(),
                refresh_token_url: None,
                client_id: "id".into(),
                client_secret: None,
                scope: None,
                client_authentication: None,
                verify_ssl: None,
                refresh_params: None,
                collection: None,
                environment_name: Some("dev".into()),
                request_path: None,
            };

            // The capturing provider fails on purpose, so only what it saw matters.
            let _ = svc
                .refresh_token_with_secrets(&req, &secrets(&[("vault.bundle", "AQIDBAU=")]))
                .await;

            let seen = provider.seen.lock().unwrap();
            assert!(matches!(
                &seen[0].2[0].material,
                CertificateMaterial::Pkcs12 {
                    bundle: CertificateSource::Inline(_),
                    ..
                }
            ));
        }

        // Review Focus item 1, for OAuth2 token requests.
        #[test]
        fn vault_certificates_oauth_missing_secret_on_another_domain_leaves_the_selected_certificate_usable(
        ) {
            let provider = CapturingProvider::new();
            let svc = service_with_certificates(
                vec![
                    vault_p12("idp.example.com", "vault.ok"),
                    vault_p12("other.example.com", "vault.missing"),
                ],
                &provider,
            );
            let config = svc.resolve_get_token_request_with_secrets(
                &get_token_request(),
                &secrets(&[("vault.ok", "AQIDBAU=")]),
            );
            let selected = rocket_http::client_cert::find_certificate(
                &config.client_certificates,
                "https://idp.example.com/token",
            )
            .expect("the certificate for the token host is selected");
            assert!(matches!(
                selected.material,
                CertificateMaterial::Pkcs12 {
                    bundle: CertificateSource::Inline(_),
                    ..
                }
            ));
        }

        #[test]
        fn vault_certificates_without_secrets_a_reference_is_unavailable_not_a_panic() {
            let provider = CapturingProvider::new();
            let svc = service_with_certificates(
                vec![vault_p12("idp.example.com", "vault.bundle")],
                &provider,
            );
            let config = svc.resolve_get_token_request(&get_token_request());
            let CertificateMaterial::Unavailable { reason } =
                &config.client_certificates[0].material
            else {
                panic!("expected an unavailable certificate");
            };
            assert!(reason.contains("vault.bundle"), "{reason}");
        }
    }

```

Run: `cargo test -j4 -p rocket-app vault_certificates`
Expected: the new OAuth2 tests FAIL (`expected inline PKCS12 material`, index out of bounds or assertion failures), plus the Step 2 failures.

- [ ] **Step 4: Write the `rocket-infra` tests (Review Focus item 1 with a real request)**

Insert into the mTLS test module of `crates/rocket-infra/src/reqwest_executor.rs` (the one that starts at line 1587 and defines `fixture`, `ok_server` and `redirecting_to`), directly before this existing line (unique anchor):

```rust
    async fn redirecting_to(target: &str) -> MockServer {
```

Code to insert:

```rust
    mod unavailable_certificates {
        use super::*;
        use rocket_http::{CertificateSource, ResolvedClientCertificate, TokenClientProvider};

        const MISSING: &str = "Client certificate secret vault.missing was not found. \
                               Check the External Secrets binding and fetch the secret names.";

        fn valid(domain: &str) -> ResolvedClientCertificate {
            ResolvedClientCertificate::pkcs12(
                domain,
                CertificateSource::File(fixture("client.p12")),
                Some("changeit".into()),
            )
        }

        fn unavailable(domain: &str) -> ResolvedClientCertificate {
            ResolvedClientCertificate::unavailable(domain, MISSING)
        }

        // Review Focus item 1.
        #[tokio::test]
        async fn unavailable_certificate_for_another_domain_does_not_affect_the_request() {
            let server = ok_server().await;
            for certs in [
                vec![valid("127.0.0.1"), unavailable("other.example.com")],
                vec![unavailable("other.example.com"), valid("127.0.0.1")],
            ] {
                let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
                req.options.client_certificates = certs;
                let resp = ReqwestExecutor::new().execute(&req).await.unwrap();
                assert_eq!(resp.status, 200);
            }
        }

        #[tokio::test]
        async fn unavailable_certificate_that_is_selected_fails_the_request_without_a_fallback() {
            let server = ok_server().await;
            let mut req = HttpRequest::new(HttpMethod::Get, format!("{}/r", server.uri()));
            // The valid certificate for the same domain comes second and must not be used.
            req.options.client_certificates = vec![unavailable("127.0.0.1"), valid("127.0.0.1")];
            let err = ReqwestExecutor::new().execute(&req).await.unwrap_err();
            assert!(err.to_string().contains("vault.missing"), "{err}");
            assert!(server.received_requests().await.unwrap().is_empty());
        }

        #[test]
        fn unavailable_certificate_fails_the_token_client_only_when_selected() {
            let provider = ReqwestTokenClientProvider;
            let certs = [unavailable("idp.example.com")];
            assert!(provider
                .client_for("https://other.example.com/token", true, &certs)
                .is_ok());
            let err = provider
                .client_for("https://idp.example.com/token", true, &certs)
                .err()
                .expect("a selected unavailable certificate is an error")
                .to_string();
            assert!(err.contains("vault.missing"), "{err}");
        }
    }

```

Run: `cargo test -j4 -p rocket-infra unavailable_certificate`
Expected: PASS already (3 tests). `identity_for_url` selects by domain before loading, and Plan B1 makes `Unavailable` an `InvalidInput(reason)`. They are regression pins for Review Focus item 1. If one fails, the executor loads an unselected entry or does not handle `Unavailable`: fix `identity_for_url` so `find_certificate` runs first and `load_identity` returns `DomainError::InvalidInput(reason.clone())` for `CertificateMaterial::Unavailable { reason }`.

- [ ] **Step 5: Replace `client_certificates.rs` with the resolving version**

Read the file first (Plan B1 edited it). Keep any tests B added. The code below replaces the whole non-test body; it does the same placeholder and relative-path work as today (in place, on the persisted type) and adds the reference step. Write `crates/rocket-app/src/client_certificates.rs`:

```rust
//! Client certificates of the selected environment, prepared for the HTTP layer.
//!
//! Shared by request execution and the OAuth2 token requests, so both resolve placeholders,
//! relative paths and RocketVault references in exactly the same way.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use rocket_environment::{resolve, EnvironmentRepository};
use rocket_http::{CertificateSource, ResolvedClientCertificate};
use rocket_shared::certificate::ClientCertificate;
use std::collections::HashMap;
use std::path::{Component, Path, PathBuf};
use zeroize::Zeroizing;

/// Returns the named environment's client certificates, ready for the executor: `{{placeholders}}`
/// resolved with `vars`, relative file paths joined onto `collection_dir`, and each RocketVault
/// reference replaced by the bytes found under `alias.secretName` in `external_secrets`.
///
/// A reference that cannot be resolved does not fail here. The entry becomes
/// `CertificateMaterial::Unavailable`, and the executor fails the request only when that entry is
/// the one selected for the URL.
///
/// No environment name, or an environment that cannot be read, means no certificates, like it
/// means no variables.
pub(crate) fn environment_client_certificates(
    repo: &dyn EnvironmentRepository,
    environment_name: Option<&str>,
    collection_dir: Option<&Path>,
    vars: &HashMap<String, String>,
    external_secrets: &HashMap<String, String>,
) -> Vec<ResolvedClientCertificate> {
    let Some(name) = environment_name else {
        return Vec::new();
    };
    let Ok(env) = repo.get(name) else {
        return Vec::new();
    };
    env.client_certificates
        .into_iter()
        .map(|c| resolve_client_certificate(c, vars))
        .map(|c| absolutize_certificate_paths(c, collection_dir))
        .map(|c| resolve_references(c, external_secrets))
        .collect()
}

/// Resolves `{{placeholders}}` in a client certificate's domain, file paths and passphrase.
/// A reference (`certificateSecret` and the like) is a key, never a template, so it stays as is.
fn resolve_client_certificate(
    mut cert: ClientCertificate,
    vars: &HashMap<String, String>,
) -> ClientCertificate {
    let r = |s: &mut String| {
        *s = resolve(s, vars).output;
    };
    match &mut cert {
        ClientCertificate::Pem {
            domain,
            certificate_file_path,
            private_key_file_path,
            passphrase,
            ..
        } => {
            r(domain);
            r(certificate_file_path);
            r(private_key_file_path);
            if let Some(p) = passphrase {
                r(p);
            }
        }
        ClientCertificate::Pkcs12 {
            domain,
            pkcs12_file_path,
            passphrase,
            ..
        } => {
            r(domain);
            r(pkcs12_file_path);
            if let Some(p) = passphrase {
                r(p);
            }
        }
    }
    cert
}

/// Joins a relative certificate file path onto the collection folder `base`.
///
/// Absolute paths and `~/` paths stay as written. So does a relative path with a `..` in it, so
/// an environment file cannot point outside the collection folder. The executor rejects any
/// path that is still relative, with a message that says what is allowed.
fn absolutize_certificate_paths(
    mut cert: ClientCertificate,
    base: Option<&Path>,
) -> ClientCertificate {
    let Some(base) = base else { return cert };
    match &mut cert {
        ClientCertificate::Pem {
            certificate_file_path,
            private_key_file_path,
            ..
        } => {
            join_onto(base, certificate_file_path);
            join_onto(base, private_key_file_path);
        }
        ClientCertificate::Pkcs12 {
            pkcs12_file_path, ..
        } => join_onto(base, pkcs12_file_path),
    }
    cert
}

fn join_onto(base: &Path, p: &mut String) {
    let path = Path::new(p.as_str());
    let stays = p.is_empty()
        || p.starts_with("~/")
        || path.is_absolute()
        || path.components().any(|c| matches!(c, Component::ParentDir));
    if stays {
        return;
    }
    // Drop `.` components so `./certs/a.pem` joins as `certs/a.pem`.
    let tidy: PathBuf = path
        .components()
        .filter(|c| !matches!(c, Component::CurDir))
        .collect();
    *p = base.join(tidy).to_string_lossy().into_owned();
}

/// How the text of a secret becomes bytes.
#[derive(Clone, Copy)]
enum Encoding {
    /// PEM text, used byte for byte.
    Text,
    /// A base64 PKCS12 bundle. Whitespace, including line breaks, is ignored.
    Base64,
}

/// Turns one persisted entry into the runtime form, looking each reference up in `secrets`.
fn resolve_references(
    cert: ClientCertificate,
    secrets: &HashMap<String, String>,
) -> ResolvedClientCertificate {
    match cert {
        ClientCertificate::Pem {
            domain,
            certificate_file_path,
            private_key_file_path,
            certificate_secret,
            private_key_secret,
            passphrase,
        } => {
            let certificate = piece_source(
                &domain,
                "certificate",
                certificate_file_path,
                certificate_secret,
                secrets,
                Encoding::Text,
            );
            let private_key = piece_source(
                &domain,
                "private key",
                private_key_file_path,
                private_key_secret,
                secrets,
                Encoding::Text,
            );
            match (certificate, private_key) {
                (Ok(certificate), Ok(private_key)) => {
                    ResolvedClientCertificate::pem(domain, certificate, private_key, passphrase)
                }
                (Err(reason), _) | (_, Err(reason)) => {
                    ResolvedClientCertificate::unavailable(domain, reason)
                }
            }
        }
        ClientCertificate::Pkcs12 {
            domain,
            pkcs12_file_path,
            pkcs12_secret,
            passphrase,
        } => {
            match piece_source(
                &domain,
                "PKCS12 bundle",
                pkcs12_file_path,
                pkcs12_secret,
                secrets,
                Encoding::Base64,
            ) {
                Ok(bundle) => ResolvedClientCertificate::pkcs12(domain, bundle, passphrase),
                Err(reason) => ResolvedClientCertificate::unavailable(domain, reason),
            }
        }
    }
}

/// The source of one piece: its file, or the bytes of its secret. Exactly one must be set.
/// The error is the user-facing reason, and never contains a secret value.
fn piece_source(
    domain: &str,
    piece: &str,
    file_path: String,
    reference: Option<String>,
    secrets: &HashMap<String, String>,
    encoding: Encoding,
) -> Result<CertificateSource, String> {
    let reference = reference.filter(|r| !r.is_empty());
    match (file_path.is_empty(), reference) {
        (false, None) => Ok(CertificateSource::File(file_path)),
        (true, Some(reference)) => {
            inline_from_secret(&reference, secrets, encoding).map(CertificateSource::Inline)
        }
        (true, None) => Err(format!(
            "Client certificate for {domain} has neither a file path nor a secret reference for its {piece}."
        )),
        (false, Some(reference)) => Err(format!(
            "Client certificate for {domain} has both a file path and the secret reference {reference} for its {piece}. Use only one."
        )),
    }
}

fn inline_from_secret(
    reference: &str,
    secrets: &HashMap<String, String>,
    encoding: Encoding,
) -> Result<Zeroizing<Vec<u8>>, String> {
    let Some(value) = secrets.get(reference) else {
        return Err(format!(
            "Client certificate secret {reference} was not found. \
             Check the External Secrets binding and fetch the secret names."
        ));
    };
    if value.trim().is_empty() {
        return Err(format!("Client certificate secret {reference} is empty."));
    }
    match encoding {
        Encoding::Text => Ok(Zeroizing::new(value.as_bytes().to_vec())),
        Encoding::Base64 => {
            let compact: Zeroizing<String> =
                Zeroizing::new(value.chars().filter(|c| !c.is_whitespace()).collect());
            STANDARD
                .decode(compact.as_bytes())
                .map(Zeroizing::new)
                // The decoder's message can quote input, so it is dropped.
                .map_err(|_| format!("Client certificate secret {reference} is not valid base64."))
        }
    }
}
```

If Plan B left tests or helpers in this file that the code above removed, restore them.

- [ ] **Step 6: Update the caller in `execution_service.rs`**

Find the function with `grep -n "fn environment_client_certificates" crates/rocket-app/src/execution_service.rs` (it is private, around line 564, Plan B1 changed its return type). Replace the whole function, from its doc comment to its closing brace, with:

```rust
    /// Returns the selected environment's client certificates, ready for the executor:
    /// `{{placeholders}}` resolved, relative paths joined onto the collection folder, and
    /// RocketVault references replaced by the fetched bytes. A missing environment means no
    /// certificates, like it means no variables.
    fn environment_client_certificates(
        &self,
        input: &ExecuteRequestInput,
        vars: &std::collections::HashMap<String, String>,
        external_secrets: &std::collections::HashMap<String, String>,
    ) -> Vec<rocket_http::ResolvedClientCertificate> {
        // Relative file paths are relative to the collection folder, so they work for a
        // collection that is shared through git.
        let base = input
            .collection
            .as_deref()
            .and_then(|c| self.collection_env_repo_factory.as_ref()?.collection_dir(c));
        let repo = self.regular_env_repo(input.collection.as_deref());
        crate::client_certificates::environment_client_certificates(
            repo.as_ref(),
            input.environment_name.as_deref(),
            base.as_deref(),
            vars,
            external_secrets,
        )
    }
```

Then in `resolve_request` replace:

```rust
        options.client_certificates = self.environment_client_certificates(input, &vars);
```

with:

```rust
        options.client_certificates =
            self.environment_client_certificates(input, &vars, external_secrets);
```

- [ ] **Step 7: Update the callers in `oauth2_service.rs`**

Replace the private `client_certificates` function (lines ~153-181, doc comment included) with:

```rust
    /// The client certificates of the named environment, with placeholders, relative paths and
    /// RocketVault references resolved the same way request execution does.
    fn client_certificates(
        &self,
        collection: Option<&str>,
        environment_name: Option<&str>,
        vars: &HashMap<String, String>,
        external_secrets: &HashMap<String, String>,
    ) -> Vec<rocket_http::ResolvedClientCertificate> {
        let factory = self.collection_env_repo_factory.as_ref();
        let base = collection.and_then(|c| factory?.collection_dir(c));
        match (factory, collection) {
            (Some(f), Some(col)) => {
                let repo = f.for_collection(col);
                crate::client_certificates::environment_client_certificates(
                    repo.as_ref(),
                    environment_name,
                    base.as_deref(),
                    vars,
                    external_secrets,
                )
            }
            _ => crate::client_certificates::environment_client_certificates(
                self.env_repo.as_ref(),
                environment_name,
                base.as_deref(),
                vars,
                external_secrets,
            ),
        }
    }
```

In `refresh_token_with_secrets` replace:

```rust
        let certificates = self.client_certificates(
            req.collection.as_deref(),
            req.environment_name.as_deref(),
            &vars,
        );
```

with:

```rust
        let certificates = self.client_certificates(
            req.collection.as_deref(),
            req.environment_name.as_deref(),
            &vars,
            external_secrets,
        );
```

In `resolve_get_token_request_with_secrets` replace:

```rust
            client_certificates: self.client_certificates(
                req.collection.as_deref(),
                req.environment_name.as_deref(),
                &vars,
            ),
```

with:

```rust
            client_certificates: self.client_certificates(
                req.collection.as_deref(),
                req.environment_name.as_deref(),
                &vars,
                external_secrets,
            ),
```

If Plan B1 already wrote the return type as an imported `ResolvedClientCertificate`, keep its form. If `ClientCertificate` is now unused at the top of the file (outside tests), remove only that import, and only if `cargo check` reports it.

- [ ] **Step 8: Run the tests and expect them to pass**

Run: `cargo test -j4 -p rocket-app vault_certificates` then `cargo test -j4 -p rocket-app client_certificate` then `cargo test -j4 -p rocket-app oauth2_service` then `cargo test -j4 -p rocket-infra unavailable_certificate`
Expected: all PASS, including the tests B1 migrated (relative paths, placeholders, `no_environment_means_no_certificates`). The two load-test callers keep passing an empty secrets map, so a vault-backed certificate is `Unavailable` there (see "Scope boundary" in Next Plan).

- [ ] **Step 9: Clippy and workspace check**

Run: `cargo clippy -j4 -p rocket-app --all-targets` then `cargo clippy -j4 -p rocket-infra --all-targets` then `cargo check -j4 --workspace`
Expected: no warnings, no errors. Also run `cargo fmt --check -p rocket-app -p rocket-infra`; if it reports a diff in a file you did not touch, leave that file alone.

- [ ] **Step 10: Commit**

Run `git status --short` and `git diff Cargo.lock` first: `Cargo.lock` must only add `zeroize` to `rocket-app`, and only the listed paths may be staged. Invoke the `dev-workflow-skills:1-git-commit` skill. Then:

```bash
git add Cargo.lock crates/rocket-app/Cargo.toml crates/rocket-app/src/client_certificates.rs crates/rocket-app/src/execution_service.rs crates/rocket-app/src/oauth2_service.rs crates/rocket-infra/src/reqwest_executor.rs
git commit -- Cargo.lock crates/rocket-app/Cargo.toml crates/rocket-app/src/client_certificates.rs crates/rocket-app/src/execution_service.rs crates/rocket-app/src/oauth2_service.rs crates/rocket-infra/src/reqwest_executor.rs
```

Suggested subject: `feat(certs): resolve vault references into inline certificate material`. Body: one short paragraph (references resolve for requests and OAuth2, an unresolved entry fails only when selected, base64 PKCS12 ignores whitespace). The message ends with `Relates to: #21`.

---

## Task C2: Security pass (redaction, size cap, leak tests)

**Model:** sonnet builds. **An opus reviewer reviews the diff before the commit** (Step 12). The implementer hands the reviewer the output of `git diff -- <the paths below>` plus the checklist in Step 12, and does not commit until the reviewer approves or the requested changes are made.

Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Precondition:** `crates/rocket-app/src/redaction.rs` had uncommitted formatting-only edits from another session when this plan was written. Run `git diff --stat -- crates/rocket-app/src/redaction.rs`. If it shows hunks you did not make, stop and ask the orchestrator to commit or discard them first, so this commit does not carry someone else's changes. Task C1 must be committed before starting.

**Files:**
- Modify: `crates/rocket-app/src/redaction.rs` (new `redaction_forms`, after `redact_secrets` ending at line 23, and tests)
- Modify: `crates/rocket-app/src/execution_service.rs` (lines 435-440 in `build_variable_scopes`; new test module next to the C1 module)
- Modify: `crates/rocket-app/src/client_certificates.rs` (size cap in `inline_from_secret`)
- Modify: `crates/rocket-app/src/flow_debug.rs` (one test, in the `tests` module that starts at line 192)
- Modify: `crates/rocket-http/src/request.rs` (tests, module at line 70-81)
- Modify: `crates/rocket-infra/src/scripting/engine.rs` (one test, near `console_log_redacts_value_copied_to_different_scope_key`, ~line 513)

**Interfaces:**
- Consumes: Task C1 (`inline_from_secret`, the `vault_certificates` test helpers), `ResolvedClientCertificate`, `CertificateSource`, `CertificateMaterial` (contract), `RequestExecutionService::secret_values(global_env_name, collection, environment_name, external_secrets)` (exists, `pub(crate)`).
- Produces: `pub(crate) fn redaction_forms(value: &str) -> Vec<String>` in `rocket-app/src/redaction.rs`, `pub(crate) const MAX_INLINE_SECRET_BYTES: usize = 1024 * 1024` in `client_certificates.rs`.

### Findings from reading the code (what each part verifies)

**(a) Redaction.** Every external secret value already enters `secret_values` whole (`build_variable_scopes`, lines 435-440), so a multi-line PEM is masked when the whole text appears, in `redact_secrets` (used by history, flow debug and run output) and in the script `redact()`. A single line of the PEM is **not** masked today. A script, a server echo or an error can print one line, or the PEM with other line endings, and it would show. So Step 2 writes a failing test and Step 3 adds `redaction_forms`, which adds the whole value, its trimmed form, and for multi-line values each line except the `-----BEGIN/END` armor lines (those are not secret and masking them would hide useful log text). `MIN_REDACTION_LEN` (6) still applies to every form. The script layer needs no code change: it masks every member of the set.

**(c) Places that could serialize or log an `HttpRequest`, `RequestOptions` or certificates** (searched with `grep` over `crates/` and `src-tauri/src/` for `serde_json::to_*`, `{:?}`, `tracing::`, `instrument`, `HistoryEntry`, `DomainEvent`, `AuditEventKind`):

| Site | What it handles | Status | Pinned by |
|---|---|---|---|
| `RequestOptions` / `HttpRequest` serde | IPC input and any JSON dump | `client_certificates` is `#[serde(skip)]` (Plan B1) | Step 7 tests in `request.rs` |
| `Debug` of `ResolvedClientCertificate`, `RequestOptions`, `HttpRequest` | `{:?}` in logs or panics | hand-written, redacting | Step 7 tests, Step 2 end-to-end test |
| `execute_with_external_secrets` `#[tracing::instrument(skip(self, input, external_secrets), fields(method, url))]` | span fields | only method and URL | note, no key material can reach it |
| `send_request` `tracing::info!` | status, duration, size | no request data | note |
| `execute_capturing` `sent: Option<HttpRequest>` then `flow_debug::build_debug_request` | Flow step debug record | reads method, url, query, headers, body, auth only; never `options`; `FlowDebugRequest` has no options field | Step 8 test in `flow_debug.rs` |
| History (`HistoryEntry::new(method, redacted_url, status, ...)`) | persisted | built from method, URL, status, sizes only; `rocket-history` has no `HttpRequest` | note |
| `DomainEvent::RequestExecuted { method, url, status, duration_ms }` | event bus | no options | note |
| Security audit (`AuditEventKind::SensitiveAuthUsed { auth_type, collection, request_path }`) | audit trail | label only; `env_audit` compares variables | note |
| Script ops (`req.rs`) | scripts reading the request | individual getters (headers, url, tags); no whole-request JSON | note |
| `export_service` `serde_json::to_string(&result.request_log)` | load-test log | `RequestLogEntry`, not `HttpRequest` | note |
| `src-tauri` `tauri_tracing_layer` `format!("{:?}", value)` | tracing field values | only the fields above reach it | note |
| Persisted `ClientCertificate` `Debug` (`rocket-shared`, hand-written) | `{:?}` of an environment | prints paths and references, `<redacted>` for the passphrase; Plan B2 must keep it that way for the new fields | reviewer checklist item 8 |

If a search in Step 1 finds a site not in this table, add a test or a note for it before going on.

- [ ] **Step 1: Re-run the audit searches and confirm the table**

Run:

```bash
grep -rn "tracing::\(info\|debug\|warn\|error\|trace\)!\|#\[tracing::instrument" crates/rocket-app/src/execution_service.rs crates/rocket-app/src/oauth2_service.rs crates/rocket-infra/src/reqwest_executor.rs
grep -rn "to_string(&\|to_value(&\|to_vec(&" crates/rocket-app/src crates/rocket-http/src src-tauri/src --include='*.rs' | grep -i "request\|options\|cert"
grep -rn "HttpRequest" src-tauri/src crates/rocket-history/src crates/rocket-shared/src --include='*.rs'
```

Expected: only the sites in the table (spans with `method`/`url`, `send_request` status logs, `export_service` `request_log`, no `HttpRequest` in `src-tauri`, `rocket-history`, `rocket-shared`). A new hit means the table is stale: stop and add a pin for it.

- [ ] **Step 2: Write the failing tests for redaction, the size cap and the end-to-end check**

(2a) In `crates/rocket-app/src/redaction.rs`, append these tests inside the existing `tests` module (before its final `}`):

```rust
    const PEM: &str = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0B\nAQEFAASCBKcwggSjAgEAAoIB\n-----END PRIVATE KEY-----\n";

    fn forms(value: &str) -> HashSet<String> {
        redaction_forms(value).into_iter().collect()
    }

    #[test]
    fn a_single_line_value_has_one_form() {
        assert_eq!(redaction_forms("sk-live-abcdef123"), vec!["sk-live-abcdef123"]);
        assert!(redaction_forms("abc").is_empty(), "below the floor");
    }

    #[test]
    fn a_multi_line_value_is_masked_whole_and_line_by_line() {
        let set = forms(PEM);
        // Whole: nothing is left behind.
        assert_eq!(redact_secrets(PEM, &set), REDACTED);
        // One line on its own, for example from a script or an echoing server.
        assert_eq!(
            redact_secrets("leaked: AQEFAASCBKcwggSjAgEAAoIB", &set),
            format!("leaked: {REDACTED}")
        );
        // The trimmed form, without the final newline.
        assert_eq!(redact_secrets(PEM.trim(), &set), REDACTED);
    }

    #[test]
    fn crlf_values_are_masked_line_by_line_too() {
        let crlf = PEM.replace('\n', "\r\n");
        let set = forms(&crlf);
        assert_eq!(redact_secrets(&crlf, &set), REDACTED);
        assert_eq!(
            redact_secrets("x MIIEvQIBADANBgkqhkiG9w0B x", &set),
            format!("x {REDACTED} x")
        );
        // Also the same PEM printed with LF only.
        assert_eq!(
            redact_secrets("a\nMIIEvQIBADANBgkqhkiG9w0B\nb", &set),
            format!("a\n{REDACTED}\nb")
        );
    }

    #[test]
    fn pem_armor_lines_and_short_lines_are_not_secret() {
        let set = forms("-----BEGIN CERTIFICATE-----\nabc\n1234567890\n-----END CERTIFICATE-----\n");
        assert!(!set.iter().any(|f| f.starts_with("-----")));
        assert!(!set.contains("abc"));
        assert!(set.contains("1234567890"));
        assert_eq!(
            redact_secrets("-----BEGIN CERTIFICATE-----", &set),
            "-----BEGIN CERTIFICATE-----"
        );
    }
```

(2b) In `crates/rocket-app/src/execution_service.rs`, insert this module directly after the C1 `mod vault_certificates { ... }` block and before the test `resolve_request_resolves_placeholders_in_inherited_collection_auth` (same anchor as in C1):

```rust
    /// Hygiene tests for vault-backed certificate material (Plan C, Task C2).
    mod vault_certificate_hygiene {
        use super::vault_certificates::{
            resolve_certificates, secrets, source_bytes, unavailable_reason, vault_p12,
            vault_pem, CERT_PEM, KEY_PEM,
        };
        use super::*;
        use rocket_http::CertificateMaterial;

        #[tokio::test]
        async fn vault_certificate_hygiene_multi_line_values_are_masked_whole_and_by_line() {
            let svc = service_with(Environment::new("dev"), None);
            let values = svc.secret_values(
                None,
                None,
                Some("dev"),
                &secrets(&[("vault.keyPem", KEY_PEM)]),
            );
            assert!(values.contains(KEY_PEM), "the whole value");
            assert!(values.contains("MIIEkeybody0123"), "the body line");
            assert!(
                !values.iter().any(|v| v.starts_with("-----")),
                "armor lines are not secret"
            );
            assert_eq!(
                crate::redaction::redact_secrets("log: MIIEkeybody0123", &values),
                "log: ••••••"
            );
            assert_eq!(crate::redaction::redact_secrets(KEY_PEM, &values), "••••••");
        }

        #[tokio::test]
        async fn vault_certificate_hygiene_short_secret_is_still_skipped() {
            let svc = service_with(Environment::new("dev"), None);
            let values = svc.secret_values(
                None,
                None,
                Some("dev"),
                &secrets(&[("vault.short", "abc")]),
            );
            assert!(values.is_empty());
        }

        #[tokio::test]
        async fn vault_certificate_hygiene_inline_material_over_the_cap_is_unavailable() {
            let cap = crate::client_certificates::MAX_INLINE_SECRET_BYTES;
            assert_eq!(cap, 1024 * 1024);

            let too_big = "A".repeat(cap + 1);
            let certs = resolve_certificates(
                vec![vault_p12("a.example.com", "vault.bundle")],
                &secrets(&[("vault.bundle", too_big.as_str())]),
            );
            assert_eq!(
                unavailable_reason(&certs[0]),
                "Client certificate secret vault.bundle is larger than 1 MiB. \
                 Check that it holds a certificate and not another file."
            );

            // Exactly at the cap is accepted (base64 of zeros, so it decodes).
            let at_cap = "A".repeat(cap);
            let certs = resolve_certificates(
                vec![vault_p12("a.example.com", "vault.bundle")],
                &secrets(&[("vault.bundle", at_cap.as_str())]),
            );
            let CertificateMaterial::Pkcs12 { bundle, .. } = &certs[0].material else {
                panic!("a secret at the cap must resolve");
            };
            assert_eq!(source_bytes(bundle).len(), cap / 4 * 3);
        }

        #[tokio::test]
        async fn vault_certificate_hygiene_resolve_request_keeps_key_text_out_of_debug_and_json() {
            let mut env = Environment::new("dev");
            env.client_certificates = vec![vault_pem(
                "api.example.com",
                "vault.certPem",
                "vault.keyPem",
                Some("{{vault.keyPass}}"),
            )];
            let svc = service_with(env, None);
            let request = svc
                .resolve_request(
                    &sample_input("https://api.example.com/x", Some("dev")),
                    &secrets(&[
                        ("vault.certPem", CERT_PEM),
                        ("vault.keyPem", KEY_PEM),
                        ("vault.keyPass", "p4ss-word"),
                    ]),
                )
                .expect("resolve_request");

            // The resolved request carries inline bytes.
            let CertificateMaterial::Pem {
                certificate,
                private_key,
                ..
            } = &request.options.client_certificates[0].material
            else {
                panic!("expected a PEM certificate");
            };
            assert_eq!(source_bytes(certificate), CERT_PEM.as_bytes());
            assert_eq!(source_bytes(private_key), KEY_PEM.as_bytes());

            // Nothing printable or serializable contains the PEM, its body or the passphrase.
            let needles = [
                "MIIEkeybody0123",
                "MIIBcertbody0123",
                "BEGIN PRIVATE KEY",
                "BEGIN CERTIFICATE",
                "p4ss-word",
            ];
            let json = serde_json::to_string(&request).expect("serialize");
            for shown in [format!("{request:?}"), format!("{request:#?}"), json.clone()] {
                for needle in needles {
                    assert!(!shown.contains(needle), "{needle} leaked into {shown}");
                }
            }
            assert!(!json.contains("clientCertificates"), "{json}");
        }
    }

```

(2c) In `crates/rocket-infra/src/scripting/engine.rs`, insert this test directly before this existing test (unique anchor):

```rust
    #[tokio::test]
    async fn console_log_redacts_value_copied_to_different_scope_key() {
```

Code to insert:

```rust
    #[tokio::test]
    async fn console_log_redacts_a_single_line_of_a_multi_line_vault_value() {
        let pem = "-----BEGIN PRIVATE KEY-----\nMIIEvQIBADANBgkqhkiG9w0B\nAQEFAASCBKcwggSjAgEAAoIB\n-----END PRIVATE KEY-----\n";
        let engine = DenoScriptEngine::new();
        let mut vars = VariableContext::default();
        vars.external_secrets.insert("vault.key".into(), pem.into());
        // The app layer adds the whole value and each body line (`redaction_forms`).
        // The script layer masks every member of the set on its own.
        vars.secret_values.insert(pem.into());
        vars.secret_values.insert("MIIEvQIBADANBgkqhkiG9w0B".into());
        vars.secret_values.insert("AQEFAASCBKcwggSjAgEAAoIB".into());
        let mut ctx = minimal_ctx(
            "const pem = rok.getSecretVar('vault.key'); \
             console.log(pem); \
             console.log(pem.split('\\n')[1]); \
             console.log('line2=' + pem.split('\\n')[2]);",
        );
        ctx.variables = vars;
        let result = engine.execute(ctx).await.expect("execute");
        assert_eq!(result.console_entries.len(), 3);
        assert_eq!(result.console_entries[0].message, "••••••");
        assert_eq!(result.console_entries[1].message, "••••••");
        assert_eq!(result.console_entries[2].message, "line2=••••••");
    }

```

Run: `cargo test -j4 -p rocket-app redaction` then `cargo test -j4 -p rocket-app vault_certificate_hygiene` then `cargo test -j4 -p rocket-infra console_log_redacts_a_single_line`
Expected: the `rocket-app` tests FAIL to compile with `cannot find function redaction_forms` and `no MAX_INLINE_SECRET_BYTES in client_certificates`. The engine test PASSES already (it pins that the script layer masks each member of the set), which is the evidence that no script-layer change is needed.

- [ ] **Step 3: Add `redaction_forms` and use it for external secrets**

In `crates/rocket-app/src/redaction.rs`, add directly after `redact_secrets` (after its closing brace, line 23):

```rust
/// Every form of a secret value that must be masked.
///
/// The whole value, its trimmed form, and, for a multi-line value such as a PEM, each line on its
/// own, so one printed line is masked too. `-----BEGIN` and `-----END` armor lines are not secret
/// and are skipped. Forms shorter than `MIN_REDACTION_LEN` are left out, like in
/// `redact_secrets`.
pub(crate) fn redaction_forms(value: &str) -> Vec<String> {
    let mut forms: Vec<String> = Vec::new();
    let mut add = |s: &str| {
        if s.len() >= MIN_REDACTION_LEN && !forms.iter().any(|f| f == s) {
            forms.push(s.to_string());
        }
    };
    add(value);
    add(value.trim());
    if value.contains('\n') || value.contains('\r') {
        for line in value.lines() {
            let line = line.trim();
            if !line.starts_with("-----") {
                add(line);
            }
        }
    }
    forms
}
```

In `crates/rocket-app/src/execution_service.rs` (`build_variable_scopes`, lines 435-440) replace:

```rust
        ctx.external_secrets = external_secrets.clone();
        for value in external_secrets.values() {
            if value.len() >= MIN_REDACTION_LEN {
                ctx.secret_values.insert(value.clone());
            }
        }
```

with:

```rust
        ctx.external_secrets = external_secrets.clone();
        for value in external_secrets.values() {
            ctx.secret_values
                .extend(crate::redaction::redaction_forms(value));
        }
```

(`MIN_REDACTION_LEN` is still used by the lines above it, so the import stays.)

- [ ] **Step 4: Add the 1 MiB cap**

In `crates/rocket-app/src/client_certificates.rs` add below the `use` lines:

```rust
/// The largest vault secret accepted as certificate material. A PKCS12 bundle as base64 is tens
/// of KiB, so this only stops a wrong secret (a large file, a dump) from being copied around.
pub(crate) const MAX_INLINE_SECRET_BYTES: usize = 1024 * 1024;
```

and in `inline_from_secret`, directly after the `let Some(value) = secrets.get(reference) else { ... };` block and before the `value.trim().is_empty()` check, insert:

```rust
    if value.len() > MAX_INLINE_SECRET_BYTES {
        return Err(format!(
            "Client certificate secret {reference} is larger than 1 MiB. \
             Check that it holds a certificate and not another file."
        ));
    }
```

- [ ] **Step 5: Run the tests and expect them to pass**

Run: `cargo test -j4 -p rocket-app redaction` then `cargo test -j4 -p rocket-app vault_certificate` then `cargo test -j4 -p rocket-app short_secret_value_is_not_added_to_secret_values` then `cargo test -j4 -p rocket-app secret_env_and_collection_vars_populate_secret_values`
Expected: PASS. The last two are existing tests that prove the floor and the plain-variable behavior are unchanged.

- [ ] **Step 6: Commit-free checkpoint for (a) and (b)**

Run `git diff --stat` and confirm only the listed paths changed. Do not commit yet: the reviewer in Step 12 sees everything at once.

- [ ] **Step 7: Leak tests for `Debug` and serde in `rocket-http`**

In `crates/rocket-http/src/request.rs`, inside the existing `tests` module (line 70), add this nested module after the `default_options` test (anchor: the `assert!(req.options.verify_ssl);` line plus the closing braces):

```rust
    mod certificate_leaks {
        use super::*;
        use crate::{CertificateSource, ResolvedClientCertificate};
        use zeroize::Zeroizing;

        const KEY_TEXT: &str =
            "-----BEGIN PRIVATE KEY-----\nMIIEleakcheckbody\n-----END PRIVATE KEY-----\n";

        /// Things that appear in the output if bytes or passphrases are printed. The decimal lists
        /// are what a derived `Debug` of `Vec<u8>` prints for `cert-leak-check-body` and `SUPER`.
        const NEEDLES: [&str; 7] = [
            "MIIEleakcheckbody",
            "cert-leak-check-body",
            "BEGIN PRIVATE KEY",
            "hunter2-passphrase",
            "p12-passphrase",
            "99, 101, 114",
            "83, 85, 80",
        ];

        fn request_with_certificates() -> HttpRequest {
            let mut req = HttpRequest::new(HttpMethod::Get, "https://api.example.com");
            req.options.client_certificates = vec![
                ResolvedClientCertificate::pem(
                    "api.example.com",
                    CertificateSource::Inline(Zeroizing::new(b"cert-leak-check-body".to_vec())),
                    CertificateSource::Inline(Zeroizing::new(KEY_TEXT.as_bytes().to_vec())),
                    Some("hunter2-passphrase".into()),
                ),
                ResolvedClientCertificate::pkcs12(
                    "b.example.com",
                    CertificateSource::Inline(Zeroizing::new(vec![0x53, 0x55, 0x50, 0x45, 0x52])),
                    Some("p12-passphrase".into()),
                ),
            ];
            req
        }

        fn assert_clean(shown: &str) {
            for needle in NEEDLES {
                assert!(!shown.contains(needle), "{needle} leaked into {shown}");
            }
        }

        #[test]
        fn debug_of_a_resolved_certificate_never_prints_bytes_or_passphrases() {
            for cert in request_with_certificates().options.client_certificates {
                assert_clean(&format!("{cert:?}"));
                assert_clean(&format!("{cert:#?}"));
            }
            let shown = format!("{:?}", request_with_certificates().options.client_certificates);
            assert!(shown.contains("inline 20 bytes"), "{shown}");
            assert!(shown.contains("<redacted>"), "{shown}");
        }

        #[test]
        fn debug_of_request_options_and_http_request_never_prints_key_material() {
            let req = request_with_certificates();
            for shown in [
                format!("{:?}", req.options),
                format!("{:#?}", req.options),
                format!("{req:?}"),
                format!("{req:#?}"),
            ] {
                assert_clean(&shown);
            }
        }

        #[test]
        fn serializing_options_or_a_request_never_contains_certificates() {
            let req = request_with_certificates();
            for json in [
                serde_json::to_string(&req.options).expect("options"),
                serde_json::to_string(&req).expect("request"),
            ] {
                assert_clean(&json);
                assert!(!json.contains("clientCertificates"), "{json}");
            }
        }

        #[test]
        fn certificates_in_ipc_input_are_ignored() {
            let options: RequestOptions = serde_json::from_str(
                r#"{"clientCertificates":[{"type":"pkcs12","domain":"x","pkcs12FilePath":"/etc/shadow"}]}"#,
            )
            .expect("unknown keys are ignored");
            assert!(options.client_certificates.is_empty());
        }
    }
```

Run: `cargo test -j4 -p rocket-http certificate_leaks`
Expected: PASS (4 tests). These pin the Plan B1 guarantees (hand-written `Debug`, `#[serde(skip)]`). If `debug_of_a_resolved_certificate_...` fails on `inline 20 bytes`, compare with the contract's `inline <n> bytes` wording and fix the assertion to the real text, never the `Debug` impl.

- [ ] **Step 8: Pin the Flow debug record**

In `crates/rocket-app/src/flow_debug.rs`, inside the `tests` module (line 192), insert before this existing test (unique anchor):

```rust
    #[test]
    fn masks_secrets_everywhere_and_appends_enabled_query_params() {
```

code:

```rust
    #[test]
    fn the_debug_record_never_carries_client_certificate_material() {
        use rocket_http::{CertificateSource, ResolvedClientCertificate};
        let mut req = request(Auth::None);
        req.options.client_certificates = vec![ResolvedClientCertificate::pem(
            "api.example.com",
            CertificateSource::Inline(zeroize::Zeroizing::new(b"PEM-BODY-LEAK-CHECK".to_vec())),
            CertificateSource::Inline(zeroize::Zeroizing::new(b"KEY-BODY-LEAK-CHECK".to_vec())),
            Some("pass-LEAK-CHECK".into()),
        )];
        let record = build_debug_request(&req, None, None, &secrets(&[]));
        let json = serde_json::to_string(&record).expect("serialize");
        assert!(!json.contains("LEAK-CHECK"), "{json}");
        assert!(!json.contains("clientCertificates"), "{json}");
    }
```

Run: `cargo test -j4 -p rocket-app the_debug_record_never_carries`
Expected: PASS.

- [ ] **Step 9: Mutation check of the leak tests**

Temporarily change the `Debug` impl in `crates/rocket-http/src/resolved_certificate.rs` (Plan B1) to print the raw bytes (for example replace the `inline <n> bytes` arm with `format!("{bytes:?}")`), run `cargo test -j4 -p rocket-http certificate_leaks`, and confirm two tests FAIL (the two Debug tests; the serde tests are guarded by the compiler instead, because the type has no `Serialize`). Then revert the change with `git checkout -- crates/rocket-http/src/resolved_certificate.rs` (the file is committed by Plan B, so this restores it). A leak test that cannot fail is not a test.

- [ ] **Step 10: Clippy and workspace check**

Run: `cargo clippy -j4 -p rocket-app --all-targets` then `cargo clippy -j4 -p rocket-http --all-targets` then `cargo clippy -j4 -p rocket-infra --all-targets` then `cargo check -j4 --workspace`
Expected: no warnings, no errors.

- [ ] **Step 11: Run the wider affected tests once**

Run: `cargo test -j4 -p rocket-app execution_service` then `cargo test -j4 -p rocket-app flow_debug` then `cargo test -j4 -p rocket-http`
Expected: PASS. (No `--workspace`.)

- [ ] **Step 12: Hand the diff to the opus reviewer, then commit**

Give the opus reviewer `git diff -- crates/rocket-app/src/redaction.rs crates/rocket-app/src/execution_service.rs crates/rocket-app/src/client_certificates.rs crates/rocket-app/src/flow_debug.rs crates/rocket-http/src/request.rs crates/rocket-infra/src/scripting/engine.rs` and this checklist. The reviewer answers each item pass or fail with `file:line`, and an overall approve or changes-requested.

1. `redaction_forms` keeps the whole value and adds only trimmed and per-line forms; `MIN_REDACTION_LEN` is unchanged and applied to every form; armor lines are excluded; CRLF and LF both work.
2. `build_variable_scopes` still inserts nothing for a value under 6 characters, and plain (non-vault) secrets behave as before.
3. No error or `Unavailable` message contains a secret value, including the base64 path (the decoder's own message is dropped) and the size cap message (it names the reference only).
4. The size cap is checked before any copy or decode, and the boundary is inclusive (exactly 1 MiB passes).
5. Nothing new implements `Serialize`, `Clone` into a plain `String`, `Display` or `to_string` for key bytes or passphrases; no `String::from_utf8` of key bytes was added.
6. The leak tests would fail if `Debug` or serde regressed (the mutation check of Step 9 was really done), and no test prints a secret on success.
7. The audit table in this task still matches the code (no new site that serializes or logs `HttpRequest`, `RequestOptions` or certificates).
8. Plan B2's `Debug` for the persisted `ClientCertificate` (`crates/rocket-shared/src/certificate.rs`) prints the new reference fields, never a value, and still redacts the passphrase.
9. `crates/rocket-app/src/redaction.rs` contains only this task's hunks (no foreign formatting hunks), and the commit is path-scoped.

Fix every requested change, re-run Steps 5, 7, 8 and 10, then invoke the `dev-workflow-skills:1-git-commit` skill and:

```bash
git add crates/rocket-app/src/redaction.rs crates/rocket-app/src/execution_service.rs crates/rocket-app/src/client_certificates.rs crates/rocket-app/src/flow_debug.rs crates/rocket-http/src/request.rs crates/rocket-infra/src/scripting/engine.rs
git commit -- crates/rocket-app/src/redaction.rs crates/rocket-app/src/execution_service.rs crates/rocket-app/src/client_certificates.rs crates/rocket-app/src/flow_debug.rs crates/rocket-http/src/request.rs crates/rocket-infra/src/scripting/engine.rs
```

Suggested subject: `fix(certs): mask vault values line by line and pin certificate leak tests`. The message ends with `Relates to: #21`.

---

## Task C3: Documentation

**Model:** haiku drafts, the orchestrator reviews. Documentation only, no code, no cargo.

Before starting, read docs/superpowers/specs/opencollection-spec-reference.md.

**Files (anchors verified by grep on 2026-10-01):**
- Modify: `docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md` (line 11 note, lines 66-71 non-goal, line 369 end of section 4.8, line 371 `## 5. Interfaces`, line 409 end of section 6)
- Modify: `crates/rocket-infra/CLAUDE.md` (line 59, the `**OAuth2 client credentials.**` paragraph)
- Modify: `crates/rocket-app/CLAUDE.md` (exists, 85 lines; line 64, the `Variable resolution in` bullet)
- Modify: `crates/rocket-environment/CLAUDE.md` (line 38, the `client_certificates` bullet, and the module table row for `repository.rs`)
- Modify: `crates/rocket-http/CLAUDE.md` (module table row `token_client`, the `RequestOptions defaults` bullet at line 44, and the `**Client certificates (mTLS).**` paragraph)
- Modify: `docs/superpowers/specs/opencollection-spec-reference.md` (line 497, section 4, and the field-spelling block in section 11)

**Interfaces:** none (docs). The text below states what Plans B and C built; confirm each statement against the merged code when reviewing.

Use the Edit tool with the exact `old_string` shown. The blocks below use four backticks so the inner fences survive.

- [ ] **Step 1: External secrets spec, dated update note**

`docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md`, old:

```text
> 📖 Before starting implementation, read `docs/superpowers/specs/opencollection-spec-reference.md`.
```

new:

````text
> 📖 Before starting implementation, read `docs/superpowers/specs/opencollection-spec-reference.md`.

> **Update 2026-10-01.** The scope of this integration now also covers client certificate
> material and OAuth2 token requests. An environment's client certificate entry can name a
> RocketVault secret (`alias.secretName`) for its certificate, private key or base64 PKCS12
> bundle, and OAuth2 token requests resolve `{{alias.secretName}}` references. See section 4.9
> and the [environment client certificates design](2026-10-01-environment-client-certificates-design.md).
> Text below that says otherwise is marked superseded.
````

- [ ] **Step 2: External secrets spec, rewrite the stale OAuth2 non-goal**

Same file, old (lines 66-71):

```text
- Not resolving external secrets inside `OAuth2Service`'s own variable
  interpolation (`crates/rocket-app/src/oauth2_service.rs`, which duplicates
  `build_variable_context`, per its own comment). Using an external-secret
  reference as part of an OAuth2 token-request field is a reasonable follow-up,
  explicitly deferred here to keep the resolution-path change scoped to
  `RequestExecutionService`/`CollectionRunnerService`.
```

new:

````text
- ~~Not resolving external secrets inside `OAuth2Service`'s own variable
  interpolation.~~ **Superseded 2026-10-01 (commit `5666e771`).** `OAuth2Service` now takes
  the resolved values (`resolve_get_token_request_with_secrets`,
  `refresh_token_with_secrets`), and the OAuth2 Tauri commands fetch them first with
  `RequestExecutionService::resolve_external_secrets`. So `{{alias.secretName}}` works in
  token-request fields and in the client certificate passphrase. Client certificate material
  is also in scope, see section 4.9.
````

- [ ] **Step 3: External secrets spec, redaction update in section 4.8**

Same file, old (end of section 4.8):

```text
`MIN_REDACTION_LEN` (6-char) threshold applies.
```

new:

````text
`MIN_REDACTION_LEN` (6-char) threshold applies.

**Update 2026-10-01.** "No new redaction code" is superseded for multi-line values.
`redaction_forms()` (`crates/rocket-app/src/redaction.rs`) now adds, for every external secret
value, the whole value, its trimmed form and, for a multi-line value such as a PEM, each line on
its own (lines shorter than `MIN_REDACTION_LEN` and `-----BEGIN`/`-----END` armor lines are
skipped). `build_variable_scopes` uses it, so history, flow debug records and run output mask a
single printed line of a PEM, not only the whole text. Script console output is masked by
`redact()` in `crates/rocket-infra/src/scripting/ops/mod.rs` (`ops/console.rs` calls it), which
replaces every member of the set and needs no change. Section 4.6's remark that
`ops/console.rs` needs no changes still holds.
````

- [ ] **Step 4: External secrets spec, new section 4.9 and a security bullet**

Same file, old:

```text
## 5. Interfaces (for the implementation plan)
```

new:

````text
### 4.9 Client certificate material (added 2026-10-01)

An environment's `clientCertificates` entry may take each piece of material from a RocketVault
secret instead of a file: `certificateSecret` and `privateKeySecret` (PEM text) for a `pem`
entry, `pkcs12Secret` (the base64 text of the DER bundle) for a `pkcs12` entry. The value is a
reference, `alias.secretName`, validated on save against the environment's bindings. It is never
a value.

- **Resolution.** `crates/rocket-app/src/client_certificates.rs` looks each reference up in the
  map from `resolve_external_secrets`, for request execution and OAuth2 alike. A found value
  becomes inline bytes (`Zeroizing`). PKCS12 is base64 decoded and ignores whitespace, so a
  wrapped secret works. A missing, empty, undecodable or over-1-MiB value makes the entry
  `Unavailable`.
- **Fail only when selected.** An `Unavailable` entry is an error only when it is the certificate
  chosen for the URL, and there is no fallback to another source. An unresolved reference on
  another domain does not affect the request.
- **Never persisted or shown.** Values are not written to the environment file, history, logs,
  events or the audit trail, and the resolved type is not `Serialize`. Its `Debug` prints sizes
  and the source kind only. Error messages name the reference, never the value.
- **Known limit.** Load tests call `resolve_request` with an empty secrets map, so a vault-backed
  certificate is `Unavailable` there.

## 5. Interfaces (for the implementation plan)
````

Same file, old (end of section 6):

```text
worse than failing loudly.
```

new:

````text
worse than failing loudly.
- Client certificate material fetched from RocketVault follows the same rules (section 4.9):
  never persisted, held as `Zeroizing` bytes, absent from `Debug` and serialization output.
````

- [ ] **Step 5: `crates/rocket-infra/CLAUDE.md` and `crates/rocket-app/CLAUDE.md`**

`crates/rocket-infra/CLAUDE.md`, old:

```text
**OAuth2 client credentials.** `ReqwestExecutor` fetches tokens synchronously as part of `execute()`. Other OAuth2 flows (authorization code, implicit) are not implemented and are silently skipped.
```

new:

````text
**OAuth2 client credentials.** `ReqwestExecutor` fetches tokens synchronously as part of `execute()`. Other OAuth2 flows (authorization code, implicit) are not implemented and are silently skipped.

**Client certificates (mTLS).** `ReqwestExecutor` and `ReqwestTokenClientProvider` share `identity_for_url` and `load_identity`. They take `rocket_http::ResolvedClientCertificate`, not the persisted `ClientCertificate`. Only the entry chosen for the URL (first domain match) is loaded. `CertificateSource::File` is read from disk, `CertificateSource::Inline` (bytes fetched from RocketVault) is used from memory, and both go through `pem_key::unencrypted_key_pem` (PEM) or `Identity::from_pkcs12_der` (PKCS12). `CertificateMaterial::Unavailable { reason }` (a reference that could not be resolved) is an `InvalidInput(reason)` only when that entry is the selected one, with no fallback to another entry. Inline bytes and passphrases are `Zeroizing` and never written to disk, logged or put in an error message. The real handshake test is `cargo test -j4 -p rocket-infra mutual_tls_handshake -- --ignored` (needs the `openssl` CLI).
````

`crates/rocket-app/CLAUDE.md`, old:

```text
- **Variable resolution in `RequestExecutionService::execute`.** Collection variables are loaded first; environment variables override them. The merged map is passed to `rocket_environment::resolve()` before the HTTP call.
```

new:

````text
- **Variable resolution in `RequestExecutionService::execute`.** Collection variables are loaded first; environment variables override them. The merged map is passed to `rocket_environment::resolve()` before the HTTP call.
- **Client certificates and RocketVault.** `client_certificates::environment_client_certificates(repo, environment_name, collection_dir, vars, external_secrets)` turns the environment's persisted `ClientCertificate` entries into `ResolvedClientCertificate`s. `RequestExecutionService::resolve_request` and `OAuth2Service` (`resolve_get_token_request_with_secrets`, `refresh_token_with_secrets`) both call it, so they resolve identically. Per entry: `{{placeholders}}` in the domain, file paths and passphrase, then relative paths joined onto the collection folder, then each `certificateSecret`, `privateKeySecret` and `pkcs12Secret` reference looked up in the RocketVault map (`alias.secretName`). A found reference becomes `CertificateSource::Inline` (PKCS12 secrets are base64 and ignore whitespace). A missing, empty, undecodable or over-1-MiB (`MAX_INLINE_SECRET_BYTES`) reference makes the entry `CertificateMaterial::Unavailable { reason }`, which the executor turns into an error only when that entry is selected for the URL. Load tests call `resolve_request` with an empty secrets map, so vault-backed certificates are `Unavailable` there. Multi-line vault values are masked whole and line by line (`redaction::redaction_forms`).
````

- [ ] **Step 6: `crates/rocket-environment/CLAUDE.md`**

Old (the `client_certificates` bullet, line 38):

```text
- **`client_certificates`** are consumed by the executor for mutual TLS: `rocket-app` copies them onto the request options and `rocket-infra` loads the matching one. Paths and passphrase may hold `{{placeholders}}`. A relative path is relative to the collection folder, which is the parent of `environments/`; `..` is not allowed, and absolute and `~/` paths are used as written.
```

new:

````text
- **`client_certificates`** are consumed by the executor for mutual TLS: `rocket-app` resolves them into `ResolvedClientCertificate`s (`rocket-http`) on the request options, and `rocket-infra` loads the matching one. Paths and passphrase may hold `{{placeholders}}`. A relative path is relative to the collection folder, which is the parent of `environments/`; `..` is not allowed, and absolute and `~/` paths are used as written. Each piece of material (certificate, private key, PKCS12 bundle) has exactly one source: a file path (`certificateFilePath`, `privateKeyFilePath`, `pkcs12FilePath`) or a RocketVault reference (`certificateSecret`, `privateKeySecret`, `pkcs12Secret`, always `alias.secretName`, never a value and never a `{{placeholder}}`). `validate_client_certificates(certs, bindings)` (`client_certificate_validation.rs`, called by `EnvironmentService` next to `validate_external_secret_bindings`) rejects a piece with no source or two sources, a reference whose alias or name is not in the environment's bindings, a path or reference field that starts with `-----BEGIN` (key text must never reach the environment file), and an empty domain. The three reference keys are Rocket extensions outside the OpenCollection schema. This crate never sees key bytes.
````

Old (table row):

```text
| `repository.rs` | `EnvironmentRepository` trait — `list`, `get`, `save`, `delete` returning `DomainResult<T>` |
```

new:

````text
| `repository.rs` | `EnvironmentRepository` trait — `list`, `get`, `save`, `delete` returning `DomainResult<T>` |
| `client_certificate_validation.rs` | `validate_client_certificates(certs, bindings)` — save-time rules for certificate entries (one source per piece, references match the environment's bindings, no key text in path or reference fields, non-empty domain) |
````

- [ ] **Step 7: `crates/rocket-http/CLAUDE.md`**

Old (start of the `token_client` row):

```text
| `token_client` | `TokenClientProvider` trait:
```

new:

````text
| `resolved_certificate` | `ResolvedClientCertificate { domain, material }`, the runtime form of a client certificate. `CertificateMaterial` is `Pem`, `Pkcs12` or `Unavailable { reason }`. Each piece is a `CertificateSource`: `File(path)` or `Inline(Zeroizing<Vec<u8>>)` (bytes from a RocketVault secret). Passphrases are `Zeroizing<String>`. Not `Serialize`. `Debug` prints the domain, the piece kinds, `file <path>` or `inline <n> bytes`, and `<redacted>` for a passphrase. |
| `token_client` | `TokenClientProvider` trait:
````

Old:

```text
- `RequestOptions` defaults: `follow_redirects = true`, `timeout_ms = 30_000`, `verify_ssl = true`. These are applied via `#[serde(default)]`, so missing fields in JSON deserialise correctly.
```

new:

````text
- `RequestOptions` defaults: `follow_redirects = true`, `timeout_ms = 30_000`, `verify_ssl = true`. These are applied via `#[serde(default)]`, so missing fields in JSON deserialise correctly.
- `RequestOptions.client_certificates` is `Vec<ResolvedClientCertificate>` with `#[serde(skip)]`: it is never serialized and never read from IPC input. The selected environment is its only source (`rocket-app` fills it). Because it can hold key bytes, nothing may log or persist an `HttpRequest` or `RequestOptions` through another type. The tests in `request.rs` (`certificate_leaks`) pin `Debug` and `serde_json` output.
````

Old (start of the mTLS paragraph, up to the first comma after `load_identity`):

```text
`RequestExecutionService::resolve_request` copies the selected environment's `client_certificates` (placeholders resolved) onto `HttpRequest.options`. `ReqwestExecutor::execute` picks the first one whose domain matches the URL (`rocket-http` `client_cert`), loads it with `load_identity`,
```

new:

````text
`RequestExecutionService::resolve_request` copies the selected environment's client certificates onto `HttpRequest.options` as `ResolvedClientCertificate`s (placeholders resolved, relative paths joined, RocketVault references fetched into inline bytes by `rocket-app`). `ReqwestExecutor::execute` picks the first one whose domain matches the URL (`rocket-http` `client_cert`), loads it with `load_identity` (from a file or from inline bytes; an `Unavailable` entry is an error only when it is the one picked, with no fallback),
````

- [ ] **Step 8: OpenCollection spec reference, document the three extension keys**

`docs/superpowers/specs/opencollection-spec-reference.md`, old (start of line 497):

```text
Rocket resolves a relative certificate file path against the collection folder
```

new (insert the extension block before it, keep the original text after it):

`````text
**Rocket extension keys (not in the OpenCollection schema).** Each piece of client certificate material may come from a RocketVault secret instead of a file. The secret is named by reference (`alias.secretName`, the same key external-secret values use), never by value:

```yaml
# PEM type: use instead of certificateFilePath / privateKeyFilePath
certificateSecret: string     # reference to a secret holding the certificate PEM
privateKeySecret: string      # reference to a secret holding the private key PEM
# PKCS12 type: use instead of pkcs12FilePath
pkcs12Secret: string          # reference to a secret holding the base64 text of the DER bundle
```

Each piece has exactly one source: a non-empty file path or a reference. With a reference the file path field is empty and is not written. A value that starts with `-----BEGIN` in a path or reference field is rejected on save, so key text never reaches the environment file. References are validated on save against the environment's `externalSecrets` bindings. The schema requires the file paths, so another OpenCollection tool may reject or drop an entry that uses a reference, the same trade-off as `externalSecrets`. Fetched values are never persisted. An unresolved reference fails a request or token request only when that certificate is the one selected for the URL.

Rocket resolves a relative certificate file path against the collection folder
`````

Old (section 11, in the spelling block):

```text
clientCertificates   importPaths           protoFiles
```

new:

````text
clientCertificates   importPaths           protoFiles
certificateSecret    privateKeySecret      pkcs12Secret     (Rocket extensions)
````

- [ ] **Step 9: Verify the anchors landed**

Run:

```bash
grep -n "Update 2026-10-01\|### 4.9 Client certificate material\|Superseded 2026-10-01" docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md
grep -n "Client certificates (mTLS)" crates/rocket-infra/CLAUDE.md
grep -n "Client certificates and RocketVault" crates/rocket-app/CLAUDE.md
grep -n "client_certificate_validation\|certificateSecret" crates/rocket-environment/CLAUDE.md
grep -n "resolved_certificate\|certificate_leaks\|Unavailable" crates/rocket-http/CLAUDE.md
grep -n "Rocket extension keys\|Rocket extensions" docs/superpowers/specs/opencollection-spec-reference.md
git diff --stat
```

Expected: every grep prints at least one line (the vault spec prints three or more), and `git diff --stat` lists only the six files above. The orchestrator then reads the diff against the merged code (names of modules, test names, the `inline <n> bytes` wording) before the commit.

- [ ] **Step 10: Commit**

Invoke the `dev-workflow-skills:1-git-commit` skill, then:

```bash
git add docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md docs/superpowers/specs/opencollection-spec-reference.md crates/rocket-infra/CLAUDE.md crates/rocket-app/CLAUDE.md crates/rocket-environment/CLAUDE.md crates/rocket-http/CLAUDE.md
git commit -- docs/superpowers/specs/2026-09-22-rocketvault-external-secrets-spec.md docs/superpowers/specs/opencollection-spec-reference.md crates/rocket-infra/CLAUDE.md crates/rocket-app/CLAUDE.md crates/rocket-environment/CLAUDE.md crates/rocket-http/CLAUDE.md
```

Suggested subject: `docs(certs): document vault-backed certificate material and redaction`. The message ends with `Relates to: #21`.

---

## Self-review notes (spec coverage)

- Spec section 6 (resolution: placeholders and paths, references, fail only when selected, unreachable vault unchanged): Task C1. Section 11 messages: exact texts in C1 tests.
- Section 8 (never persisted, `Zeroizing`, names not values in errors, redaction of multi-line values): Task C2 (redaction, leak tests, audit table).
- Section 12 tests: resolution (found, missing, bad base64, placeholders, paths), redaction of multi-line values, `Debug` and serialization, missing reference only when selected (executor and app), OAuth2 capturing provider. The inline-material handshake test belongs to Plan B3.
- Section 14 risk 2 (size cap): C2 Step 4.
- Section 13 docs row: Task C3.
- Review Focus 1 and 3 are owned here with named tests.

## Next Plan

[03-plan-d-certificates-ui.md](03-plan-d-certificates-ui.md): the Certificates tab in the environment dialog (Plan D). It needs only the Plan B2 types and can run beside Plan C.

**Scope boundary to carry forward:** `run_load_test` and `LoadTestService::run` call `resolve_request` with an empty secrets map (the same boundary the RocketVault plan 06 set), so a load test against a host that needs a vault-backed certificate fails with the "secret ... was not found" message. Extending vault resolution to load tests is not part of Plans B to D.
