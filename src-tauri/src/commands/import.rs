use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use rocket_environment::EnvironmentRepository;
use rocket_import::{EnvironmentRepositoryFactory, ImportReport, ImportService};
use rocket_infra::{FsCollectionRepo, FsEnvironmentRepo};
use tauri::State;

struct FsEnvFactory(PathBuf);
impl EnvironmentRepositoryFactory for FsEnvFactory {
    fn make(&self, collection_name: &str) -> Box<dyn EnvironmentRepository> {
        Box::new(FsEnvironmentRepo::new(
            self.0.join("collections").join(collection_name).join("environments"),
        ))
    }
}

fn make_import_service(base: PathBuf) -> ImportService {
    ImportService::new(
        base.clone(),
        Box::new(FsCollectionRepo::new_standalone(base.join("collections"))),
        Box::new(FsEnvFactory(base)),
    )
}

/// Import a Bruno collection or workspace directory. Type is auto-detected from content.
#[tauri::command]
pub async fn import_bruno(
    path: String,
    target_workspace_id: String,
    create_new_workspace: Option<bool>,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_auto(
            &PathBuf::from(&path),
            &target_workspace_id,
            create_new_workspace.unwrap_or(false),
        )
        .map_err(|e| e.to_string())
}

/// Import a Postman Collection JSON file (v2.0 or v2.1).
#[tauri::command]
pub async fn import_postman_collection(
    path: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_postman_collection(&PathBuf::from(&path), &target_workspace_id)
        .map_err(|e| e.to_string())
}

/// Import a Postman environment JSON file into an existing collection.
#[tauri::command]
pub async fn import_postman_environment(
    json_path: String,
    collection_name: String,
    target_workspace_id: String,
    workspace_path: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<ImportReport, String> {
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_postman_environment(
            &PathBuf::from(&json_path),
            &collection_name,
            &target_workspace_id,
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
    let base = workspace_path
        .lock()
        .map_err(|_| "workspace path lock poisoned".to_string())?
        .clone();
    make_import_service(base)
        .import_auto_from_zip(
            &PathBuf::from(&zip_path),
            &target_workspace_id,
            create_new_workspace.unwrap_or(false),
        )
        .map_err(|e| e.to_string())
}
