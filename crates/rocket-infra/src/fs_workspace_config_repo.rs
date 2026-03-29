use std::fs;
use std::path::Path;

use rocket_shared::error::{DomainError, DomainResult};
use rocket_workspace::{WorkspaceConfig, WorkspaceConfigRepository};

/// Filesystem implementation of `WorkspaceConfigRepository`.
/// Reads and writes `workspace.yml` inside each workspace directory.
pub struct FsWorkspaceConfigRepo;

impl FsWorkspaceConfigRepo {
    pub fn new() -> Self {
        Self
    }
}

impl Default for FsWorkspaceConfigRepo {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceConfigRepository for FsWorkspaceConfigRepo {
    fn load(&self, workspace_path: &Path) -> DomainResult<WorkspaceConfig> {
        let config_path = workspace_path.join("workspace.yml");
        if !config_path.exists() {
            let name = workspace_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "Untitled".into());
            return Ok(WorkspaceConfig::new(name));
        }

        let content = fs::read_to_string(&config_path).map_err(|e| {
            DomainError::Io(format!("Failed to read workspace.yml: {e}"))
        })?;

        serde_yaml::from_str(&content).map_err(|e| {
            DomainError::InvalidInput(format!("Failed to parse workspace.yml: {e}"))
        })
    }

    fn save(&self, workspace_path: &Path, config: &WorkspaceConfig) -> DomainResult<()> {
        fs::create_dir_all(workspace_path).map_err(|e| {
            DomainError::Io(format!("Failed to create workspace directory: {e}"))
        })?;

        let config_path = workspace_path.join("workspace.yml");
        let content = serde_yaml::to_string(config).map_err(|e| {
            DomainError::InvalidInput(format!("Failed to serialize workspace.yml: {e}"))
        })?;

        fs::write(&config_path, content).map_err(|e| {
            DomainError::Io(format!("Failed to write workspace.yml: {e}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn load_returns_default_when_no_file_exists() {
        let tmp = TempDir::new().unwrap();
        let ws_path = tmp.path().join("my-project");
        fs::create_dir_all(&ws_path).unwrap();
        let repo = FsWorkspaceConfigRepo::new();
        let config = repo.load(&ws_path).unwrap();
        assert_eq!(config.name, "my-project");
        assert!(config.collections.is_empty());
    }

    #[test]
    fn save_and_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let ws_path = tmp.path().join("test-ws");
        let repo = FsWorkspaceConfigRepo::new();
        let mut config = WorkspaceConfig::new("Test Workspace");
        config.description = Some("A test".to_string());
        config.add_embedded_collection("Users API");
        repo.save(&ws_path, &config).unwrap();
        assert!(ws_path.join("workspace.yml").exists());
        let loaded = repo.load(&ws_path).unwrap();
        assert_eq!(loaded.name, "Test Workspace");
        assert_eq!(loaded.description, Some("A test".to_string()));
        assert_eq!(loaded.collections.len(), 1);
    }

    #[test]
    fn save_creates_directory_if_missing() {
        let tmp = TempDir::new().unwrap();
        let ws_path = tmp.path().join("new-ws");
        let repo = FsWorkspaceConfigRepo::new();
        repo.save(&ws_path, &WorkspaceConfig::new("New")).unwrap();
        assert!(ws_path.join("workspace.yml").exists());
    }

    #[test]
    fn load_invalid_yaml_returns_error() {
        let tmp = TempDir::new().unwrap();
        let ws_path = tmp.path().join("bad-ws");
        fs::create_dir_all(&ws_path).unwrap();
        fs::write(ws_path.join("workspace.yml"), "{{{{invalid").unwrap();
        let repo = FsWorkspaceConfigRepo::new();
        assert!(repo.load(&ws_path).is_err());
    }
}
