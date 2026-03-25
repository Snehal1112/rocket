use rocket_infra::NotifyFileWatcher;
use rocket_shared::error::DomainError;
use rocket_shared::events::EventPublisher;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub fn get_app_data_dir() -> Result<String, DomainError> {
    let dir = dirs::home_dir()
        .ok_or_else(|| DomainError::Internal("Home directory not found".into()))?
        .join(".rocket-api");
    Ok(dir.to_string_lossy().to_string())
}

#[tauri::command]
pub fn watch_collections(
    watcher: State<'_, NotifyFileWatcher>,
    publisher: State<'_, Arc<dyn EventPublisher>>,
) -> Result<(), DomainError> {
    let collections_dir = dirs::home_dir()
        .ok_or_else(|| DomainError::Internal("Home directory not found".into()))?
        .join(".rocket-api")
        .join("collections");
    watcher
        .start(collections_dir, Arc::clone(&publisher))
        .map_err(DomainError::Internal)
}

#[tauri::command]
pub fn stop_watching(watcher: State<'_, NotifyFileWatcher>) {
    watcher.stop();
}
