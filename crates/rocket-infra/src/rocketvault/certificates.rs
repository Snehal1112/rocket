//! Certificate calls on top of the contract in `certificate_api.rs`: the paged list walk, the
//! name-to-id cache and the export. Only ids are cached. Material never is.

use rand::{distributions::Alphanumeric, rngs::OsRng, Rng};
use rocket_environment::{
    SecretManagerConnection, VaultCertificateMaterial, VaultCertificateSummary,
};
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
                return Err(DomainError::Http(
                    certificate_api::TOKEN_REJECTED.to_string(),
                ));
            }
            if !status.is_success() {
                return Err(certificate_api::list_failed(status));
            }
            let body = resp.bytes().await.map_err(|e| {
                DomainError::Http(format!(
                    "RocketVault certificate list could not be read: {e}"
                ))
            })?;
            let parsed = certificate_api::parse_certificate_page(&body)?;
            let page_len = parsed.certificates.len();
            let found =
                until.is_some_and(|name| parsed.certificates.iter().any(|c| c.name == name));
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
            return Err(DomainError::NotFound(
                certificate_api::NOT_FOUND.to_string(),
            ));
        };
        self.certificate_ids.insert(key, found.id.clone());
        Ok(found.id)
    }

    /// Drops every cached certificate id of `connection_id`.
    pub(super) fn forget_connection_ids(&self, connection_id: &str) {
        let prefix = format!("{connection_id}\u{1f}");
        self.certificate_ids
            .retain(|key, _| !key.starts_with(&prefix));
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
                Err(DomainError::NotFound(
                    certificate_api::NOT_FOUND.to_string(),
                ))
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
        let mut body =
            certificate_api::export_request_body(format, password.as_ref().map(|p| p.as_str()))?;
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
                DomainError::Http(format!(
                    "RocketVault certificate export request failed: {e}"
                ))
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
                Err(DomainError::Http(
                    certificate_api::TOKEN_REJECTED.to_string(),
                ))
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
async fn read_capped(mut resp: reqwest::Response, cap: usize) -> DomainResult<Zeroizing<Vec<u8>>> {
    let too_large = || {
        DomainError::Http(format!(
            "RocketVault sent a response larger than {cap} bytes."
        ))
    };
    if resp.content_length().is_some_and(|n| n > cap as u64) {
        return Err(too_large());
    }
    let mut body = Zeroizing::new(Vec::with_capacity(cap));
    while let Some(chunk) = resp
        .chunk()
        .await
        .map_err(|e| DomainError::Http(format!("RocketVault response could not be read: {e}")))?
    {
        if body.len() + chunk.len() > cap {
            return Err(too_large());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Cache key for a certificate id. The connection's id and base URL are part of it, so an
/// edited connection never reuses an id read through the old one.
fn certificate_id_key(
    connection: &SecretManagerConnection,
    vault_name: &str,
    name: &str,
) -> String {
    [
        connection.id.as_str(),
        connection.base_url.as_str(),
        vault_name,
        name,
    ]
    .join("\u{1f}")
}

#[cfg(test)]
mod tests {
    use super::super::certificate_api::{
        LIST_PAGE_SIZE, MAX_EXPORT_BYTES, MAX_LIST_PAGES, PKCS12_PASSWORD_LEN,
    };
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

    const LIST_PATH: &str = "/api/v1/vaults/prod-vault/certificates";

    fn conn(base_url: String) -> SecretManagerConnection {
        SecretManagerConnection {
            id: "conn-1".into(),
            label: "Test".into(),
            base_url,
            client_id: "rocketapi".into(),
            verify_ssl: true,
            allow_insecure_http: true, // the mock server is http://127.0.0.1:<port>
            provider: Default::default(),
            config: None,
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
        mount_page(
            &server,
            0,
            ResponseTemplate::new(200).set_body_json(body),
            1,
        )
        .await;

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
        mount_page(
            &server,
            0,
            page_of(vec![entry("id-1", "client-a")], Some(1)),
            1,
        )
        .await;

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
    async fn forgetting_a_connection_drops_only_its_cached_ids() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_page(
            &server,
            0,
            page_of(vec![entry("id-1", "client-a")], Some(1)),
            1,
        )
        .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let c = conn(server.uri());
        let mut other = conn(server.uri());
        other.id = "conn-other".to_string();
        fetcher
            .certificate_id(&c, "shh", "prod-vault", "client-a", false)
            .await
            .expect("id");
        fetcher.certificate_ids.insert(
            super::certificate_id_key(&other, "v", "n"),
            "keep".to_string(),
        );
        assert_eq!(fetcher.certificate_ids.len(), 2);

        VaultSecretFetcher::forget_connection(&fetcher, &c.id);

        assert_eq!(fetcher.certificate_ids.len(), 1);
        assert!(fetcher
            .certificate_ids
            .contains_key(&super::certificate_id_key(&other, "v", "n")));
    }

    #[tokio::test]
    async fn a_name_missing_from_the_vault_is_not_found_and_not_cached() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_page(
            &server,
            0,
            page_of(vec![entry("id-1", "other")], Some(1)),
            2,
        )
        .await;

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
        mount_page(
            &server,
            0,
            page_of(vec![entry("id-1", "client-a")], Some(1)),
            1,
        )
        .await;

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

    // Spec risk 3: the cache key holds the connection's base URL, so an edited connection
    // misses the id cached through the old URL. Nothing is cleared.
    #[tokio::test]
    async fn a_changed_base_url_does_not_reuse_a_cached_id() {
        let first = MockServer::start().await;
        mount_token(&first).await;
        mount_page(
            &first,
            0,
            page_of(vec![entry("id-on-first", "client-a")], Some(1)),
            1,
        )
        .await;
        let second = MockServer::start().await;
        mount_token(&second).await;
        mount_page(
            &second,
            0,
            page_of(vec![entry("id-on-second", "client-a")], Some(1)),
            1,
        )
        .await;

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

    // The cache key holds the connection id, the vault and the name. Changing any one of them
    // misses the cached id and lists again.
    #[tokio::test]
    async fn a_different_connection_id_does_not_reuse_a_cached_id() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_page(
            &server,
            0,
            page_of(vec![entry("id-1", "client-a")], Some(1)),
            2,
        )
        .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let first = conn(server.uri());
        let mut second = first.clone();
        second.id = "conn-2".into();
        for c in [&first, &second] {
            fetcher
                .certificate_id(c, "shh", "prod-vault", "client-a", false)
                .await
                .expect("id");
        }
    }

    #[tokio::test]
    async fn a_different_vault_does_not_reuse_a_cached_id() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_page(
            &server,
            0,
            page_of(vec![entry("id-1", "client-a")], Some(1)),
            1,
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/api/v1/vaults/other-vault/certificates"))
            .respond_with(page_of(vec![entry("other-id", "client-a")], Some(1)))
            .expect(1)
            .mount(&server)
            .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let c = conn(server.uri());
        let in_prod = fetcher
            .certificate_id(&c, "shh", "prod-vault", "client-a", false)
            .await
            .expect("prod");
        let in_other = fetcher
            .certificate_id(&c, "shh", "other-vault", "client-a", false)
            .await
            .expect("other");
        assert_eq!((in_prod.as_str(), in_other.as_str()), ("id-1", "other-id"));
    }

    #[tokio::test]
    async fn a_different_name_does_not_reuse_a_cached_id() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        mount_page(
            &server,
            0,
            page_of(
                vec![entry("id-a", "client-a"), entry("id-b", "client-b")],
                Some(2),
            ),
            2,
        )
        .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let c = conn(server.uri());
        let a = fetcher
            .certificate_id(&c, "shh", "prod-vault", "client-a", false)
            .await
            .expect("a");
        let b = fetcher
            .certificate_id(&c, "shh", "prod-vault", "client-b", false)
            .await
            .expect("b");
        assert_eq!((a.as_str(), b.as_str()), ("id-a", "id-b"));
    }

    // A server that always sends a full page and no total must not loop forever.
    #[tokio::test]
    async fn the_list_walk_stops_at_the_page_cap() {
        let server = MockServer::start().await;
        mount_token(&server).await;
        Mock::given(method("GET"))
            .and(path(LIST_PATH))
            .respond_with(page_of(filler("x", LIST_PAGE_SIZE), None))
            .expect(MAX_LIST_PAGES as u64)
            .mount(&server)
            .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let err = fetcher
            .list_certificates(&conn(server.uri()), "shh", "prod-vault")
            .await
            .expect_err("capped");
        assert_eq!(
            err,
            DomainError::Http(format!(
                "RocketVault listed more than {} certificates; Rocket stopped reading.",
                MAX_LIST_PAGES * LIST_PAGE_SIZE
            ))
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
        assert!(
            err.to_string().contains("cannot list certificates"),
            "{err}"
        );
    }

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
        mount_page(
            server,
            0,
            page_of(vec![entry("id-1", "client-a")], Some(1)),
            list_calls,
        )
        .await;
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
        match fetch(&fetcher, &server, VaultCertificateFormat::Pem)
            .await
            .expect("export")
        {
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
        assert_eq!(
            export_bodies(&server).await,
            vec![json!({ "format": "pem" })]
        );
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
            assert!(
                password.chars().all(|c| c.is_ascii_alphanumeric()),
                "{password}"
            );
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
        assert!(
            fetcher.certificate_ids.is_empty(),
            "a missing id is forgotten"
        );
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
        mount_export(
            &server,
            "id-1",
            ResponseTemplate::new(403).set_body_string("Forbidden"),
            1,
        )
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
        mount_export(
            &server,
            "id-1",
            rocketvault_error(409, "certificate_disabled"),
            1,
        )
        .await;

        let fetcher = ReqwestVaultSecretFetcher::new();
        let err = fetch(&fetcher, &server, VaultCertificateFormat::Pem)
            .await
            .expect_err("409");
        assert_eq!(
            err,
            DomainError::InvalidInput("Certificate is disabled.".into())
        );
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
        assert!(
            !err.contains("ZmFrZS1rZXk") && !err.contains("BEGIN"),
            "{err}"
        );
    }

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
            provider: Default::default(),
            config: None,
        };
        let secret = var("ROCKETVAULT_CLIENT_SECRET");
        let vault = var("ROCKETVAULT_VAULT");
        let name = var("ROCKETVAULT_CERTIFICATE");

        let fetcher = ReqwestVaultSecretFetcher::new();
        let listed = fetcher
            .list_certificates(&connection, &secret, &vault)
            .await
            .expect("list");
        assert!(
            listed.iter().any(|c| c.name == name),
            "{name} is not listed"
        );

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
            .unwrap_or_else(|e| {
                panic!(
                    "the platform TLS stack cannot load the {format:?} export ({e}); for PKCS12, \
                     OpenSSL 3 may refuse a legacy (RC2) bundle"
                )
            });

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
}
