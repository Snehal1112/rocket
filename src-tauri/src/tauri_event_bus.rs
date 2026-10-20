use rocket_shared::events::{DomainEvent, EventPublisher};
use tauri::{AppHandle, Emitter};

pub struct TauriEventBus {
    app: AppHandle,
}

impl TauriEventBus {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }
}

impl EventPublisher for TauriEventBus {
    fn publish(&self, event: DomainEvent) {
        let event_name = channel_for(&event);
        let _ = self.app.emit(event_name, &event);
        // A branch switch replaces the working-tree content visible to the
        // collection sidebar, so also fire "collection-changed" so that any
        // expanded CollectionNode re-fetches its request tree.
        if matches!(
            &event,
            DomainEvent::BranchSwitched { .. } | DomainEvent::BranchMerged { .. }
        ) {
            let _ = self.app.emit("collection-changed", &event);
        }
    }
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
}
