use std::fmt;

use crate::external_secret::ExternalSecretRef;
use crate::secret_manager::SecretManagerConnection;
use rocket_shared::certificate::VaultCertificateFormat;
use rocket_shared::error::{DomainError, DomainResult};
use zeroize::Zeroizing;

/// One certificate in a vault, as the Certificates tab picker shows it. Names and metadata
/// only, never key material.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VaultCertificateSummary {
    pub id: String,
    pub name: String,
    /// Created exportable over an exportable key. RocketVault never changes this flag.
    pub exportable: bool,
    pub enabled: bool,
    /// As RocketVault reports it, for example `RSA-2048` or `EC-P256`.
    pub key_algorithm: String,
    pub expires_at: Option<String>,
}

/// The exported material of one certificate. It lives in memory only and is wiped on drop. It
/// is deliberately not `Clone` or `Serialize`, and its `Debug` prints sizes only.
pub enum VaultCertificateMaterial {
    /// The certificate chain (leaf first, no root) and an unencrypted PKCS#8 private key, as PEM.
    Pem {
        certificate: Zeroizing<Vec<u8>>,
        private_key: Zeroizing<Vec<u8>>,
        key_algorithm: String,
    },
    /// A PKCS12 bundle and the one-time password it was exported with.
    Pkcs12 {
        bundle: Zeroizing<Vec<u8>>,
        password: Zeroizing<String>,
        key_algorithm: String,
    },
}

impl VaultCertificateMaterial {
    pub fn format(&self) -> VaultCertificateFormat {
        match self {
            VaultCertificateMaterial::Pem { .. } => VaultCertificateFormat::Pem,
            VaultCertificateMaterial::Pkcs12 { .. } => VaultCertificateFormat::Pkcs12,
        }
    }

    pub fn key_algorithm(&self) -> &str {
        match self {
            VaultCertificateMaterial::Pem { key_algorithm, .. }
            | VaultCertificateMaterial::Pkcs12 { key_algorithm, .. } => key_algorithm,
        }
    }
}

/// Prints `<n> bytes` in place of material.
struct ByteCount(usize);

impl fmt::Debug for ByteCount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} bytes", self.0)
    }
}

