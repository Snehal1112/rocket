// Fixture ACP agent used by integration tests in rocket-infra. Speaks real
// ACP over stdio so tests can exercise the client transport against a real
// (if trivial) agent process instead of a fake.
use std::sync::Arc;

use agent_client_protocol::schema::v1::{
    AgentCapabilities, CancelNotification, ConfigOptionUpdate, ContentBlock, ContentChunk, Cost,
    EmbeddedResourceResource, InitializeRequest, InitializeResponse, McpCapabilities,
    NewSessionRequest, NewSessionResponse, PermissionOption, PermissionOptionKind,
    PromptCapabilities, PromptRequest, PromptResponse, RequestPermissionOutcome,
    RequestPermissionRequest, SessionConfigOption, SessionConfigOptionCategory,
    SessionConfigSelectOption, SessionId, SessionNotification, SessionUpdate,
    SetSessionConfigOptionRequest, SetSessionConfigOptionResponse, StopReason, TextContent,
    ToolCall, ToolCallStatus, ToolCallUpdate, ToolCallUpdateFields, ToolKind, UsageUpdate,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Responder, Result, Stdio};
use tokio::sync::Notify;

/// The fixture's session options. Choosing the `opus` model adds an effort
/// option, like the real adapter does for models that support it. The
/// boolean `fast` option is a test hook: Rocket must skip boolean options.
fn fixture_config_options(model: &str) -> Vec<SessionConfigOption> {
    let mut options = vec![
        SessionConfigOption::select(
            "model",
            "Model",
            model.to_string(),
            vec![
                SessionConfigSelectOption::new("default", "Default"),
                SessionConfigSelectOption::new("opus", "Opus").description("Most capable"),
            ],
        )
        .category(SessionConfigOptionCategory::Model),
        SessionConfigOption::boolean("fast", "Fast mode", false),
    ];
    if model == "opus" {
        options.push(
            SessionConfigOption::select(
                "effort",
                "Effort",
                "high",
                vec![
                    SessionConfigSelectOption::new("low", "Low"),
                    SessionConfigSelectOption::new("high", "High"),
                ],
            )
            .category(SessionConfigOptionCategory::ThoughtLevel),
        );
    }
    options
}

/// A text chunk of the agent's reply.
fn text_update(text: &str) -> SessionUpdate {
    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(TextContent::new(
        text,
    ))))
}

/// Describes the prompt's blocks, so tests can see how the client sent them.
fn describe_blocks(blocks: &[ContentBlock]) -> String {
    blocks
        .iter()
        .map(|block| match block {
            ContentBlock::Text(_) => "text".to_string(),
            ContentBlock::Resource(resource) => match &resource.resource {
                EmbeddedResourceResource::TextResourceContents(contents) => format!(
                    "resource:{}:{}",
                    contents.uri,
                    contents.mime_type.as_deref().unwrap_or("none")
                ),
                _ => "resource:blob".to_string(),
            },
            _ => "other".to_string(),
        })
        .collect::<Vec<_>>()
        .join("|")
}

