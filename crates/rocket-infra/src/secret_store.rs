//! OS-keychain backed storage for environment secret values.
//! This is the only place environment secrets touch the keychain.

use rocket_environment::secret_store::SecretStore;
use rocket_shared::error::{DomainError, DomainResult};

/// Keychain service namespace for environment secrets. Deliberately distinct
/// from the git-credential service name ("rocket-api").
const KEYRING_SERVICE: &str = "com.rocketapi.env-secrets";

/// Stores secret values in the OS-native secret store: macOS Keychain,
/// Windows Credential Manager, or the Linux Secret Service.
pub struct KeyringSecretStore;

impl SecretStore for KeyringSecretStore {
    fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &account(scope_id, key))
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
        let entry = keyring::Entry::new(KEYRING_SERVICE, &account(scope_id, key))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        entry
            .set_password(value)
            .map_err(|e| DomainError::Internal(e.to_string()))
    }

    fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, &account(scope_id, key))
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
        assert_eq!(account("a1b2c3d4e5f60718:prod", "API_KEY"), "a1b2c3d4e5f60718:prod:API_KEY");
    }

    #[test]
    fn service_name_is_distinct_from_git_credentials() {
        // Git credentials use "rocket-api" (src-tauri/src/commands/git.rs:185).
        // Sharing a service name would let one feature clobber the other's entries.
        assert_ne!(KEYRING_SERVICE, "rocket-api");
        assert_eq!(KEYRING_SERVICE, "com.rocketapi.env-secrets");
    }

    // Real-keychain coverage, ignored by default: CI has no Secret Service or
    // Keychain daemon, so a failure here is an environment problem rather than
    // a code problem. Run locally with:
    //   cargo test -p rocket-infra keyring_ -- --ignored
    #[test]
    #[ignore = "requires a real OS keychain"]
    fn keyring_set_get_delete_roundtrip() {
        let store = KeyringSecretStore;
        let scope = "rocket-infra-test-scope";
        store.set(scope, "TEST_KEY", "sk-live-123").expect("set");
        assert_eq!(store.get(scope, "TEST_KEY").expect("get"), Some("sk-live-123".to_string()));
        store.delete(scope, "TEST_KEY").expect("delete");
        assert_eq!(store.get(scope, "TEST_KEY").expect("get after delete"), None);
    }

    #[test]
    #[ignore = "requires a real OS keychain"]
    fn keyring_delete_of_missing_entry_is_ok() {
        assert!(KeyringSecretStore.delete("rocket-infra-test-scope", "NO_SUCH_KEY").is_ok());
    }
}
