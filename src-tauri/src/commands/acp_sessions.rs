use std::sync::Arc;

use rocket_acp::{ConfigOption, SessionInfo};
use rocket_app::{
    AcpSessionService, AssistantMode, CollectionService, McpHttpServerCredentials, McpToolService,
    SessionIsolation, WORKSPACE_ASSISTANT_INSTRUCTIONS,
};
use rocket_shared::error::DomainError;
use tauri::{Manager, State};

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
    // Used after the handshake to record the session's mode;
    // `app_handle` itself moves into `spawn_mcp_http_server`.
    let mode_handle = app_handle.clone();
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
                // Tag later tool calls with the real session id, so the mode
                // and the test-result cache share the key the session end
                // clears.
                handle.binding.bind(&info.session_id);
                // The per-tab chat keeps its old reach until Plan 05 removes
                // it: every tool is available, and the run switch still gates
                // running and writing.
                if let Some(mcp_tool_svc) = mode_handle.try_state::<Arc<McpToolService>>() {
                    mcp_tool_svc.open_session(&info.session_id, AssistantMode::Agent);
                }
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
                // kill its agent and run its cleanup here.
                let _ = svc.end_untracked(&info.session_id).await;
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
    mcp_tool_svc: State<'_, Arc<McpToolService>>,
) -> Result<String, DomainError> {
    let mut parts = prompt_parts(prompt, resources)?;
    // The workspace outline goes with the first prompt of a workspace
    // assistant session only. Per-tab sessions never stored one, so this
    // adds nothing for them. It is only peeked here and discarded once the
    // agent accepted the prompt, so a refused or failed prompt keeps it for
    // the next one.
    let has_outline = match mcp_tool_svc.peek_outline_preamble(&session_id) {
        Some(preamble) => {
            parts.insert(0, preamble);
            true
        }
        None => false,
    };
    let stop_reason = svc.send_prompt(&session_id, parts).await?;
    if has_outline {
        mcp_tool_svc.discard_outline(&session_id);
    }
    Ok(stop_reason)
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

/// Changes the workspace assistant's mode. Needs no restart: the tool list
/// stays the same, and each tool checks the mode when it is called.
#[tauri::command]
pub async fn set_assistant_mode(
    session_id: String,
    mode: AssistantMode,
    mcp_tool_svc: State<'_, Arc<McpToolService>>,
) -> Result<(), DomainError> {
    mcp_tool_svc.set_mode(&session_id, mode)
}

/// The ACP config option id the adapter uses for the model.
const MODEL_CONFIG_ID: &str = "model";

/// The model value to send right after start, or `None` when nothing
/// should be sent: no model was asked for, the agent reports no `model`
/// option, the asked-for model is not one of its choices (a remembered
/// choice the credential no longer offers), or it is already current.
fn model_to_apply<'a>(options: &[ConfigOption], requested: Option<&'a str>) -> Option<&'a str> {
    let requested = requested?;
    let option = options.iter().find(|o| o.id == MODEL_CONFIG_ID)?;
    let offered = option.choices.iter().any(|choice| choice.value == requested);
    (offered && option.current_value != requested).then_some(requested)
}

/// Starts the workspace assistant for the active workspace. The frontend
/// sends no path: every tool resolves the active workspace itself.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn start_workspace_assistant(
    agent_config_id: String,
    mode: AssistantMode,
    model: Option<String>,
    app_handle: tauri::AppHandle,
    registry: State<'_, Arc<McpServerRegistry>>,
    mcp_tool_svc: State<'_, Arc<McpToolService>>,
    resources: State<'_, Arc<SessionResourceRegistry>>,
    svc: State<'_, AcpSessionService>,
) -> Result<AgentSessionStartedDto, DomainError> {
    start_workspace_assistant_inner(
        agent_config_id,
        mode,
        model,
        app_handle,
        &registry,
        &mcp_tool_svc,
        &resources,
        &svc,
    )
    .await
}

