use std::sync::Arc;

use rocket_app::AcpSessionService;
use rocket_shared::error::DomainError;
use tauri::State;

use crate::mcp::registry::McpServerRegistry;

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
    mcp_registry: State<'_, Arc<McpServerRegistry>>,
) -> Result<(), DomainError> {
    let result = svc.end_session(&session_id).await;
    // Always sweep the MCP server, even if the ACP session was already gone
    // (e.g. the agent process had already crashed) -- a no-op if this
    // session never had one (agent autonomy was off).
    mcp_registry.end_session(&session_id);
    result
}
