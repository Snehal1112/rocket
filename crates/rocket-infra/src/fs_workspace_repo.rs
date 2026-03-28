use std::fs;
use std::path::PathBuf;

use rocket_shared::error::{DomainError, DomainResult};
use rocket_workspace::{WorkspaceRegistry, WorkspaceRepository};

pub struct FsWorkspaceRepo {
    registry_path: PathBuf,
    default_workspace_path: PathBuf,
}

impl FsWorkspaceRepo {
    pub fn new(app_data_dir: PathBuf) -> Self {
        Self {
            registry_path: app_data_dir.join("workspaces.yml"),
            default_workspace_path: app_data_dir.join("Default Workspace"),
        }
    }
}

impl WorkspaceRepository for FsWorkspaceRepo {
    fn load(&self) -> DomainResult<WorkspaceRegistry> {
        if !self.registry_path.exists() {
            fs::create_dir_all(&self.default_workspace_path).map_err(|e| {
                DomainError::Io(format!("Failed to create default workspace dir: {e}"))
            })?;
            let registry =
                WorkspaceRegistry::new_with_default(self.default_workspace_path.clone());
            self.save(&registry)?;
            return Ok(registry);
        }

        let content = fs::read_to_string(&self.registry_path).map_err(|e| {
            DomainError::Io(format!("Failed to read workspaces.yml: {e}"))
        })?;

        serde_yaml::from_str(&content).map_err(|e| {
            DomainError::InvalidInput(format!("Failed to parse workspaces.yml: {e}"))
        })
    }

    fn save(&self, registry: &WorkspaceRegistry) -> DomainResult<()> {
        if let Some(parent) = self.registry_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                DomainError::Io(format!("Failed to create app data dir: {e}"))
            })?;
        }

        let content = serde_yaml::to_string(registry).map_err(|e| {
            DomainError::InvalidInput(format!("Failed to serialize workspaces.yml: {e}"))
        })?;

        fs::write(&self.registry_path, content).map_err(|e| {
            DomainError::Io(format!("Failed to write workspaces.yml: {e}"))
        })
    }
}
