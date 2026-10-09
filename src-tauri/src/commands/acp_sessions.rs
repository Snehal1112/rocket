use std::sync::Arc;

use rocket_acp::ConfigOption;
use rocket_app::{
    AcpSessionService, AssistantMode, McpHttpServerCredentials, McpToolService,
    SessionIsolation, WORKSPACE_ASSISTANT_INSTRUCTIONS,
};
use rocket_shared::error::DomainError;
use tauri::State;

use crate::agent_session::cleanup::{SessionResourceRegistry, SessionResources};
use crate::agent_session::scratch::SessionScratch;
use crate::commands::acp_session_dto::{
    prompt_parts, AgentSessionStartedDto, ConfigOptionDto, PromptResourceDto,
};
use crate::mcp::registry::McpServerRegistry;

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
    // assistant session only. It is only peeked here and discarded once the
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
/// tests can drive it with `tauri::test::MockRuntime` (its
/// integration test is `src-tauri/tests/acp_mcp_start_workspace_assistant.rs`).
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

    // Mode and workspace pin come first, so no tool call can run under the
    // real id without them.
    mcp_tool_svc.begin_assistant_session(
        &info.session_id,
        mode,
        info.prompt_capabilities.embedded_context,
    );
    handle.binding.bind(&info.session_id);
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
