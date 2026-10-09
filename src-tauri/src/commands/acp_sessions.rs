use std::sync::Arc;

use rocket_acp::{PromptPart, SessionInfo};
use rocket_app::{AcpSessionService, CollectionService, McpHttpServerCredentials, McpToolService};
use rocket_shared::error::DomainError;
use tauri::State;

use crate::mcp::registry::McpServerRegistry;

#[tauri::command]
pub async fn start_agent_session(
    agent_config_id: String,
    cwd: String,
    collection: String,
    app_handle: tauri::AppHandle,
    collection_svc: State<'_, CollectionService>,
    registry: State<'_, Arc<McpServerRegistry>>,
    svc: State<'_, AcpSessionService>,
) -> Result<String, DomainError> {
    start_agent_session_inner(
        agent_config_id,
        cwd,
        collection,
        app_handle,
        &collection_svc,
        &registry,
        &svc,
    )
    .await
    .map(|info| info.session_id)
}

/// The real orchestration behind `start_agent_session`, generic over
/// `R: tauri::Runtime` purely so tests can drive it with
/// `tauri::test::MockRuntime` instead of the production `Wry` runtime — the
/// same reasoning `RocketMcpToolServer`/`spawn_mcp_http_server`
/// (`src-tauri/src/mcp/tool_server.rs`) already document for the identical
/// problem. The `#[tauri::command]` wrapper above stays concretely typed to
/// `tauri::AppHandle` (i.e. `AppHandle<Wry>`), since that is what Tauri's IPC
/// dispatch requires; this function does the actual work and is what
/// `src-tauri/tests/acp_mcp_start_agent_session.rs` calls directly.
///
/// This resolves the open question Plan 04 explicitly left for this plan:
/// which identifier is available to pass as `spawn_mcp_http_server`'s
/// `session_id` before the ACP handshake completes. The real ACP-protocol
/// session id is only known once `svc.start_session(...)` returns, but the
/// HTTP server (and the `session_id` baked into its `RocketMcpToolServer`,
/// used to tag every `McpToolService` call and `DomainEvent::AcpToolInvoked`
/// audit event for that server's whole lifetime) must already be running
/// before the handshake, so its port/token can be put in `NewSessionRequest`.
/// This mints a Rocket-side UUID *only* for that pre-handshake purpose, and
/// separately registers the resulting handle in `McpServerRegistry` keyed by
/// the *real* post-handshake session id — because that is the id
/// `end_agent_session`/`send_agent_prompt` address a session by everywhere
/// else in this codebase.
pub async fn start_agent_session_inner<R: tauri::Runtime>(
    agent_config_id: String,
    cwd: String,
    collection: String,
    app_handle: tauri::AppHandle<R>,
    collection_svc: &CollectionService,
    registry: &McpServerRegistry,
    svc: &AcpSessionService,
) -> Result<SessionInfo, DomainError> {
    let autonomy_enabled = collection_svc
        .get_settings(&collection)?
        .agent_autonomy_enabled;

    let mcp_handle = if autonomy_enabled {
        // A Rocket-minted, pre-handshake-only identifier — see this
        // function's doc comment. It is never surfaced to the frontend and
        // never used as the session's real identity; it exists solely so
        // RocketMcpToolServer has *something* stable to tag its own tool
        // calls/audit events with for as long as this HTTP server runs.
        let mcp_session_id = uuid::Uuid::new_v4().to_string();
        Some(
            crate::mcp::tool_server::spawn_mcp_http_server(app_handle, mcp_session_id)
                .await
                .map_err(|e| {
                    DomainError::Internal(format!("failed to start MCP tool server: {e}"))
                })?,
        )
    } else {
        None
    };
    let mcp_credentials = mcp_handle.as_ref().map(|h| McpHttpServerCredentials {
        port: h.port,
        token: h.token.clone(),
    });

    let result = svc
        .start_session(&agent_config_id, &cwd, &collection, mcp_credentials)
        .await;

    match (result, mcp_handle) {
        (Ok(info), Some(handle)) => {
            // Registered under the *real* ACP session id, not the
            // pre-handshake mcp_session_id minted above — this is the id
            // end_agent_session/send_agent_prompt (and McpServerRegistry's
            // other callers) all address a session by.
            registry.register(info.session_id.clone(), handle);
            Ok(info)
        }
        (Ok(info), None) => Ok(info),
        (Err(e), Some(handle)) => {
            // start_session failed after the HTTP server was already bound —
            // never leave an orphaned listener holding a live token. `shutdown`
            // is synchronous (Plan 04) — no `.await` here.
            handle.shutdown();
            Err(e)
        }
        (Err(e), None) => Err(e),
    }
}

#[tauri::command]
pub async fn send_agent_prompt(
    session_id: String,
    prompt: String,
    svc: State<'_, AcpSessionService>,
) -> Result<String, DomainError> {
    svc.send_prompt(&session_id, vec![PromptPart::Text(prompt)])
        .await
}

#[tauri::command]
pub async fn end_agent_session(
    session_id: String,
    svc: State<'_, AcpSessionService>,
    mcp_registry: State<'_, Arc<McpServerRegistry>>,
    mcp_tool_svc: State<'_, Arc<McpToolService>>,
) -> Result<(), DomainError> {
    let result = svc.end_session(&session_id).await;
    // Always sweep the MCP server, even if the ACP session was already gone
    // (e.g. the agent process had already crashed) -- a no-op if this
    // session never had one (agent autonomy was off).
    mcp_registry.end_session(&session_id);
    // Same teardown point sweeps McpToolService::test_result_cache (Post-
    // Plan-03 review caveat (e)) -- that cache has no eviction of its own,
    // and Plan 05 mints a fresh session_id per ACP session, so entries would
    // otherwise accumulate in memory for the life of the process. A no-op
    // for a session that never ran an MCP tool call.
    mcp_tool_svc.forget_session(&session_id);
    result
}
