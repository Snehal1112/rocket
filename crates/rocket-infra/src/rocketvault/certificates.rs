//! Certificate calls on top of the contract in `certificate_api.rs`: the paged list walk, the
//! name-to-id cache and the export. Only ids are cached. Material never is.

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
    use super::super::ReqwestVaultSecretFetcher;
    use rocket_environment::{
        SecretManagerConnection, VaultCertificateSummary, VaultSecretFetcher,
    };
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

    // Spec risk 3: the cache is cleared when a connection changes.
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
}
