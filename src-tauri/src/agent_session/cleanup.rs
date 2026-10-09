use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

use rocket_app::{McpToolService, ProposalService, SessionCleanup};

use crate::agent_session::scratch::SessionScratch;
use crate::mcp::registry::McpServerRegistry;

/// What one live agent session owns outside `AcpSessionService`, apart from
/// its MCP server handle, which stays in `McpServerRegistry`.
pub struct SessionResources {
    pub scratch: SessionScratch,
    /// The pre-handshake id the session's MCP tool server tags its calls
    /// with until it is bound to the real id. Cleanup forgets it as well, so
    /// no cache entry made under it is left behind.
    pub mcp_session_id: Option<String>,
}

/// Tauri-managed state (`Arc<SessionResourceRegistry>`), keyed by the real
/// ACP session id.
#[derive(Default)]
pub struct SessionResourceRegistry {
    entries: Mutex<HashMap<String, SessionResources>>,
}

impl SessionResourceRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Stores a session's resources. A replaced entry is dropped, which
    /// removes its scratch directories.
    pub fn register(&self, session_id: String, resources: SessionResources) {
        let replaced = self
            .entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(session_id, resources);
        drop(replaced);
    }

    /// Removes and returns a session's resources, if it has any.
    pub fn take(&self, session_id: &str) -> Option<SessionResources> {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(session_id)
    }

    /// Drops every entry, which removes every scratch directory. The app-exit
    /// paths call this as a backstop after `end_all_sessions`.
    pub fn clear_all(&self) {
        let drained = std::mem::take(
            &mut *self.entries.lock().unwrap_or_else(PoisonError::into_inner),
        );
        drop(drained);
    }

    pub fn len(&self) -> usize {
        self.entries.lock().unwrap_or_else(PoisonError::into_inner).len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

type CacheForgetter = Box<dyn Fn(&str) + Send + Sync>;

/// The production `SessionCleanup`. `AcpSessionService` calls it once per
/// session on every end path.
pub struct TauriSessionCleanup {
    mcp_registry: Arc<McpServerRegistry>,
    resources: Arc<SessionResourceRegistry>,
    forget_cache: CacheForgetter,
}

impl TauriSessionCleanup {
    pub fn new(
        mcp_registry: Arc<McpServerRegistry>,
        mcp_tool_svc: Arc<McpToolService>,
        resources: Arc<SessionResourceRegistry>,
        proposals: Arc<ProposalService>,
    ) -> Self {
        Self::with_cache_forgetter(mcp_registry, resources, move |id| {
            mcp_tool_svc.forget_session(id);
            // Pending proposals die with their session. Nothing was written.
            proposals.clear_session(id);
        })
    }

    /// Test seam. `forget_cache` stands in for `McpToolService::forget_session`.
    pub fn with_cache_forgetter(
        mcp_registry: Arc<McpServerRegistry>,
        resources: Arc<SessionResourceRegistry>,
        forget_cache: impl Fn(&str) + Send + Sync + 'static,
    ) -> Self {
        Self {
            mcp_registry,
            resources,
            forget_cache: Box::new(forget_cache),
        }
    }
}

impl SessionCleanup for TauriSessionCleanup {
    fn on_session_ended(&self, session_id: &str) {
        // A no-op for a session that never had an MCP server.
        self.mcp_registry.end_session(session_id);
        (self.forget_cache)(session_id);
        if let Some(resources) = self.resources.take(session_id) {
            if let Some(mcp_session_id) = resources.mcp_session_id.as_deref() {
                (self.forget_cache)(mcp_session_id);
            }
            // Dropping the scratch removes its directories.
            drop(resources);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mcp::tool_server::{McpHttpServerHandle, McpSessionBinding};
    use tempfile::TempDir;
    use tokio_util::sync::CancellationToken;

    fn handle() -> (McpHttpServerHandle, CancellationToken) {
        let shutdown = CancellationToken::new();
        let handle = McpHttpServerHandle {
            port: 4100,
            token: "token".to_string(),
            binding: Arc::new(McpSessionBinding::new("mcp-pre-handshake".to_string())),
            shutdown: shutdown.clone(),
        };
        (handle, shutdown)
    }

    fn recording_cleanup(
        mcp_registry: Arc<McpServerRegistry>,
        resources: Arc<SessionResourceRegistry>,
    ) -> (TauriSessionCleanup, Arc<Mutex<Vec<String>>>) {
        let forgotten = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&forgotten);
        let cleanup = TauriSessionCleanup::with_cache_forgetter(mcp_registry, resources, move |id| {
            sink.lock().expect("lock forgotten").push(id.to_string());
        });
        (cleanup, forgotten)
    }

    #[test]
    fn on_session_ended_forgets_both_ids_shuts_the_server_and_removes_scratch() {
        let parent = TempDir::new().expect("tempdir");
        let mcp_registry = Arc::new(McpServerRegistry::new());
        let resources = Arc::new(SessionResourceRegistry::new());
        let (server, shutdown) = handle();
        mcp_registry.register("acp-1".to_string(), server);
        let scratch = SessionScratch::create_in(parent.path()).expect("scratch");
        let root = scratch.root().to_path_buf();
        resources.register(
            "acp-1".to_string(),
            SessionResources {
                scratch,
                mcp_session_id: Some("mcp-pre-handshake".to_string()),
            },
        );
        let (cleanup, forgotten) =
            recording_cleanup(Arc::clone(&mcp_registry), Arc::clone(&resources));

        cleanup.on_session_ended("acp-1");

        assert!(shutdown.is_cancelled(), "the MCP server must be shut down");
        assert!(!root.exists(), "the scratch directories must be removed");
        assert!(resources.is_empty());
        assert_eq!(
            *forgotten.lock().expect("lock"),
            vec!["acp-1".to_string(), "mcp-pre-handshake".to_string()],
            "the test-result cache is keyed by the pre-handshake id, so both must be forgotten"
        );
    }

    #[test]
    fn on_session_ended_twice_is_harmless() {
        let parent = TempDir::new().expect("tempdir");
        let mcp_registry = Arc::new(McpServerRegistry::new());
        let resources = Arc::new(SessionResourceRegistry::new());
        resources.register(
            "acp-2".to_string(),
            SessionResources {
                scratch: SessionScratch::create_in(parent.path()).expect("scratch"),
                mcp_session_id: None,
            },
        );
        let (cleanup, forgotten) = recording_cleanup(mcp_registry, Arc::clone(&resources));

        cleanup.on_session_ended("acp-2");
        cleanup.on_session_ended("acp-2");

        assert!(resources.is_empty());
        assert_eq!(
            *forgotten.lock().expect("lock"),
            vec!["acp-2".to_string(), "acp-2".to_string()]
        );
    }

    #[test]
    fn on_session_ended_leaves_other_sessions_alone() {
        let parent = TempDir::new().expect("tempdir");
        let mcp_registry = Arc::new(McpServerRegistry::new());
        let resources = Arc::new(SessionResourceRegistry::new());
        let other = SessionScratch::create_in(parent.path()).expect("scratch");
        let other_root = other.root().to_path_buf();
        resources.register(
            "other".to_string(),
            SessionResources {
                scratch: other,
                mcp_session_id: None,
            },
        );
        let (cleanup, _forgotten) = recording_cleanup(mcp_registry, Arc::clone(&resources));

        cleanup.on_session_ended("acp-3");

        assert!(other_root.exists());
        assert_eq!(resources.len(), 1);
    }

    #[test]
    fn clear_all_removes_every_scratch() {
        let parent = TempDir::new().expect("tempdir");
        let resources = SessionResourceRegistry::new();
        let a = SessionScratch::create_in(parent.path()).expect("scratch a");
        let b = SessionScratch::create_in(parent.path()).expect("scratch b");
        let (root_a, root_b) = (a.root().to_path_buf(), b.root().to_path_buf());
        resources.register("a".to_string(), SessionResources { scratch: a, mcp_session_id: None });
        resources.register("b".to_string(), SessionResources { scratch: b, mcp_session_id: None });

        resources.clear_all();

        assert!(resources.is_empty());
        assert!(!root_a.exists());
        assert!(!root_b.exists());
    }
}
