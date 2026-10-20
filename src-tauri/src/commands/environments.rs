use rocket_app::{EnvironmentService, RequestExecutionService, WorkspaceService};
use rocket_audit::{
    actor::AuditActor,
    publisher::{NullSecurityAuditPublisher, SecurityAuditPublisher},
};
use rocket_environment::Environment;
use rocket_infra::FsEnvironmentRepo;
use rocket_shared::{
    error::DomainError,
    events::{EventPublisher, NullEventPublisher},
};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use tauri::{AppHandle, State};

use crate::tauri_event_bus::TauriEventBus;

/// Creates a fresh `EnvironmentService` scoped to a single collection.
///
/// Validates `collection` to prevent path traversal attacks before constructing
/// the environment directory path.
fn env_service_for(
    collection: &str,
    ws_path: &Path,
    events: Box<dyn EventPublisher>,
    audit: Arc<dyn SecurityAuditPublisher>,
) -> Result<EnvironmentService, DomainError> {
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
    Ok(EnvironmentService::new_with_audit(
        Box::new(FsEnvironmentRepo::with_secret_store(
            env_dir,
            crate::env_secret_store(),
        )),
        events,
        audit,
    )
    .with_audit_actor(AuditActor::User))
}

#[tauri::command]
pub fn list_environments(
    collection: String,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<Vec<Environment>, DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    env_service_for(
        &collection,
        &ws,
        Box::new(NullEventPublisher),
        Arc::new(NullSecurityAuditPublisher),
    )?
    .list()
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
    env_service_for(
        &collection,
        &ws,
        Box::new(NullEventPublisher),
        Arc::new(NullSecurityAuditPublisher),
    )?
    .get(&name)
}

#[tauri::command]
pub fn save_environment(
    collection: String,
    env: Environment,
    app: AppHandle,
    audit: State<'_, Arc<dyn SecurityAuditPublisher>>,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
    secret_managers: State<'_, rocket_app::SecretManagerService>,
) -> Result<(), DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    Environment::validate_name(&env.name)?;
    env_service_for(
        &collection,
        &ws,
        Box::new(TauriEventBus::new(app)),
        Arc::clone(&audit),
    )?
    .save_with_capabilities(&env, &*secret_managers)
}

#[tauri::command]
pub fn delete_environment(
    collection: String,
    name: String,
    app: AppHandle,
    audit: State<'_, Arc<dyn SecurityAuditPublisher>>,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<(), DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    Environment::validate_name(&name)?;
    env_service_for(
        &collection,
        &ws,
        Box::new(TauriEventBus::new(app)),
        Arc::clone(&audit),
    )?
    .delete(&name)
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
fn global_env_service(
    ws_path: &Path,
    events: Box<dyn EventPublisher>,
    audit: Arc<dyn SecurityAuditPublisher>,
) -> Result<EnvironmentService, DomainError> {
    let env_dir = ws_path.join("environments");
    Ok(EnvironmentService::new_with_audit(
        Box::new(FsEnvironmentRepo::with_secret_store(
            env_dir,
            crate::env_secret_store(),
        )),
        events,
        audit,
    )
    .with_audit_actor(AuditActor::User))
}

#[tauri::command]
pub fn list_global_environments(
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<Vec<Environment>, DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    global_env_service(
        &ws,
        Box::new(NullEventPublisher),
        Arc::new(NullSecurityAuditPublisher),
    )?
    .list()
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
    global_env_service(
        &ws,
        Box::new(NullEventPublisher),
        Arc::new(NullSecurityAuditPublisher),
    )?
    .get(&name)
}

#[tauri::command]
pub fn save_global_environment(
    env: Environment,
    app: AppHandle,
    audit: State<'_, Arc<dyn SecurityAuditPublisher>>,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
    secret_managers: State<'_, rocket_app::SecretManagerService>,
) -> Result<(), DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    Environment::validate_name(&env.name)?;
    global_env_service(&ws, Box::new(TauriEventBus::new(app)), Arc::clone(&audit))?
        .save_with_capabilities(&env, &*secret_managers)
}

#[tauri::command]
pub fn delete_global_environment(
    name: String,
    app: AppHandle,
    audit: State<'_, Arc<dyn SecurityAuditPublisher>>,
    workspace: State<'_, Arc<Mutex<PathBuf>>>,
) -> Result<(), DomainError> {
    let ws = workspace
        .lock()
        .map_err(|_| DomainError::Internal("workspace lock poisoned".into()))?;
    Environment::validate_name(&name)?;
    global_env_service(&ws, Box::new(TauriEventBus::new(app)), Arc::clone(&audit))?.delete(&name)
}

/// One variable of a flattened environment, with the environment that defines it.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedVariableDto {
    pub key: String,
    pub value: String,
    pub enabled: bool,
    pub secret: bool,
    pub source: String,
}

