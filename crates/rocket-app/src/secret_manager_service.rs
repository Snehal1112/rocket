use crate::vault_secret_resolution::VAULT_CONNECTION_SCOPE;
use rocket_environment::external_secret::ExternalSecretRef;
use rocket_environment::secret_manager::{
    SecretManagerConnection, SecretManagerRepository, SecretProviderKind,
};
use rocket_environment::secret_store::SecretStore;
use rocket_environment::vault_secret_fetcher::VaultSecretFetcher;
use rocket_environment::VaultCertificateSummary;
use rocket_shared::error::{DomainError, DomainResult};
use std::sync::Arc;

pub struct SecretManagerService {
    repo: Box<dyn SecretManagerRepository>,
    secret_store: Arc<dyn SecretStore>,
    fetcher: Arc<dyn VaultSecretFetcher>,
}

impl SecretManagerService {
    pub fn new(
        repo: Box<dyn SecretManagerRepository>,
        secret_store: Arc<dyn SecretStore>,
        fetcher: Arc<dyn VaultSecretFetcher>,
    ) -> Self {
        Self {
            repo,
            secret_store,
            fetcher,
        }
    }

    pub fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
        self.repo.list()
    }

    /// `client_secret: Some(s)` sets/overwrites the keychain entry for this
    /// connection before persisting the connection record — if the keychain
    /// write fails, the connection record is never saved, so we never end up
    /// with a connection pointing at a secret that was never actually
    /// stored. `client_secret: None` is an edit that does not change the
    /// secret (e.g. relabeling a connection) — the keychain write is skipped
    /// and only the connection record is saved, but only if a keychain entry
    /// for this connection already exists; a brand-new connection with no
    /// stored secret and no prior keychain entry is rejected rather than
    /// silently persisted as a permanently broken record. The credential is
    /// optional for providers that report `credential_optional`.
    pub fn save(
        &self,
        connection: SecretManagerConnection,
        client_secret: Option<String>,
    ) -> DomainResult<()> {
        validate_connection(&connection, self.fetcher.as_ref())?;
        // A saved connection keeps its provider. Otherwise a stored credential
        // could be sent to a different provider by an edit over IPC.
        if let Some(existing) = self.repo.get(&connection.id)? {
            if existing.provider != connection.provider {
                return Err(DomainError::InvalidInput(format!(
                    "the provider of connection {} cannot be changed from {} to {}",
                    connection.id,
                    existing.provider.display_name(),
                    connection.provider.display_name()
                )));
            }
        }
        if client_secret
            .as_deref()
            .is_some_and(|s| s.trim().is_empty())
        {
            return Err(DomainError::InvalidInput(
                "client_secret must not be empty".to_string(),
            ));
        }
        match client_secret {
            Some(secret) => {
                self.secret_store
                    .set(VAULT_CONNECTION_SCOPE, &connection.id, &secret)?;
            }
            None => {
                let has_existing_secret = self
                    .secret_store
                    .get(VAULT_CONNECTION_SCOPE, &connection.id)?
                    .is_some();
                let optional = self.fetcher.capabilities(&connection).credential_optional;
                if !has_existing_secret && !optional {
                    return Err(DomainError::InvalidInput(
                        "a new connection must be saved with a client_secret".to_string(),
                    ));
                }
            }
        }
        self.repo.save(&connection)?;
        self.fetcher.forget_connection(&connection.id);
        Ok(())
    }

    /// Deletes the connection record, then best-effort deletes its keychain
    /// entry. A keychain delete failure is logged and ignored rather than
    /// failing the whole delete: a stale keychain entry for a connection
    /// that no longer exists is not a confidentiality problem (nothing can
    /// look it up without the connection id, which is already gone from the
    /// repo), mirroring the existing `KeyringSecretStore`/hardening-spec
    /// precedent that delete-path keychain failures are non-fatal.
    pub fn delete(&self, id: &str) -> DomainResult<()> {
        self.repo.delete(id)?;
        self.fetcher.forget_connection(id);
        if let Err(err) = self.secret_store.delete(VAULT_CONNECTION_SCOPE, id) {
            tracing::warn!(error = %err, id = %id, "failed to delete keychain entry for vault connection");
        }
        Ok(())
    }

    fn connection_and_secret(&self, id: &str) -> DomainResult<(SecretManagerConnection, String)> {
        let connection = self
            .repo
            .get(id)?
            .ok_or_else(|| DomainError::NotFound(id.to_string()))?;
        let stored = self.secret_store.get(VAULT_CONNECTION_SCOPE, id)?;
        let secret = match stored {
            Some(secret) => secret,
            None if self.fetcher.capabilities(&connection).credential_optional => String::new(),
            None => {
                return Err(DomainError::Internal(format!(
                    "no client secret available for connection {id} — it was never stored, or the OS keychain is locked/unavailable"
                )))
            }
        };
        Ok((connection, secret))
    }

    pub async fn test_connection(&self, id: &str, vault_name: &str) -> DomainResult<()> {
        let (connection, secret) = self.connection_and_secret(id)?;
        self.fetcher
            .test_connection(&connection, &secret, vault_name)
            .await
    }

    pub async fn fetch_secret_names(
        &self,
        id: &str,
        vault_name: &str,
    ) -> DomainResult<Vec<ExternalSecretRef>> {
        let (connection, secret) = self.connection_and_secret(id)?;
        self.fetcher
            .list_secrets(&connection, &secret, vault_name)
            .await
    }

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

    /// Fetches one secret's raw value directly, without going through the
    /// per-environment `ExternalSecretBinding`/alias flow — for callers (like
    /// `AgentConfigService`) that need an app-global credential rather than a
    /// value bound to a specific `Environment`.
    pub async fn resolve_secret_value(
        &self,
        connection_id: &str,
        vault_name: &str,
        secret_id: &str,
    ) -> DomainResult<Option<String>> {
        let (connection, client_secret) = self.connection_and_secret(connection_id)?;
        self.fetcher
            .get_secret_value(&connection, &client_secret, vault_name, secret_id)
            .await
    }
}

