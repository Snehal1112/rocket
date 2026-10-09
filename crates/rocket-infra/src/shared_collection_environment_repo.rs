use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_environment::secret_store::SecretStore;
use rocket_environment::{EnvironmentRepository, EnvironmentRepositoryFactory};

use crate::FsEnvironmentRepo;

/// Builds a short-lived `FsEnvironmentRepo` scoped to
/// `<workspace>/collections/<collection>/environments`, resolving the
/// workspace path from a shared handle at call time so a workspace switch
/// takes effect without rebuilding the service graph — mirrors
/// `SharedPathCollectionRepo`.
///
/// `secret_store` is `None` when built via `new()`, which makes every
/// `FsEnvironmentRepo` this factory hands out silently drop `secret: true`
/// variable values on save (`FsEnvironmentRepo::new`'s own documented
/// behavior) — fine for a caller that only ever reads/writes non-secret
/// variables through this factory, but a data-loss hazard for one that does
/// a full read-modify-write over an environment that may also hold secrets.
/// Use `with_secret_store` for that case (see its doc comment).
pub struct SharedCollectionEnvironmentRepo {
    active_workspace_path: Arc<Mutex<PathBuf>>,
    secret_store: Option<Arc<dyn SecretStore>>,
}

impl SharedCollectionEnvironmentRepo {
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>) -> Self {
        Self {
            active_workspace_path,
            secret_store: None,
        }
    }

    /// Same as `new`, but every `FsEnvironmentRepo` handed out is backed by
    /// `secret_store` (see `FsEnvironmentRepo::with_secret_store`), so a
    /// save preserves any existing `secret: true` variable's value instead
    /// of silently dropping it. Required for any caller that does a
    /// read-modify-write over a whole environment — e.g.
    /// `McpToolService::set_env_var` — since `src-tauri/src/commands/
    /// environments.rs::env_service_for` already establishes this exact
    /// pattern for the equivalent single-collection case.
    pub fn with_secret_store(
        active_workspace_path: Arc<Mutex<PathBuf>>,
        secret_store: Arc<dyn SecretStore>,
    ) -> Self {
        Self {
            active_workspace_path,
            secret_store: Some(secret_store),
        }
    }
}

impl EnvironmentRepositoryFactory for SharedCollectionEnvironmentRepo {
    fn for_collection(&self, collection: &str) -> Box<dyn EnvironmentRepository> {
        let base = self
            .active_workspace_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .join("collections")
            .join(collection)
            .join("environments");
        match &self.secret_store {
            Some(store) => Box::new(FsEnvironmentRepo::with_secret_store(
                base,
                Arc::clone(store),
            )),
            None => Box::new(FsEnvironmentRepo::new(base)),
        }
    }

