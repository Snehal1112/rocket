//! Tracks every live per-session MCP HTTP server so the app's existing
//! exit-sweep machinery (see `spawn_exit_signal_listener` and the
//! `RunEvent::Exit` handler in `lib.rs`) can shut all of them down, and so a
//! single ended ACP session can shut down just its own server.
//!
//! This is a separate registry from `AcpAgentClient`'s internal `sessions`/
//! `in_flight` maps (`crates/rocket-infra/src/acp_agent_client.rs`), not a
//! new field on that type: `McpHttpServerHandle` controls a server task
//! (a `CancellationToken` observed by a `tauri::async_runtime::spawn` task)
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
    /// `shutdown_all`. Called by whoever spawned the server (Plan 05's
    /// `start_agent_session`), keyed by the id it will end the session by.
    /// `spawn_mcp_http_server` does not register on its own.
    ///
    /// If a handle was already registered under `session_id`, that older
    /// server is shut down. Otherwise it would drop out of the registry
    /// while still bound to a port with a live bearer token, and no sweep
    /// could ever reach it again.
    pub fn register(&self, session_id: String, handle: McpHttpServerHandle) {
        let replaced = self
            .handles
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(session_id, handle);
        if let Some(old) = replaced {
            old.shutdown();
        }
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
    use tokio_util::sync::CancellationToken;

    fn test_handle(port: u16, token: &str) -> (McpHttpServerHandle, CancellationToken) {
        let shutdown = CancellationToken::new();
        let handle = McpHttpServerHandle {
            port,
            token: token.to_string(),
            shutdown: shutdown.clone(),
        };
        (handle, shutdown)
    }

    #[test]
    fn end_session_shuts_down_only_that_sessions_server() {
        let registry = McpServerRegistry::new();
        let (handle_a, shutdown_a) = test_handle(4000, "token-a");
        let (handle_b, shutdown_b) = test_handle(4001, "token-b");
        registry.register("session-a".to_string(), handle_a);
        registry.register("session-b".to_string(), handle_b);

        registry.end_session("session-a");

        assert!(shutdown_a.is_cancelled(), "session-a should be shut down");
        assert!(
            !shutdown_b.is_cancelled(),
            "session-b must not be shut down by ending session-a"
        );
    }

    #[test]
    fn shutdown_all_shuts_down_every_registered_server() {
        let registry = McpServerRegistry::new();
        let (handle_a, shutdown_a) = test_handle(4002, "token-a");
        let (handle_b, shutdown_b) = test_handle(4003, "token-b");
        registry.register("session-a".to_string(), handle_a);
        registry.register("session-b".to_string(), handle_b);

        registry.shutdown_all();

        assert!(shutdown_a.is_cancelled());
        assert!(shutdown_b.is_cancelled());
    }

    #[test]
    fn registering_over_an_existing_session_shuts_down_the_replaced_server() {
        let registry = McpServerRegistry::new();
        let (first, first_shutdown) = test_handle(4004, "token-first");
        let (second, second_shutdown) = test_handle(4005, "token-second");
        registry.register("session-a".to_string(), first);

        registry.register("session-a".to_string(), second);

        assert!(
            first_shutdown.is_cancelled(),
            "a replaced server must not stay bound with no registry entry left to sweep it"
        );
        assert!(!second_shutdown.is_cancelled());

        registry.end_session("session-a");
        assert!(second_shutdown.is_cancelled());
    }
}
