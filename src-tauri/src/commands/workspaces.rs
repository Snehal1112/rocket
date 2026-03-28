use std::path::PathBuf;
use std::sync::Mutex;

use rocket_app::WorkspaceService;
use rocket_shared::error::DomainError;
use rocket_workspace::Workspace;
use tauri::State;

#[tauri::command]
pub fn list_workspaces(
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<Vec<Workspace>, DomainError> {
    svc.lock().unwrap().list()
}

#[tauri::command]
pub fn get_active_workspace(
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<Workspace, DomainError> {
    svc.lock().unwrap().get_active()
}

#[tauri::command]
pub fn create_workspace(
    name: String,
    path: String,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<Workspace, DomainError> {
    svc.lock().unwrap().create(&name, PathBuf::from(path))
}

#[tauri::command]
pub fn switch_workspace(
    id: String,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<Workspace, DomainError> {
    svc.lock().unwrap().switch(&id)
}

#[tauri::command]
pub fn rename_workspace(
    id: String,
    new_name: String,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    svc.lock().unwrap().rename(&id, &new_name)
}

#[tauri::command]
pub fn close_workspace(
    id: String,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    svc.lock().unwrap().close(&id)
}

#[tauri::command]
pub fn delete_workspace(
    id: String,
    svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    svc.lock().unwrap().delete(&id)
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
