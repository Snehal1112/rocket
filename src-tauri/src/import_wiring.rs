//! Wiring for the import commands: the filesystem pieces `ImportService` needs, built
//! in one place. The factory is managed state and holds the shared active-workspace
//! path, so every import goes to the workspace that is active at that moment.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_environment::EnvironmentRepository;
use rocket_import::{EnvironmentRepositoryFactory, ImportService};
use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo};

/// Makes the environment repository of one collection under a workspace.
pub struct FsEnvFactory(PathBuf);

impl EnvironmentRepositoryFactory for FsEnvFactory {
    fn make(&self, collection_name: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::with_secret_store(
            self.0
                .join("collections")
                .join(collection_name)
                .join("environments"),
            crate::env_secret_store(),
        ))
    }
}

/// Builds `ImportService`s over the active workspace.
pub struct ImportServiceFactory {
    active_workspace_path: Arc<Mutex<PathBuf>>,
}

impl ImportServiceFactory {
    pub fn new(active_workspace_path: Arc<Mutex<PathBuf>>) -> Self {
        Self {
            active_workspace_path,
        }
    }

    /// The active workspace folder right now.
    pub fn workspace_path(&self) -> Result<PathBuf, String> {
        self.active_workspace_path
            .lock()
            .map(|path| path.clone())
            .map_err(|_| "workspace path lock poisoned".to_string())
    }

    /// An import service over the workspace at `base`. Take `base` from `workspace_path`
    /// once per command when the command also needs it, so both agree after a switch.
    pub fn service_for(&self, base: PathBuf) -> ImportService {
        ImportService::new(
            base.clone(),
            Box::new(FsCollectionRepo::new_standalone(base.join("collections"))),
            Box::new(FsEnvFactory(base)),
            std::sync::Arc::new(rocket_infra::FsImportFiles),
        )
    }

    /// An import service over the active workspace.
    pub fn service(&self) -> Result<ImportService, String> {
        Ok(self.service_for(self.workspace_path()?))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_path_follows_the_shared_path() {
        let shared = Arc::new(Mutex::new(PathBuf::from("/ws/a")));
        let factory = ImportServiceFactory::new(Arc::clone(&shared));
        assert_eq!(
            factory.workspace_path().expect("path"),
            PathBuf::from("/ws/a")
        );

        *shared.lock().expect("lock") = PathBuf::from("/ws/b");

        assert_eq!(
            factory.workspace_path().expect("path"),
            PathBuf::from("/ws/b")
        );
    }

    #[test]
    fn poisoned_lock_is_an_error_not_a_panic() {
        let shared = Arc::new(Mutex::new(PathBuf::from("/ws/a")));
        let poisoner = Arc::clone(&shared);
        let _ = std::thread::spawn(move || {
            let _guard = poisoner.lock().expect("lock");
            panic!("poison the lock");
        })
        .join();
        let factory = ImportServiceFactory::new(shared);

        assert_eq!(
            factory.workspace_path().err().as_deref(),
            Some("workspace path lock poisoned")
        );
        assert!(factory.service().is_err());
    }

    #[test]
    fn env_factory_points_at_the_collection_environments_folder() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let factory = FsEnvFactory(tmp.path().to_path_buf());
        let repo = factory.make("My API");
        // A new, empty collection has no environments yet.
        assert!(repo.list().expect("list").is_empty());
    }
}
