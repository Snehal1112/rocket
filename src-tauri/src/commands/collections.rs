use rocket_app::CollectionService;
use rocket_collection::{Collection, CollectionSummary, CollectionVariable, Request};
use rocket_shared::error::DomainError;
use serde::Serialize;
use std::fs;
use std::path::Path;
use tauri::State;

#[tauri::command]
pub fn list_collections(
    svc: State<'_, CollectionService>,
) -> Result<Vec<CollectionSummary>, DomainError> {
    svc.list()
}

#[tauri::command]
pub fn get_collection(
    name: String,
    svc: State<'_, CollectionService>,
) -> Result<Collection, DomainError> {
    svc.get(&name)
}

#[tauri::command]
pub fn create_collection(
    name: String,
    svc: State<'_, CollectionService>,
) -> Result<Collection, DomainError> {
    svc.create(&name)
}

#[tauri::command]
pub fn delete_collection(
    name: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.delete(&name)
}

#[tauri::command]
pub fn rename_collection(
    old_name: String,
    new_name: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.rename(&old_name, &new_name)
}

#[tauri::command]
pub fn save_request(
    collection: String,
    path: String,
    request: Request,
    svc: State<'_, CollectionService>,
) -> Result<Request, DomainError> {
    svc.save_request(&collection, &path, &request)
}

#[tauri::command]
pub fn rename_request(
    collection: String,
    old_path: String,
    new_name: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.rename_request(&collection, &old_path, &new_name)
}

#[tauri::command]
pub fn delete_request(
    collection: String,
    path: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.delete_request(&collection, &path)
}

#[tauri::command]
pub fn create_folder(
    collection: String,
    path: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.create_folder(&collection, &path)
}

#[tauri::command]
pub fn delete_folder(
    collection: String,
    path: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.delete_folder(&collection, &path)
}

#[tauri::command]
pub fn move_item(
    src_collection: String,
    src_path: String,
    dst_collection: String,
    dst_path: String,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.move_item(&src_collection, &src_path, &dst_collection, &dst_path)
}

#[tauri::command]
pub fn reorder_items(
    collection: String,
    folder_path: String,
    ordered_names: Vec<String>,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.reorder_items(&collection, &folder_path, &ordered_names)
}

#[tauri::command]
pub fn get_collection_settings(
    name: String,
    svc: State<'_, CollectionService>,
) -> Result<rocket_collection::CollectionSettings, DomainError> {
    svc.get_settings(&name)
}

#[tauri::command]
pub fn save_collection_settings(
    collection: String,
    settings: rocket_collection::CollectionSettings,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.save_settings(&collection, &settings)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionScanResult {
    pub name: String,
    pub path: String,
}

#[tauri::command]
pub fn scan_collections_in_path(path: String) -> Result<Vec<CollectionScanResult>, DomainError> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Ok(vec![]);
    }
    let mut results = Vec::new();
    let entries = fs::read_dir(dir)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    for entry in entries {
        let entry = entry.map_err(|e| DomainError::Internal(e.to_string()))?;
        let entry_path = entry.path();
        if !entry_path.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        if entry_path.join("opencollection.yml").exists() {
            results.push(CollectionScanResult {
                name,
                path: entry_path.to_string_lossy().to_string(),
            });
        }
    }
    results.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    Ok(results)
}

#[tauri::command]
pub fn get_folder_chain_variables(
    collection: String,
    request_path: String,
    svc: State<'_, CollectionService>,
) -> Result<Vec<CollectionVariable>, DomainError> {
    svc.get_folder_chain_variables(&collection, &request_path)
}

#[tauri::command]
pub fn save_folder_variables(
    collection: String,
    folder_path: String,
    vars: Vec<CollectionVariable>,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.save_folder_variables(&collection, &folder_path, vars)
}

#[tauri::command]
pub fn get_request_variables(
    collection: String,
    request_path: String,
    svc: State<'_, CollectionService>,
) -> Result<Vec<CollectionVariable>, DomainError> {
    svc.get_request_variables(&collection, &request_path)
}

#[tauri::command]
pub fn save_request_variables(
    collection: String,
    request_path: String,
    vars: Vec<CollectionVariable>,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.save_request_variables(&collection, &request_path, vars)
}
