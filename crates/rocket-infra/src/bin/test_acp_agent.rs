// Fixture ACP agent used by integration tests in rocket-infra. Speaks real
// ACP over stdio so tests can exercise the client transport against a real
// (if trivial) agent process instead of a fake.
use agent_client_protocol::schema::v1::{
    AgentCapabilities, ContentBlock, ContentChunk, InitializeRequest, InitializeResponse,
    NewSessionRequest, NewSessionResponse, PromptRequest, PromptResponse, SessionId,
    SessionNotification, SessionUpdate, StopReason, TextContent,
};
use agent_client_protocol::{Agent, Client, ConnectionTo, Responder, Result, Stdio};

#[tokio::main]
async fn main() -> Result<()> {
    Agent
        .builder()
        .on_receive_request(
            async move |req: InitializeRequest, responder, _conn: ConnectionTo<Client>| {
                responder.respond(
                    InitializeResponse::new(req.protocol_version)
                        .agent_capabilities(AgentCapabilities::new()),
                )
            },
            agent_client_protocol::on_receive_request!(),
        )
        .on_receive_request(
            async move |_req: NewSessionRequest,
                        responder: Responder<NewSessionResponse>,
                        _conn: ConnectionTo<Client>| {
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