#[tokio::main]
async fn main() -> Result<()> {
    // Set by `session/cancel` and awaited by a `__WAIT_FOR_CANCEL__` prompt.
    // `notify_one` keeps a permit, so a cancel that arrives first is not lost.
    let cancel = Arc::new(Notify::new());
    let cancel_for_prompt = Arc::clone(&cancel);

    Agent
        .builder()
        .on_receive_request(
            async move |req: InitializeRequest, responder, _conn: ConnectionTo<Client>| {
                // Test hook: lets tests prove `AcpAgentClient::start_session`
                // reads `InitializeResponse.agent_capabilities.mcp_capabilities.http`
                // correctly whether it is `false` (the default here) or `true`.
                let mcp_capabilities =
                    if std::env::var("FIXTURE_ADVERTISE_MCP_HTTP").as_deref() == Ok("1") {
                        McpCapabilities::new().http(true)
                    } else {
                        McpCapabilities::new()
                    };
                // Test hook: embedded context is advertised unless this is set.
                let embedded_context =
                    std::env::var("FIXTURE_NO_EMBEDDED_CONTEXT").as_deref() != Ok("1");
                responder.respond(
                    InitializeResponse::new(req.protocol_version).agent_capabilities(
                        AgentCapabilities::new()
                            .mcp_capabilities(mcp_capabilities)
                            .prompt_capabilities(
                                PromptCapabilities::new()
                                    .image(true)
                                    .embedded_context(embedded_context),
                            ),
                    ),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: NewSessionRequest,
                        responder: Responder<NewSessionResponse>,
                        _conn: ConnectionTo<Client>| {
                // Test hook: dumps the `mcp_servers` this session request
                // carried, so integration tests can assert on
                // `AcpAgentClient`'s `McpServerSpec` -> `McpServer` mapping
                // without implementing an MCP client themselves.
                if let Ok(path) = std::env::var("MCP_SERVERS_DUMP_PATH") {
                    let dump = serde_json::to_string(&req.mcp_servers)
                        .unwrap_or_else(|e| format!("<serialize error: {e}>"));
                    let _ = std::fs::write(path, dump);
                }
                // Test hook: dumps the `_meta` this session request carried.
                if let Ok(path) = std::env::var("SESSION_META_DUMP_PATH") {
                    let dump = serde_json::to_string(&req.meta)
                        .unwrap_or_else(|e| format!("<serialize error: {e}>"));
                    let _ = std::fs::write(path, dump);
                }
                responder.respond(
                    NewSessionResponse::new(SessionId::new("fixture-session"))
                        .config_options(fixture_config_options("default")),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: SetSessionConfigOptionRequest,
                        responder: Responder<SetSessionConfigOptionResponse>,
                        _conn: ConnectionTo<Client>| {
                let model = req
                    .value
                    .as_value_id()
                    .map(|value| value.to_string())
                    .unwrap_or_default();
                responder.respond(SetSessionConfigOptionResponse::new(fixture_config_options(
                    &model,
                )))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_notification(
            move |_notification: CancelNotification, _conn: ConnectionTo<Client>| {
                let cancel = Arc::clone(&cancel);
                async move {
                    cancel.notify_one();
                    Ok(())
                }
            },
            agent_client_protocol::on_receive_notification!(),
        )
        .on_receive_request(
            move |req: PromptRequest,
                  responder: Responder<PromptResponse>,
                  conn: ConnectionTo<Client>| {
                let cancel = Arc::clone(&cancel_for_prompt);
                async move {
                    // Sentinels may sit in any text block, because resources
                    // come before the prompt text.
                    let texts: Vec<String> = req
                        .prompt
                        .iter()
                        .filter_map(|block| match block {
                            ContentBlock::Text(text) => Some(text.text.clone()),
                            _ => None,
                        })
                        .collect();
                    let has = |sentinel: &str| texts.iter().any(|t| t.as_str() == sentinel);
                    // Test hook: an abrupt, uncooperative exit (no response,
                    // the connection just drops) for the crash-handling path.
                    if has("__CRASH__") {
                        std::process::exit(1);
                    }
                    // Test hook: never answer, so tests can end a prompt
                    // that is still in flight.
                    if has("__HANG__") {
                        std::future::pending::<()>().await;
                    }
                    let session_id = req.session_id.clone();
                    // Test hook: answer `cancelled` once `session/cancel`
                    // arrives. The answer comes from a spawned task, so the
                    // dispatch loop stays free to receive the cancel.
                    if has("__WAIT_FOR_CANCEL__") {
                        return conn.spawn(async move {
                            cancel.notified().await;
                            responder.respond(PromptResponse::new(StopReason::Cancelled))
                        });
                    }
                    // Test hook: ask the client for a permission and report
                    // its answer as reply text. Spawned, because waiting for
                    // the answer inside a handler would block the loop.
                    if has("__PERMISSION__") {
                        let task_conn = conn.clone();
                        return conn.spawn(async move {
                            let response = task_conn
                                .send_request(RequestPermissionRequest::new(
                                    session_id.clone(),
                                    ToolCallUpdate::new(
                                        "call-p",
                                        ToolCallUpdateFields::new()
                                            .title("Run a command".to_string()),
                                    ),
                                    vec![
                                        PermissionOption::new(
                                            "allow-once",
                                            "Allow",
                                            PermissionOptionKind::AllowOnce,
                                        ),
                                        PermissionOption::new(
                                            "reject-once",
                                            "Reject",
                                            PermissionOptionKind::RejectOnce,
                                        ),
                                    ],
                                ))
                                .block_task()
                                .await?;
                            let outcome = match response.outcome {
                                RequestPermissionOutcome::Selected(selected) => {
                                    format!("permission:selected:{}", selected.option_id)
                                }
                                RequestPermissionOutcome::Cancelled => {
                                    "permission:cancelled".to_string()
                                }
                                _ => "permission:other".to_string(),
                            };
                            task_conn.send_notification(SessionNotification::new(
                                session_id,
                                text_update(&outcome),
                            ))?;
                            responder.respond(PromptResponse::new(StopReason::EndTurn))
                        });
                    }
                    // Test hook: one of each typed update before the reply.
                    if has("__TOOLS__") {
                        for update in [
                            SessionUpdate::ToolCall(
                                ToolCall::new("call-1", "Read file")
                                    .kind(ToolKind::Read)
                                    .status(ToolCallStatus::Pending),
                            ),
                            SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
                                "call-1",
                                ToolCallUpdateFields::new().status(ToolCallStatus::Completed),
                            )),
                            SessionUpdate::UsageUpdate(
                                UsageUpdate::new(53_000, 200_000).cost(Cost::new(0.045, "USD")),
                            ),
                            SessionUpdate::ConfigOptionUpdate(ConfigOptionUpdate::new(
                                fixture_config_options("opus"),
                            )),
                        ] {
                            conn.send_notification(SessionNotification::new(
                                session_id.clone(),
                                update,
                            ))?;
                        }
                    }
                    // Test hook: reply with a description of the prompt blocks.
                    let reply = if has("__DESCRIBE__") {
                        describe_blocks(&req.prompt)
                    } else {
                        "fixture reply".to_string()
                    };
                    conn.send_notification(SessionNotification::new(
                        session_id,
                        text_update(&reply),
                    ))?;
                    responder.respond(PromptResponse::new(StopReason::EndTurn))
                }
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(Stdio::new())
        .await
}