/// The work behind `start_workspace_assistant`, generic over the runtime so
/// tests can drive it with `tauri::test::MockRuntime` (same reason as
/// `start_agent_session_inner`).
///
/// Order matters: the tool server must run before the handshake (its port
/// and token go into `session/new`). After the handshake, everything is
/// recorded under the real ACP id: the binding, the mode and the outline,
/// the registry entry and the scratch hand-off. The session is tracked last,
/// so a sweep never sees a session whose resources are not registered yet.
#[allow(clippy::too_many_arguments)]
pub async fn start_workspace_assistant_inner<R: tauri::Runtime>(
    agent_config_id: String,
    mode: AssistantMode,
    model: Option<String>,
    app_handle: tauri::AppHandle<R>,
    registry: &McpServerRegistry,
    mcp_tool_svc: &McpToolService,
    resources: &SessionResourceRegistry,
    svc: &AcpSessionService,
) -> Result<AgentSessionStartedDto, DomainError> {
    // An empty working directory and an empty CLAUDE_CONFIG_DIR, created
    // before the MCP server, so a failure here leaves nothing bound.
    let scratch = SessionScratch::create().map_err(|e| {
        DomainError::Internal(format!("failed to create the agent scratch directory: {e}"))
    })?;
    let (cwd, isolation) = scratch.isolation()?;
    // The workspace assistant's own instructions replace the default
    // Rocket prompt; the config dir stays the scratch one.
    let isolation = SessionIsolation {
        system_prompt_append: WORKSPACE_ASSISTANT_INSTRUCTIONS.to_string(),
        ..isolation
    };
    let provisional_id = uuid::Uuid::new_v4().to_string();
    let handle =
        crate::mcp::tool_server::spawn_mcp_http_server(app_handle, provisional_id.clone())
            .await
            .map_err(|e| DomainError::Internal(format!("failed to start MCP tool server: {e}")))?;
    let credentials = McpHttpServerCredentials {
        port: handle.port,
        token: handle.token.clone(),
    };

    let info = match svc
        .start_workspace_session(&agent_config_id, &cwd, credentials, isolation)
        .await
    {
        Ok(info) => info,
        Err(e) => {
            // Never leave a bound listener with a live token behind. The
            // scratch directories go when `scratch` is dropped here.
            handle.shutdown();
            return Err(e);
        }
    };

    handle.binding.bind(&info.session_id);
    mcp_tool_svc.begin_assistant_session(
        &info.session_id,
        mode,
        info.prompt_capabilities.embedded_context,
    );
    registry.register(info.session_id.clone(), handle);
    // Cleanup removes the scratch and forgets both ids when the session
    // ends on any path.
    resources.register(
        info.session_id.clone(),
        SessionResources {
            scratch,
            mcp_session_id: Some(provisional_id),
        },
    );
    // Tracked last, so a sweep never sees a session whose resources are
    // not registered yet.
    if !svc.track(&info.session_id) {
        // The app is shutting down. Nothing tracks this session, so kill
        // its agent and run its cleanup here.
        let _ = svc.end_untracked(&info.session_id).await;
        return Err(DomainError::Internal(
            "the app is shutting down".to_string(),
        ));
    }

    let mut config_options = info.config_options;
    if let Some(value) = model_to_apply(&config_options, model.as_deref()) {
        // A failed model change keeps the agent's default model rather
        // than failing a session that is already running.
        if let Ok(updated) = svc
            .set_config_option(&info.session_id, MODEL_CONFIG_ID, value)
            .await
        {
            config_options = updated;
        }
    }

    Ok(AgentSessionStartedDto {
        session_id: info.session_id,
        config_options: config_options
            .into_iter()
            .map(ConfigOptionDto::from)
            .collect(),
    })
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

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_acp::ConfigChoice;

    fn model_option(current: &str, choices: &[&str]) -> ConfigOption {
        ConfigOption {
            id: MODEL_CONFIG_ID.to_string(),
            name: "Model".to_string(),
            category: Some("model".to_string()),
            current_value: current.to_string(),
            choices: choices
                .iter()
                .map(|value| ConfigChoice {
                    value: value.to_string(),
                    name: value.to_string(),
                    description: None,
                })
                .collect(),
        }
    }

    #[test]
    fn model_to_apply_skips_unknown_and_current_models() {
        let options = vec![model_option("default", &["default", "opus", "sonnet"])];

        assert_eq!(model_to_apply(&options, Some("opus")), Some("opus"));
        assert_eq!(model_to_apply(&options, Some("default")), None, "already current");
        assert_eq!(
            model_to_apply(&options, Some("retired-model")),
            None,
            "a remembered model the credential no longer offers is not sent"
        );
        assert_eq!(model_to_apply(&options, None), None);
        assert_eq!(model_to_apply(&[], Some("opus")), None, "no model option");
    }
}
