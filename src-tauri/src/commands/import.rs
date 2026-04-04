use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_import::{ImportReport, ImportService};
use tauri::State;

/// Import a single Bruno collection directory into the active workspace.
#[tauri::command]
pub async fn import_bruno_collection(
    path: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path.lock().unwrap().clone();
    let service = ImportService::new_with_workspace_path(&base);
    service
        .import_collection(&PathBuf::from(&path), &target_workspace_id)
        .map_err(|e| e.to_string())
}

/// Import a Bruno workspace directory.
/// `create_new_workspace`: true = create a new RocketAPI workspace;
/// false = add collections to the workspace identified by `target_workspace_id`.
#[tauri::command]
pub async fn import_bruno_workspace(
    path: String,
    create_new_workspace: bool,
    target_workspace_id: Option<String>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path.lock().unwrap().clone();
    let service = ImportService::new_with_workspace_path(&base);
    service
        .import_workspace(
            &PathBuf::from(&path),
            create_new_workspace,
            target_workspace_id.as_deref(),
        )
        .map_err(|e| e.to_string())
}
