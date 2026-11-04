use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::validation::non_empty_name;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    pub path: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceRegistry {
    pub workspaces: Vec<Workspace>,
    pub active_workspace_id: String,
    #[serde(default)]
    pub multi_workspace_mode: bool,
}

impl Workspace {
    pub fn new(name: &str, path: PathBuf) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            path,
            description: None,
            pinned: false,
        }
    }

    pub fn validate_name(name: &str) -> DomainResult<()> {
        non_empty_name(name, "Workspace name cannot be empty")
    }
}

impl WorkspaceRegistry {
    pub fn new_with_default(default_path: PathBuf) -> Self {
        let default = Workspace {
            id: "default".to_string(),
            name: "My Workspace".to_string(),
            path: default_path,
            description: None,
            pinned: true,
        };
        Self {
            active_workspace_id: default.id.clone(),
            workspaces: vec![default],
            multi_workspace_mode: false,
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
            w.name.to_lowercase() == name.to_lowercase() && exclude_id.is_none_or(|id| w.id != id)
        })
    }

    /// Checks that a workspace with this name and path could be added: the path must not be
    /// registered yet, and the name must not be taken in any letter case. A caller runs it
    /// before it writes any file, so a refusal leaves nothing behind.
    pub fn ensure_can_add(&self, name: &str, path: &Path) -> DomainResult<()> {
        if self.workspaces.iter().any(|w| w.path == path) {
            return Err(DomainError::AlreadyExists(
                "This workspace is already open".into(),
            ));
        }
        if self.name_exists(name, None) {
            return Err(DomainError::AlreadyExists(name.into()));
        }
        Ok(())
    }

    /// Adds a workspace after `ensure_can_add`. A refusal leaves the registry unchanged.
    pub fn add(&mut self, workspace: Workspace) -> DomainResult<()> {
        self.ensure_can_add(&workspace.name, &workspace.path)?;
        self.workspaces.push(workspace);
        Ok(())
    }

    /// Renames a workspace and returns its old name. The new name must be valid and unused by
    /// any other workspace (the workspace itself may change only the letter case).
    pub fn rename(&mut self, id: &str, new_name: &str) -> DomainResult<String> {
        Workspace::validate_name(new_name)?;
        if self.name_exists(new_name, Some(id)) {
            return Err(DomainError::AlreadyExists(new_name.into()));
        }
        let workspace = self
            .find_by_id_mut(id)
            .ok_or_else(|| DomainError::NotFound(id.into()))?;
        Ok(std::mem::replace(&mut workspace.name, new_name.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn registry_with(names: &[(&str, &str, &str)]) -> WorkspaceRegistry {
        let mut reg = WorkspaceRegistry::new_with_default(PathBuf::from("/tmp/default"));
        for (id, name, path) in names {
            let mut ws = Workspace::new(name, PathBuf::from(path));
            ws.id = id.to_string();
            reg.workspaces.push(ws);
        }
        reg
    }

    #[test]
    fn ensure_can_add_refuses_a_path_already_open() {
        let reg = registry_with(&[]);
        let err = reg
            .ensure_can_add("Other", Path::new("/tmp/default"))
            .expect_err("path taken");
        assert_eq!(
            err.to_string(),
            DomainError::AlreadyExists("This workspace is already open".into()).to_string()
        );
    }

    #[test]
    fn ensure_can_add_refuses_a_name_in_another_case() {
        let reg = registry_with(&[]);
        let err = reg
            .ensure_can_add("MY WORKSPACE", Path::new("/tmp/new"))
            .expect_err("name taken");
        assert_eq!(
            err.to_string(),
            DomainError::AlreadyExists("MY WORKSPACE".into()).to_string()
        );
    }

    #[test]
    fn ensure_can_add_checks_the_path_before_the_name() {
        let reg = registry_with(&[]);
        let err = reg
            .ensure_can_add("My Workspace", Path::new("/tmp/default"))
            .expect_err("both taken");
        assert!(err.to_string().contains("already open"), "{err}");
    }

    #[test]
    fn add_pushes_after_the_checks() {
        let mut reg = registry_with(&[]);
        reg.add(Workspace::new("Second", PathBuf::from("/tmp/second")))
            .expect("added");
        assert_eq!(reg.workspaces.len(), 2);
    }

    #[test]
    fn add_of_a_duplicate_leaves_the_registry_unchanged() {
        let mut reg = registry_with(&[]);
        let before = reg.clone();
        assert!(reg
            .add(Workspace::new("my workspace", PathBuf::from("/tmp/x")))
            .is_err());
        assert_eq!(reg, before);
    }

    #[test]
    fn rename_changes_the_name_and_returns_the_old_one() {
        let mut reg = registry_with(&[("a", "Alpha", "/tmp/a")]);
        let old = reg.rename("a", "Beta").expect("renamed");
        assert_eq!(old, "Alpha");
        assert_eq!(reg.find_by_id("a").expect("a").name, "Beta");
    }

    #[test]
    fn rename_to_a_duplicate_keeps_the_old_name() {
        let mut reg = registry_with(&[("a", "Alpha", "/tmp/a"), ("b", "Beta", "/tmp/b")]);
        assert!(reg.rename("a", "beta").is_err());
        assert_eq!(reg.find_by_id("a").expect("a").name, "Alpha");
    }

    #[test]
    fn rename_to_the_same_name_in_another_case_is_allowed() {
        let mut reg = registry_with(&[("a", "Alpha", "/tmp/a")]);
        reg.rename("a", "ALPHA").expect("own name");
        assert_eq!(reg.find_by_id("a").expect("a").name, "ALPHA");
    }

    #[test]
    fn rename_of_a_missing_id_is_not_found() {
        let mut reg = registry_with(&[]);
        let err = reg.rename("nope", "Fresh").expect_err("missing");
        assert_eq!(
            err.to_string(),
            DomainError::NotFound("nope".into()).to_string()
        );
    }

    #[test]
    fn rename_rejects_an_empty_name() {
        let mut reg = registry_with(&[("a", "Alpha", "/tmp/a")]);
        assert!(reg.rename("a", "  ").is_err());
        assert_eq!(reg.find_by_id("a").expect("a").name, "Alpha");
    }

    #[test]
    fn rename_checks_the_duplicate_before_the_id() {
        let mut reg = registry_with(&[]);
        let err = reg.rename("nope", "my workspace").expect_err("duplicate");
        assert!(matches!(err, DomainError::AlreadyExists(_)), "{err}");
    }

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
        assert!(reg.name_exists("my workspace", None));
        assert!(!reg.name_exists("my workspace", Some("default")));
    }

    #[test]
    fn find_by_id_returns_none_for_missing() {
        let reg = WorkspaceRegistry::new_with_default(PathBuf::from("/tmp/default"));
        assert!(reg.find_by_id("nonexistent").is_none());
    }

    #[test]
    fn new_workspace_has_no_description() {
        let ws = Workspace::new("Test", PathBuf::from("/tmp/test"));
        assert_eq!(ws.description, None);
    }

    #[test]
    fn workspace_description_serde_roundtrip() {
        let ws = Workspace {
            id: "test-id".to_string(),
            name: "Test".to_string(),
            path: PathBuf::from("/tmp/test"),
            description: Some("My description".to_string()),
            pinned: false,
        };
        let yaml = serde_yaml::to_string(&ws).unwrap();
        let back: Workspace = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(back.description, Some("My description".to_string()));
    }

    #[test]
    fn workspace_deserializes_without_description_backward_compat() {
        let yaml = "id: old-ws\nname: Old\npath: /tmp/old\n";
        let ws: Workspace = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(ws.description, None);
    }

    #[test]
    fn new_workspace_is_not_pinned() {
        let ws = Workspace::new("Test", PathBuf::from("/tmp/test"));
        assert!(!ws.pinned);
    }

    #[test]
    fn default_workspace_is_pinned() {
        let reg = WorkspaceRegistry::new_with_default(PathBuf::from("/tmp/default"));
        assert!(reg.workspaces[0].pinned);
    }

    #[test]
    fn workspace_pinned_serde_roundtrip() {
        let mut ws = Workspace::new("Test", PathBuf::from("/tmp/test"));
        ws.pinned = true;
        let yaml = serde_yaml::to_string(&ws).unwrap();
        let back: Workspace = serde_yaml::from_str(&yaml).unwrap();
        assert!(back.pinned);
    }

    #[test]
    fn workspace_deserializes_without_pinned_defaults_false() {
        let yaml = "id: old-ws\nname: Old\npath: /tmp/old\n";
        let ws: Workspace = serde_yaml::from_str(yaml).unwrap();
        assert!(!ws.pinned);
    }

    #[test]
    fn new_registry_has_multi_workspace_mode_false() {
        let reg = WorkspaceRegistry::new_with_default(PathBuf::from("/tmp/default"));
        assert!(!reg.multi_workspace_mode);
    }

    #[test]
    fn registry_multi_workspace_mode_serde_roundtrip() {
        let mut reg = WorkspaceRegistry::new_with_default(PathBuf::from("/tmp/default"));
        reg.multi_workspace_mode = true;
        let yaml = serde_yaml::to_string(&reg).unwrap();
        let back: WorkspaceRegistry = serde_yaml::from_str(&yaml).unwrap();
        assert!(back.multi_workspace_mode);
    }

    #[test]
    fn registry_deserializes_without_multi_workspace_mode_backward_compat() {
        let yaml = "activeWorkspaceId: default\nworkspaces:\n  - id: default\n    name: Default\n    path: /tmp/d\n    pinned: true\n";
        let reg: WorkspaceRegistry = serde_yaml::from_str(yaml).unwrap();
        assert!(!reg.multi_workspace_mode);
    }
}
