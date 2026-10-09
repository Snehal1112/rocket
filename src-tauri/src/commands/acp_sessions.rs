use std::sync::Arc;

use rocket_acp::SessionInfo;
use rocket_app::{AcpSessionService, CollectionService, McpHttpServerCredentials};
use rocket_shared::error::DomainError;
use tauri::State;

use crate::agent_session::cleanup::{SessionResourceRegistry, SessionResources};
use crate::agent_session::scratch::SessionScratch;
use crate::commands::acp_session_dto::{
    prompt_parts, AgentSessionStartedDto, ConfigOptionDto, PromptResourceDto,
};
use crate::mcp::registry::McpServerRegistry;

#[tauri::command]
pub async fn start_agent_session(
    agent_config_id: String,
    cwd: String,
    collection: String,
    app_handle: tauri::AppHandle,
    collection_svc: State<'_, CollectionService>,
    registry: State<'_, Arc<McpServerRegistry>>,
    resources: State<'_, Arc<SessionResourceRegistry>>,
    svc: State<'_, AcpSessionService>,
) -> Result<AgentSessionStartedDto, DomainError> {
    start_agent_session_inner(
        agent_config_id,
        cwd,
        collection,
        app_handle,
        &collection_svc,
        &registry,
        &resources,
        &svc,
    )
    .await
    .map(AgentSessionStartedDto::from)
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
///
/// Every session starts isolated. A fresh `SessionScratch` provides an empty
/// working directory and an empty `CLAUDE_CONFIG_DIR`, so the agent loads no
/// user, project or local settings, and `SessionIsolation` adds the `_meta`
/// options that switch off built-in tools. The frontend's requested cwd is
/// ignored for that reason. The scratch and the pre-handshake MCP id are
/// registered under the real session id, and `TauriSessionCleanup` releases
/// them on every end path.
#[allow(clippy::too_many_arguments)]
pub async fn start_agent_session_inner<R: tauri::Runtime>(
    agent_config_id: String,
    _requested_cwd: String,
    collection: String,
    app_handle: tauri::AppHandle<R>,
    collection_svc: &CollectionService,
    registry: &McpServerRegistry,
    resources: &SessionResourceRegistry,
    svc: &AcpSessionService,
) -> Result<SessionInfo, DomainError> {
    let autonomy_enabled = collection_svc
        .get_settings(&collection)?
        .agent_autonomy_enabled;

    // Created before the MCP server, so a failure here leaves nothing bound.
    let scratch = SessionScratch::create().map_err(|e| {
        DomainError::Internal(format!("failed to create the agent scratch directory: {e}"))
    })?;
    let (scratch_cwd, isolation) = scratch.isolation()?;

    // A Rocket-minted, pre-handshake-only identifier. See this function's
    // doc comment. It is kept so cleanup can forget the tool server's cache.
    let mcp_session_id = autonomy_enabled.then(|| uuid::Uuid::new_v4().to_string());
    let mcp_handle = match &mcp_session_id {
        Some(id) => Some(
            crate::mcp::tool_server::spawn_mcp_http_server(app_handle, id.clone())
                .await
                .map_err(|e| {
                    DomainError::Internal(format!("failed to start MCP tool server: {e}"))
                })?,
        ),
        None => None,
    };
    let mcp_credentials = mcp_handle.as_ref().map(|h| McpHttpServerCredentials {
        port: h.port,
        token: h.token.clone(),
    });

    let result = svc
        .start_session(
            &agent_config_id,
            &scratch_cwd,
            &collection,
            mcp_credentials,
            Some(isolation),
        )
        .await;

    match (result, mcp_handle) {
        (Ok(info), handle) => {
            // Registered under the real ACP session id, which every other
            // command addresses a session by.
            if let Some(handle) = handle {
                registry.register(info.session_id.clone(), handle);
            }
            resources.register(
                info.session_id.clone(),
                SessionResources {
                    scratch,
                    mcp_session_id,
                },
            );
            // Tracked last, so a sweep never sees a session whose resources
            // are not registered yet.
            if !svc.track(&info.session_id) {
                // The app is shutting down. Nothing tracks this session, so
                // release its resources here and kill its agent.
                registry.end_session(&info.session_id);
                drop(resources.take(&info.session_id));
                let _ = svc.end_session(&info.session_id).await;
                return Err(DomainError::Internal(
                    "the app is shutting down".to_string(),
                ));
            }
            // The `start_agent_session` command maps this to the DTO with
            // `.map(AgentSessionStartedDto::from)`, as Plan 01 left it.
            Ok(info)
        }
        (Err(e), handle) => {
            // Never leave a bound listener with a live token behind.
            if let Some(handle) = handle {
                handle.shutdown();
            }
            // Dropping the scratch removes its directories.
            drop(scratch);
            Err(e)
        }
    }
}

/// Sends one prompt turn. `resources` become embedded text resources ahead
/// of the prompt text. Resolves with the stop reason; a stopped turn
/// resolves with `cancelled`.
#[tauri::command]
pub async fn send_agent_prompt(
    session_id: String,
    prompt: String,
    resources: Option<Vec<PromptResourceDto>>,
    svc: State<'_, AcpSessionService>,
) -> Result<String, DomainError> {
    let parts = prompt_parts(prompt, resources)?;
    svc.send_prompt(&session_id, parts).await
}

/// Asks the agent to stop the running turn. The session stays open.
#[tauri::command]
pub async fn cancel_agent_prompt(
    session_id: String,
    svc: State<'_, AcpSessionService>,
) -> Result<(), DomainError> {
    svc.cancel(&session_id).await
}

/// Changes one session option, such as the model or the effort level, and
/// returns the agent's new option list.
#[tauri::command]
pub async fn set_agent_config_option(
    session_id: String,
    config_id: String,
    value: String,
    svc: State<'_, AcpSessionService>,
) -> Result<Vec<ConfigOptionDto>, DomainError> {
    let options = svc
        .set_config_option(&session_id, &config_id, &value)
        .await?;
    Ok(options.into_iter().map(ConfigOptionDto::from).collect())
}

#[tauri::command]
pub async fn end_agent_session(
    session_id: String,
    svc: State<'_, AcpSessionService>,
) -> Result<(), DomainError> {
    // AcpSessionService runs TauriSessionCleanup, which ends the MCP server,
    // forgets the tool caches and removes the scratch directories.
    svc.end_session(&session_id).await
}

/// Ends every agent session the backend still tracks and returns how many.
/// The webview calls this once per load, from its app-lifetime assistant
/// event bridge, and every assistant start waits for it. At that point the
/// webview owns no session, so every tracked session is a leftover from
/// before a reload.
#[tauri::command]
pub async fn end_stale_assistant_sessions(
    svc: State<'_, AcpSessionService>,
) -> Result<usize, DomainError> {
    Ok(svc.end_tracked_sessions().await)
}
