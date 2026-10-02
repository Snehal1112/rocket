//! Fetches the RocketVault certificate selected for a URL, right before a request or token
//! request is sent.
//!
//! Resolution leaves a `vault` entry as `CertificateMaterial::Deferred`, which holds names only.
//! Here the entry chosen for a URL, and only that one, is exported and replaced by inline
//! material, so an entry for another domain never causes a RocketVault call. A failed export
//! makes that entry `Unavailable` with the reason, and the executor then fails the request.
//! There is no fallback to another entry. The material is never cached and never leaves memory.
//!
//! A PKCS12 bundle is not opened here. RocketVault exports it with `legacy` compat, which may
//! use RC2, and an OpenSSL 3 without the legacy provider cannot open that. Such a bundle fails
//! later, when `load_identity` in `rocket-infra/src/reqwest_executor.rs` loads the identity,
//! with "Cannot load PKCS12 client certificate (inline, for <domain>): ...". Picking the PEM
//! format for the entry avoids it.

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

    fn deferred(
        domain: &str,
        name: &str,
        format: VaultCertificateFormat,
    ) -> ResolvedClientCertificate {
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
        let fetcher = FakeCertificateFetcher::new(&[
            ("client-a", FakeExport::Ok),
            ("client-b", FakeExport::Ok),
        ]);
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
        assert!(
            lines[1].starts_with("deferred other.example.com"),
            "{lines:?}"
        );
    }

    // Review Focus 3.
    #[tokio::test]
    async fn a_vault_certificate_for_another_domain_makes_no_vault_call() {
        let fetcher = FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok)]);
        let mut certs = vec![deferred(
            "api.example.com",
            "client-a",
            VaultCertificateFormat::Pem,
        )];
        run(&mut certs, &["https://unrelated.example.org/"], &fetcher).await;

        assert!(fetcher.calls().is_empty());
        assert!(certs.iter().all(|c| c.is_deferred()));
    }

    // Review Focus 4.
    #[tokio::test]
    async fn a_failed_fetch_fails_only_the_selected_entry_with_no_fallback() {
        // The later vault entry would export fine, so fetching it would be a silent fallback.
        let fetcher = FakeCertificateFetcher::new(&[
            (
                "client-a",
                FakeExport::Fail("RocketVault rejected the access token (401)."),
            ),
            ("client-b", FakeExport::Ok),
        ]);
        let mut certs = vec![
            deferred("api.example.com", "client-a", VaultCertificateFormat::Pem),
            deferred("api.example.com", "client-b", VaultCertificateFormat::Pem),
            ResolvedClientCertificate::pkcs12(
                "api.example.com",
                CertificateSource::File("/certs/fallback.p12".into()),
                None,
            ),
        ];
        run(&mut certs, &["https://api.example.com/"], &fetcher).await;

        // Only the selected entry was exported, never the later matching vault entry.
        assert_eq!(fetcher.calls(), vec!["prod-vault/client-a/pem"]);
        let lines = describe_all(&certs);
        assert!(
            lines[0].starts_with("unavailable api.example.com")
                && lines[0].contains("client-a")
                && lines[0].contains("binding prod")
                && lines[0].contains("(401)"),
            "{lines:?}"
        );
        assert!(certs[1].is_deferred(), "{lines:?}");
        assert_eq!(
            lines[1],
            "deferred api.example.com prod:client-b pem conn:conn-1 vault:prod-vault"
        );
        // The file entry for the same domain is untouched, and the first match still wins, so
        // the executor fails the request with the reason instead of using another entry.
        assert_eq!(
            lines[2],
            "pkcs12 api.example.com file:/certs/fallback.p12 pass:-"
        );
        assert!(matches!(
            find_certificate(&certs, "https://api.example.com/").map(|c| &c.material),
            Some(CertificateMaterial::Unavailable { .. })
        ));
    }

    #[tokio::test]
    async fn a_missing_keychain_secret_names_the_certificate_and_the_binding() {
        let fetcher = FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok)]);
        let repo = FakeSecretManagerRepo(connection());
        let access = VaultCertificateAccess {
            connections: &repo,
            secret_store: &rocket_environment::NullSecretStore,
            fetcher: &*fetcher,
        };
        let mut certs = vec![deferred(
            "api.example.com",
            "client-a",
            VaultCertificateFormat::Pem,
        )];
        materialize_selected(&mut certs, &["https://api.example.com/"], Some(&access)).await;

        let lines = describe_all(&certs);
        assert!(
            lines[0].starts_with("unavailable api.example.com")
                && lines[0].contains("client-a")
                && lines[0].contains("binding prod")
                && lines[0].contains("No client secret is stored"),
            "{lines:?}"
        );
        assert!(fetcher.calls().is_empty(), "{:?}", fetcher.calls());
    }

    #[tokio::test]
    async fn pem_material_becomes_inline_pem_with_no_passphrase() {
        let fetcher = FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok)]);
        let mut certs = vec![deferred(
            "api.example.com",
            "client-a",
            VaultCertificateFormat::Pem,
        )];
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
        let mut certs = vec![deferred(
            "api.example.com",
            "client-a",
            VaultCertificateFormat::Pkcs12,
        )];
        run(&mut certs, &["https://api.example.com/"], &fetcher).await;

        assert_eq!(fetcher.calls(), vec!["prod-vault/client-a/pkcs12"]);
        // The password is compared, never printed, so a failure message cannot quote it.
        match &certs[0].material {
            CertificateMaterial::Pkcs12 {
                bundle: CertificateSource::Inline(bundle),
                passphrase,
            } => {
                assert_eq!(bundle.as_slice(), FAKE_BUNDLE);
                assert!(
                    passphrase
                        .as_deref()
                        .is_some_and(|p| p.as_str() == FAKE_PASSWORD),
                    "the passphrase is not the one-time password"
                );
            }
            other => panic!("expected inline PKCS12, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn two_urls_on_one_certificate_fetch_it_once() {
        let fetcher = FakeCertificateFetcher::new(&[("client-a", FakeExport::Ok)]);
        let mut certs = vec![deferred(
            "api.example.com",
            "client-a",
            VaultCertificateFormat::Pem,
        )];
        run(
            &mut certs,
            &[
                "https://api.example.com/v1",
                "https://api.example.com/oauth/token",
            ],
            &fetcher,
        )
        .await;
        assert_eq!(fetcher.calls().len(), 1);
    }

    #[tokio::test]
    async fn a_request_url_and_a_token_url_fetch_their_own_certificates() {
        let fetcher = FakeCertificateFetcher::new(&[
            ("client-a", FakeExport::Ok),
            ("idp-cert", FakeExport::Ok),
        ]);
        let mut certs = vec![
            deferred("api.example.com", "client-a", VaultCertificateFormat::Pem),
            deferred(
                "idp.example.com",
                "idp-cert",
                VaultCertificateFormat::Pkcs12,
            ),
        ];
        run(
            &mut certs,
            &[
                "https://api.example.com/v1",
                "https://idp.example.com/token",
            ],
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
        let mut certs = vec![deferred(
            "api.example.com",
            "client-a",
            VaultCertificateFormat::Pem,
        )];
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
        let mut certs = vec![deferred(
            "api.example.com",
            "client-a",
            VaultCertificateFormat::Pem,
        )];
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
        let fetcher = FakeCertificateFetcher::new(&[
            ("client-a", FakeExport::Ok),
            ("idp-cert", FakeExport::Ok),
        ]);
        let mut certs = vec![
            deferred("api.example.com", "client-a", VaultCertificateFormat::Pem),
            deferred(
                "idp.example.com",
                "idp-cert",
                VaultCertificateFormat::Pkcs12,
            ),
        ];
        run(
            &mut certs,
            &["https://api.example.com/", "https://idp.example.com/token"],
            &fetcher,
        )
        .await;
        let shown = format!("{certs:?}");
        // The messages never quote `shown`, which would print the secret on a failure.
        assert!(
            !shown.contains(FAKE_PASSWORD),
            "Debug output shows the one-time password"
        );
        assert!(
            !shown.contains("BEGIN") && !shown.contains("c2VjcmV0"),
            "Debug output shows key bytes"
        );
    }

    #[test]
    fn certificate_urls_adds_the_client_credentials_token_url() {
        let mut request = HttpRequest::new(HttpMethod::Get, "https://api.example.com/v1");
        assert_eq!(
            certificate_urls(&request),
            vec!["https://api.example.com/v1"]
        );

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
            vec![
                "https://api.example.com/v1",
                "https://idp.example.com/token"
            ]
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
