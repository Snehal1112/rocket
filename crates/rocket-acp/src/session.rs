use crate::{AcpUpdate, ConfigOption, McpServerSpec, PromptPart, SessionInfo};
use rocket_shared::error::DomainResult;
use tokio::sync::mpsc::UnboundedSender;

/// Protocol-focused contract for driving one ACP agent session: spawning the
/// process and handshaking, sending a prompt and streaming typed updates,
/// changing session options, stopping a turn, and ending the session. Has no
/// knowledge of `DomainEvent`, Tauri, or the `agent-client-protocol` crate.
/// Those live in `rocket-infra`'s `AcpAgentClient` and `rocket-app`'s
/// `AcpSessionService`.
#[async_trait::async_trait]
pub trait AcpSessionClient: Send + Sync {
    /// Spawns the agent process and performs the `initialize` → `session/new`
    /// handshake. Returns the ACP session id with the option list and prompt
    /// capabilities the agent reported. `mcp_servers` is passed through to
    /// `session/new` (an empty slice means chat-only). `meta`, when given,
    /// must be a JSON object and is sent as the `_meta` of `session/new`.
    async fn start_session(
        &self,
        command: &str,
        args: &[String],
        cwd: &str,
        env: &[(String, String)],
        mcp_servers: &[McpServerSpec],
        meta: Option<serde_json::Value>,
    ) -> DomainResult<SessionInfo>;

    /// Sends `session/prompt` with the given parts. Every update the agent
    /// sends during the turn is forwarded through `update_tx` while this call
    /// is pending. Resolves with the raw `stopReason` string; a stopped turn
    /// resolves with `cancelled`.
    async fn send_prompt(
        &self,
        session_id: &str,
        parts: Vec<PromptPart>,
        update_tx: UnboundedSender<AcpUpdate>,
    ) -> DomainResult<String>;

    /// Sends `session/cancel`. It never waits for the running turn, so it
    /// must not take the per-session prompt lock. The session stays open.
    async fn cancel(&self, session_id: &str) -> DomainResult<()>;

    /// Sends `session/set_config_option` and returns the agent's new option
    /// list. Changing the model can add or remove other options.
    async fn set_config_option(
        &self,
        session_id: &str,
        config_id: &str,
        value: &str,
    ) -> DomainResult<Vec<ConfigOption>>;

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
    use crate::PromptCapabilities;
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
            _mcp_servers: &[McpServerSpec],
            _meta: Option<serde_json::Value>,
        ) -> DomainResult<SessionInfo> {
            Ok(SessionInfo {
                session_id: "session-1".to_string(),
                config_options: Vec::new(),
                prompt_capabilities: PromptCapabilities::default(),
            })
        }

        async fn send_prompt(
            &self,
            _session_id: &str,
            _parts: Vec<PromptPart>,
            update_tx: mpsc::UnboundedSender<AcpUpdate>,
        ) -> DomainResult<String> {
            let _ = update_tx.send(AcpUpdate::Text {
                text: "hello".to_string(),
            });
            Ok("end_turn".to_string())
        }

        async fn cancel(&self, _session_id: &str) -> DomainResult<()> {
            Ok(())
        }

        async fn set_config_option(
            &self,
            _session_id: &str,
            _config_id: &str,
            _value: &str,
        ) -> DomainResult<Vec<ConfigOption>> {
            Ok(Vec::new())
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

        let info = client
            .start_session("echo", &[], "/tmp", &[], &[], None)
            .await
            .expect("start_session");
        assert_eq!(info.session_id, "session-1");

        let (tx, mut rx) = mpsc::unbounded_channel();
        let stop_reason = client
            .send_prompt(
                &info.session_id,
                vec![PromptPart::Text("hi".to_string())],
                tx,
            )
            .await
            .expect("send_prompt");
        assert_eq!(stop_reason, "end_turn");
        assert_eq!(
            rx.recv().await,
            Some(AcpUpdate::Text {
                text: "hello".to_string()
            })
        );

        client.cancel(&info.session_id).await.expect("cancel");
        let options = client
            .set_config_option(&info.session_id, "model", "opus")
            .await
            .expect("set_config_option");
        assert!(options.is_empty());
        client
            .end_session(&info.session_id)
            .await
            .expect("end_session");
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
                _mcp_servers: &[McpServerSpec],
                _meta: Option<serde_json::Value>,
            ) -> DomainResult<SessionInfo> {
                Err(DomainError::InvalidInput("command not found".to_string()))
            }
            async fn send_prompt(
                &self,
                _session_id: &str,
                _parts: Vec<PromptPart>,
                _update_tx: mpsc::UnboundedSender<AcpUpdate>,
            ) -> DomainResult<String> {
                unreachable!("not exercised by this test")
            }
            async fn cancel(&self, _session_id: &str) -> DomainResult<()> {
                unreachable!("not exercised by this test")
            }
            async fn set_config_option(
                &self,
                _session_id: &str,
                _config_id: &str,
                _value: &str,
            ) -> DomainResult<Vec<ConfigOption>> {
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
            .start_session("bad-command", &[], "/tmp", &[], &[], None)
            .await
            .expect_err("must propagate the error");
        assert!(matches!(err, DomainError::InvalidInput(_)));
    }
}