// Hand-written so a `{:?}` never prints key bytes or the password.
impl fmt::Debug for VaultCertificateMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VaultCertificateMaterial::Pem {
                certificate,
                private_key,
                key_algorithm,
            } => f
                .debug_struct("Pem")
                .field("certificate", &ByteCount(certificate.len()))
                .field("private_key", &ByteCount(private_key.len()))
                .field("key_algorithm", key_algorithm)
                .finish(),
            VaultCertificateMaterial::Pkcs12 {
                bundle,
                key_algorithm,
                ..
            } => f
                .debug_struct("Pkcs12")
                .field("bundle", &ByteCount(bundle.len()))
                .field("password", &"<redacted>")
                .field("key_algorithm", key_algorithm)
                .finish(),
        }
    }
}

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

    /// Lists the certificates in `vault_name`: names and metadata, never key material. Backs
    /// the Certificates tab picker. The default refuses, for fetchers with no certificate
    /// support (test fakes written before certificates existed).
    async fn list_certificates(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<Vec<VaultCertificateSummary>> {
        Err(DomainError::Internal(
            "this vault fetcher cannot list certificates".to_string(),
        ))
    }

    /// Drops anything cached for `connection_id`. Called when the connection is edited or
    /// deleted. The default does nothing, for fetchers that cache nothing.
    fn forget_connection(&self, _connection_id: &str) {}

    /// Exports the certificate named `certificate_name` in `format`. A PKCS12 export uses a
    /// fresh random password, returned with the bundle and never stored. Nothing is cached.
    /// The default refuses, like `list_certificates`.
    async fn fetch_certificate(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
        _certificate_name: &str,
        _format: VaultCertificateFormat,
    ) -> DomainResult<VaultCertificateMaterial> {
        Err(DomainError::Internal(
            "this vault fetcher cannot export certificates".to_string(),
        ))
    }
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

    async fn list_certificates(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
    ) -> DomainResult<Vec<VaultCertificateSummary>> {
        Err(DomainError::Internal(
            "no vault secret fetcher configured".to_string(),
        ))
    }

    async fn fetch_certificate(
        &self,
        _connection: &SecretManagerConnection,
        _client_secret: &str,
        _vault_name: &str,
        _certificate_name: &str,
        _format: VaultCertificateFormat,
    ) -> DomainResult<VaultCertificateMaterial> {
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
            provider: Default::default(),
            config: None,
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

    /// A fetcher written before certificates existed: only the three original methods.
    struct SecretsOnlyFetcher;

    #[async_trait::async_trait]
    impl VaultSecretFetcher for SecretsOnlyFetcher {
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

    #[tokio::test]
    async fn a_fetcher_without_certificate_support_refuses_by_default() {
        let fetcher = SecretsOnlyFetcher;
        let conn = dummy_connection();
        let err = fetcher
            .list_certificates(&conn, "shh", "prod-vault")
            .await
            .expect_err("the default refuses");
        assert!(matches!(err, DomainError::Internal(_)), "{err:?}");
        let err = fetcher
            .fetch_certificate(
                &conn,
                "shh",
                "prod-vault",
                "client-a",
                VaultCertificateFormat::Pem,
            )
            .await
            .expect_err("the default refuses");
        assert!(matches!(err, DomainError::Internal(_)), "{err:?}");
    }

    #[tokio::test]
    async fn null_fetcher_certificate_calls_error() {
        let fetcher = NullVaultSecretFetcher;
        let conn = dummy_connection();
        let expected = DomainError::Internal("no vault secret fetcher configured".to_string());
        assert_eq!(
            fetcher
                .list_certificates(&conn, "shh", "prod-vault")
                .await
                .expect_err("null fetcher must error"),
            expected
        );
        assert_eq!(
            fetcher
                .fetch_certificate(
                    &conn,
                    "shh",
                    "prod-vault",
                    "client-a",
                    VaultCertificateFormat::Pkcs12
                )
                .await
                .expect_err("null fetcher must error"),
            expected
        );
    }

    #[test]
    fn material_debug_prints_sizes_and_never_bytes_or_the_password() {
        let pem = VaultCertificateMaterial::Pem {
            certificate: Zeroizing::new(b"-----BEGIN CERTIFICATE-----\nAAAA\n".to_vec()),
            private_key: Zeroizing::new(b"-----BEGIN PRIVATE KEY-----\nc2VjcmV0\n".to_vec()),
            key_algorithm: "RSA-2048".into(),
        };
        let shown = format!("{pem:?}");
        assert!(
            shown.contains("bytes") && shown.contains("RSA-2048"),
            "{shown}"
        );
        assert!(
            !shown.contains("BEGIN") && !shown.contains("c2VjcmV0"),
            "{shown}"
        );

        let p12 = VaultCertificateMaterial::Pkcs12 {
            bundle: Zeroizing::new(vec![0x30, 0x82, 0x01]),
            password: Zeroizing::new("one-time-pass-123".into()),
            key_algorithm: "EC-P256".into(),
        };
        let shown = format!("{p12:#?}");
        assert!(
            shown.contains("3 bytes") && shown.contains("<redacted>"),
            "{shown}"
        );
        assert!(!shown.contains("one-time-pass-123"), "{shown}");
        // A byte vector would print as `[48, 130, 1]`.
        assert!(!shown.contains('['), "{shown}");
    }

    #[test]
    fn material_reports_its_format_and_key_algorithm() {
        let p12 = VaultCertificateMaterial::Pkcs12 {
            bundle: Zeroizing::new(vec![1]),
            password: Zeroizing::new("p".into()),
            key_algorithm: "EC-P256".into(),
        };
        assert_eq!(p12.format(), VaultCertificateFormat::Pkcs12);
        assert_eq!(p12.key_algorithm(), "EC-P256");
        let pem = VaultCertificateMaterial::Pem {
            certificate: Zeroizing::new(vec![1]),
            private_key: Zeroizing::new(vec![2]),
            key_algorithm: "RSA-2048".into(),
        };
        assert_eq!(pem.format(), VaultCertificateFormat::Pem);
    }
}
