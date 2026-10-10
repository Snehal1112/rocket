use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_environment::secret_store::SecretStore;
use rocket_environment::{Environment, EnvironmentRepository};
use rocket_shared::error::DomainResult;

use crate::FsEnvironmentRepo;

/// The workspace-level ("global") environments in `<active workspace>/environments`.
///
/// Every call builds a short-lived `FsEnvironmentRepo` from the shared active
/// workspace path, like `SharedPathCollectionRepo`, so a workspace switch takes
/// effect on the next call. Secret values keep their keychain scope, because
/// that scope is derived from the environments directory.
///
/// A read-modify-write should go through `pinned()`, so both halves use the
/// same workspace even if the user switches in between.
pub struct SharedPathEnvironmentRepo {
    active_workspace_path: Arc<Mutex<PathBuf>>,
    secret_store: Arc<dyn SecretStore>,
}

impl SharedPathEnvironmentRepo {
    pub fn with_secret_store(
        active_workspace_path: Arc<Mutex<PathBuf>>,
        secret_store: Arc<dyn SecretStore>,
    ) -> Self {
        Self {
            active_workspace_path,
            secret_store,
        }
    }

    /// The repository for the workspace that is active right now.
    fn current(&self) -> FsEnvironmentRepo {
        let dir = self
            .active_workspace_path
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .join("environments");
        FsEnvironmentRepo::with_secret_store(dir, Arc::clone(&self.secret_store))
    }
}

impl EnvironmentRepository for SharedPathEnvironmentRepo {
    fn list(&self) -> DomainResult<Vec<Environment>> {
        self.current().list()
    }

    fn get(&self, name: &str) -> DomainResult<Environment> {
        self.current().get(name)
    }

    fn save(&self, env: &Environment) -> DomainResult<()> {
        self.current().save(env)
    }

    fn delete(&self, name: &str) -> DomainResult<()> {
        self.current().delete(name)
    }

    fn pinned(&self) -> Option<Box<dyn EnvironmentRepository>> {
        Some(Box::new(self.current()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::secret_store::NullSecretStore;
    use rocket_environment::Variable;

    fn env(name: &str, key: &str, value: &str) -> Environment {
        let mut env = Environment::new(name);
        env.set_variable(Variable::new(key, value));
        env
    }

    #[test]
    fn reads_and_writes_follow_the_active_workspace() {
        let a = tempfile::tempdir().expect("tempdir");
        let b = tempfile::tempdir().expect("tempdir");
        let path = Arc::new(Mutex::new(a.path().to_path_buf()));
        let repo = SharedPathEnvironmentRepo::with_secret_store(
            Arc::clone(&path),
            Arc::new(NullSecretStore),
        );

        repo.save(&env("dev", "host", "a.test")).expect("save in a");
        assert!(a.path().join("environments/dev.yml").exists());

        *path.lock().expect("lock") = b.path().to_path_buf();
        assert!(repo.list().expect("list in b").is_empty());
        assert!(
            repo.get("dev").is_err(),
            "a's environment must not show in b"
        );
        repo.save(&env("dev", "host", "b.test")).expect("save in b");
        assert!(b.path().join("environments/dev.yml").exists());
    }

    #[test]
    fn switching_back_and_forth_does_not_mix_data() {
        let a = tempfile::tempdir().expect("tempdir");
        let b = tempfile::tempdir().expect("tempdir");
        let path = Arc::new(Mutex::new(a.path().to_path_buf()));
        let repo = SharedPathEnvironmentRepo::with_secret_store(
            Arc::clone(&path),
            Arc::new(NullSecretStore),
        );
        repo.save(&env("dev", "host", "a.test")).expect("save in a");
        *path.lock().expect("lock") = b.path().to_path_buf();
        repo.save(&env("dev", "host", "b.test")).expect("save in b");
        *path.lock().expect("lock") = a.path().to_path_buf();
        assert_eq!(
            repo.get("dev").expect("a").get_value("host"),
            Some("a.test")
        );
        *path.lock().expect("lock") = b.path().to_path_buf();
        assert_eq!(
            repo.get("dev").expect("b").get_value("host"),
            Some("b.test")
        );
    }

    #[test]
    fn a_pinned_repository_stays_on_its_workspace_after_a_switch() {
        let a = tempfile::tempdir().expect("tempdir");
        let b = tempfile::tempdir().expect("tempdir");
        let path = Arc::new(Mutex::new(a.path().to_path_buf()));
        let repo = SharedPathEnvironmentRepo::with_secret_store(
            Arc::clone(&path),
            Arc::new(NullSecretStore),
        );
        repo.save(&env("dev", "host", "a.test")).expect("save in a");

        let pinned = repo.pinned().expect("a shared repository pins");
        let mut read = pinned.get("dev").expect("read in a");
        *path.lock().expect("lock") = b.path().to_path_buf();
        read.set_variable(Variable::new("token", "t"));
        pinned.save(&read).expect("write back");

        assert!(!b.path().join("environments/dev.yml").exists());
        *path.lock().expect("lock") = a.path().to_path_buf();
        assert_eq!(repo.get("dev").expect("a").get_value("token"), Some("t"));
    }
}
