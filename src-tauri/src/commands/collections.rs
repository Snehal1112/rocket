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

/// Result of analyzing a cloned repository's structure.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClonedRepoStructure {
    /// "workspace" | "collection" | "multi_collection" | "unknown"
    pub kind: String,
    /// Path to open as workspace (for "workspace" kind).
    pub workspace_path: Option<String>,
    /// Detected collections (for "collection" or "multi_collection" kind).
    pub collections: Vec<CollectionScanResult>,
}

/// Analyze a cloned directory to determine its structure.
///
/// Detection order:
/// 1. Path has `workspace.yml` -> workspace (also scan for collections inside)
/// 2. Path has `opencollection.yml` -> single collection at root
/// 3. Path has `collections/` subdir with collections -> multi-collection workspace-like
/// 4. Direct children have `opencollection.yml` -> multi-collection
/// 5. Otherwise -> unknown
#[tauri::command]
pub fn detect_cloned_structure(path: String) -> Result<ClonedRepoStructure, DomainError> {
    let dir = Path::new(&path);
    if !dir.is_dir() {
        return Ok(ClonedRepoStructure {
            kind: "unknown".into(),
            workspace_path: None,
            collections: vec![],
        });
    }

    // Case 1: Workspace root with workspace.yml.
    if dir.join("workspace.yml").exists() {
        let collections = scan_opencollection_dirs(dir)?;
        return Ok(ClonedRepoStructure {
            kind: "workspace".into(),
            workspace_path: Some(path),
            collections,
        });
    }

    // Case 2: Single collection at root.
    if dir.join("opencollection.yml").exists() {
        let name = dir.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "collection".into());
        return Ok(ClonedRepoStructure {
            kind: "collection".into(),
            workspace_path: None,
            collections: vec![CollectionScanResult {
                name,
                path: path.clone(),
            }],
        });
    }

    // Case 3: collections/ subdirectory.
    let collections_subdir = dir.join("collections");
    if collections_subdir.is_dir() {
        let collections = scan_opencollection_dirs(&collections_subdir)?;
        if !collections.is_empty() {
            return Ok(ClonedRepoStructure {
                kind: "multi_collection".into(),
                workspace_path: None,
                collections,
            });
        }
    }

    // Case 4: Direct children are collections.
    let collections = scan_opencollection_dirs(dir)?;
    if !collections.is_empty() {
        return Ok(ClonedRepoStructure {
            kind: "multi_collection".into(),
            workspace_path: None,
            collections,
        });
    }

    Ok(ClonedRepoStructure {
        kind: "unknown".into(),
        workspace_path: None,
        collections: vec![],
    })
}

/// Scan direct subdirectories for ones containing `opencollection.yml`.
fn scan_opencollection_dirs(dir: &Path) -> Result<Vec<CollectionScanResult>, DomainError> {
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
pub fn get_folder_variables(
    collection: String,
    folder_path: String,
    svc: State<'_, CollectionService>,
) -> Result<Vec<CollectionVariable>, DomainError> {
    svc.get_folder_variables(&collection, &folder_path)
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

#[tauri::command]
pub fn update_request_docs(
    collection: String,
    path: String,
    docs: Option<String>,
    svc: State<'_, CollectionService>,
) -> Result<(), DomainError> {
    svc.update_request_docs(&collection, &path, docs)
}
