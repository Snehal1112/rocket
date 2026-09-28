// Fixture ACP agent used by integration tests in rocket-infra. Speaks real
// ACP over stdio so tests can exercise the client transport against a real
// (if trivial) agent process instead of a fake.
use agent_client_protocol::schema::v1::{
    AgentCapabilities, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse,
    McpCapabilities, NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse,
    SessionId, SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Responder, Result, Stdio};

#[tokio::main]
async fn main() -> Result<()> {
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
                responder.respond(
                    InitializeResponse::new(req.protocol_version).agent_capabilities(
                        AgentCapabilities::new().mcp_capabilities(mcp_capabilities),
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
                responder.respond(NewSessionResponse::new(SessionId::new("fixture-session")))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |req: PromptRequest,
                        responder: Responder<PromptResponse>,
                        conn: ConnectionTo<Client>| {
                // Test hook: a sentinel prompt text triggers an abrupt,
                // uncooperative exit (no response, connection just drops) so
                // Task 3's crash-handling path can be exercised against a
                // real broken connection rather than only a fake.
                if let Some(ContentBlock::Text(text)) = req.prompt.first() {
                    if text.text == "__CRASH__" {
                        std::process::exit(1);
                    }
                    // Test hook: never answer, so tests can cancel a prompt
                    // that is still in flight.
                    if text.text == "__HANG__" {
                        std::future::pending::<()>().await;
                    }
                }
                conn.send_notification(SessionNotification::new(
                    req.session_id,
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                        TextContent::new("fixture reply"),
                    ))),
                ))?;
                responder.respond(PromptResponse::new(StopReason::EndTurn))
            },
            agent_client_protocol::on_receive_request!(),
        )
        .connect_to(Stdio::new())
        .await
}
