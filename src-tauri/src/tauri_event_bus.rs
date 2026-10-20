use rocket_shared::events::{DomainEvent, EventPublisher};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::{AppHandle, Emitter};

/// Process-wide event counter, shared by every `TauriEventBus` instance.
static NEXT_SEQ: AtomicU64 = AtomicU64::new(1);

/// Failed emits per channel, used to rate-limit the warning.
static EMIT_FAILURES: Mutex<BTreeMap<&'static str, u64>> = Mutex::new(BTreeMap::new());

pub struct TauriEventBus {
    app: AppHandle,
}

impl TauriEventBus {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    fn emit_value(&self, channel: &'static str, payload: &Value) {
        if let Err(err) = self.app.emit(channel, payload) {
            // The payload is left out on purpose, it may hold secrets. The
            // `backend-log` emitter does not use this bus, so this cannot loop.
            if let Some(count) = failure_to_log(&EMIT_FAILURES, channel) {
                tracing::warn!(channel, failures = count, error = %err, "event emit failed");
            }
        }
    }
}

impl EventPublisher for TauriEventBus {
    fn publish(&self, event: DomainEvent) {
        let event_name = channel_for(&event);
        let seq = next_seq();
        let payload = match envelope(&event, seq, now_ms()) {
            Ok(value) => value,
            Err(err) => {
                tracing::warn!(channel = event_name, error = %err, "event serialize failed");
                return;
            }
        };
        self.emit_value(event_name, &payload);
        // A branch switch replaces the working-tree content visible to the
        // collection sidebar, so also fire "collection-changed" so that any
        // expanded CollectionNode re-fetches its request tree.
        if matches!(
            &event,
            DomainEvent::BranchSwitched { .. } | DomainEvent::BranchMerged { .. }
        ) {
            self.emit_value("collection-changed", &payload);
        }
    }
}

/// The next process-wide sequence number. It starts at 1 and only grows.
fn next_seq() -> u64 {
    NEXT_SEQ.fetch_add(1, Ordering::Relaxed)
}

/// Unix time in milliseconds. A clock before the epoch reads as 0.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// The event as a JSON object plus the `seq` and `tsMs` envelope keys.
/// A key the event already has is never overwritten.
fn envelope(event: &DomainEvent, seq: u64, ts_ms: u64) -> Result<Value, serde_json::Error> {
    Ok(add_envelope(serde_json::to_value(event)?, seq, ts_ms))
}

/// Adds `seq` and `tsMs` to a JSON object unless it has those keys already.
fn add_envelope(mut value: Value, seq: u64, ts_ms: u64) -> Value {
    if let Value::Object(map) = &mut value {
        map.entry("seq").or_insert_with(|| Value::from(seq));
        map.entry("tsMs").or_insert_with(|| Value::from(ts_ms));
    }
    value
}

/// Counts one failure on `channel`. Returns the count when it should be
/// logged: the first failure, then every 100th.
fn failure_to_log(
    failures: &Mutex<BTreeMap<&'static str, u64>>,
    channel: &'static str,
) -> Option<u64> {
    let mut map = failures.lock().unwrap_or_else(|e| e.into_inner());
    let count = map.entry(channel).or_insert(0);
    *count += 1;
    (*count == 1 || count.is_multiple_of(100)).then_some(*count)
}

