//! Tracks every live per-session MCP HTTP server so the app's existing
//! exit-sweep machinery (see `spawn_exit_signal_listener` and the
//! `RunEvent::Exit` handler in `lib.rs`) can shut all of them down, and so a
//! single ended ACP session can shut down just its own server.
//!
//! This is a separate registry from `AcpAgentClient`'s internal `sessions`/
//! `in_flight` maps (`crates/rocket-infra/src/acp_agent_client.rs`), not a
//! new field on that type: `McpHttpServerHandle` carries a `tauri` runtime
//! handle (an `Arc<Notify>` driven by a `tauri::async_runtime::spawn` task)
//! and is meaningless outside a running Tauri app, whereas `rocket-infra`
//! must not depend on `tauri` at all (this repo's DDD boundary —
//! `rocket-infra` is Tauri-agnostic I/O, wired into the app by `src-tauri`).
//! A parallel registry here, swept from the same two call sites that already
//! sweep `AcpAgentClient` (`spawn_exit_signal_listener`, the `RunEvent::Exit`
//! handler), gives the same guarantee — no bound port or live token survives
//! app exit — without crossing that boundary.

use std::collections::HashMap;
use std::sync::{Mutex, PoisonError};

use crate::mcp::tool_server::McpHttpServerHandle;

/// Tauri-managed state (`app.manage(Arc::new(McpServerRegistry::new()))`). Holds one
/// handle per live ACP session's MCP HTTP server, keyed by session id.
#[derive(Default)]
pub struct McpServerRegistry {
    handles: Mutex<HashMap<String, McpHttpServerHandle>>,
}

impl McpServerRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a newly spawned server so it is reachable by `end_session`/
    /// `shutdown_all`. Called by `spawn_mcp_http_server` itself.
    pub fn register(&self, session_id: String, handle: McpHttpServerHandle) {
        self.handles
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(session_id, handle);
    }

    /// Shuts down and forgets one session's server, if it has one. A no-op
    /// (not an error) if that session never had an HTTP server — e.g. agent
    /// autonomy was disabled for its collection, so `spawn_mcp_http_server`
    /// was never called for it.
    pub fn end_session(&self, session_id: &str) {
        let removed = self
            .handles
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(session_id);
        if let Some(handle) = removed {
            handle.shutdown();
        }
    }

    /// Shuts down every tracked server. Called from the same two app-exit
    /// paths that already call `AcpSessionClient::end_all_sessions`
    /// (`spawn_exit_signal_listener` and the `RunEvent::Exit` handler in
    /// `lib.rs`), so a crashed or killed app cannot leave a bound port or a
    /// live bearer token behind.
    pub fn shutdown_all(&self) {
        let handles: Vec<McpHttpServerHandle> = {
            let mut map = self.handles.lock().unwrap_or_else(PoisonError::into_inner);
            std::mem::take(&mut *map).into_values().collect()
        };
        for handle in handles {
            handle.shutdown();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tokio::sync::Notify;

    fn test_handle(port: u16, token: &str) -> (McpHttpServerHandle, Arc<Notify>) {
        let notify = Arc::new(Notify::new());
        let handle = McpHttpServerHandle {
            port,
            token: token.to_string(),
            shutdown: Arc::clone(&notify),
        };
        (handle, notify)
    }

    #[tokio::test]
    async fn end_session_notifies_only_that_sessions_shutdown() {
        let registry = McpServerRegistry::new();
        let (handle_a, notify_a) = test_handle(4000, "token-a");
        let (handle_b, notify_b) = test_handle(4001, "token-b");
        registry.register("session-a".to_string(), handle_a);
        registry.register("session-b".to_string(), handle_b);

        registry.end_session("session-a");

        // `Notify::notified()` resolves immediately once `notify_one` has
        // already fired, even called after the fact (a stored "permit"), so
        // this does not race `end_session` above.
        tokio::time::timeout(std::time::Duration::from_millis(50), notify_a.notified())
            .await
            .expect("session-a should have been notified to shut down");
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(20), notify_b.notified())
                .await
                .is_err(),
            "session-b must not be notified by ending session-a"
        );
    }

    #[tokio::test]
    async fn shutdown_all_notifies_every_registered_session() {
        let registry = McpServerRegistry::new();
        let (handle_a, notify_a) = test_handle(4002, "token-a");
        let (handle_b, notify_b) = test_handle(4003, "token-b");
        registry.register("session-a".to_string(), handle_a);
        registry.register("session-b".to_string(), handle_b);

        registry.shutdown_all();

        for notify in [&notify_a, &notify_b] {
            tokio::time::timeout(std::time::Duration::from_millis(50), notify.notified())
                .await
                .expect("every registered session should be notified");
        }
    }
}
