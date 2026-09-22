use rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository};
use rocket_environment::secret_store::SecretStore;
use rocket_environment::vault_secret_fetcher::VaultSecretFetcher;
use rocket_shared::error::{DomainError, DomainResult};
use std::sync::Arc;

/// scope_id under which every vault connection's client_secret is stored in
/// the injected SecretStore. All connections share this one scope because
/// `key` (the connection's `id`) already uniquely identifies each one within
/// it — see this plan's Global Constraints for why that differs from
/// environment-secret scoping.
const VAULT_CONNECTION_SCOPE: &str = "vault-connection";

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
    /// entirely and only the connection record is saved.
    pub fn save(
        &self,
        connection: SecretManagerConnection,
        client_secret: Option<String>,
    ) -> DomainResult<()> {
        if let Some(secret) = client_secret {
            self.secret_store
                .set(VAULT_CONNECTION_SCOPE, &connection.id, &secret)?;
        }
        self.repo.save(&connection)
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
        if let Err(err) = self.secret_store.delete(VAULT_CONNECTION_SCOPE, id) {
            tracing::warn!(error = %err, id = %id, "failed to delete keychain entry for vault connection");
        }
        Ok(())
    }

    async fn connection_and_secret(
        &self,
        id: &str,
    ) -> DomainResult<(SecretManagerConnection, String)> {
        let connection = self
            .repo
            .get(id)?
            .ok_or_else(|| DomainError::NotFound(id.to_string()))?;
        let secret = self
            .secret_store
            .get(VAULT_CONNECTION_SCOPE, id)?
            .ok_or_else(|| {
                DomainError::Internal("connection has no stored client secret".to_string())
            })?;
        Ok((connection, secret))
    }

    pub async fn test_connection(&self, id: &str, vault_name: &str) -> DomainResult<()> {
        let (connection, secret) = self.connection_and_secret(id).await?;
        self.fetcher
            .test_connection(&connection, &secret, vault_name)
            .await
    }

    pub async fn fetch_secret_names(
        &self,
        id: &str,
        vault_name: &str,
    ) -> DomainResult<Vec<rocket_environment::external_secret::ExternalSecretRef>> {
        let (connection, secret) = self.connection_and_secret(id).await?;
        self.fetcher.list_secrets(&connection, &secret, vault_name).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::external_secret::ExternalSecretRef;
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
            self.entries.lock().expect("lock FakeSecretStore").insert(
                (scope_id.to_string(), key.to_string()),
                value.to_string(),
            );
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
            Ok(None)
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
        service.save(conn.clone(), None).expect("edit without resecret");

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

    #[test]
    fn keychain_set_failure_during_save_prevents_connection_persistence() {
        let repo = FakeRepo::new();
        let store = FakeSecretStore::new();
        store.fail_next_set();
        let service = SecretManagerService::new(
            Box::new(repo),
            Arc::new(store),
            Arc::new(FakeFetcher),
        );
        let conn = sample_connection("conn-1");

        let result = service.save(conn, Some("shh-its-a-secret".to_string()));

        assert!(result.is_err(), "expected keychain failure to surface as an error");
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
            }),
        );

        let result = service.fetch_secret_names("no-such-conn", "prod-vault").await;

        assert!(
            matches!(result, Err(DomainError::NotFound(_))),
            "expected DomainError::NotFound for an unknown connection id, got {result:?}"
        );
    }
}