/// Rejects a connection record that could never work, before anything is
/// written to the keychain or to `secret_managers.yml`. The https-only rule
/// for non-loopback hosts is enforced by the RocketVault fetcher on every
/// call, so it is not repeated here. Other providers' rules come from their
/// fetcher.
fn validate_connection(
    connection: &SecretManagerConnection,
    fetcher: &dyn VaultSecretFetcher,
) -> DomainResult<()> {
    for (field, value) in [("id", &connection.id), ("label", &connection.label)] {
        if value.trim().is_empty() {
            return Err(DomainError::InvalidInput(format!(
                "secret manager connection {field} must not be empty"
            )));
        }
    }
    if connection.provider != SecretProviderKind::RocketVault {
        return fetcher.validate_connection(connection);
    }
    for (field, value) in [
        ("base_url", &connection.base_url),
        ("client_id", &connection.client_id),
    ] {
        if value.trim().is_empty() {
            return Err(DomainError::InvalidInput(format!(
                "secret manager connection {field} must not be empty"
            )));
        }
    }
    let parsed = url::Url::parse(&connection.base_url).map_err(|e| {
        DomainError::InvalidInput(format!("invalid base_url '{}': {e}", connection.base_url))
    })?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(DomainError::InvalidInput(format!(
            "base_url '{}' must be an http:// or https:// URL",
            connection.base_url
        )));
    }
    Ok(())
}

