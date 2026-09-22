use rocket_environment::secret_manager::{SecretManagerConnection, SecretManagerRepository};
use rocket_environment::secret_store::SecretStore;
use rocket_environment::vault_secret_fetcher::VaultSecretFetcher;
use rocket_shared::error::DomainResult;
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
}
