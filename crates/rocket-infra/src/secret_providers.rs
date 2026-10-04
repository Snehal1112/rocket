//! Routes secret-manager calls to the implementation for a connection's provider.

use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use rocket_environment::{
    ExternalSecretRef, ProviderCapabilities, SecretManagerConnection, SecretProviderKind,
    VaultCertificateMaterial, VaultCertificateSummary, VaultSecretFetcher,
};
use rocket_shared::certificate::VaultCertificateFormat;
use rocket_shared::error::{DomainError, DomainResult};

use crate::azurekeyvault::AzureKeyVaultFetcher;
use crate::rocketvault::ReqwestVaultSecretFetcher;

/// One `VaultSecretFetcher` that holds an implementation per provider and
/// routes every call on `connection.provider`. It is wired once at startup, so
/// `rocket-app` keeps holding a single trait object for every connection.
#[derive(Default)]
pub struct DispatchingSecretFetcher {
    providers: HashMap<SecretProviderKind, Arc<dyn VaultSecretFetcher>>,
}

impl DispatchingSecretFetcher {
    pub fn new() -> Self {
        Self::default()
    }

    /// A dispatcher with every provider this build ships registered.
    pub fn with_providers() -> Self {
        let mut dispatcher = Self::new();
        dispatcher.register(
            SecretProviderKind::RocketVault,
            Arc::new(ReqwestVaultSecretFetcher::new()),
        );
        dispatcher.register(
            SecretProviderKind::Azure,
            Arc::new(AzureKeyVaultFetcher::new()),
        );
        dispatcher
    }

    pub fn register(&mut self, kind: SecretProviderKind, fetcher: Arc<dyn VaultSecretFetcher>) {
        self.providers.insert(kind, fetcher);
    }

    fn provider_for(
        &self,
        connection: &SecretManagerConnection,
    ) -> DomainResult<&Arc<dyn VaultSecretFetcher>> {
        self.providers.get(&connection.provider).ok_or_else(|| {
            DomainError::InvalidInput(format!(
                "{} is not available in this build of Rocket",
                connection.provider.display_name()
            ))
        })
    }
}

