use rocket_app::HistoryService;
use rocket_history::{HistoryEntry, HistoryFilter};
use rocket_shared::error::DomainError;
use tauri::State;

#[tauri::command]
pub fn list_history(
    limit: Option<usize>,
    svc: State<'_, HistoryService>,
) -> Result<Vec<HistoryEntry>, DomainError> {
    svc.list(limit)
}

#[tauri::command]
pub fn get_history_entry(
    id: String,
    svc: State<'_, HistoryService>,
) -> Result<HistoryEntry, DomainError> {
    svc.get(&id)
}

#[tauri::command]
pub fn clear_history(svc: State<'_, HistoryService>) -> Result<(), DomainError> {
    svc.clear()
}

#[tauri::command]
pub fn search_history(
    filter: HistoryFilter,
    svc: State<'_, HistoryService>,
) -> Result<Vec<HistoryEntry>, DomainError> {
    svc.search(&filter)
}
