use std::path::PathBuf;
use serde::{Deserialize, Serialize};
use rocket_shared::error::{DomainError, DomainResult};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRegistry {
    pub workspaces: Vec<Workspace>,
    pub active_workspace_id: String,
}

impl Workspace {
    pub fn new(name: &str, path: PathBuf) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            path,
        }
    }

    pub fn validate_name(name: &str) -> DomainResult<()> {
        if name.trim().is_empty() {
            return Err(DomainError::InvalidInput(
                "Workspace name cannot be empty".into(),
            ));
        }
        Ok(())
    }
}

impl WorkspaceRegistry {
    pub fn new_with_default(default_path: PathBuf) -> Self {
        let default = Workspace {
            id: "default".to_string(),
            name: "Default Workspace".to_string(),
            path: default_path,
        };
        Self {
            active_workspace_id: default.id.clone(),
            workspaces: vec![default],
        }
    }

    pub fn find_by_id(&self, id: &str) -> Option<&Workspace> {
        self.workspaces.iter().find(|w| w.id == id)
    }

    pub fn find_by_id_mut(&mut self, id: &str) -> Option<&mut Workspace> {
        self.workspaces.iter_mut().find(|w| w.id == id)
    }

    pub fn active(&self) -> Option<&Workspace> {
        self.find_by_id(&self.active_workspace_id)
    }

    pub fn name_exists(&self, name: &str, exclude_id: Option<&str>) -> bool {
        self.workspaces.iter().any(|w| {
            w.name.to_lowercase() == name.to_lowercase()
                && exclude_id.is_none_or(|id| w.id != id)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn validate_name_rejects_empty() {
        assert!(Workspace::validate_name("").is_err());
        assert!(Workspace::validate_name("   ").is_err());
    }

    #[test]
    fn validate_name_accepts_valid() {
        assert!(Workspace::validate_name("My Workspace").is_ok());
    }

    #[test]
    fn new_with_default_sets_id() {
        let reg = WorkspaceRegistry::new_with_default(PathBuf::from("/tmp/default"));
        assert_eq!(reg.workspaces.len(), 1);
        assert_eq!(reg.workspaces[0].id, "default");
        assert_eq!(reg.active_workspace_id, "default");
    }

    #[test]
    fn name_exists_case_insensitive() {
        let reg = WorkspaceRegistry::new_with_default(PathBuf::from("/tmp/default"));
        assert!(reg.name_exists("default workspace", None));
        assert!(!reg.name_exists("default workspace", Some("default")));
    }

    #[test]
    fn find_by_id_returns_none_for_missing() {
        let reg = WorkspaceRegistry::new_with_default(PathBuf::from("/tmp/default"));
        assert!(reg.find_by_id("nonexistent").is_none());
    }
}
