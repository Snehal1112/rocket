use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_shared::error::{DomainError, DomainResult};
use rocket_shared::events::{DomainEvent, EventPublisher};
use rocket_workspace::{Workspace, WorkspaceRepository};

pub struct WorkspaceService {
    repo: Box<dyn WorkspaceRepository>,
    publisher: Box<dyn EventPublisher>,
    active_path: Arc<Mutex<PathBuf>>,
}

impl WorkspaceService {
    pub fn new(
        repo: Box<dyn WorkspaceRepository>,
        publisher: Box<dyn EventPublisher>,
        active_path: Arc<Mutex<PathBuf>>,
    ) -> Self {
        Self { repo, publisher, active_path }
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
