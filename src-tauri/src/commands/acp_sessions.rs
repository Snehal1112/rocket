use rocket_app::AcpSessionService;
use rocket_shared::error::DomainError;
use tauri::State;

#[tauri::command]
pub async fn start_agent_session(
    agent_config_id: String,
    cwd: String,
    collection: Option<String>,
    svc: State<'_, AcpSessionService>,
) -> Result<String, DomainError> {
    svc.start_session(&agent_config_id, &cwd, collection.as_deref())
        .await
}

#[tauri::command]
pub async fn send_agent_prompt(
    session_id: String,
    prompt: String,
    svc: State<'_, AcpSessionService>,
) -> Result<String, DomainError> {
    svc.send_prompt(&session_id, prompt).await
}

#[tauri::command]
pub async fn end_agent_session(
    session_id: String,
    svc: State<'_, AcpSessionService>,
) -> Result<(), DomainError> {
    svc.end_session(&session_id).await
}
