use std::sync::Mutex;

use rocket_app::{EnvironmentService, WorkspaceService};
use rocket_environment::Environment;
use rocket_shared::error::DomainError;
use tauri::State;

#[tauri::command]
pub fn list_environments(
    svc: State<'_, EnvironmentService>,
) -> Result<Vec<Environment>, DomainError> {
    svc.list()
}

#[tauri::command]
pub fn get_environment(
    name: String,
    svc: State<'_, EnvironmentService>,
) -> Result<Environment, DomainError> {
    svc.get(&name)
}

#[tauri::command]
pub fn save_environment(
    env: Environment,
    svc: State<'_, EnvironmentService>,
) -> Result<(), DomainError> {
    svc.save(&env)
}

#[tauri::command]
pub fn delete_environment(
    name: String,
    svc: State<'_, EnvironmentService>,
) -> Result<(), DomainError> {
    svc.delete(&name)
}

#[tauri::command]
pub fn get_global_environment_name(
    workspace_svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<Option<String>, DomainError> {
    workspace_svc.lock().expect("workspace service lock poisoned").get_global_environment_name()
}

#[tauri::command]
pub fn set_global_environment(
    name: Option<String>,
    workspace_svc: State<'_, Mutex<WorkspaceService>>,
) -> Result<(), DomainError> {
    workspace_svc.lock().expect("workspace service lock poisoned").set_global_environment(name)
}

#[tauri::command]
pub fn get_process_env_vars() -> std::collections::HashMap<String, String> {
    std::env::vars().collect()
}
