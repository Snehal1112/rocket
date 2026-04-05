use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_import::{ImportReport, ImportService};
use tauri::State;

/// Import a Bruno collection or workspace directory. Type is auto-detected from content.
#[tauri::command]
pub async fn import_bruno(
    path: String,
    target_workspace_id: String,
    create_new_workspace: Option<bool>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path.lock().unwrap().clone();
    let service = ImportService::new_with_workspace_path(&base);
    service
        .import_auto(
            &PathBuf::from(&path),
            &target_workspace_id,
            create_new_workspace.unwrap_or(false),
        )
        .map_err(|e| e.to_string())
}

/// Extract a Bruno ZIP and import the contained collection or workspace.
#[tauri::command]
pub async fn import_bruno_zip(
    zip_path: String,
    target_workspace_id: String,
    create_new_workspace: Option<bool>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path.lock().unwrap().clone();
    let service = ImportService::new_with_workspace_path(&base);
    service
        .import_auto_from_zip(
            &PathBuf::from(&zip_path),
            &target_workspace_id,
            create_new_workspace.unwrap_or(false),
        )
        .map_err(|e| e.to_string())
}
