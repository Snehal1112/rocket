use rocket_app::{
    RequestExecutionService, WebSocketConnectInput, WebSocketSendInput, WebSocketService,
};
use rocket_shared::error::DomainError;
use tauri::State;

/// Opens a session under a caller-chosen id. Frames and status changes arrive as the
/// `ws:message` and `ws:status` events; this only reports whether the connect succeeded.
#[tauri::command]
pub async fn ws_connect(
    session_id: String,
    input: WebSocketConnectInput,
    exec: State<'_, RequestExecutionService>,
    sessions: State<'_, WebSocketService>,
) -> Result<(), DomainError> {
    // The id is reserved before resolving, so a disconnect during a slow vault fetch cancels
    // the connect instead of being ignored.
    sessions
        .connect_with(&session_id, exec.resolve_websocket(&input))
        .await
}

#[tauri::command]
pub async fn ws_send(
    session_id: String,
    input: WebSocketSendInput,
    exec: State<'_, RequestExecutionService>,
    sessions: State<'_, WebSocketService>,
) -> Result<(), DomainError> {
    let frame = exec
        .resolve_websocket_message(&input.scope, input.kind, &input.data)
        .await?;
    sessions.send(&session_id, frame).await
}

#[tauri::command]
pub async fn ws_disconnect(
    session_id: String,
    sessions: State<'_, WebSocketService>,
) -> Result<(), DomainError> {
    sessions.disconnect(&session_id).await
}