/// An environment flattened with its `extends` chain.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResolvedEnvironmentDto {
    pub name: String,
    pub variables: Vec<ResolvedVariableDto>,
    pub warnings: Vec<String>,
}

/// The `process.env` map plus the keys whose value came from a `.env` file.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessEnvDto {
    pub vars: std::collections::HashMap<String, String>,
    pub dot_env_keys: Vec<String>,
}

fn to_resolved_dto(name: &str, flat: &rocket_environment::Flattened) -> ResolvedEnvironmentDto {
    ResolvedEnvironmentDto {
        name: name.to_string(),
        variables: flat
            .variables
            .iter()
            .map(|f| ResolvedVariableDto {
                key: f.variable.key.clone(),
                value: f.variable.value.clone(),
                enabled: f.variable.enabled,
                secret: f.variable.secret,
                source: f.source.clone(),
            })
            .collect(),
        warnings: flat.warnings.clone(),
    }
}

fn to_process_env_dto(env: rocket_app::ProcessEnv) -> ProcessEnvDto {
    ProcessEnvDto {
        vars: env.vars,
        dot_env_keys: env.dot_env_keys,
    }
}

/// Reads an environment with its `extends` chain applied. `None` for `collection` reads
/// the global store. A cycle comes back as an error naming the environments.
#[tauri::command]
pub fn get_resolved_environment(
    collection: Option<String>,
    name: String,
    exec: State<'_, RequestExecutionService>,
) -> Result<ResolvedEnvironmentDto, String> {
    Environment::validate_name(&name).map_err(|e| e.to_string())?;
    let flat = exec
        .resolved_environment(collection.as_deref(), &name)
        .map_err(|e| e.to_string())?;
    Ok(to_resolved_dto(&name, &flat))
}

/// The host environment for `{{process.env.*}}`. With a `collection` that is not allowed
/// host environment access on this computer, the map is empty. Without a collection (a
/// scratch request, a preview) it is the full map. `.env` files of the active
/// environments are layered under the real OS environment.
#[tauri::command]
pub fn get_process_env_vars(
    collection: Option<String>,
    environment: Option<String>,
    global_environment: Option<String>,
    exec: State<'_, RequestExecutionService>,
) -> ProcessEnvDto {
    to_process_env_dto(exec.process_env_for(
        collection.as_deref(),
        environment.as_deref(),
        global_environment.as_deref(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_environment::{FlatVariable, Flattened, Variable};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn flat_one(secret: bool) -> Flattened {
        let mut v = Variable::new("k", "v");
        v.secret = secret;
        Flattened {
            variables: vec![FlatVariable {
                variable: v,
                source: "base".into(),
            }],
            warnings: vec!["w".into()],
        }
    }

    #[test]
    fn resolved_dto_keeps_source_and_secret_flag() {
        let dto = to_resolved_dto("child", &flat_one(true));
        assert_eq!(dto.name, "child");
        assert_eq!(dto.variables[0].source, "base");
        assert!(dto.variables[0].secret);
        assert!(dto.variables[0].enabled);
        assert_eq!(dto.warnings, vec!["w".to_string()]);
    }

    #[test]
    fn resolved_dto_serialises_camel_case() -> TestResult {
        let json = serde_json::to_value(to_resolved_dto("c", &flat_one(false)))?;
        assert_eq!(json["variables"][0]["source"], "base");
        assert!(json["warnings"].is_array());
        let json = serde_json::to_value(to_process_env_dto(rocket_app::ProcessEnv {
            vars: Default::default(),
            dot_env_keys: vec!["A".into()],
        }))?;
        assert_eq!(json["dotEnvKeys"][0], "A");
        Ok(())
    }

    #[test]
    fn process_env_dto_always_has_a_dot_env_keys_array() -> TestResult {
        let dto = to_process_env_dto(rocket_app::ProcessEnv::default());
        let json = serde_json::to_value(&dto)?;
        assert!(json["dotEnvKeys"].is_array());
        assert_eq!(json["dotEnvKeys"].as_array().map(Vec::len), Some(0));
        Ok(())
    }

    #[test]
    fn a_cycle_maps_to_an_error_string_naming_the_environments() {
        let mut a = Environment::new("a");
        a.extends = Some("b".into());
        let mut b = Environment::new("b");
        b.extends = Some("a".into());
        let lookup = |n: &str| match n {
            "a" => Some(a.clone()),
            "b" => Some(b.clone()),
            _ => None,
        };
        let err = match rocket_environment::flatten_environment(&a, &lookup) {
            Ok(_) => String::new(),
            Err(e) => e.to_string(),
        };
        assert!(err.contains("a -> b -> a"), "{err}");
    }
}
