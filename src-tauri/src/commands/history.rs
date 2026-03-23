use rocket_app::HistoryService;
use rocket_history::HistoryEntry;
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
