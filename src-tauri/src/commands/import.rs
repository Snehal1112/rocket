use rocket_import::{ImportReport, ImportService};
use std::path::PathBuf;

/// Import a single Bruno collection directory into the active workspace.
#[tauri::command]
pub async fn import_bruno_collection(
    path: String,
    target_workspace_id: String,
) -> Result<ImportReport, String> {
    let service = ImportService::new();
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
) -> Result<ImportReport, String> {
    let service = ImportService::new();
    service
        .import_workspace(
            &PathBuf::from(&path),
            create_new_workspace,
            target_workspace_id.as_deref(),
        )
        .map_err(|e| e.to_string())
}