impl rocket_environment::ProviderCapabilityLookup for SecretManagerService {
    fn provider_of(
        &self,
        connection_id: &str,
    ) -> DomainResult<Option<rocket_environment::ConnectionProvider>> {
        let Some(connection) = self.repo.get(connection_id)? else {
            return Ok(None);
        };
        Ok(Some(rocket_environment::ConnectionProvider {
            kind: connection.provider,
            capabilities: self.fetcher.capabilities(&connection),
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::external_secret::ExternalSecretRef;
    use rocket_environment::secret_manager::ProviderConfig;
    use rocket_shared::error::DomainError;
    use std::sync::Mutex;

    struct FakeRepo(Mutex<Vec<SecretManagerConnection>>);

    impl FakeRepo {
        fn new() -> Self {
            Self(Mutex::new(Vec::new()))
        }
    }

    impl SecretManagerRepository for FakeRepo {
        fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
            Ok(self.0.lock().expect("lock FakeRepo").clone())
        }
        fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
            Ok(self
                .0
                .lock()
                .expect("lock FakeRepo")
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }
        fn save(&self, connection: &SecretManagerConnection) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeRepo");
            guard.retain(|c| c.id != connection.id);
            guard.push(connection.clone());
            Ok(())
        }
        fn delete(&self, id: &str) -> DomainResult<()> {
            self.0.lock().expect("lock FakeRepo").retain(|c| c.id != id);
            Ok(())
        }
    }

    struct FakeSecretStore {
        entries: Mutex<std::collections::HashMap<(String, String), String>>,
        fail_next_set: std::sync::atomic::AtomicBool,
    }

    impl FakeSecretStore {
        fn new() -> Self {
            Self {
                entries: Mutex::new(std::collections::HashMap::new()),
                fail_next_set: std::sync::atomic::AtomicBool::new(false),
            }
        }
        fn fail_next_set(&self) {
            self.fail_next_set
                .store(true, std::sync::atomic::Ordering::SeqCst);
        }
    }

    impl SecretStore for FakeSecretStore {
        fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
            Ok(self
                .entries
                .lock()
                .expect("lock FakeSecretStore")
                .get(&(scope_id.to_string(), key.to_string()))
                .cloned())
        }
        fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()> {
            if self
                .fail_next_set
                .swap(false, std::sync::atomic::Ordering::SeqCst)
            {
                return Err(DomainError::Internal("keychain write failed".to_string()));
            }
            self.entries
                .lock()
                .expect("lock FakeSecretStore")
                .insert((scope_id.to_string(), key.to_string()), value.to_string());
            Ok(())
        }
        fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()> {
            self.entries
                .lock()
                .expect("lock FakeSecretStore")
                .remove(&(scope_id.to_string(), key.to_string()));
            Ok(())
        }
    }

    struct FakeFetcher;

    #[async_trait::async_trait]
    impl VaultSecretFetcher for FakeFetcher {
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

    struct ConfigurableFakeFetcher {
        list_result: DomainResult<Vec<ExternalSecretRef>>,
        test_result: DomainResult<()>,
        secret_value_result: DomainResult<Option<String>>,
    }

    // DomainError derives PartialEq but not Clone — this test-only helper
    // reconstructs an equivalent error by matching on the variants this
    // module's tests actually produce, so a fake can be configured with a
    // canned Err(..) and still return that error from every call.
    fn clone_domain_error(err: &DomainError) -> DomainError {
        match err {
            DomainError::NotFound(msg) => DomainError::NotFound(msg.clone()),
            DomainError::Internal(msg) => DomainError::Internal(msg.clone()),
            other => DomainError::Internal(other.to_string()),
        }
    }

    #[async_trait::async_trait]
    impl VaultSecretFetcher for ConfigurableFakeFetcher {
        async fn list_secrets(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<Vec<ExternalSecretRef>> {
            match &self.list_result {
                Ok(refs) => Ok(refs.clone()),
                Err(err) => Err(clone_domain_error(err)),
            }
        }
        async fn get_secret_value(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
            _secret_id: &str,
        ) -> DomainResult<Option<String>> {
            match &self.secret_value_result {
                Ok(v) => Ok(v.clone()),
                Err(err) => Err(clone_domain_error(err)),
            }
        }
        async fn test_connection(
            &self,
            _connection: &SecretManagerConnection,
            _client_secret: &str,
            _vault_name: &str,
        ) -> DomainResult<()> {
            match &self.test_result {
                Ok(()) => Ok(()),
                Err(err) => Err(clone_domain_error(err)),
            }
        }
    }

    fn sample_connection(id: &str) -> SecretManagerConnection {
        SecretManagerConnection {
            id: id.to_string(),
            label: "Prod RocketVault".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: Default::default(),
            config: None,
        }
    }

    #[test]
    fn save_with_secret_stores_both_connection_and_keychain_entry() {
        let repo = FakeRepo::new();
        let store = Arc::new(FakeSecretStore::new());
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::clone(&store) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );
        let conn = sample_connection("conn-1");

        service
            .save(conn.clone(), Some("shh-its-a-secret".to_string()))
            .expect("save with secret");

        let listed = service.list().expect("list connections");
        assert_eq!(listed, vec![conn]);
        assert_eq!(
            store
                .get("vault-connection", "conn-1")
                .expect("get keychain entry"),
            Some("shh-its-a-secret".to_string())
        );
    }

    #[test]
    fn save_without_secret_on_brand_new_connection_is_rejected() {
        let repo = FakeRepo::new();
        let store = Arc::new(FakeSecretStore::new());
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::clone(&store) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );
        let conn = sample_connection("conn-1");

        let result = service.save(conn, None);

        assert!(
            matches!(result, Err(DomainError::InvalidInput(_))),
            "expected DomainError::InvalidInput for a new connection saved without a client_secret, got {result:?}"
        );
        assert!(
            service.list().expect("list connections").is_empty(),
            "a rejected save must not persist the connection record"
        );
    }

    #[test]
    fn save_without_secret_leaves_existing_keychain_entry_untouched() {
        let repo = FakeRepo::new();
        let store = Arc::new(FakeSecretStore::new());
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::clone(&store) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );
        let mut conn = sample_connection("conn-1");
        service
            .save(conn.clone(), Some("original-secret".to_string()))
            .expect("initial save with secret");

        conn.label = "Renamed RocketVault".to_string();
        service
            .save(conn.clone(), None)
            .expect("edit without resecret");

        let listed = service.list().expect("list connections");
        assert_eq!(listed, vec![conn]);
        assert_eq!(
            store
                .get("vault-connection", "conn-1")
                .expect("get keychain entry"),
            Some("original-secret".to_string()),
            "keychain entry must be untouched by a client_secret: None save"
        );
    }

