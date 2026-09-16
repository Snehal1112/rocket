use rocket_shared::error::DomainResult;

/// Backend for the real value of a `secret: true` Variable. Never touches YAML.
///
/// `scope_id` uniquely identifies the environment file a secret belongs to, so
/// two environments that share a variable name never share a stored secret.
/// Implementations live in `rocket-infra` — this crate does no I/O.
pub trait SecretStore: Send + Sync {
    fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>>;
    fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()>;
    fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()>;
}

/// No-op store for tests and contexts with no keychain, such as headless CI.
/// Reads always miss, writes are discarded.
pub struct NullSecretStore;

impl SecretStore for NullSecretStore {
    fn get(&self, _scope_id: &str, _key: &str) -> DomainResult<Option<String>> {
        Ok(None)
    }

    fn set(&self, _scope_id: &str, _key: &str, _value: &str) -> DomainResult<()> {
        Ok(())
    }

    fn delete(&self, _scope_id: &str, _key: &str) -> DomainResult<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trait_is_object_safe() {
        fn _assert(_: std::sync::Arc<dyn SecretStore>) {}
    }

    #[test]
    fn null_store_never_returns_a_value() {
        let store = NullSecretStore;
        store.set("scope", "API_KEY", "sk-live-123").expect("null set");
        assert_eq!(store.get("scope", "API_KEY").expect("null get"), None);
    }

    #[test]
    fn null_store_delete_is_ok() {
        assert!(NullSecretStore.delete("scope", "API_KEY").is_ok());
    }
}
