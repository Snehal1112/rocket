use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};
use rocket_workspace::{Workspace, WorkspaceRepository, WorkspaceConfig, WorkspaceConfigRepository};

pub struct WorkspaceService {
    repo: Box<dyn WorkspaceRepository>,
    config_repo: Box<dyn WorkspaceConfigRepository>,
    publisher: Box<dyn EventPublisher>,
    active_path: Arc<Mutex<PathBuf>>,
}

impl WorkspaceService {
    pub fn new(
        repo: Box<dyn WorkspaceRepository>,
        config_repo: Box<dyn WorkspaceConfigRepository>,
        publisher: Box<dyn EventPublisher>,
        active_path: Arc<Mutex<PathBuf>>,
    ) -> Self {
        Self { repo, config_repo, publisher, active_path }
    }

    pub fn list(&self) -> DomainResult<Vec<Workspace>> {
        Ok(self.repo.load()?.workspaces)
    }

    pub fn get_active(&self) -> DomainResult<Workspace> {
        let registry = self.repo.load()?;
        registry
            .active()
            .cloned()
            .ok_or_else(|| DomainError::NotFound("active workspace".into()))
    }

    pub fn create(&self, name: &str, path: PathBuf) -> DomainResult<Workspace> {
        Workspace::validate_name(name)?;
        if !path.exists() {
            fs::create_dir_all(&path).map_err(|e| {
                DomainError::Io(format!("Failed to create workspace directory: {e}"))
            })?;
        }
        // Create subdirectories.
        fs::create_dir_all(path.join("collections")).map_err(|e| {
            DomainError::Io(format!("Failed to create collections dir: {e}"))
        })?;
        fs::create_dir_all(path.join("environments")).map_err(|e| {
            DomainError::Io(format!("Failed to create environments dir: {e}"))
        })?;
        // Write workspace.yml inside the workspace directory.
        let config = WorkspaceConfig::new(name);
        self.config_repo.save(&path, &config)?;
        let mut registry = self.repo.load()?;
        if registry.name_exists(name, None) {
            return Err(DomainError::AlreadyExists(name.into()));
        }
        let workspace = Workspace::new(name, path.clone());
        registry.workspaces.push(workspace.clone());
        self.repo.save(&registry)?;
        self.publisher.publish(DomainEvent::WorkspaceCreated {
            id: workspace.id.clone(),
            name: workspace.name.clone(),
            path: path.to_string_lossy().to_string(),
        });
        Ok(workspace)
    }

