use rocket_app::EnvironmentService;
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