    #[test]
    fn delete_removes_both_connection_and_keychain_entry() {
        let repo = FakeRepo::new();
        let store = Arc::new(FakeSecretStore::new());
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::clone(&store) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );
        let conn = sample_connection("conn-1");
        service
            .save(conn, Some("shh-its-a-secret".to_string()))
            .expect("save with secret");

        service.delete("conn-1").expect("delete connection");

        assert!(service.list().expect("list connections").is_empty());
        assert_eq!(
            store
                .get("vault-connection", "conn-1")
                .expect("get keychain entry after delete"),
            None
        );
    }

    struct ForgetRecorder(std::sync::Mutex<Vec<String>>);

    #[async_trait::async_trait]
    impl VaultSecretFetcher for ForgetRecorder {
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
        fn forget_connection(&self, connection_id: &str) {
            if let Ok(mut seen) = self.0.lock() {
                seen.push(connection_id.to_string());
            }
        }
    }

    #[test]
    fn save_and_delete_tell_the_fetcher_to_forget_the_connection() {
        let recorder = Arc::new(ForgetRecorder(std::sync::Mutex::new(Vec::new())));
        let service = SecretManagerService::new(
            Box::new(FakeRepo::new()),
            Arc::new(FakeSecretStore::new()),
            Arc::clone(&recorder) as Arc<dyn VaultSecretFetcher>,
        );
        service
            .save(sample_connection("conn-1"), Some("shh".to_string()))
            .expect("save");
        service.delete("conn-1").expect("delete");

        let seen = recorder.0.lock().expect("lock").clone();
        assert_eq!(seen, vec!["conn-1".to_string(), "conn-1".to_string()]);
    }

    #[test]
    fn keychain_set_failure_during_save_prevents_connection_persistence() {
        let repo = FakeRepo::new();
        let store = FakeSecretStore::new();
        store.fail_next_set();
        let service =
            SecretManagerService::new(Box::new(repo), Arc::new(store), Arc::new(FakeFetcher));
        let conn = sample_connection("conn-1");

        let result = service.save(conn, Some("shh-its-a-secret".to_string()));

        assert!(
            result.is_err(),
            "expected keychain failure to surface as an error"
        );
        assert!(
            service.list().expect("list connections").is_empty(),
            "connection record must not be persisted when the keychain write fails"
        );
    }

    #[tokio::test]
    async fn test_connection_succeeds_when_fetcher_succeeds() {
        let repo = FakeRepo::new();
        let store = FakeSecretStore::new();
        store
            .set("vault-connection", "conn-1", "shh-its-a-secret")
            .expect("seed keychain entry");
        repo.save(&sample_connection("conn-1"))
            .expect("seed connection");
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(store),
            Arc::new(ConfigurableFakeFetcher {
                list_result: Ok(Vec::new()),
                test_result: Ok(()),
                secret_value_result: Ok(None),
            }),
        );

        service
            .test_connection("conn-1", "prod-vault")
            .await
            .expect("test_connection should succeed");
    }

    #[tokio::test]
    async fn fetch_secret_names_returns_fetcher_output_unchanged() {
        let repo = FakeRepo::new();
        let store = FakeSecretStore::new();
        store
            .set("vault-connection", "conn-1", "shh-its-a-secret")
            .expect("seed keychain entry");
        repo.save(&sample_connection("conn-1"))
            .expect("seed connection");
        let expected = vec![ExternalSecretRef {
            name: "stripe-key".to_string(),
            secret_id: "b6f1c2e0-1234-4a5b-9abc-000000000001".to_string(),
        }];
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(store),
            Arc::new(ConfigurableFakeFetcher {
                list_result: Ok(expected.clone()),
                test_result: Ok(()),
                secret_value_result: Ok(None),
            }),
        );

        let names = service
            .fetch_secret_names("conn-1", "prod-vault")
            .await
            .expect("fetch_secret_names should succeed");

        assert_eq!(names, expected);
    }

    #[tokio::test]
    async fn connection_id_with_no_stored_secret_errors_clearly() {
        let repo = FakeRepo::new();
        repo.save(&sample_connection("conn-1"))
            .expect("seed connection");
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(FakeSecretStore::new()), // no keychain entry seeded
            Arc::new(ConfigurableFakeFetcher {
                list_result: Ok(Vec::new()),
                test_result: Ok(()),
                secret_value_result: Ok(None),
            }),
        );

        let result = service.test_connection("conn-1", "prod-vault").await;

        assert!(
            matches!(result, Err(DomainError::Internal(_))),
            "expected DomainError::Internal for a connection with no stored secret, got {result:?}"
        );
    }

    #[tokio::test]
    async fn unknown_connection_id_errors_clearly() {
        let service = SecretManagerService::new(
            Box::new(FakeRepo::new()),
            Arc::new(FakeSecretStore::new()),
            Arc::new(ConfigurableFakeFetcher {
                list_result: Ok(Vec::new()),
                test_result: Ok(()),
                secret_value_result: Ok(None),
            }),
        );

        let result = service
            .fetch_secret_names("no-such-conn", "prod-vault")
            .await;

        assert!(
            matches!(result, Err(DomainError::NotFound(_))),
            "expected DomainError::NotFound for an unknown connection id, got {result:?}"
        );
    }

    #[test]
    fn save_rejects_blank_fields_and_bad_base_url() {
        let service = SecretManagerService::new(
            Box::new(FakeRepo::new()),
            Arc::new(FakeSecretStore::new()),
            Arc::new(FakeFetcher),
        );
        let mut no_label = sample_connection("c1");
        no_label.label = "  ".to_string();
        let mut bad_url = sample_connection("c2");
        bad_url.base_url = "vault.internal:8774".to_string();
        let mut ftp_url = sample_connection("c3");
        ftp_url.base_url = "ftp://vault.internal".to_string();
        for conn in [no_label, bad_url, ftp_url] {
            let err = service
                .save(conn, Some("s3cret".to_string()))
                .expect_err("must reject");
            assert!(matches!(err, DomainError::InvalidInput(_)), "got {err:?}");
        }
        assert!(service.list().expect("list connections").is_empty());
    }

    #[test]
    fn save_rejects_blank_client_secret() {
        let store = Arc::new(FakeSecretStore::new());
        let service = SecretManagerService::new(
            Box::new(FakeRepo::new()),
            store.clone(),
            Arc::new(FakeFetcher),
        );
        let err = service
            .save(sample_connection("c1"), Some("   ".to_string()))
            .expect_err("must reject");
        assert!(matches!(err, DomainError::InvalidInput(_)));
        assert!(store
            .get(VAULT_CONNECTION_SCOPE, "c1")
            .expect("get")
            .is_none());
    }

    #[tokio::test]
    async fn resolve_secret_value_returns_fetcher_result_unchanged() {
        let repo = FakeRepo::new();
        let store = FakeSecretStore::new();
        store
            .set("vault-connection", "conn-1", "shh-its-a-secret")
            .expect("seed keychain entry");
        repo.save(&sample_connection("conn-1"))
            .expect("seed connection");
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(store),
            Arc::new(ConfigurableFakeFetcher {
                list_result: Ok(Vec::new()),
                test_result: Ok(()),
                secret_value_result: Ok(Some("sk-abc123".to_string())),
            }),
        );

        let value = service
            .resolve_secret_value("conn-1", "prod-vault", "secret-id-1")
            .await
            .expect("resolve_secret_value should succeed");

        assert_eq!(value, Some("sk-abc123".to_string()));
    }

    #[tokio::test]
    async fn resolve_secret_value_unknown_connection_errors() {
        let service = SecretManagerService::new(
            Box::new(FakeRepo::new()),
            Arc::new(FakeSecretStore::new()),
            Arc::new(ConfigurableFakeFetcher {
                list_result: Ok(Vec::new()),
                test_result: Ok(()),
                secret_value_result: Ok(None),
            }),
        );

        let result = service
            .resolve_secret_value("no-such-conn", "prod-vault", "secret-id-1")
            .await;

        assert!(
            matches!(result, Err(DomainError::NotFound(_))),
            "expected DomainError::NotFound for an unknown connection id, got {result:?}"
        );
    }

    #[tokio::test]
    async fn resolve_secret_value_propagates_transport_failure() {
        let repo = FakeRepo::new();
        let store = FakeSecretStore::new();
        store
            .set("vault-connection", "conn-1", "shh-its-a-secret")
            .expect("seed keychain entry");
        repo.save(&sample_connection("conn-1"))
            .expect("seed connection");
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(store),
            Arc::new(ConfigurableFakeFetcher {
                list_result: Ok(Vec::new()),
                test_result: Ok(()),
                secret_value_result: Err(DomainError::Internal("network timeout".to_string())),
            }),
        );

        let result = service
            .resolve_secret_value("conn-1", "prod-vault", "secret-id-1")
            .await;

        assert!(
            matches!(result, Err(DomainError::Internal(_))),
            "a transport failure must surface as DomainError::Internal, not be swallowed, got {result:?}"
        );
    }

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
                (
                    "locked",
                    crate::test_doubles::FakeExport::Fail("not exportable"),
                ),
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

    use rocket_environment::{ProviderCapabilities, SecretProviderKind};

    struct P2Repo {
        rows: std::sync::Mutex<Vec<SecretManagerConnection>>,
    }
    impl P2Repo {
        fn new() -> Self {
            Self {
                rows: std::sync::Mutex::new(Vec::new()),
            }
        }
    }
    impl SecretManagerRepository for P2Repo {
        fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
            Ok(self.rows.lock().expect("lock").clone())
        }
        fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
            Ok(self
                .rows
                .lock()
                .expect("lock")
                .iter()
                .find(|c| c.id == id)
                .cloned())
        }
        fn save(&self, c: &SecretManagerConnection) -> DomainResult<()> {
            let mut rows = self.rows.lock().expect("lock");
            rows.retain(|r| r.id != c.id);
            rows.push(c.clone());
            Ok(())
        }
        fn delete(&self, id: &str) -> DomainResult<()> {
            self.rows.lock().expect("lock").retain(|r| r.id != id);
            Ok(())
        }
    }

    struct P2Store {
        values: std::sync::Mutex<std::collections::HashMap<String, String>>,
    }
    impl P2Store {
        fn new() -> Self {
            Self {
                values: std::sync::Mutex::new(std::collections::HashMap::new()),
            }
        }
    }
    impl SecretStore for P2Store {
        fn get(&self, scope: &str, key: &str) -> DomainResult<Option<String>> {
            Ok(self
                .values
                .lock()
                .expect("lock")
                .get(&format!("{scope}/{key}"))
                .cloned())
        }
        fn set(&self, scope: &str, key: &str, value: &str) -> DomainResult<()> {
            self.values
                .lock()
                .expect("lock")
                .insert(format!("{scope}/{key}"), value.to_string());
            Ok(())
        }
        fn delete(&self, scope: &str, key: &str) -> DomainResult<()> {
            self.values
                .lock()
                .expect("lock")
                .remove(&format!("{scope}/{key}"));
            Ok(())
        }
    }

    /// A fetcher with scripted capabilities and an optional validation error.
    struct P2Fetcher {
        caps: ProviderCapabilities,
        reject: Option<&'static str>,
        seen_credential: std::sync::Mutex<Option<String>>,
    }
    impl P2Fetcher {
        fn new(caps: ProviderCapabilities, reject: Option<&'static str>) -> Arc<Self> {
            Arc::new(Self {
                caps,
                reject,
                seen_credential: std::sync::Mutex::new(None),
            })
        }
    }
    #[async_trait::async_trait]
    impl VaultSecretFetcher for P2Fetcher {
        async fn list_secrets(
            &self,
            _c: &SecretManagerConnection,
            _s: &str,
            _v: &str,
        ) -> DomainResult<Vec<ExternalSecretRef>> {
            Ok(vec![])
        }
        async fn get_secret_value(
            &self,
            _c: &SecretManagerConnection,
            _s: &str,
            _v: &str,
            _id: &str,
        ) -> DomainResult<Option<String>> {
            Ok(None)
        }
        async fn test_connection(
            &self,
            _c: &SecretManagerConnection,
            secret: &str,
            _v: &str,
        ) -> DomainResult<()> {
            *self.seen_credential.lock().expect("lock") = Some(secret.to_string());
            Ok(())
        }
        fn capabilities(&self, _c: &SecretManagerConnection) -> ProviderCapabilities {
            self.caps
        }
        fn validate_connection(&self, _c: &SecretManagerConnection) -> DomainResult<()> {
            match self.reject {
                Some(msg) => Err(DomainError::InvalidInput(msg.to_string())),
                None => Ok(()),
            }
        }
    }

    fn p2_connection(id: &str, provider: SecretProviderKind) -> SecretManagerConnection {
        SecretManagerConnection {
            id: id.to_string(),
            label: "L".to_string(),
            base_url: String::new(),
            client_id: String::new(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider,
            config: None,
        }
    }

    fn p2_service(fetcher: Arc<P2Fetcher>) -> SecretManagerService {
        SecretManagerService::new(Box::new(P2Repo::new()), Arc::new(P2Store::new()), fetcher)
    }

    #[test]
    fn rocketvault_still_requires_base_url_client_id_and_a_secret() {
        let svc = p2_service(P2Fetcher::new(ProviderCapabilities::default(), None));

        let blank = p2_connection("c1", SecretProviderKind::RocketVault);
        let err = svc
            .save(blank, Some("s".to_string()))
            .expect_err("blank base_url");
        assert!(err.to_string().contains("base_url"), "got: {err}");

        let mut ok = p2_connection("c2", SecretProviderKind::RocketVault);
        ok.base_url = "https://v:8774".to_string();
        ok.client_id = "rocketapi".to_string();
        let err = svc
            .save(ok, None)
            .expect_err("a new connection needs a secret");
        assert!(err.to_string().contains("client_secret"), "got: {err}");
    }

    #[test]
    fn another_provider_uses_its_own_validation_and_skips_the_rocketvault_url_rules() {
        let svc = p2_service(P2Fetcher::new(
            ProviderCapabilities::default(),
            Some("tenant is required"),
        ));
        let err = svc
            .save(
                p2_connection("c1", SecretProviderKind::Azure),
                Some("s".to_string()),
            )
            .expect_err("the provider rejects it");
        assert!(err.to_string().contains("tenant is required"), "got: {err}");

        let svc = p2_service(P2Fetcher::new(ProviderCapabilities::default(), None));
        svc.save(
            p2_connection("c2", SecretProviderKind::Azure),
            Some("s".to_string()),
        )
        .expect("empty base_url is fine for a provider that does not use it");
    }

    #[test]
    fn a_credential_optional_provider_saves_without_a_secret() {
        let caps = ProviderCapabilities {
            credential_optional: true,
            ..ProviderCapabilities::default()
        };
        let svc = p2_service(P2Fetcher::new(caps, None));
        svc.save(p2_connection("c1", SecretProviderKind::Azure), None)
            .expect("no credential is allowed for this provider");
    }

    #[test]
    fn a_provider_that_needs_a_credential_rejects_a_new_connection_without_one() {
        let svc = p2_service(P2Fetcher::new(ProviderCapabilities::default(), None));
        let err = svc
            .save(p2_connection("c1", SecretProviderKind::Azure), None)
            .expect_err("a credential is required");
        assert!(err.to_string().contains("client_secret"), "got: {err}");
    }

    #[tokio::test]
    async fn an_optional_credential_reaches_the_fetcher_as_an_empty_string() {
        let caps = ProviderCapabilities {
            credential_optional: true,
            ..ProviderCapabilities::default()
        };
        let fetcher = P2Fetcher::new(caps, None);
        let svc = p2_service(Arc::clone(&fetcher));
        svc.save(p2_connection("c1", SecretProviderKind::Azure), None)
            .expect("save");

        svc.test_connection("c1", "vault")
            .await
            .expect("an optional credential is not an error");

        assert_eq!(
            fetcher.seen_credential.lock().expect("lock").as_deref(),
            Some("")
        );
    }

    #[test]
    fn the_service_reports_a_connections_provider_and_capabilities() {
        use rocket_environment::ProviderCapabilityLookup;
        let caps = ProviderCapabilities {
            certificates: true,
            ..ProviderCapabilities::default()
        };
        let svc = p2_service(P2Fetcher::new(caps, None));
        let mut c = p2_connection("c1", SecretProviderKind::RocketVault);
        c.base_url = "https://v:8774".to_string();
        c.client_id = "rocketapi".to_string();
        svc.save(c, Some("s".to_string())).expect("save");

        let info = svc.provider_of("c1").expect("lookup").expect("exists");
        assert_eq!(info.kind, SecretProviderKind::RocketVault);
        assert!(info.capabilities.certificates);
        assert!(svc.provider_of("missing").expect("lookup").is_none());
    }

    fn azure_connection(id: &str) -> SecretManagerConnection {
        SecretManagerConnection {
            label: "Prod Azure".to_string(),
            base_url: "https://prod-kv.vault.azure.net".to_string(),
            client_id: "app-id".to_string(),
            provider: SecretProviderKind::Azure,
            config: Some(ProviderConfig::Azure {
                tenant_id: "tenant-1".to_string(),
                authority_host: None,
            }),
            ..sample_connection(id)
        }
    }

    #[test]
    fn save_rejects_changing_the_provider_of_an_existing_connection() {
        let store = Arc::new(FakeSecretStore::new());
        let service = SecretManagerService::new(
            Box::new(FakeRepo::new()),
            Arc::clone(&store) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );
        service
            .save(sample_connection("conn-1"), Some("original".to_string()))
            .expect("initial RocketVault save");

        let result = service.save(azure_connection("conn-1"), Some("attacker".to_string()));

        assert!(
            matches!(result, Err(DomainError::InvalidInput(_))),
            "expected InvalidInput, got {result:?}"
        );
        assert_eq!(
            store.get("vault-connection", "conn-1").expect("get"),
            Some("original".to_string()),
            "a rejected save must not overwrite the stored credential"
        );
        let listed = service.list().expect("list");
        assert_eq!(listed[0].provider, SecretProviderKind::RocketVault);
    }

    #[test]
    fn save_keeps_allowing_edits_that_do_not_change_the_provider() {
        let service = SecretManagerService::new(
            Box::new(FakeRepo::new()),
            Arc::new(FakeSecretStore::new()) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );
        let mut conn = azure_connection("az-1");
        service
            .save(conn.clone(), Some("s".to_string()))
            .expect("first save");
        conn.label = "Renamed".to_string();
        service.save(conn, None).expect("edit keeps the provider");
    }

    #[test]
    fn a_credential_required_non_rocketvault_connection_needs_a_secret_on_save() {
        let service = SecretManagerService::new(
            Box::new(FakeRepo::new()),
            Arc::new(FakeSecretStore::new()) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );

        let result = service.save(azure_connection("az-1"), None);

        assert!(
            matches!(result, Err(DomainError::InvalidInput(_))),
            "got {result:?}"
        );
        assert!(service.list().expect("list").is_empty());
    }

    #[tokio::test]
    async fn a_credential_required_connection_with_no_stored_secret_fails_at_connection_time() {
        let repo = FakeRepo::new();
        repo.save(&azure_connection("az-1"))
            .expect("seed the record directly");
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(FakeSecretStore::new()) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );

        let result = service.test_connection("az-1", "any").await;

        match result {
            Err(DomainError::Internal(msg)) => {
                assert!(msg.contains("no client secret"), "got: {msg}")
            }
            other => panic!("expected an Internal error, got {other:?}"),
        }
    }

    #[test]
    fn an_azure_connection_reaches_certificate_gating_without_certificate_support() {
        let repo = FakeRepo::new();
        repo.save(&azure_connection("az-1"))
            .expect("seed the record");
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(FakeSecretStore::new()) as Arc<dyn SecretStore>,
            Arc::new(FakeFetcher),
        );

        let provider = rocket_environment::ProviderCapabilityLookup::provider_of(&service, "az-1")
            .expect("lookup")
            .expect("the connection exists");

        assert_eq!(provider.kind, SecretProviderKind::Azure);
        assert!(!provider.capabilities.certificates);
    }
}