    pub fn switch(&self, id: &str) -> DomainResult<Workspace> {
        let mut registry = self.repo.load()?;
        let workspace = registry
            .find_by_id(id)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(id.into()))?;
        registry.active_workspace_id = id.to_string();
        self.repo.save(&registry)?;
        *self.active_path.lock().unwrap() = workspace.path.clone();
        self.publisher.publish(DomainEvent::WorkspaceSwitched {
            id: workspace.id.clone(),
            name: workspace.name.clone(),
            path: workspace.path.to_string_lossy().to_string(),
        });
        Ok(workspace)
    }

    pub fn rename(&self, id: &str, new_name: &str) -> DomainResult<()> {
        Workspace::validate_name(new_name)?;
        let mut registry = self.repo.load()?;
        if registry.name_exists(new_name, Some(id)) {
            return Err(DomainError::AlreadyExists(new_name.into()));
        }
        let workspace = registry
            .find_by_id_mut(id)
            .ok_or_else(|| DomainError::NotFound(id.into()))?;
        let old_name = workspace.name.clone();
        workspace.name = new_name.to_string();
        self.repo.save(&registry)?;
        self.publisher.publish(DomainEvent::WorkspaceRenamed {
            id: id.to_string(),
            old_name,
            new_name: new_name.to_string(),
        });
        Ok(())
    }

    pub fn close(&self, id: &str) -> DomainResult<()> {
        let mut registry = self.repo.load()?;
        if registry.workspaces.len() <= 1 {
            return Err(DomainError::InvalidInput(
                "Cannot close the last workspace".into(),
            ));
        }
        registry.workspaces.retain(|w| w.id != id);
        if registry.active_workspace_id == id {
            registry.active_workspace_id = registry.workspaces[0].id.clone();
            *self.active_path.lock().unwrap() = registry.workspaces[0].path.clone();
        }
        self.repo.save(&registry)?;
        self.publisher.publish(DomainEvent::WorkspaceClosed { id: id.to_string() });
        Ok(())
    }

    pub fn pin(&self, id: &str) -> DomainResult<()> {
        let mut registry = self.repo.load()?;
        let workspace = registry
            .find_by_id_mut(id)
            .ok_or_else(|| DomainError::NotFound(id.into()))?;
        workspace.pinned = true;
        self.repo.save(&registry)?;
        self.publisher.publish(DomainEvent::WorkspacePinned { id: id.to_string() });
        Ok(())
    }

    pub fn unpin(&self, id: &str) -> DomainResult<()> {
        let mut registry = self.repo.load()?;
        let workspace = registry
            .find_by_id_mut(id)
            .ok_or_else(|| DomainError::NotFound(id.into()))?;
        workspace.pinned = false;
        self.repo.save(&registry)?;
        self.publisher.publish(DomainEvent::WorkspaceUnpinned { id: id.to_string() });
        Ok(())
    }

    pub fn update_description(&self, id: &str, description: Option<&str>) -> DomainResult<()> {
        let mut registry = self.repo.load()?;
        let workspace = registry
            .find_by_id_mut(id)
            .ok_or_else(|| DomainError::NotFound(id.into()))?;
        workspace.description = description.map(|s| s.to_string());
        self.repo.save(&registry)?;
        self.publisher.publish(DomainEvent::WorkspaceDescriptionUpdated {
            id: id.to_string(),
            description: description.map(|s| s.to_string()),
        });
        Ok(())
    }

    pub fn get_workspace_config(&self, workspace_id: &str) -> DomainResult<WorkspaceConfig> {
        let registry = self.repo.load()?;
        let workspace = registry
            .find_by_id(workspace_id)
            .ok_or_else(|| DomainError::NotFound(workspace_id.into()))?;
        self.config_repo.load(&workspace.path)
    }

    /// Open an existing workspace from disk. The directory must contain `workspace.yml`.
    pub fn open_workspace(&self, path: PathBuf) -> DomainResult<Workspace> {
        if !path.join("workspace.yml").exists() {
            return Err(DomainError::NotFound(
                "workspace.yml not found in the selected directory".into(),
            ));
        }

        let config = self.config_repo.load(&path)?;
        let mut registry = self.repo.load()?;

        if registry.workspaces.iter().any(|w| w.path == path) {
            return Err(DomainError::AlreadyExists("This workspace is already open".into()));
        }

        if registry.name_exists(&config.name, None) {
            return Err(DomainError::AlreadyExists(config.name.clone()));
        }

        let mut workspace = Workspace::new(&config.name, path.clone());
        workspace.description = config.description;

        registry.workspaces.push(workspace.clone());
        self.repo.save(&registry)?;
        self.publisher.publish(DomainEvent::WorkspaceCreated {
            id: workspace.id.clone(),
            name: workspace.name.clone(),
            path: path.to_string_lossy().to_string(),
        });
        Ok(workspace)
    }

    pub fn get_multi_workspace_mode(&self) -> DomainResult<bool> {
        Ok(self.repo.load()?.multi_workspace_mode)
    }

    pub fn set_multi_workspace_mode(&self, enabled: bool) -> DomainResult<()> {
        let mut registry = self.repo.load()?;
        registry.multi_workspace_mode = enabled;
        self.repo.save(&registry)?;
        Ok(())
    }

    pub fn delete(&self, id: &str) -> DomainResult<()> {
        if id == "default" {
            return Err(DomainError::InvalidInput(
                "Cannot delete the default workspace".into(),
            ));
        }
        let mut registry = self.repo.load()?;
        if registry.workspaces.len() <= 1 {
            return Err(DomainError::InvalidInput(
                "Cannot delete the last workspace".into(),
            ));
        }
        let workspace = registry
            .find_by_id(id)
            .cloned()
            .ok_or_else(|| DomainError::NotFound(id.into()))?;
        if workspace.path.exists() {
            fs::remove_dir_all(&workspace.path).map_err(|e| {
                DomainError::Io(format!("Failed to delete workspace directory: {e}"))
            })?;
        }
        registry.workspaces.retain(|w| w.id != id);
        if registry.active_workspace_id == id {
            registry.active_workspace_id = registry.workspaces[0].id.clone();
            *self.active_path.lock().unwrap() = registry.workspaces[0].path.clone();
        }
        self.repo.save(&registry)?;
        self.publisher.publish(DomainEvent::WorkspaceDeleted { id: id.to_string() });
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::DomainResult;
    use rocket_shared::events::NullEventPublisher;
    use rocket_workspace::{WorkspaceRegistry, WorkspaceConfig, WorkspaceConfigRepository};
    use std::path::Path;
    use std::sync::{Arc, Mutex};
    use tempfile::TempDir;

    struct MockWorkspaceRepo {
        registry: Mutex<WorkspaceRegistry>,
    }

    impl MockWorkspaceRepo {
        fn new(default_path: PathBuf) -> Self {
            Self {
                registry: Mutex::new(WorkspaceRegistry::new_with_default(default_path)),
            }
        }
    }

    impl WorkspaceRepository for MockWorkspaceRepo {
        fn load(&self) -> DomainResult<WorkspaceRegistry> {
            Ok(self.registry.lock().unwrap().clone())
        }
        fn save(&self, registry: &WorkspaceRegistry) -> DomainResult<()> {
            *self.registry.lock().unwrap() = registry.clone();
            Ok(())
        }
    }

    struct MockWorkspaceConfigRepo;

    impl WorkspaceConfigRepository for MockWorkspaceConfigRepo {
        fn load(&self, workspace_path: &Path) -> DomainResult<WorkspaceConfig> {
            let config_path = workspace_path.join("workspace.yml");
            if config_path.exists() {
                let content = std::fs::read_to_string(&config_path)
                    .map_err(|e| DomainError::Io(e.to_string()))?;
                serde_yaml::from_str(&content)
                    .map_err(|e| DomainError::InvalidInput(e.to_string()))
            } else {
                let name = workspace_path.file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "Test".into());
                Ok(WorkspaceConfig::new(name))
            }
        }
        fn save(&self, workspace_path: &Path, config: &WorkspaceConfig) -> DomainResult<()> {
            std::fs::create_dir_all(workspace_path)
                .map_err(|e| DomainError::Io(e.to_string()))?;
            let content = serde_yaml::to_string(config)
                .map_err(|e| DomainError::InvalidInput(e.to_string()))?;
            std::fs::write(workspace_path.join("workspace.yml"), content)
                .map_err(|e| DomainError::Io(e.to_string()))
        }
    }

    fn make_service(tmp: &TempDir) -> WorkspaceService {
        let default_path = tmp.path().join("default");
        std::fs::create_dir_all(&default_path).unwrap();
        let repo = Box::new(MockWorkspaceRepo::new(default_path.clone()));
        let config_repo = Box::new(MockWorkspaceConfigRepo);
        let active_path = Arc::new(Mutex::new(default_path));
        WorkspaceService::new(repo, config_repo, Box::new(NullEventPublisher), active_path)
    }

    #[test]
    fn list_returns_default() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        assert_eq!(svc.list().unwrap().len(), 1);
    }

    #[test]
    fn create_adds_workspace() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws = svc.create("New WS", tmp.path().join("new-ws")).unwrap();
        assert_eq!(ws.name, "New WS");
        assert_eq!(svc.list().unwrap().len(), 2);
    }

    #[test]
    fn create_rejects_duplicate_name() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        svc.create("My WS", tmp.path().join("ws1")).unwrap();
        assert!(svc.create("My WS", tmp.path().join("ws2")).is_err());
    }

    #[test]
    fn create_rejects_empty_name() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        assert!(svc.create("", tmp.path().join("ws")).is_err());
    }

    #[test]
    fn switch_updates_active_path() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws = svc.create("Other", tmp.path().join("other")).unwrap();
        svc.switch(&ws.id).unwrap();
        let active = svc.get_active().unwrap();
        assert_eq!(active.id, ws.id);
    }

    #[test]
    fn rename_updates_name() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws = svc.create("Old", tmp.path().join("ws")).unwrap();
        svc.rename(&ws.id, "New").unwrap();
        assert!(svc.list().unwrap().iter().any(|w| w.name == "New"));
    }

    #[test]
    fn rename_rejects_duplicate() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws = svc.create("Alpha", tmp.path().join("ws")).unwrap();
        assert!(svc.rename(&ws.id, "Default Workspace").is_err());
    }

    #[test]
    fn cannot_close_last_workspace() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        assert!(svc.close("default").is_err());
    }

    #[test]
    fn close_switches_active_if_needed() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws = svc.create("Other", tmp.path().join("other")).unwrap();
        svc.switch(&ws.id).unwrap();
        svc.close(&ws.id).unwrap();
        assert_eq!(svc.get_active().unwrap().id, "default");
    }

    #[test]
    fn cannot_delete_default() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        svc.create("Other", tmp.path().join("other")).unwrap();
        assert!(svc.delete("default").is_err());
    }

    #[test]
    fn delete_removes_directory() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let path = tmp.path().join("to-delete");
        std::fs::create_dir_all(&path).unwrap();
        let ws = svc.create("ToDelete", path.clone()).unwrap();
        svc.delete(&ws.id).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn pin_workspace() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws = svc.create("Pinnable", tmp.path().join("pin-ws")).unwrap();
        svc.pin(&ws.id).unwrap();
        let list = svc.list().unwrap();
        let pinned = list.iter().find(|w| w.id == ws.id).unwrap();
        assert!(pinned.pinned);
    }

    #[test]
    fn unpin_workspace() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        svc.unpin("default").unwrap();
        let list = svc.list().unwrap();
        let def = list.iter().find(|w| w.id == "default").unwrap();
        assert!(!def.pinned);
    }

    #[test]
    fn pin_nonexistent_fails() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        assert!(svc.pin("nonexistent").is_err());
    }

    #[test]
    fn update_description_sets_value() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        svc.update_description("default", Some("My desc")).unwrap();
        let ws = svc.get_active().unwrap();
        assert_eq!(ws.description, Some("My desc".to_string()));
    }

    #[test]
    fn update_description_to_none_clears_it() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        svc.update_description("default", Some("Initial")).unwrap();
        svc.update_description("default", None).unwrap();
        let ws = svc.get_active().unwrap();
        assert_eq!(ws.description, None);
    }

    #[test]
    fn update_description_nonexistent_fails() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        assert!(svc.update_description("nope", Some("x")).is_err());
    }

    #[test]
    fn create_writes_workspace_yml_and_subdirs() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws_path = tmp.path().join("structured-ws");
        svc.create("Structured", ws_path.clone()).unwrap();
        assert!(ws_path.join("workspace.yml").exists());
        assert!(ws_path.join("collections").is_dir());
        assert!(ws_path.join("environments").is_dir());
    }

    #[test]
    fn get_workspace_config_returns_config() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws = svc.create("Configurable", tmp.path().join("cfg-ws")).unwrap();
        let config = svc.get_workspace_config(&ws.id).unwrap();
        assert_eq!(config.name, "Configurable");
    }

    #[test]
    fn open_existing_workspace() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws_path = tmp.path().join("ext-ws");
        std::fs::create_dir_all(&ws_path).unwrap();
        let cfg = WorkspaceConfig::new("External");
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        std::fs::write(ws_path.join("workspace.yml"), yaml).unwrap();

        let ws = svc.open_workspace(ws_path).unwrap();
        assert_eq!(ws.name, "External");
        assert_eq!(svc.list().unwrap().len(), 2);
    }

    #[test]
    fn open_workspace_without_yml_fails() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws_path = tmp.path().join("no-cfg");
        std::fs::create_dir_all(&ws_path).unwrap();
        assert!(svc.open_workspace(ws_path).is_err());
    }

    #[test]
    fn open_workspace_already_registered_fails() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        let ws_path = tmp.path().join("dup");
        std::fs::create_dir_all(&ws_path).unwrap();
        let cfg = WorkspaceConfig::new("Dup");
        let yaml = serde_yaml::to_string(&cfg).unwrap();
        std::fs::write(ws_path.join("workspace.yml"), yaml).unwrap();
        svc.open_workspace(ws_path.clone()).unwrap();
        assert!(svc.open_workspace(ws_path).is_err());
    }

    #[test]
    fn get_and_set_multi_workspace_mode() {
        let tmp = TempDir::new().unwrap();
        let svc = make_service(&tmp);
        assert!(!svc.get_multi_workspace_mode().unwrap());
        svc.set_multi_workspace_mode(true).unwrap();
        assert!(svc.get_multi_workspace_mode().unwrap());
    }
}