/// The frontend channel for each event.
pub(crate) fn channel_for(event: &DomainEvent) -> &'static str {
    match event {
        // File watcher events are collection changes — the sidebar
        // needs to refresh when files are created/modified/deleted.
        DomainEvent::FileChanged { .. } => "collection-changed",
        DomainEvent::RequestExecuted { .. } => "request-executed",
        DomainEvent::FileWatcherFailed { .. } => "file-watcher-failed",
        // Collection Runner events — each gets its own channel so the run
        // view can append steps without re-reading the whole run.
        DomainEvent::RunnerStarted { .. } => "runner-started",
        DomainEvent::RunnerStepCompleted { .. } => "runner-step-completed",
        DomainEvent::RunnerFinished { .. } => "runner-finished",
        // Flow events — mirrors the Collection Runner events above.
        DomainEvent::FlowRunStarted { .. } => "flow-run-started",
        DomainEvent::FlowStepStarted { .. } => "flow-step-started",
        DomainEvent::FlowStepProgress { .. } => "flow-step-progress",
        DomainEvent::FlowStepCompleted { .. } => "flow-step-completed",
        DomainEvent::FlowRunFinished { .. } => "flow-run-finished",
        // ACP AI-assist session events — mirrors the Flow/Collection
        // Runner events above: each variant gets its own frontend channel.
        DomainEvent::AcpSessionStarted { .. } => "agent-session-started",
        DomainEvent::AcpSessionChunk { .. } => "agent-session-chunk",
        DomainEvent::AcpThoughtChunk { .. } => "agent-session-thought",
        DomainEvent::AcpSessionFinished { .. } => "agent-session-finished",
        DomainEvent::AcpSessionFailed { .. } => "agent-session-failed",
        // gRPC session events — each variant gets its own channel, like the ACP and Flow events.
        DomainEvent::GrpcSessionStarted { .. } => "grpc-session-started",
        DomainEvent::GrpcSessionHeaders { .. } => "grpc-session-headers",
        DomainEvent::GrpcSessionMessage { .. } => "grpc-session-message",
        DomainEvent::GrpcSessionFinished { .. } => "grpc-session-finished",
        // WebSocket sessions: one channel for frames, one for lifecycle.
        DomainEvent::WebSocketMessage { .. } => "ws:message",
        DomainEvent::WebSocketStatus { .. } => "ws:status",
        // GraphQL subscriptions: one channel for results, one for lifecycle.
        DomainEvent::GraphQlSubscriptionMessage { .. } => "graphql:subscription-message",
        DomainEvent::GraphQlSubscriptionStatus { .. } => "graphql:subscription-status",
        DomainEvent::AcpToolInvoked { .. } => "agent-tool-invoked",
        DomainEvent::AcpProposalCreated { .. } => "agent-proposal-created",
        DomainEvent::AcpProposalResolved { .. } => "agent-proposal-resolved",
        DomainEvent::AcpToolActivity { .. } => "agent-session-tool-activity",
        DomainEvent::AcpConfigOptionsChanged { .. } => "agent-session-config-options",
        DomainEvent::AcpUsage { .. } => "agent-session-usage",
        DomainEvent::AcpPermissionRequested { .. } => "agent-permission-requested",
        DomainEvent::AcpPermissionResolved { .. } => "agent-permission-resolved",
        // Stored assistant conversations. The payloads hold ids only.
        DomainEvent::AssistantConversationSaved { .. } => "assistant-conversation-saved",
        DomainEvent::AssistantConversationDeleted { .. } => "assistant-conversation-deleted",
        DomainEvent::AssistantConversationsCleared => "assistant-conversations-cleared",
        // Load test events keep the channel names the frontend listens on.
        DomainEvent::LoadTestProgress(_) => "load_test_progress",
        DomainEvent::LoadTestComplete(_) => "load_test_complete",
        DomainEvent::CollectionCreated { .. }
        | DomainEvent::CollectionDeleted { .. }
        | DomainEvent::CollectionRenamed { .. } => "collection-changed",
        DomainEvent::RequestSaved { .. } | DomainEvent::RequestDeleted { .. } => {
            "collection-changed"
        }
        DomainEvent::ItemMoved { .. } => "collection-changed",
        DomainEvent::FlowRequestPathsRewritten { .. } => "flow-request-paths-rewritten",
        DomainEvent::FolderCreated { .. }
        | DomainEvent::FolderDeleted { .. }
        | DomainEvent::ItemsReordered { .. }
        | DomainEvent::CollectionSettingsSaved { .. }
        | DomainEvent::FolderVariablesSaved { .. }
        | DomainEvent::FolderSettingsSaved { .. }
        | DomainEvent::RequestVariablesSaved { .. } => "collection-changed",
        DomainEvent::CollectionVariableWritten { .. } => "collection-changed",
        DomainEvent::CollectionTrustChanged { .. } => "collection-trust-changed",
        DomainEvent::EnvironmentSaved { .. } | DomainEvent::EnvironmentDeleted { .. } => {
            "environment-changed"
        }
        DomainEvent::HistoryCleared => "history-changed",
        // Workspace events — each variant gets its own channel so the
        // frontend store can update the precise slice of state it needs.
        DomainEvent::WorkspaceCreated { .. } => "workspace-created",
        DomainEvent::WorkspaceSwitched { .. } => "workspace-switched",
        DomainEvent::WorkspaceRenamed { .. } => "workspace-renamed",
        DomainEvent::WorkspaceClosed { .. } => "workspace-closed",
        DomainEvent::WorkspaceDeleted { .. } => "workspace-deleted",
        DomainEvent::WorkspacePinned { .. } => "workspace-pinned",
        DomainEvent::WorkspaceUnpinned { .. } => "workspace-unpinned",
        DomainEvent::WorkspaceDescriptionUpdated { .. } => "workspace-description-updated",
        // Git events
        DomainEvent::GitStatusChanged { .. }
        | DomainEvent::GitCommit { .. }
        | DomainEvent::GitPush { .. }
        | DomainEvent::GitPull { .. }
        | DomainEvent::BranchSwitched { .. }
        | DomainEvent::BranchMerged { .. }
        | DomainEvent::GitStashChanged { .. }
        | DomainEvent::GitConflictDetected { .. }
        | DomainEvent::GitCloned { .. }
        | DomainEvent::GitRemoteAdded { .. }
        | DomainEvent::GitRemoteRemoved { .. } => "git-changed",
        // Script events
        DomainEvent::ConsoleOutput { .. } => "script-console",
        DomainEvent::TestsCompleted { .. } => "script-tests",
        DomainEvent::ScriptError { .. } => "script-error",
        DomainEvent::ScriptVariableWritten { .. } => "script-variable-written",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assistant_conversation_events_have_their_own_channels() {
        let id = || "c1".to_string();
        assert_eq!(
            channel_for(&DomainEvent::AssistantConversationSaved {
                conversation_id: id()
            }),
            "assistant-conversation-saved"
        );
        assert_eq!(
            channel_for(&DomainEvent::AssistantConversationDeleted {
                conversation_id: id()
            }),
            "assistant-conversation-deleted"
        );
        assert_eq!(
            channel_for(&DomainEvent::AssistantConversationsCleared),
            "assistant-conversations-cleared"
        );
        assert_eq!(channel_for(&DomainEvent::HistoryCleared), "history-changed");
    }

    #[test]
    fn file_watcher_failed_has_its_own_channel() {
        let event = DomainEvent::FileWatcherFailed {
            message: "m".into(),
        };
        assert_eq!(channel_for(&event), "file-watcher-failed");
    }

    #[test]
    fn flow_request_paths_rewritten_has_its_own_channel() {
        let event = DomainEvent::FlowRequestPathsRewritten {
            collection: "c".into(),
            from: "a".into(),
            to: "b".into(),
            flows: Vec::new(),
            failed: Vec::new(),
        };
        assert_eq!(channel_for(&event), "flow-request-paths-rewritten");
    }

    #[test]
    fn permission_events_have_their_own_channels() {
        assert_eq!(
            channel_for(&DomainEvent::AcpPermissionRequested {
                session_id: "s".into(),
                request_id: "perm-1".into(),
                call_id: "c".into(),
                title: "t".into(),
                kind: "other".into(),
                summary: String::new(),
                options: Vec::new(),
            }),
            "agent-permission-requested"
        );
        assert_eq!(
            channel_for(&DomainEvent::AcpPermissionResolved {
                session_id: "s".into(),
                request_id: "perm-1".into(),
                option_id: None,
            }),
            "agent-permission-resolved"
        );
    }

    #[test]
    fn load_test_events_keep_their_channel_names() {
        use rocket_shared::load_test::{LoadTestProgressEvent, LoadTestResult};
        let progress = LoadTestProgressEvent {
            elapsed_ms: 0,
            completed: 0,
            active_concurrent: 0,
            succeeded: 0,
            failed_status: 0,
            failed_transport: 0,
            requests_per_second: 0.0,
            p50_ms: 0.0,
            p95_ms: 0.0,
            p99_ms: 0.0,
            current_phase_index: 0,
            recent_log: vec![],
        };
        assert_eq!(
            channel_for(&DomainEvent::LoadTestProgress(progress)),
            "load_test_progress"
        );
        let result = LoadTestResult {
            total_requests: 0,
            succeeded: 0,
            failed: 0,
            failed_transport: 0,
            failed_status: 0,
            min_latency_ms: 0.0,
            avg_latency_ms: 0.0,
            p50_latency_ms: 0.0,
            p95_latency_ms: 0.0,
            p99_latency_ms: 0.0,
            max_latency_ms: 0.0,
            requests_per_second: 0.0,
            total_duration_ms: 0.0,
            phase_timeline: vec![],
            request_log: vec![],
            time_series: vec![],
        };
        assert_eq!(
            channel_for(&DomainEvent::LoadTestComplete(result)),
            "load_test_complete"
        );
    }

    #[test]
    fn envelope_adds_seq_and_ts_ms_to_the_event_object() {
        let event = DomainEvent::FileWatcherFailed {
            message: "m".into(),
        };
        let value = envelope(&event, 7, 1234).expect("envelope");
        assert_eq!(
            value,
            serde_json::json!({
                "type": "fileWatcherFailed",
                "message": "m",
                "seq": 7,
                "tsMs": 1234,
            })
        );
    }

    #[test]
    fn envelope_keeps_unit_variants_as_objects() {
        let value = envelope(&DomainEvent::HistoryCleared, 1, 2).expect("envelope");
        assert_eq!(value["type"], "historyCleared");
        assert_eq!(value["seq"], 1);
    }

    #[test]
    fn envelope_does_not_overwrite_existing_keys() {
        let value = add_envelope(serde_json::json!({ "seq": "own", "tsMs": "own" }), 9, 9);
        assert_eq!(value["seq"], "own");
        assert_eq!(value["tsMs"], "own");
    }

    #[test]
    fn seq_is_strictly_increasing() {
        let a = next_seq();
        let b = next_seq();
        let c = next_seq();
        assert!(a < b && b < c);
        assert!(a >= 1);
    }

    #[test]
    fn now_ms_is_after_2020() {
        assert!(now_ms() > 1_577_836_800_000);
    }

    #[test]
    fn failure_log_is_first_then_every_hundredth_per_channel() {
        let failures = Mutex::new(BTreeMap::new());
        assert_eq!(failure_to_log(&failures, "a"), Some(1));
        for _ in 2..100 {
            assert_eq!(failure_to_log(&failures, "a"), None);
        }
        assert_eq!(failure_to_log(&failures, "a"), Some(100));
        assert_eq!(failure_to_log(&failures, "a"), None);
        // Another channel counts on its own.
        assert_eq!(failure_to_log(&failures, "b"), Some(1));
    }
}