#[async_trait]
impl VaultSecretFetcher for DispatchingSecretFetcher {
    async fn list_secrets(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<Vec<ExternalSecretRef>> {
        self.provider_for(connection)?
            .list_secrets(connection, client_secret, vault_name)
            .await
    }

    async fn get_secret_value(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
        secret_id: &str,
    ) -> DomainResult<Option<String>> {
        self.provider_for(connection)?
            .get_secret_value(connection, client_secret, vault_name, secret_id)
            .await
    }

    async fn test_connection(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<()> {
        self.provider_for(connection)?
            .test_connection(connection, client_secret, vault_name)
            .await
    }

    async fn list_certificates(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
    ) -> DomainResult<Vec<VaultCertificateSummary>> {
        self.provider_for(connection)?
            .list_certificates(connection, client_secret, vault_name)
            .await
    }

    async fn fetch_certificate(
        &self,
        connection: &SecretManagerConnection,
        client_secret: &str,
        vault_name: &str,
        certificate_name: &str,
        format: VaultCertificateFormat,
    ) -> DomainResult<VaultCertificateMaterial> {
        self.provider_for(connection)?
            .fetch_certificate(
                connection,
                client_secret,
                vault_name,
                certificate_name,
                format,
            )
            .await
    }

    fn forget_connection(&self, connection_id: &str) {
        // Only the id is known here, so every provider is told. A provider
        // that never cached anything for it does nothing.
        for provider in self.providers.values() {
            provider.forget_connection(connection_id);
        }
    }

    fn capabilities(&self, connection: &SecretManagerConnection) -> ProviderCapabilities {
        self.providers
            .get(&connection.provider)
            .map(|provider| provider.capabilities(connection))
            .unwrap_or_default()
    }

    fn validate_connection(&self, connection: &SecretManagerConnection) -> DomainResult<()> {
        self.provider_for(connection)?
            .validate_connection(connection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use rocket_environment::{ExternalSecretRef, SecretManagerConnection};
    use rocket_shared::error::DomainResult;
    use std::sync::Mutex;

    /// Records which connection ids it was asked to forget and answers a fixed value.
    struct RecordingProvider {
        value: &'static str,
        forgotten: Mutex<Vec<String>>,
        caps: ProviderCapabilities,
    }

    impl RecordingProvider {
        fn new(value: &'static str, caps: ProviderCapabilities) -> Arc<Self> {
            Arc::new(Self {
                value,
                forgotten: Mutex::new(Vec::new()),
                caps,
            })
        }
    }

    #[async_trait]
    impl VaultSecretFetcher for RecordingProvider {
        async fn list_secrets(
            &self,
            _c: &SecretManagerConnection,
            _s: &str,
            _v: &str,
        ) -> DomainResult<Vec<ExternalSecretRef>> {
            Ok(vec![ExternalSecretRef {
                name: self.value.to_string(),
                secret_id: self.value.to_string(),
            }])
        }
        async fn get_secret_value(
            &self,
            _c: &SecretManagerConnection,
            _s: &str,
            _v: &str,
            _id: &str,
        ) -> DomainResult<Option<String>> {
            Ok(Some(self.value.to_string()))
        }
        async fn test_connection(
            &self,
            _c: &SecretManagerConnection,
            _s: &str,
            _v: &str,
        ) -> DomainResult<()> {
            Ok(())
        }
        fn forget_connection(&self, id: &str) {
            self.forgotten
                .lock()
                .expect("lock forgotten")
                .push(id.to_string());
        }
        fn capabilities(&self, _c: &SecretManagerConnection) -> ProviderCapabilities {
            self.caps
        }
    }

    fn conn(provider: SecretProviderKind) -> SecretManagerConnection {
        SecretManagerConnection {
            id: "c1".to_string(),
            label: "L".to_string(),
            base_url: String::new(),
            client_id: String::new(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider,
            config: None,
        }
    }

    #[tokio::test]
    async fn routes_each_call_to_the_connections_provider() {
        let mut d = DispatchingSecretFetcher::new();
        d.register(
            SecretProviderKind::RocketVault,
            RecordingProvider::new("from-rv", ProviderCapabilities::default()),
        );
        d.register(
            SecretProviderKind::Azure,
            RecordingProvider::new("from-azure", ProviderCapabilities::default()),
        );

        let rv = d
            .get_secret_value(&conn(SecretProviderKind::RocketVault), "s", "v", "id")
            .await
            .expect("rocketvault routes");
        let az = d
            .get_secret_value(&conn(SecretProviderKind::Azure), "s", "v", "id")
            .await
            .expect("azure routes");

        assert_eq!(rv.as_deref(), Some("from-rv"));
        assert_eq!(az.as_deref(), Some("from-azure"));
    }

    #[tokio::test]
    async fn an_unregistered_provider_errors_and_names_it_without_falling_back() {
        let mut d = DispatchingSecretFetcher::new();
        d.register(
            SecretProviderKind::RocketVault,
            RecordingProvider::new("from-rv", ProviderCapabilities::default()),
        );
        let aws = conn(SecretProviderKind::Aws);

        let err = d
            .list_secrets(&aws, "s", "v")
            .await
            .expect_err("aws is not registered");
        assert!(
            err.to_string().contains("AWS Secrets Manager"),
            "got: {err}"
        );

        let err = d
            .test_connection(&aws, "s", "v")
            .await
            .expect_err("aws is not registered");
        assert!(
            err.to_string().contains("AWS Secrets Manager"),
            "got: {err}"
        );

        let err = d
            .validate_connection(&aws)
            .expect_err("aws is not registered");
        assert!(
            err.to_string().contains("AWS Secrets Manager"),
            "got: {err}"
        );
    }

    #[test]
    fn capabilities_come_from_the_connections_provider_and_default_when_unregistered() {
        let mut d = DispatchingSecretFetcher::new();
        let caps = ProviderCapabilities {
            certificates: false,
            credential_optional: true,
            fetch_on_reference: true,
        };
        d.register(SecretProviderKind::Azure, RecordingProvider::new("x", caps));

        assert_eq!(d.capabilities(&conn(SecretProviderKind::Azure)), caps);
        assert_eq!(
            d.capabilities(&conn(SecretProviderKind::Gcp)),
            ProviderCapabilities::default()
        );
    }

    #[test]
    fn forget_connection_reaches_every_registered_provider() {
        let rv = RecordingProvider::new("a", ProviderCapabilities::default());
        let az = RecordingProvider::new("b", ProviderCapabilities::default());
        let mut d = DispatchingSecretFetcher::new();
        d.register(SecretProviderKind::RocketVault, rv.clone());
        d.register(SecretProviderKind::Azure, az.clone());

        d.forget_connection("c1");

        assert_eq!(rv.forgotten.lock().expect("lock").as_slice(), ["c1"]);
        assert_eq!(az.forgotten.lock().expect("lock").as_slice(), ["c1"]);
    }

    #[test]
    fn with_providers_reports_certificate_support_for_rocketvault_only() {
        let d = DispatchingSecretFetcher::with_providers();
        assert!(
            d.capabilities(&conn(SecretProviderKind::RocketVault))
                .certificates
        );
        assert!(
            !d.capabilities(&conn(SecretProviderKind::Azure))
                .certificates
        );
    }

    fn azure_connection() -> SecretManagerConnection {
        SecretManagerConnection {
            id: "az-1".to_string(),
            label: "Azure".to_string(),
            base_url: "https://kv.vault.azure.net".to_string(),
            client_id: "app-id".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: SecretProviderKind::Azure,
            config: Some(rocket_environment::ProviderConfig::Azure {
                tenant_id: "tenant-1".to_string(),
                authority_host: None,
            }),
        }
    }

    #[test]
    fn with_providers_registers_azure() {
        let d = DispatchingSecretFetcher::with_providers();
        let conn = azure_connection();

        assert!(d.validate_connection(&conn).is_ok());
        assert!(!d.capabilities(&conn).certificates);
    }

    #[test]
    fn with_providers_still_refuses_providers_that_do_not_exist_yet() {
        let d = DispatchingSecretFetcher::with_providers();
        let mut conn = azure_connection();
        conn.provider = SecretProviderKind::Aws;
        conn.config = None;

        let err = d.validate_connection(&conn).expect_err("AWS is not built");

        assert!(
            err.to_string().contains("not available in this build"),
            "got: {err}"
        );
    }
}
