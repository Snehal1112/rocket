use rocket_app::{EnvironmentService, RequestExecutionService, WorkspaceService};
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
    Ok(EnvironmentService::new(
        Box::new(FsEnvironmentRepo::with_secret_store(
            env_dir,
            crate::env_secret_store(),
        )),
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
    Environment::validate_name(&name)?;
    env_service_for(&collection, &ws)?.get(&name)
}

#[tauri::command]
pub fn save_environment(
    collection: String,
    env: Environment,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
    secret_managers: State<'_, rocket_app::SecretManagerService>,
) -> Result<(), DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    Environment::validate_name(&env.name)?;
    env_service_for(&collection, &ws)?.save_with_capabilities(&env, &*secret_managers)
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
    Environment::validate_name(&name)?;
    env_service_for(&collection, &ws)?.delete(&name)
}

#[tauri::command]
pub fn get_global_environment_name(
    workspace_svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<Option<String>, DomainError> {
    workspace_svc
        .lock()
        .map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?
        .get_global_environment_name()
}

#[tauri::command]
pub fn set_global_environment(
    name: Option<String>,
    workspace_svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    if let Some(name) = &name {
        Environment::validate_name(name)?;
    }
    workspace_svc
        .lock()
        .map_err(|_| DomainError::Internal("workspace service lock poisoned".into()))?
        .set_global_environment(name)
}

/// Creates an `EnvironmentService` scoped to the workspace-level environments directory.
fn global_env_service(ws_path: &Path) -> Result<EnvironmentService, DomainError> {
    let env_dir = ws_path.join("environments");
    Ok(EnvironmentService::new(
        Box::new(FsEnvironmentRepo::with_secret_store(
            env_dir,
            crate::env_secret_store(),
        )),
        Box::new(NullEventPublisher),
    ))
}

#[tauri::command]
pub fn list_global_environments(
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<Vec<Environment>, DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    global_env_service(&ws)?.list()
}

#[tauri::command]
pub fn get_global_environment(
    name: String,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<Environment, DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    Environment::validate_name(&name)?;
    global_env_service(&ws)?.get(&name)
}

#[tauri::command]
pub fn save_global_environment(
    env: Environment,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
    secret_managers: State<'_, rocket_app::SecretManagerService>,
) -> Result<(), DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    Environment::validate_name(&env.name)?;
    global_env_service(&ws)?.save_with_capabilities(&env, &*secret_managers)
}

#[tauri::command]
pub fn delete_global_environment(
    name: String,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<(), DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    Environment::validate_name(&name)?;
    global_env_service(&ws)?.delete(&name)
}

/// The host environment for `{{process.env.*}}`. With a `collection` that is not allowed
/// host environment access on this computer, the map is empty. Without a collection (a
/// scratch request, a preview) it is the full map.
#[tauri::command]
pub fn get_process_env_vars(
    collection: Option<String>,
    exec: State<'_, RequestExecutionService>,
) -> std::collections::HashMap<String, String> {
    if exec.process_env_allowed(collection.as_deref()) {
        std::env::vars().collect()
    } else {
        std::collections::HashMap::new()
    }
}
