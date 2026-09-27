use rocket_shared::error::DomainResult;
use tokio::sync::mpsc::UnboundedSender;

/// Protocol-focused contract for driving one ACP agent session: spawning the
/// process and handshaking, sending a prompt and streaming its response, and
/// ending the session. Has no knowledge of `DomainEvent`, Tauri, or the
/// `agent-client-protocol` crate — those live in the concrete implementation
/// (`rocket-infra`'s `AcpAgentClient`, Plan 03) and the orchestration layer
/// (`rocket-app`'s `AcpSessionService`, Plan 04) respectively.
#[async_trait::async_trait]
pub trait AcpSessionClient: Send + Sync {
    /// Spawns the agent process and performs the `initialize` → `session/new`
    /// handshake. Returns the ACP-provided `sessionId`, used as-is for every
    /// later call — no separate Rocket-side id translation layer.
    async fn start_session(
        &self,
        command: &str,
        args: &[String],
        cwd: &str,
        env: &[(String, String)],
    ) -> DomainResult<String>;

    /// Sends `session/prompt`. As `agent_message_chunk` updates arrive from
    /// the agent, their text is forwarded through `chunk_tx` — the caller
    /// reads it concurrently while this call is still pending. Resolves with
    /// the raw `stopReason` string once the agent's turn ends.
    async fn send_prompt(
        &self,
        session_id: &str,
        prompt: String,
        chunk_tx: UnboundedSender<String>,
    ) -> DomainResult<String>;

    /// Ends the session by explicitly killing the agent process — dropping a
    /// connection handle does not kill the underlying child process by
    /// default in either `std` or `tokio`, so this must be an active kill,
    /// not passive cleanup. No ACP-level shutdown handshake exists.
    async fn end_session(&self, session_id: &str) -> DomainResult<()>;

    /// Kills every currently-tracked session's process, for use when the
    /// whole application is shutting down (Tauri does not drop managed
    /// state on exit, so nothing else calls `end_session` for sessions
    /// still open at quit time). Best-effort: an individual session's kill
    /// failure must not prevent cleanup of the rest. It must also cover a
    /// session still in its handshake, and later `start_session` calls may be
    /// refused, since nothing would kill a session stored after the sweep.
    async fn end_all_sessions(&self) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::error::{DomainError, DomainResult};
    use tokio::sync::mpsc;

    struct FakeSessionClient;

    #[async_trait::async_trait]
    impl AcpSessionClient for FakeSessionClient {
        async fn start_session(
            &self,
            _command: &str,
            _args: &[String],
            _cwd: &str,
            _env: &[(String, String)],
        ) -> DomainResult<String> {
            Ok("session-1".to_string())
        }

        async fn send_prompt(
            &self,
            _session_id: &str,
            _prompt: String,
            chunk_tx: mpsc::UnboundedSender<String>,
        ) -> DomainResult<String> {
            let _ = chunk_tx.send("hello".to_string());
            Ok("end_turn".to_string())
        }

        async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
            Ok(())
        }

        async fn end_all_sessions(&self) -> DomainResult<()> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn trait_is_object_safe_and_callable_through_a_trait_object() {
        let client: Box<dyn AcpSessionClient> = Box::new(FakeSessionClient);

        let session_id = client
            .start_session("echo", &[], "/tmp", &[])
            .await
            .expect("start_session");
        assert_eq!(session_id, "session-1");

        let (tx, mut rx) = mpsc::unbounded_channel();
        let stop_reason = client
            .send_prompt(&session_id, "hi".to_string(), tx)
            .await
            .expect("send_prompt");
        assert_eq!(stop_reason, "end_turn");
        assert_eq!(rx.recv().await, Some("hello".to_string()));

        client.end_session(&session_id).await.expect("end_session");
    }

    #[tokio::test]
    async fn start_session_errors_propagate_as_domain_errors() {
        struct FailingClient;
        #[async_trait::async_trait]
        impl AcpSessionClient for FailingClient {
            async fn start_session(
                &self,
                _command: &str,
                _args: &[String],
                _cwd: &str,
                _env: &[(String, String)],
            ) -> DomainResult<String> {
                Err(DomainError::InvalidInput("command not found".to_string()))
            }
            async fn send_prompt(
                &self,
                _session_id: &str,
                _prompt: String,
                _chunk_tx: mpsc::UnboundedSender<String>,
            ) -> DomainResult<String> {
                unreachable!("not exercised by this test")
            }
            async fn end_session(&self, _session_id: &str) -> DomainResult<()> {
                unreachable!("not exercised by this test")
            }
            async fn end_all_sessions(&self) -> DomainResult<()> {
                unreachable!("not exercised by this test")
            }
        }

        let client: Box<dyn AcpSessionClient> = Box::new(FailingClient);
        let err = client
            .start_session("bad-command", &[], "/tmp", &[])
            .await
            .expect_err("must propagate the error");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }
}
