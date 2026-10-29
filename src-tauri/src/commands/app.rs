use rocket_shared::error::DomainError;

#[tauri::command]
pub fn get_app_data_dir() -> Result<String, DomainError> {
    let dir = rocket_shared::paths::home_dir()
        .ok_or_else(|| DomainError::Internal("Home directory not found".into()))?
        .join(".rocket-api");
    Ok(dir.to_string_lossy().to_string())
}
