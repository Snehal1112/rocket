use rocket_environment::secret_manager::SecretManagerRepository;
use rocket_environment::secret_store::SecretStore;
use rocket_environment::vault_secret_fetcher::VaultSecretFetcher;
use rocket_shared::error::{DomainError, DomainResult};

const VAULT_CONNECTION_SCOPE: &str = "vault-connection";

/// Resolves one `{{alias.secretName}}` binding to its live value by looking
/// up the connection, its stored client_secret, then calling the fetcher.
/// Shared between `SecretManagerService`-adjacent callers (this plan) and
/// `RequestExecutionService` (Plan 06) so both go through one code path
/// rather than duplicating this three-step lookup.
///
/// A fetcher `Ok(None)` (the secret was deleted from the vault after the
/// binding was created) passes through as `Ok(None)` unchanged — this
/// function does not decide whether that is a hard failure. Per spec §4.6,
/// that decision belongs to Plan 06's caller, which knows whether it is
/// resolving for a live request (hard-fail) or a background/advisory check.
pub async fn resolve_vault_secret_value(
    repo: &dyn SecretManagerRepository,
    secret_store: &dyn SecretStore,
    fetcher: &dyn VaultSecretFetcher,
    connection_id: &str,
    vault_name: &str,
    secret_id: &str,
) -> DomainResult<Option<String>> {
    let connection = repo
        .get(connection_id)?
        .ok_or_else(|| DomainError::NotFound(connection_id.to_string()))?;
    let client_secret = secret_store
        .get(VAULT_CONNECTION_SCOPE, connection_id)?
        .ok_or_else(|| {
            DomainError::Internal("connection has no stored client secret".to_string())
        })?;
    fetcher
        .get_secret_value(&connection, &client_secret, vault_name, secret_id)
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::external_secret::ExternalSecretRef;
    use rocket_environment::secret_manager::SecretManagerConnection;
    use std::sync::Mutex;

    struct FakeRepo(Mutex<Vec<SecretManagerConnection>>);

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

    struct FakeSecretStore(Mutex<std::collections::HashMap<(String, String), String>>);

    impl SecretStore for FakeSecretStore {
        fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
            Ok(self
                .0
                .lock()
                .expect("lock FakeSecretStore")
                .get(&(scope_id.to_string(), key.to_string()))
                .cloned())
        }
        fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()> {
            self.0.lock().expect("lock FakeSecretStore").insert(
                (scope_id.to_string(), key.to_string()),
                value.to_string(),
            );
            Ok(())
        }
        fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()> {
            self.0
                .lock()
                .expect("lock FakeSecretStore")
                .remove(&(scope_id.to_string(), key.to_string()));
            Ok(())
        }
    }

    struct FakeFetcher {
        value_result: DomainResult<Option<String>>,
    }

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
            match &self.value_result {
                Ok(value) => Ok(value.clone()),
                Err(DomainError::Internal(msg)) => Err(DomainError::Internal(msg.clone())),
                Err(other) => Err(DomainError::Internal(other.to_string())),
            }
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

    #[tokio::test]
    async fn successful_resolution_returns_the_value() {
        let repo = FakeRepo(Mutex::new(vec![sample_connection("conn-1")]));
        let store = FakeSecretStore(Mutex::new(std::collections::HashMap::new()));
        store
            .set("vault-connection", "conn-1", "shh-its-a-secret")
            .expect("seed keychain entry");
        let fetcher = FakeFetcher {
            value_result: Ok(Some("sk-live-abc123".to_string())),
        };

        let result = resolve_vault_secret_value(
            &repo,
            &store,
            &fetcher,
            "conn-1",
            "prod-vault",
            "secret-id-1",
        )
        .await
        .expect("resolve_vault_secret_value should succeed");

        assert_eq!(result, Some("sk-live-abc123".to_string()));
    }

    #[tokio::test]
    async fn missing_connection_errors() {
        let repo = FakeRepo(Mutex::new(Vec::new()));
        let store = FakeSecretStore(Mutex::new(std::collections::HashMap::new()));
        let fetcher = FakeFetcher {
            value_result: Ok(Some("unused".to_string())),
        };

        let result = resolve_vault_secret_value(
            &repo,
            &store,
            &fetcher,
            "no-such-conn",
            "prod-vault",
            "secret-id-1",
        )
        .await;

        assert!(
            matches!(result, Err(DomainError::NotFound(_))),
            "expected DomainError::NotFound for a missing connection, got {result:?}"
        );
    }

    #[tokio::test]
    async fn missing_stored_secret_errors() {
        let repo = FakeRepo(Mutex::new(vec![sample_connection("conn-1")]));
        let store = FakeSecretStore(Mutex::new(std::collections::HashMap::new())); // no keychain entry
        let fetcher = FakeFetcher {
            value_result: Ok(Some("unused".to_string())),
        };

        let result = resolve_vault_secret_value(
            &repo,
            &store,
            &fetcher,
            "conn-1",
            "prod-vault",
            "secret-id-1",
        )
        .await;

        assert!(
            matches!(result, Err(DomainError::Internal(_))),
            "expected DomainError::Internal for a connection with no stored secret, got {result:?}"
        );
    }

    #[tokio::test]
    async fn fetcher_returning_none_passes_through_unchanged() {
        let repo = FakeRepo(Mutex::new(vec![sample_connection("conn-1")]));
        let store = FakeSecretStore(Mutex::new(std::collections::HashMap::new()));
        store
            .set("vault-connection", "conn-1", "shh-its-a-secret")
            .expect("seed keychain entry");
        let fetcher = FakeFetcher { value_result: Ok(None) };

        let result = resolve_vault_secret_value(
            &repo,
            &store,
            &fetcher,
            "conn-1",
            "prod-vault",
            "deleted-secret-id",
        )
        .await
        .expect("a fetcher Ok(None) must not be turned into an error by this helper");

        assert_eq!(
            result, None,
            "Ok(None) from the fetcher (secret deleted from the vault) must pass through as Ok(None), not an error"
        );
    }
}
