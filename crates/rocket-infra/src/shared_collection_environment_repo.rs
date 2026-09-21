use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_environment::{EnvironmentRepository, EnvironmentRepositoryFactory};

use crate::FsEnvironmentRepo;

/// Builds a short-lived `FsEnvironmentRepo` scoped to
/// `<workspace>/collections/<collection>/environments`, resolving the
/// workspace path from a shared handle at call time so a workspace switch
/// takes effect without rebuilding the service graph — mirrors
/// `SharedPathCollectionRepo`.
pub struct SharedCollectionEnvironmentRepo {
    active_workspace_path: Arc<Mutex<PathBuf>>,
}

impl SharedCollectionEnvironmentRepo {
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>) -> Self {
        Self {
            active_workspace_path,
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
        Box::new(FsEnvironmentRepo::new(base))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::{Environment, Variable};
    use std::error::Error;
    use tempfile::TempDir;

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
