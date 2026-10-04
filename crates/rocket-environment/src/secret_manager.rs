use crate::vault_secret_fetcher::ProviderCapabilities;
use rocket_shared::error::DomainResult;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
}

/// Which kind of secret manager a connection talks to. Rows written before
/// providers existed have no `provider` key and load as `RocketVault`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SecretProviderKind {
    #[default]
    RocketVault,
    Azure,
    Aws,
    Hashicorp,
    Gcp,
}

impl SecretProviderKind {
    /// True for the value a row gets when the key is absent. Used to keep
    /// RocketVault rows byte-identical to the format older builds read.
    pub fn is_default(&self) -> bool {
        *self == Self::RocketVault
    }

    /// The name shown to users and used in error messages.
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::RocketVault => "RocketVault",
            Self::Azure => "Azure Key Vault",
            Self::Aws => "AWS Secrets Manager",
            Self::Hashicorp => "HashiCorp Vault",
            Self::Gcp => "Google Secret Manager",
        }
    }
}

/// Typed, non-secret settings for one provider (tenant, region, project and
/// so on). Each provider's own spec adds its variant. Persisted as a serde
/// YAML tag, and never renamed to camelCase: IPC uses its own DTO.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProviderConfig {
    /// Azure AD service principal settings. The vault URL is the connection's
    /// `base_url` and the app registration id is its `client_id`.
    Azure {
        tenant_id: String,
        /// Overrides `https://login.microsoftonline.com`. Used by tests.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        authority_host: Option<String>,
    },
}

/// An app-level, reusable connection to a RocketVault server. Persisted
/// separately from any workspace/environment (see Plan 04's
/// `FsSecretManagerRepo`) — the actual `client_secret` never lives on this
/// struct or in its persisted form; it is stored only in the OS keychain,
/// looked up by `id` (see Plan 04/05).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SecretManagerConnection {
    pub id: String,
    pub label: String,
    pub base_url: String,
    pub client_id: String,
    #[serde(default = "default_true")]
    pub verify_ssl: bool,
    #[serde(default)]
    pub allow_insecure_http: bool,
    #[serde(default, skip_serializing_if = "SecretProviderKind::is_default")]
    pub provider: SecretProviderKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub config: Option<ProviderConfig>,
}

/// Persistence boundary for `SecretManagerConnection`. No I/O in this crate —
/// `rocket-infra`'s `FsSecretManagerRepo` (Plan 04) implements this.
pub trait SecretManagerRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<SecretManagerConnection>>;
    fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>>;
    fn save(&self, connection: &SecretManagerConnection) -> DomainResult<()>;
    fn delete(&self, id: &str) -> DomainResult<()>;
}

/// A connection's provider and what that provider supports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectionProvider {
    pub kind: SecretProviderKind,
    pub capabilities: ProviderCapabilities,
}

/// Answers "which provider does this connection id use and what can it do".
/// Lets save-time validation reject a certificate that a connection's provider
/// cannot supply, without the validator holding connections itself.
pub trait ProviderCapabilityLookup: Send + Sync {
    /// `Ok(None)` means no connection has this id.
    fn provider_of(&self, connection_id: &str) -> DomainResult<Option<ConnectionProvider>>;
}

#[cfg(test)]
mod tests {
    use super::{ProviderConfig, SecretManagerConnection, SecretManagerRepository, SecretProviderKind};
    use rocket_shared::error::DomainResult;
    use std::sync::Mutex;

    #[test]
    fn connection_serde_roundtrip_no_camelcase() {
        let c = SecretManagerConnection {
            id: "conn-1".to_string(),
            label: "Prod RocketVault".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: SecretProviderKind::RocketVault,
            config: None,
        };
        let json = serde_json::to_string(&c).expect("serialize SecretManagerConnection");
        // Deliberately NOT camelCase — plain field names, see Global Constraints.
        assert!(
            json.contains("\"base_url\""),
            "expected snake_case field, got: {json}"
        );
        assert!(
            json.contains("\"client_id\""),
            "expected snake_case field, got: {json}"
        );
        let back: SecretManagerConnection =
            serde_json::from_str(&json).expect("deserialize SecretManagerConnection");
        assert_eq!(c, back);
    }

    #[test]
    fn connection_verify_ssl_defaults_true_when_absent() {
        let json = r#"{"id":"c1","label":"L","base_url":"https://x","client_id":"cid"}"#;
        let c: SecretManagerConnection =
            serde_json::from_str(json).expect("deserialize minimal connection");
        assert!(c.verify_ssl, "verify_ssl should default to true for safety");
        assert!(!c.allow_insecure_http);
    }

