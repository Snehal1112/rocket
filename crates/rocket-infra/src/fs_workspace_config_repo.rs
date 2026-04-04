use std::fs;
use std::path::Path;

use rocket_shared::error::{DomainError, DomainResult};
use rocket_workspace::{WorkspaceConfig, WorkspaceConfigRepository};

use crate::opencollection::OcWorkspaceConfig;

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

        // Try new format (has info.name block — OcWorkspaceConfig)
        if let Ok(oc) = serde_yaml::from_str::<OcWorkspaceConfig>(&content) {
            return Ok(WorkspaceConfig::from(oc));
        }
        // Fall back to old format (flat name: at root)
        serde_yaml::from_str::<WorkspaceConfig>(&content).map_err(|e| {
            DomainError::InvalidInput(format!("Failed to parse workspace.yml: {e}"))
        })
    }

    fn save(&self, workspace_path: &Path, config: &WorkspaceConfig) -> DomainResult<()> {
        fs::create_dir_all(workspace_path).map_err(|e| {
            DomainError::Io(format!("Failed to create workspace directory: {e}"))
        })?;

        let config_path = workspace_path.join("workspace.yml");
        let oc = OcWorkspaceConfig::from(config.clone());
        let content = serde_yaml::to_string(&oc).map_err(|e| {
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

    #[test]
    fn save_writes_new_format() {
        let tmp = TempDir::new().unwrap();
        let ws_path = tmp.path().join("my-api");
        let repo = FsWorkspaceConfigRepo::new();
        let mut cfg = WorkspaceConfig::new("My API");
        cfg.description = Some("Great docs".into());
        repo.save(&ws_path, &cfg).unwrap();

        let raw = fs::read_to_string(ws_path.join("workspace.yml")).unwrap();
        assert!(raw.contains("opencollection:"), "missing opencollection header:\n{raw}");
        assert!(raw.contains("info:"), "missing info block:\n{raw}");
        assert!(
            raw.contains("name: My API") || raw.contains("name: \"My API\""),
            "missing info.name:\n{raw}"
        );
        assert!(raw.contains("docs:"), "missing docs field:\n{raw}");
        assert!(!raw.contains("\nname: "), "unexpected root-level name:\n{raw}");
        assert!(!raw.contains("description:"), "description field should be renamed to docs:\n{raw}");
    }

    #[test]
    fn load_new_format_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let ws_path = tmp.path().join("ws");
        let repo = FsWorkspaceConfigRepo::new();
        let mut cfg = WorkspaceConfig::new("Roundtrip WS");
        cfg.description = Some("My desc".into());
        cfg.add_embedded_collection("my-api");
        repo.save(&ws_path, &cfg).unwrap();

        let loaded = repo.load(&ws_path).unwrap();
        assert_eq!(loaded.name, "Roundtrip WS");
        assert_eq!(loaded.description.as_deref(), Some("My desc"));
        assert_eq!(loaded.collections.len(), 1);
        assert_eq!(loaded.collections[0].name, "my-api");
        assert_eq!(loaded.collections[0].ref_type, rocket_workspace::CollectionRefType::Embedded);
    }

    #[test]
    fn load_new_format_external_collection_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let ws_path = tmp.path().join("ext-ws");
        let repo = FsWorkspaceConfigRepo::new();
        let mut cfg = WorkspaceConfig::new("Ext WS");
        cfg.add_external_collection("shared", std::path::PathBuf::from("/abs/path/to/shared"));
        repo.save(&ws_path, &cfg).unwrap();

        let loaded = repo.load(&ws_path).unwrap();
        assert_eq!(loaded.collections.len(), 1);
        assert_eq!(loaded.collections[0].ref_type, rocket_workspace::CollectionRefType::External);
        assert_eq!(loaded.collections[0].path, Some(std::path::PathBuf::from("/abs/path/to/shared")));
    }

    #[test]
    fn load_old_format_backward_compat() {
        let tmp = TempDir::new().unwrap();
        let ws_path = tmp.path().join("old-ws");
        fs::create_dir_all(&ws_path).unwrap();
        let old_yaml = "name: Old Workspace\ndescription: Legacy\ncollections:\n- name: users\n  type: embedded\n";
        fs::write(ws_path.join("workspace.yml"), old_yaml).unwrap();

        let repo = FsWorkspaceConfigRepo::new();
        let cfg = repo.load(&ws_path).unwrap();
        assert_eq!(cfg.name, "Old Workspace");
        assert_eq!(cfg.description.as_deref(), Some("Legacy"));
        assert_eq!(cfg.collections.len(), 1);
    }
}
