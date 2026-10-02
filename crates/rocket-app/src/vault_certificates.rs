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