    // In-memory fake exercising the trait contract — mirrors the style of
    // inline mocks already used throughout rocket-app's own tests.
    struct FakeRepo(Mutex<Vec<SecretManagerConnection>>);
    impl SecretManagerRepository for FakeRepo {
        fn list(&self) -> DomainResult<Vec<SecretManagerConnection>> {
            let guard = self.0.lock().expect("lock FakeRepo");
            Ok(guard.clone())
        }
        fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>> {
            let guard = self.0.lock().expect("lock FakeRepo");
            Ok(guard.iter().find(|c| c.id == id).cloned())
        }
        fn save(&self, connection: &SecretManagerConnection) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeRepo");
            guard.retain(|c| c.id != connection.id);
            guard.push(connection.clone());
            Ok(())
        }
        fn delete(&self, id: &str) -> DomainResult<()> {
            let mut guard = self.0.lock().expect("lock FakeRepo");
            guard.retain(|c| c.id != id);
            Ok(())
        }
    }

    #[test]
    fn repository_trait_save_get_delete_roundtrip() {
        let repo = FakeRepo(Mutex::new(Vec::new()));
        let c = SecretManagerConnection {
            id: "conn-1".to_string(),
            label: "Prod".to_string(),
            base_url: "https://vault.internal:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: SecretProviderKind::RocketVault,
            config: None,
        };
        repo.save(&c).expect("save connection");
        assert_eq!(repo.get("conn-1").expect("get connection"), Some(c.clone()));
        assert_eq!(repo.list().expect("list connections").len(), 1);
        repo.delete("conn-1").expect("delete connection");
        assert_eq!(repo.get("conn-1").expect("get after delete"), None);
    }

    #[test]
    fn provider_defaults_to_rocketvault_when_absent_from_yaml() {
        let yaml = "id: c1\nlabel: Prod\nbase_url: https://v:8774\nclient_id: rocketapi\n";
        let c: SecretManagerConnection = serde_yaml::from_str(yaml).expect("old row parses");
        assert_eq!(c.provider, SecretProviderKind::RocketVault);
        assert!(c.config.is_none());
    }

    #[test]
    fn rocketvault_connection_serializes_without_provider_or_config() {
        let c = SecretManagerConnection {
            id: "c1".to_string(),
            label: "Prod".to_string(),
            base_url: "https://v:8774".to_string(),
            client_id: "rocketapi".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: SecretProviderKind::RocketVault,
            config: None,
        };
        let yaml = serde_yaml::to_string(&c).expect("serialize");
        assert!(
            !yaml.contains("provider"),
            "an older build must read this: {yaml}"
        );
        assert!(
            !yaml.contains("config"),
            "an older build must read this: {yaml}"
        );
    }

    #[test]
    fn non_default_provider_is_serialized_lowercase() {
        let c = SecretManagerConnection {
            id: "c2".to_string(),
            label: "Azure".to_string(),
            base_url: String::new(),
            client_id: String::new(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: SecretProviderKind::Azure,
            config: None,
        };
        let yaml = serde_yaml::to_string(&c).expect("serialize");
        assert!(yaml.contains("provider: azure"), "got: {yaml}");
        let back: SecretManagerConnection = serde_yaml::from_str(&yaml).expect("round trip");
        assert_eq!(back, c);
    }

    fn azure_connection() -> SecretManagerConnection {
        SecretManagerConnection {
            id: "az-1".to_string(),
            label: "Prod Azure".to_string(),
            base_url: "https://prod-kv.vault.azure.net".to_string(),
            client_id: "11111111-1111-1111-1111-111111111111".to_string(),
            verify_ssl: true,
            allow_insecure_http: false,
            provider: SecretProviderKind::Azure,
            config: Some(ProviderConfig::Azure {
                tenant_id: "22222222-2222-2222-2222-222222222222".to_string(),
                authority_host: None,
            }),
        }
    }

    #[test]
    fn azure_config_round_trips_through_yaml() {
        let c = azure_connection();
        let yaml = serde_yaml::to_string(&c).expect("serialize");
        assert!(yaml.contains("provider: azure"), "got: {yaml}");
        assert!(yaml.contains("tenant_id"), "persistence keeps snake_case: {yaml}");
        assert!(
            !yaml.contains("authority_host"),
            "an unset authority host is not written: {yaml}"
        );
        let back: SecretManagerConnection = serde_yaml::from_str(&yaml).expect("round trip");
        assert_eq!(back, c);
    }

    #[test]
    fn azure_config_keeps_an_authority_host_override() {
        let mut c = azure_connection();
        c.config = Some(ProviderConfig::Azure {
            tenant_id: "t".to_string(),
            authority_host: Some("http://127.0.0.1:9999".to_string()),
        });
        let yaml = serde_yaml::to_string(&c).expect("serialize");
        let back: SecretManagerConnection = serde_yaml::from_str(&yaml).expect("round trip");
        assert_eq!(back, c);
    }

    #[test]
    fn azure_config_without_a_tenant_does_not_load() {
        let yaml = "id: c1\nlabel: X\nbase_url: https://v\nclient_id: a\nprovider: azure\nconfig: !Azure {}\n";
        let err = serde_yaml::from_str::<SecretManagerConnection>(yaml)
            .expect_err("a missing tenant must not default to empty");
        assert!(err.to_string().contains("tenant_id"), "got: {err}");
    }

    #[test]
    fn unknown_provider_is_rejected_and_named() {
        let yaml = "id: c1\nlabel: X\nprovider: bogus\n";
        let err = serde_yaml::from_str::<SecretManagerConnection>(yaml)
            .expect_err("unknown provider must not load");
        assert!(err.to_string().contains("bogus"), "got: {err}");
    }

    #[test]
    fn provider_display_names() {
        assert_eq!(
            SecretProviderKind::RocketVault.display_name(),
            "RocketVault"
        );
        assert_eq!(SecretProviderKind::Azure.display_name(), "Azure Key Vault");
        assert_eq!(
            SecretProviderKind::Aws.display_name(),
            "AWS Secrets Manager"
        );
        assert_eq!(
            SecretProviderKind::Hashicorp.display_name(),
            "HashiCorp Vault"
        );
        assert_eq!(
            SecretProviderKind::Gcp.display_name(),
            "Google Secret Manager"
        );
    }

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn SecretManagerRepository>) {}
    }
}
