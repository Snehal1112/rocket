use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_app::WorkspaceService;
use rocket_infra::NotifyFileWatcher;
use rocket_shared::error::DomainError;
use rocket_workspace::{RepositoryId, RequestGuardPolicy, Workspace, WorkspaceConfig};
use serde::Serialize;
use tauri::State;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDto {
    pub id: String,
    pub repository_id: String,
    pub name: String,
    pub path: String,
    pub description: Option<String>,
    pub pinned: bool,
}

impl TryFrom<Workspace> for WorkspaceDto {
    type Error = DomainError;

    fn try_from(workspace: Workspace) -> Result<Self, Self::Error> {
        let repository_id = RepositoryId::workspace(&workspace.id)?.to_string();
        Ok(Self {
            id: workspace.id,
            repository_id,
            name: workspace.name,
            path: workspace.path.to_string_lossy().into_owned(),
            description: workspace.description,
            pinned: workspace.pinned,
        })
    }
}

#[tauri::command]
pub fn list_workspaces(
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<Vec<WorkspaceDto>, DomainError> {
    svc.lock()
        .map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?
        .list()?
        .into_iter()
        .map(WorkspaceDto::try_from)
        .collect()
}

#[tauri::command]
pub fn get_active_workspace(
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<WorkspaceDto, DomainError> {
    svc.lock()
        .map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?
        .get_active()?
        .try_into()
}

#[tauri::command]
pub fn create_workspace(
    name: String,
    path: String,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<WorkspaceDto, DomainError> {
    svc.lock()
        .map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?
        .create(&name, PathBuf::from(path))?
        .try_into()
}

#[tauri::command]
pub fn switch_workspace(
    id: String,
    svc: State<'_, Mutex<WorkspaceService>>,
    watcher: State<'_, NotifyFileWatcher>,
    app: tauri::AppHandle,
) -> Result<WorkspaceDto, DomainError> {
    let workspace = svc.lock().map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?.switch(&id)?;
    // Restart the file watcher on the new workspace's collections directory so
    // filesystem changes in the new workspace trigger sidebar refreshes.
    let new_collections_dir = workspace.path.join("collections");
    std::fs::create_dir_all(&new_collections_dir).ok();
    watcher.stop();
    let publisher = Arc::new(crate::tauri_event_bus::TauriEventBus::new(app));
    let _ = watcher.start(new_collections_dir, publisher);
    workspace.try_into()
}

#[tauri::command]
pub fn rename_workspace(
    id: String,
    new_name: String,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    svc.lock().map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?.rename(&id, &new_name)
}

#[tauri::command]
pub fn close_workspace(
    id: String,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    svc.lock().map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?.close(&id)
}

#[tauri::command]
pub fn delete_workspace(
    id: String,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    svc.lock().map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?.delete(&id)
}

#[tauri::command]
pub fn pin_workspace(
    id: String,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    svc.lock().map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?.pin(&id)
}

#[tauri::command]
pub fn unpin_workspace(
    id: String,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    svc.lock().map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?.unpin(&id)
}

#[tauri::command]
pub fn update_workspace_description(
    id: String,
    description: Option<String>,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    svc.lock().map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?.update_description(&id, description.as_deref())
}

#[tauri::command]
pub fn open_workspace(
    path: String,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<WorkspaceDto, DomainError> {
    svc.lock()
        .map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?
        .open_workspace(PathBuf::from(path))?
        .try_into()
}

#[tauri::command]
pub fn get_workspace_config(
    workspace_id: String,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<WorkspaceConfig, DomainError> {
    svc.lock().map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?.get_workspace_config(&workspace_id)
}

#[tauri::command]
pub fn get_multi_workspace_mode(
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<bool, DomainError> {
    svc.lock().map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?.get_multi_workspace_mode()
}

#[tauri::command]
pub fn set_multi_workspace_mode(
    enabled: bool,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    svc.lock().map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?.set_multi_workspace_mode(enabled)
}

#[tauri::command]
pub fn update_request_guard_policy(
    workspace_id: String,
    policy: RequestGuardPolicy,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    svc.lock().map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?.update_request_guard_policy(&workspace_id, policy)
}

#[tauri::command]
pub fn link_external_collection(
    workspace_id: String,
    collection_path: String,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    svc.lock().map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?.link_external_collection(&workspace_id, PathBuf::from(collection_path))
}

#[tauri::command]
pub async fn open_folder_picker(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    let (tx, rx) = tokio::sync::oneshot::channel();
    // blocking_pick_folder panics inside an async context (Tauri v2 always
    // dispatches commands within the tokio runtime). Use the callback form and
    // bridge it into the async world via a oneshot channel instead.
    app.dialog().file().pick_folder(move |folder| {
        let _ = tx.send(folder);
    });
    rx.await
        .map_err(|_| "Dialog closed unexpectedly".to_string())
        .map(|f| f.map(|p| p.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workspace_dto_contains_backend_issued_repository_id() {
        let workspace = Workspace {
            id: "workspace-1".into(),
            name: "Workspace".into(),
            path: PathBuf::from("/tmp/workspace"),
            description: None,
            pinned: false,
        };

        let dto = WorkspaceDto::try_from(workspace).expect("valid workspace DTO");

        assert_eq!(dto.repository_id, "workspace:workspace-1");
        let json = serde_json::to_value(dto).expect("serialize workspace DTO");
        assert_eq!(json["repositoryId"], "workspace:workspace-1");
    }
}
