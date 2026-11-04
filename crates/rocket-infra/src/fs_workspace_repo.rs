use std::fs;
use std::path::PathBuf;

use rocket_shared::error::{DomainError, DomainResult};
use rocket_workspace::{WorkspaceRegistry, WorkspaceRepository};

use crate::atomic_write;

pub struct FsWorkspaceRepo {
    registry_path: PathBuf,
    default_workspace_path: PathBuf,
}

impl FsWorkspaceRepo {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            registry_path: app_data_dir.join("workspaces.yml"),
            default_workspace_path: app_data_dir.join("My Workspace"),
        }
    }
}

impl WorkspaceRepository for FsWorkspaceRepo {
    fn load(&self) -> DomainResult<WorkspaceRegistry> {
        if !self.registry_path.exists() {
            fs::create_dir_all(&self.default_workspace_path).map_err(|e| {
                DomainError::Io(format!("Failed to create default workspace dir: {e}"))
            })?;
            let registry = WorkspaceRegistry::new_with_default(self.default_workspace_path.clone());
            self.save(&registry)?;
            return Ok(registry);
        }

        let content = fs::read_to_string(&self.registry_path)
            .map_err(|e| DomainError::Io(format!("Failed to read workspaces.yml: {e}")))?;

        serde_yaml::from_str(&content)
            .map_err(|e| DomainError::InvalidInput(format!("Failed to parse workspaces.yml: {e}")))
    }

    fn save(&self, registry: &WorkspaceRegistry) -> DomainResult<()> {
        if let Some(parent) = self.registry_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| DomainError::Io(format!("Failed to create app data dir: {e}")))?;
        }

        let content = serde_yaml::to_string(registry).map_err(|e| {
            DomainError::InvalidInput(format!("Failed to serialize workspaces.yml: {e}"))
        })?;

        atomic_write(&self.registry_path, content.as_bytes())
            .map_err(|e| DomainError::Io(format!("Failed to write workspaces.yml: {e}")))
    }

    fn ensure_workspace_dirs(&self, path: &std::path::Path) -> DomainResult<()> {
        for subdir in [path, &path.join("collections"), &path.join("environments")] {
            fs::create_dir_all(subdir)
                .map_err(|e| DomainError::Io(format!("Failed to create workspace dir: {e}")))?;
        }
        Ok(())
    }

    fn workspace_config_exists(&self, path: &std::path::Path) -> bool {
        path.join("workspace.yml").exists()
    }

    fn collection_manifest_exists(&self, path: &std::path::Path) -> bool {
        path.join("opencollection.yml").exists()
    }

    fn remove_workspace_dir(&self, path: &std::path::Path) -> DomainResult<()> {
        if !path.exists() {
            return Ok(());
        }
        fs::remove_dir_all(path)
            .map_err(|e| DomainError::Io(format!("Failed to delete workspace directory: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_repo(tmp: &TempDir) -> FsWorkspaceRepo {
        FsWorkspaceRepo::new(tmp.path().to_path_buf())
    }

    #[test]
    fn first_load_creates_default_workspace() {
        let tmp = TempDir::new().unwrap();
        let repo = make_repo(&tmp);
        let registry = repo.load().unwrap();
        assert_eq!(registry.workspaces.len(), 1);
        assert_eq!(registry.workspaces[0].id, "default");
        assert_eq!(registry.workspaces[0].name, "My Workspace");
        assert!(tmp.path().join("workspaces.yml").exists());
        assert!(tmp.path().join("My Workspace").exists());
    }

    #[test]
    fn save_and_reload_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let repo = make_repo(&tmp);
        let mut registry = repo.load().unwrap();
        registry.workspaces.push(rocket_workspace::Workspace::new(
            "My API",
            tmp.path().join("my-api"),
        ));
        repo.save(&registry).unwrap();

        let reloaded = repo.load().unwrap();
        assert_eq!(reloaded.workspaces.len(), 2);
        assert_eq!(reloaded.workspaces[1].name, "My API");
    }

    #[test]
    fn subsequent_loads_read_existing_file() {
        let tmp = TempDir::new().unwrap();
        let repo = make_repo(&tmp);
        repo.load().unwrap(); // creates the file
                              // Second load must read the file, not recreate it.
        let registry = repo.load().unwrap();
        assert_eq!(registry.workspaces.len(), 1);
    }

    #[test]
    fn config_and_manifest_checks_look_for_their_files() {
        let tmp = TempDir::new().expect("tmp");
        let repo = make_repo(&tmp);
        assert!(!repo.workspace_config_exists(tmp.path()));
        assert!(!repo.collection_manifest_exists(tmp.path()));

        std::fs::write(tmp.path().join("workspace.yml"), "name: x\n").expect("write");
        std::fs::write(tmp.path().join("opencollection.yml"), "name: x\n").expect("write");

        assert!(repo.workspace_config_exists(tmp.path()));
        assert!(repo.collection_manifest_exists(tmp.path()));
    }

    #[test]
    fn remove_workspace_dir_deletes_the_tree_and_accepts_a_missing_dir() {
        let tmp = TempDir::new().expect("tmp");
        let repo = make_repo(&tmp);
        let dir = tmp.path().join("ws");
        std::fs::create_dir_all(dir.join("collections")).expect("mkdir");

        repo.remove_workspace_dir(&dir).expect("remove");
        assert!(!dir.exists());

        repo.remove_workspace_dir(&dir).expect("missing dir is ok");
    }
}
