use rocket_shared::error::DomainResult;
use serde::{Deserialize, Serialize};

fn default_true() -> bool {
    true
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
}

/// Persistence boundary for `SecretManagerConnection`. No I/O in this crate —
/// `rocket-infra`'s `FsSecretManagerRepo` (Plan 04) implements this.
pub trait SecretManagerRepository: Send + Sync {
    fn list(&self) -> DomainResult<Vec<SecretManagerConnection>>;
    fn get(&self, id: &str) -> DomainResult<Option<SecretManagerConnection>>;
    fn save(&self, connection: &SecretManagerConnection) -> DomainResult<()>;
    fn delete(&self, id: &str) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::{SecretManagerConnection, SecretManagerRepository};
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
        };
        repo.save(&c).expect("save connection");
        assert_eq!(repo.get("conn-1").expect("get connection"), Some(c.clone()));
        assert_eq!(repo.list().expect("list connections").len(), 1);
        repo.delete("conn-1").expect("delete connection");
        assert_eq!(repo.get("conn-1").expect("get after delete"), None);
    }

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: Box<dyn SecretManagerRepository>) {}
    }
}
