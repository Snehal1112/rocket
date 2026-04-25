use tauri::Manager;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiState {
    pub active_mode: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workspace_tabs: Option<UiStateWorkspaceTabs>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub layout_direction: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiStateWorkspaceTabs {
    pub workspace_id: String,
}

#[tauri::command]
pub fn load_ui_state(app_handle: tauri::AppHandle) -> Result<Option<UiState>, String> {
    let config_dir = app_handle
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?;
    let path = config_dir.join("ui-state.yml");
    if !path.exists() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let state: UiState = serde_yaml::from_str(&content).map_err(|e| e.to_string())?;
    Ok(Some(state))
}

#[tauri::command]
pub fn save_ui_state(app_handle: tauri::AppHandle, state: UiState) -> Result<(), String> {
    let config_dir = app_handle
        .path()
        .app_config_dir()
        .map_err(|e| e.to_string())?;
    std::fs::create_dir_all(&config_dir).map_err(|e| e.to_string())?;
    let content = serde_yaml::to_string(&state).map_err(|e| e.to_string())?;
    std::fs::write(config_dir.join("ui-state.yml"), content).map_err(|e| e.to_string())
}
