use crate::external_secret::ExternalSecretRef;
use crate::secret_manager::SecretManagerConnection;
use rocket_shared::error::{DomainError, DomainResult};

/// Fetches secret names and values from a RocketVault server.
///
/// Takes `connection`/`client_secret`/`vault_name` as call arguments rather
/// than being constructed bound to one connection. One injected
/// `Arc<dyn VaultSecretFetcher>` instance serves every configured
/// `SecretManagerConnection` in the app, exactly the way `HttpExecutor`
/// (`crates/rocket-http/src/executor.rs`) serves every request regardless of
/// target host — a single `ReqwestExecutor` handles requests to any URL, it
/// is never rebuilt per host. This keeps `rocket-app` free of any
/// RocketVault-specific concrete type, per this repo's DDD boundary rule
/// (rocket-app: trait-first, no infra concrete coupling). A per-connection
/// struct would instead force whatever wires the trait object to either hold
/// one fetcher instance per configured connection or reconstruct one on
/// every call — both push infra concerns upward across the exact crate
/// boundary this trait exists to prevent.
#[async_trait::async_trait]
pub trait VaultSecretFetcher: Send + Sync {
    /// Lists secret *names* (never values) visible in `vault_name` through
    /// `connection`. Backs the "Fetch Secrets" action (spec §4.1/§4.5).
    async fn list_secrets(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<Vec<ExternalSecretRef>>;

    /// Fetches one secret's value by its vault-assigned `secret_id`.
    /// Returns `Ok(None)` if the id is no longer present in the vault (a
    /// stale binding, per spec §4.6), not an error — only a genuine
    /// transport/auth failure is an `Err` here.
    async fn get_secret_value(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
        secret_id: &str,
    ) -> DomainResult<Option<String>>;

    /// Verifies `connection`/`client_secret` can authenticate and reach
    /// `vault_name`, without fetching or returning any secret data. Backs
    /// the connection form's "Test Connection" action.
    async fn test_connection(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<()>;
}

/// No-op fetcher for tests and contexts with no RocketVault client wired —
/// mirrors `NullSecretStore` (`crate::secret_store`). Every method fails
/// loudly instead of returning empty data: an empty secret list or a silent
/// `Ok(None)` here could be mistaken for "this vault really has no secrets"
/// rather than "no fetcher is configured", so this type always surfaces the
/// misconfiguration as an error.
pub struct NullVaultSecretFetcher;

#[async_trait::async_trait]
impl VaultSecretFetcher for NullVaultSecretFetcher {
    async fn list_secrets(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<Vec<ExternalSecretRef>> {
        Err(DomainError::Internal(
            "no vault secret fetcher configured".to_string(),
        ))
    }

    async fn get_secret_value(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
        _secret_id: &str,
    ) -> DomainResult<Option<String>> {
        Err(DomainError::Internal(
            "no vault secret fetcher configured".to_string(),
        ))
    }

    async fn test_connection(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<()> {
        Err(DomainError::Internal(
            "no vault secret fetcher configured".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_connection() -> SecretManagerConnection {
        SecretManagerConnection {
            id: "conn-1".to_string(),
            label: "Test Vault".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
        }
    }

    #[test]
    fn trait_is_object_safe() {
        // Arc, not Box: Plan 05's SecretManagerService holds
        // `Arc<dyn VaultSecretFetcher>` (see plan index), so this is the
        // shape that actually matters downstream.
        fn _assert(_: std::sync::Arc<dyn VaultSecretFetcher>) {}
    }

    #[tokio::test]
    async fn null_fetcher_list_secrets_errors() {
        let fetcher = NullVaultSecretFetcher;
        let err = fetcher
            .list_secrets(&dummy_connection(), "shh", "prod-vault")
            .await
            .expect_err("null fetcher must error on list_secrets");
        assert_eq!(
            err,
            DomainError::Internal("no vault secret fetcher configured".to_string())
        );
    }

    #[tokio::test]
    async fn null_fetcher_get_secret_value_errors() {
        let fetcher = NullVaultSecretFetcher;
        let err = fetcher
            .get_secret_value(&dummy_connection(), "shh", "prod-vault", "secret-id-1")
            .await
            .expect_err("null fetcher must error on get_secret_value");
        assert_eq!(
            err,
            DomainError::Internal("no vault secret fetcher configured".to_string())
        );
    }

    #[tokio::test]
    async fn null_fetcher_test_connection_errors() {
        let fetcher = NullVaultSecretFetcher;
        let err = fetcher
            .test_connection(&dummy_connection(), "shh", "prod-vault")
            .await
            .expect_err("null fetcher must error on test_connection");
        assert_eq!(
            err,
            DomainError::Internal("no vault secret fetcher configured".to_string())
        );
    }
}