    fn collection_dir(&self, collection: &str) -> Option<PathBuf> {
        Some(
            self.active_workspace_path
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .join("collections")
                .join(collection),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collection_dir_is_the_parent_of_the_environments_dir() {
        let repo = SharedCollectionEnvironmentRepo::new(Arc::new(Mutex::new(PathBuf::from("/ws"))));
        assert_eq!(
            repo.collection_dir("api"),
            Some(PathBuf::from("/ws/collections/api"))
        );
    }

    use rocket_environment::{Environment, Variable};
    use rocket_shared::error::DomainResult;
    use std::collections::HashMap;
    use std::error::Error;
    use tempfile::TempDir;

    /// In-memory `SecretStore` double, keyed by `(scope_id, key)`. Tests must
    /// never touch a real OS keychain — mirrors `fs_environment_repo.rs`'s
    /// own private `InMemorySecretStore` (not reusable here — it is scoped to
    /// that file's own `#[cfg(test)] mod tests`).
    #[derive(Default)]
    struct InMemorySecretStore {
        values: Mutex<HashMap<(String, String), String>>,
    }
    impl SecretStore for InMemorySecretStore {
        fn get(&self, scope_id: &str, key: &str) -> DomainResult<Option<String>> {
            Ok(self
                .values
                .lock()
                .expect("lock")
                .get(&(scope_id.to_string(), key.to_string()))
                .cloned())
        }
        fn set(&self, scope_id: &str, key: &str, value: &str) -> DomainResult<()> {
            self.values
                .lock()
                .expect("lock")
                .insert((scope_id.to_string(), key.to_string()), value.to_string());
            Ok(())
        }
        fn delete(&self, scope_id: &str, key: &str) -> DomainResult<()> {
            self.values
                .lock()
                .expect("lock")
                .remove(&(scope_id.to_string(), key.to_string()));
            Ok(())
        }
    }

    #[test]
    fn with_secret_store_preserves_a_secret_value_across_an_unrelated_save(
    ) -> Result<(), Box<dyn Error>> {
        // Regression coverage for the data-loss hazard `with_secret_store`
        // exists to avoid: `new()`'s `NullSecretStore`-backed repo silently
        // drops `secret: true` values on save, which would permanently erase
        // a legacy secret the first time an unrelated key is written
        // (`McpToolService::set_env_var`'s read-modify-write, for example).
        let tmp = TempDir::new()?;
        let ws_path = Arc::new(Mutex::new(tmp.path().to_path_buf()));
        let store: Arc<dyn SecretStore> = Arc::new(InMemorySecretStore::default());
        let factory = SharedCollectionEnvironmentRepo::with_secret_store(ws_path, Arc::clone(&store));

        let mut env = Environment::new("dev");
        let mut secret_var = Variable::new("API_KEY", "sk-live-abc");
        secret_var.secret = true;
        env.set_variable(secret_var);
        env.set_variable(Variable::new("HOST", "api.example.com"));
        factory.for_collection("my-collection").save(&env)?;

        // Read-modify-write an unrelated, non-secret key, exactly the shape
        // `McpToolService::set_env_var` does.
        let repo = factory.for_collection("my-collection");
        let mut reloaded = repo.get("dev")?;
        reloaded
            .variables
            .iter_mut()
            .find(|v| v.key == "HOST")
            .expect("HOST variable")
            .value = "api2.example.com".to_string();
        repo.save(&reloaded)?;

        let final_env = factory.for_collection("my-collection").get("dev")?;
        assert_eq!(final_env.get_value("HOST"), Some("api2.example.com"));
        assert_eq!(
            final_env
                .variables
                .iter()
                .find(|v| v.key == "API_KEY")
                .map(|v| v.value.as_str()),
            Some("sk-live-abc"),
            "the secret value must survive an unrelated key's save"
        );
        Ok(())
    }

    #[test]
    fn resolves_environment_under_the_collection_directory() -> Result<(), Box<dyn Error>> {
        let tmp = TempDir::new()?;
        let ws_path = Arc::new(Mutex::new(tmp.path().to_path_buf()));
        let factory = SharedCollectionEnvironmentRepo::new(ws_path);

        let repo = factory.for_collection("my-collection");
        let mut env = Environment::new("local");
        env.set_variable(Variable::new("token", "abc123"));
        repo.save(&env)?;

        assert!(tmp
            .path()
            .join("collections/my-collection/environments/local.yml")
            .exists());

        let fetched = factory.for_collection("my-collection").get("local")?;
        assert_eq!(fetched.get_value("token"), Some("abc123"));
        Ok(())
    }

    #[test]
    fn different_collections_are_isolated() -> Result<(), Box<dyn Error>> {
        let tmp = TempDir::new()?;
        let ws_path = Arc::new(Mutex::new(tmp.path().to_path_buf()));
        let factory = SharedCollectionEnvironmentRepo::new(ws_path);

        let mut env = Environment::new("local");
        env.set_variable(Variable::new("token", "abc123"));
        factory.for_collection("collection-a").save(&env)?;

        let result = factory.for_collection("collection-b").get("local");
        assert!(result.is_err());
        Ok(())
    }

    #[test]
    fn variable_deduplication_across_two_independent_get_save_cycles() -> Result<(), Box<dyn Error>>
    {
        // Set up: create a factory over a temp directory.
        let tmp = TempDir::new()?;
        let ws_path = Arc::new(Mutex::new(tmp.path().to_path_buf()));
        let factory = SharedCollectionEnvironmentRepo::new(ws_path);

        // Step 1: Save an environment with an existing HOST variable (no token yet).
        let mut env = Environment::new("local");
        env.set_variable(Variable::new("HOST", "old-host"));
        factory.for_collection("my-collection").save(&env)?;

        // Step 2: Simulate script run #1 - fetch, add token=v1, save.
        let repo1 = factory.for_collection("my-collection");
        let mut env1 = repo1.get("local")?;
        env1.set_variable(Variable::new("token", "v1"));
        repo1.save(&env1)?;

        // Step 3: Simulate script run #2 (fresh get→modify→save cycle).
        // This uses a FRESH factory.for_collection() call, just like a second 'Send' click.
        let repo2 = factory.for_collection("my-collection");
        let mut env2 = repo2.get("local")?;
        env2.set_variable(Variable::new("token", "v2"));
        repo2.save(&env2)?;

        // Step 4: Verify the final state - exactly one "token" variable exists with value "v2".
        let final_repo = factory.for_collection("my-collection");
        let final_env = final_repo.get("local")?;

        let token_vars: Vec<_> = final_env
            .variables
            .iter()
            .filter(|v| v.key == "token")
            .collect();

        assert_eq!(
            token_vars.len(),
            1,
            "Expected exactly one 'token' variable, found {}",
            token_vars.len()
        );
        assert_eq!(
            final_env.get_value("token"),
            Some("v2"),
            "Expected token value to be 'v2'"
        );

        // Verify the other variable is still intact.
        assert_eq!(
            final_env.get_value("HOST"),
            Some("old-host"),
            "Expected HOST to remain 'old-host'"
        );

        Ok(())
    }
}
