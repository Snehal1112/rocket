use rocket_app::EnvironmentService;
use rocket_environment::Environment;
use rocket_infra::FsEnvironmentRepo;
use rocket_shared::{error::DomainError, events::NullEventPublisher};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::State;

/// Creates a fresh `EnvironmentService` scoped to a single collection.
///
/// Validates `collection` to prevent path traversal attacks before constructing
/// the environment directory path.
fn env_service_for(collection: &str, ws_path: &Path) -> Result<EnvironmentService, DomainError> {
    // Reject names that could escape the workspace directory.
    if collection.contains('\0')
        || collection.starts_with('/')
        || collection.starts_with('\\')
        || Path::new(collection)
            .components()
            .any(|c| c == std::path::Component::ParentDir)
    {
        return Err(DomainError::InvalidInput("invalid collection name".into()));
    }
    let env_dir = ws_path
        .join("collections")
        .join(collection)
        .join("environments");
    std::fs::create_dir_all(&env_dir).ok();
    Ok(EnvironmentService::new(
        Box::new(FsEnvironmentRepo::new(env_dir)),
        Box::new(NullEventPublisher),
    ))
}

#[tauri::command]
pub fn list_environments(
    collection: String,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<Vec<Environment>, DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    env_service_for(&collection, &ws)?.list()
}

#[tauri::command]
pub fn get_environment(
    collection: String,
    name: String,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<Environment, DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    env_service_for(&collection, &ws)?.get(&name)
}

#[tauri::command]
pub fn save_environment(
    collection: String,
    env: Environment,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<(), DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    env_service_for(&collection, &ws)?.save(&env)
}

#[tauri::command]
pub fn delete_environment(
    collection: String,
    name: String,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<(), DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    env_service_for(&collection, &ws)?.delete(&name)
}
