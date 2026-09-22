//! OS-keychain backed storage for environment secret values.
//! This is the only place environment secrets touch the keychain.

use rocket_environment::secret_store::SecretStore;
use rocket_shared::error::{DomainError, DomainResult};

/// Stores secret values in the OS-native secret store: macOS Keychain,
/// Windows Credential Manager, or the Linux Secret Service. Configurable by
/// keychain service label so one implementation serves multiple secret
/// namespaces (environment variable secrets, RocketVault connection client
/// secrets) without their entries colliding.
pub struct KeyringSecretStore {
    service: &'static str,
}

impl KeyringSecretStore {
    /// Backs environment variable secret values. Uses the exact service
    /// string this type used before it was generalized to take a
    /// configurable label, so existing keychain entries keep resolving
    /// unchanged.
    pub fn new_env_secrets() -> Self {
        Self {
            service: "com.rocketapi.env-secrets",
        }
    }

    /// Backs RocketVault connection client secrets (Plan 05's
    /// `SecretManagerService`, scope_id = "vault-connection", key =
    /// connection id). A distinct keychain service label from
    /// `new_env_secrets()` so the two features can never share or clobber
    /// each other's entries.
    pub fn new_vault_connections() -> Self {
        Self {
            service: "com.rocketapi.vault-connection",
        }
    }
}

impl SecretStore for KeyringSecretStore {
    fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
        let entry = keyring::Entry::new(self.service, &account(scope_id, key))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        match entry.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            // A locked or unavailable keychain must not hard-fail an environment
            // load, which would brick app startup. Treat it as a miss.
            Err(e) => {
                tracing::warn!(error = %e, "keychain unavailable, environment secret unreadable");
                Ok(None)
            }
        }
    }

    fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()> {
        let entry = keyring::Entry::new(self.service, &account(scope_id, key))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        entry
            .set_password(value)
            .map_err(|e| DomainError::Internal(e.to_string()))
    }

    fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()> {
        let entry = keyring::Entry::new(self.service, &account(scope_id, key))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(DomainError::Internal(e.to_string())),
        }
    }
}

/// Keychain account name for one secret. `scope_id` already encodes the
/// environment file, so this only has to append the variable key.
fn account(scope_id: &str, key: &str) -> String {
    format!("{scope_id}:{key}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn account_namespaces_scope_and_key() {
        assert_eq!(
            account("a1b2c3d4e5f60718:prod", "API_KEY"),
            "a1b2c3d4e5f60718:prod:API_KEY"
        );
    }

    #[test]
    fn service_name_is_distinct_from_git_credentials() {
        // Git credentials use "rocket-api" (src-tauri/src/commands/git.rs:185).
        // Sharing a service name would let one feature clobber the other's entries.
        let store = KeyringSecretStore::new_env_secrets();
        assert_ne!(store.service, "rocket-api");
        assert_eq!(store.service, "com.rocketapi.env-secrets");
    }

    #[test]
    fn distinct_keychain_namespaces_for_env_and_vault_connections() {
        let env_store = KeyringSecretStore::new_env_secrets();
        let vault_store = KeyringSecretStore::new_vault_connections();
        // Two genuinely distinct keychain service labels — sharing one would let
        // an environment-secret entry collide with a vault-connection entry (or
        // vice versa) if the same scope_id/key pair were ever reused across
        // features. No real OS keychain is touched here; this only inspects the
        // struct's own field, the same way `service_name_is_distinct_from_git_credentials`
        // below checks a string constant without touching a keychain.
        assert_ne!(env_store.service, vault_store.service);
        assert_eq!(env_store.service, "com.rocketapi.env-secrets");
        assert_eq!(vault_store.service, "com.rocketapi.vault-connection");
    }

    // Real-keychain coverage, ignored by default: CI has no Secret Service or
    // Keychain daemon, so a failure here is an environment problem rather than
    // a code problem. Run locally with:
    //   cargo test -p rocket-infra keyring_ -- --ignored
    #[test]
    #[ignore = "requires a real OS keychain"]
    fn keyring_set_get_delete_roundtrip() {
        let store = KeyringSecretStore::new_env_secrets();
        let scope = "rocket-infra-test-scope";
        store.set(scope, "TEST_KEY", "sk-live-123").expect("set");
        assert_eq!(
            store.get(scope, "TEST_KEY").expect("get"),
            Some("sk-live-123".to_string())
        );
        store.delete(scope, "TEST_KEY").expect("delete");
        assert_eq!(
            store.get(scope, "TEST_KEY").expect("get after delete"),
            None
        );
    }

    #[test]
    #[ignore = "requires a real OS keychain"]
    fn keyring_delete_of_missing_entry_is_ok() {
        assert!(KeyringSecretStore::new_env_secrets()
            .delete("rocket-infra-test-scope", "NO_SUCH_KEY")
            .is_ok());
    }
}
