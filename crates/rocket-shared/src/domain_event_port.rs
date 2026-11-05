//! `PublishedEvent` for the global `DomainEvent`. It is removed when the
//! event enum is split into per-context enums.

use crate::events::DomainEvent;
use crate::published_event::PublishedEvent;

impl PublishedEvent for DomainEvent {
    fn channel(&self) -> &'static str {
        match self {
            // File watcher events are collection changes, so the sidebar
            // refreshes when files are created, modified or deleted.
            DomainEvent::FileChanged { .. } => "collection-changed",
            DomainEvent::RequestExecuted { .. } => "request-executed",
            DomainEvent::FileWatcherFailed { .. } => "file-watcher-failed",
            DomainEvent::RunnerStarted { .. } => "runner-started",
            DomainEvent::RunnerStepCompleted { .. } => "runner-step-completed",
            DomainEvent::RunnerFinished { .. } => "runner-finished",
            DomainEvent::FlowRunStarted { .. } => "flow-run-started",
            DomainEvent::FlowStepStarted { .. } => "flow-step-started",
            DomainEvent::FlowStepProgress { .. } => "flow-step-progress",
            DomainEvent::FlowStepCompleted { .. } => "flow-step-completed",
            DomainEvent::FlowRunFinished { .. } => "flow-run-finished",
            DomainEvent::AcpSessionStarted { .. } => "agent-session-started",
            DomainEvent::AcpSessionChunk { .. } => "agent-session-chunk",
            DomainEvent::AcpThoughtChunk { .. } => "agent-session-thought",
            DomainEvent::AcpSessionFinished { .. } => "agent-session-finished",
            DomainEvent::AcpSessionFailed { .. } => "agent-session-failed",
            DomainEvent::GrpcSessionStarted { .. } => "grpc-session-started",
            DomainEvent::GrpcSessionHeaders { .. } => "grpc-session-headers",
            DomainEvent::GrpcSessionMessage { .. } => "grpc-session-message",
            DomainEvent::GrpcSessionFinished { .. } => "grpc-session-finished",
            DomainEvent::WebSocketMessage { .. } => "ws:message",
            DomainEvent::WebSocketStatus { .. } => "ws:status",
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
            DomainEvent::AssistantConversationSaved { .. } => "assistant-conversation-saved",
            DomainEvent::AssistantConversationDeleted { .. } => "assistant-conversation-deleted",
            DomainEvent::AssistantConversationsCleared => "assistant-conversations-cleared",
            DomainEvent::OpenApiSyncChecked { .. } => "openapi-sync-checked",
            DomainEvent::OpenApiSyncApplied { .. } => "openapi-sync-applied",
            // Load test events keep the channel names the frontend listens on.
            DomainEvent::LoadTestProgress(_) => "load_test_progress",
            DomainEvent::LoadTestComplete(_) => "load_test_complete",
            DomainEvent::CollectionCreated { .. }
            | DomainEvent::CollectionDeleted { .. }
            | DomainEvent::CollectionRenamed { .. }
            | DomainEvent::RequestSaved { .. }
            | DomainEvent::RequestDeleted { .. }
            | DomainEvent::ItemMoved { .. }
            | DomainEvent::FolderCreated { .. }
            | DomainEvent::FolderDeleted { .. }
            | DomainEvent::ItemsReordered { .. }
            | DomainEvent::CollectionSettingsSaved { .. }
            | DomainEvent::FolderVariablesSaved { .. }
            | DomainEvent::FolderSettingsSaved { .. }
            | DomainEvent::RequestVariablesSaved { .. }
            | DomainEvent::CollectionVariableWritten { .. } => "collection-changed",
            DomainEvent::FlowRequestPathsRewritten { .. } => "flow-request-paths-rewritten",
            DomainEvent::CollectionTrustChanged { .. } => "collection-trust-changed",
            DomainEvent::EnvironmentSaved { .. } | DomainEvent::EnvironmentDeleted { .. } => {
                "environment-changed"
            }
            DomainEvent::WorkspaceCreated { .. } => "workspace-created",
            DomainEvent::WorkspaceSwitched { .. } => "workspace-switched",
            DomainEvent::WorkspaceRenamed { .. } => "workspace-renamed",
            DomainEvent::WorkspaceClosed { .. } => "workspace-closed",
            DomainEvent::WorkspaceDeleted { .. } => "workspace-deleted",
            DomainEvent::WorkspacePinned { .. } => "workspace-pinned",
            DomainEvent::WorkspaceUnpinned { .. } => "workspace-unpinned",
            DomainEvent::WorkspaceDescriptionUpdated { .. } => "workspace-description-updated",
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
            DomainEvent::ConsoleOutput { .. } => "script-console",
            DomainEvent::TestsCompleted { .. } => "script-tests",
            DomainEvent::ScriptError { .. } => "script-error",
            DomainEvent::ScriptVariableWritten { .. } => "script-variable-written",
        }
    }

    fn to_json(&self) -> Result<serde_json::Value, serde_json::Error> {
        serde_json::to_value(self)
    }

    /// A branch switch replaces the working-tree content the collection
    /// sidebar shows, so it also fires `collection-changed`.
    fn also_emit_on(&self) -> &'static [&'static str] {
        match self {
            DomainEvent::BranchSwitched { .. } | DomainEvent::BranchMerged { .. } => {
                &["collection-changed"]
            }
            _ => &[],
        }
    }

    /// Only events that the bus also sends on the `collection-changed`
    /// channel count, so a dropped watcher event is always covered by a
    /// service event. Git events are left out.
    fn written_collections(&self) -> Vec<&str> {
        match self {
            DomainEvent::CollectionCreated { name } | DomainEvent::CollectionDeleted { name } => {
                vec![name.as_str()]
            }
            DomainEvent::CollectionRenamed { old_name, new_name } => {
                vec![old_name.as_str(), new_name.as_str()]
            }
            DomainEvent::ItemMoved {
                src_collection,
                dst_collection,
                ..
            } => vec![src_collection.as_str(), dst_collection.as_str()],
            DomainEvent::RequestSaved { collection, .. }
            | DomainEvent::RequestDeleted { collection, .. }
            | DomainEvent::FolderCreated { collection, .. }
            | DomainEvent::FolderDeleted { collection, .. }
            | DomainEvent::ItemsReordered { collection, .. }
            | DomainEvent::CollectionSettingsSaved { collection }
            | DomainEvent::FolderVariablesSaved { collection, .. }
            | DomainEvent::FolderSettingsSaved { collection, .. }
            | DomainEvent::RequestVariablesSaved { collection, .. }
            | DomainEvent::CollectionVariableWritten { collection, .. } => {
                vec![collection.as_str()]
            }
            _ => Vec::new(),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::*;
    use crate::load_test::{LoadTestProgressEvent, LoadTestResult};
    use std::collections::BTreeSet;

    fn s() -> String {
        "x".to_string()
    }

    /// One value of every variant.
    fn one_of_each() -> Vec<DomainEvent> {
        vec![
            DomainEvent::CollectionCreated { name: s() },
            DomainEvent::CollectionDeleted { name: s() },
            DomainEvent::CollectionRenamed {
                old_name: "a".into(),
                new_name: "b".into(),
            },
            DomainEvent::RequestSaved {
                collection: s(),
                path: s(),
            },
            DomainEvent::RequestDeleted {
                collection: s(),
                path: s(),
            },
            DomainEvent::ItemMoved {
                src_collection: "a".into(),
                src_path: s(),
                dst_collection: "b".into(),
                dst_path: s(),
            },
            DomainEvent::FlowRequestPathsRewritten {
                collection: s(),
                from: s(),
                to: s(),
                flows: Vec::new(),
                failed: Vec::new(),
            },
            DomainEvent::FolderCreated {
                collection: s(),
                path: s(),
            },
            DomainEvent::FolderDeleted {
                collection: s(),
                path: s(),
            },
            DomainEvent::ItemsReordered {
                collection: s(),
                folder_path: s(),
            },
            DomainEvent::CollectionSettingsSaved { collection: s() },
            DomainEvent::CollectionTrustChanged { collection: s() },
            DomainEvent::FolderVariablesSaved {
                collection: s(),
                folder_path: s(),
            },
            DomainEvent::RequestVariablesSaved {
                collection: s(),
                request_path: s(),
            },
            DomainEvent::FolderSettingsSaved {
                collection: s(),
                folder_path: s(),
            },
            DomainEvent::EnvironmentSaved { name: s() },
            DomainEvent::EnvironmentDeleted { name: s() },
            DomainEvent::WorkspaceCreated {
                id: s(),
                name: s(),
                path: s(),
            },
            DomainEvent::WorkspaceSwitched {
                id: s(),
                name: s(),
                path: s(),
            },
            DomainEvent::WorkspaceRenamed {
                id: s(),
                old_name: s(),
                new_name: s(),
            },
            DomainEvent::WorkspaceClosed { id: s() },
            DomainEvent::WorkspaceDeleted { id: s() },
            DomainEvent::WorkspacePinned { id: s() },
            DomainEvent::WorkspaceUnpinned { id: s() },
            DomainEvent::WorkspaceDescriptionUpdated {
                id: s(),
                description: None,
            },
            DomainEvent::RequestExecuted {
                method: s(),
                url: s(),
                status: 200,
                duration_ms: 1,
            },
            DomainEvent::RunnerStarted {
                run_id: s(),
                collection: s(),
                folder_path: None,
                total_steps: 1,
            },
            DomainEvent::RunnerStepCompleted {
                run_id: s(),
                index: 0,
                item_name: s(),
                request_path: s(),
                status: s(),
                status_code: None,
                duration_ms: 1,
                test_pass_count: 0,
                test_fail_count: 0,
                script_error: None,
                error: None,
            },
            DomainEvent::RunnerFinished {
                run_id: s(),
                stopped_reason: s(),
                step_count: 1,
                failed_count: 0,
            },
            DomainEvent::FlowRunStarted {
                run_id: s(),
                flow_name: s(),
                collection: s(),
                total_nodes: 1,
                callbacks: Vec::new(),
                partial: None,
            },
            DomainEvent::FlowStepStarted {
                run_id: s(),
                node_id: s(),
            },
            DomainEvent::FlowStepProgress {
                run_id: s(),
                node_id: s(),
                attempt: None,
                max_attempts: None,
                message: s(),
                live: None,
            },
            DomainEvent::FlowStepCompleted {
                run_id: s(),
                node_id: s(),
                status: FlowNodeStatus::Success,
                status_code: None,
                duration_ms: None,
                error: None,
                value: None,
                skip_reason: None,
                branch: None,
                logs: Vec::new(),
                debug_request: None,
                attempts: None,
                exchange: None,
                trace: None,
            },
            DomainEvent::FlowRunFinished {
                run_id: s(),
                stopped_reason: s(),
                node_count: 1,
                failed_count: 0,
                skipped_count: 0,
                not_taken_count: 0,
            },
            DomainEvent::AcpSessionStarted { session_id: s() },
            DomainEvent::AcpSessionChunk {
                session_id: s(),
                text: s(),
            },
            DomainEvent::AcpThoughtChunk {
                session_id: s(),
                text: s(),
            },
            DomainEvent::AcpSessionFinished {
                session_id: s(),
                stop_reason: s(),
            },
            DomainEvent::AcpSessionFailed {
                session_id: s(),
                error: s(),
            },
            DomainEvent::AcpToolInvoked {
                session_id: s(),
                tool: s(),
                summary: s(),
            },
            DomainEvent::AcpProposalCreated {
                session_id: s(),
                proposal_id: s(),
                summary: s(),
            },
            DomainEvent::AcpProposalResolved {
                session_id: s(),
                proposal_id: s(),
                status: s(),
            },
            DomainEvent::AcpToolActivity {
                session_id: s(),
                call_id: s(),
                title: s(),
                status: s(),
            },
            DomainEvent::AcpConfigOptionsChanged {
                session_id: s(),
                options: Vec::new(),
            },
            DomainEvent::AcpUsage {
                session_id: s(),
                used: 1,
                size: 2,
                cost_usd: None,
            },
            DomainEvent::AcpPermissionRequested {
                session_id: s(),
                request_id: s(),
                call_id: s(),
                title: s(),
                kind: s(),
                summary: s(),
                options: Vec::new(),
            },
            DomainEvent::AcpPermissionResolved {
                session_id: s(),
                request_id: s(),
                option_id: None,
            },
            DomainEvent::AssistantConversationSaved {
                conversation_id: s(),
            },
            DomainEvent::AssistantConversationDeleted {
                conversation_id: s(),
            },
            DomainEvent::AssistantConversationsCleared,
            DomainEvent::OpenApiSyncChecked {
                collection: s(),
                status: s(),
            },
            DomainEvent::OpenApiSyncApplied {
                collection: s(),
                applied: 1,
                stale: 0,
            },
            DomainEvent::LoadTestProgress(LoadTestProgressEvent {
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
                recent_log: Vec::new(),
            }),
            DomainEvent::LoadTestComplete(LoadTestResult {
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
                phase_timeline: Vec::new(),
                request_log: Vec::new(),
                time_series: Vec::new(),
            }),
            DomainEvent::GrpcSessionStarted {
                session_id: s(),
                method_type: s(),
            },
            DomainEvent::GrpcSessionHeaders {
                session_id: s(),
                headers: Vec::new(),
            },
            DomainEvent::GrpcSessionMessage {
                session_id: s(),
                index: 0,
                json: s(),
            },
            DomainEvent::GrpcSessionFinished {
                session_id: s(),
                code: 0,
                code_name: s(),
                message: s(),
                trailers: Vec::new(),
                duration_ms: 1,
            },
            DomainEvent::WebSocketMessage {
                session_id: s(),
                direction: WebSocketDirection::In,
                kind: WebSocketPayloadKind::Text,
                data: s(),
                size: 1,
                timestamp_ms: 0,
                truncated: false,
            },
            DomainEvent::WebSocketStatus {
                session_id: s(),
                state: WebSocketSessionState::Open,
                subprotocol: None,
                code: None,
                reason: None,
            },
            DomainEvent::GraphQlSubscriptionMessage {
                session_id: s(),
                event: GraphQlSubscriptionEventKind::Next,
                data: s(),
                timestamp_ms: 0,
            },
            DomainEvent::GraphQlSubscriptionStatus {
                session_id: s(),
                state: WebSocketSessionState::Open,
                dialect: None,
                reason: None,
            },
            DomainEvent::FileChanged {
                path: s(),
                event_type: FileChangeKind::Modify,
                collection: None,
            },
            DomainEvent::FileWatcherFailed { message: s() },
            DomainEvent::GitStatusChanged { collection: s() },
            DomainEvent::GitCommit {
                collection: s(),
                message: s(),
                sha: s(),
            },
            DomainEvent::GitPush {
                collection: s(),
                remote: s(),
            },
            DomainEvent::GitPull {
                collection: s(),
                remote: s(),
            },
            DomainEvent::BranchSwitched {
                collection: s(),
                branch: s(),
            },
            DomainEvent::BranchMerged {
                collection: s(),
                branch: s(),
            },
            DomainEvent::GitStashChanged { collection: s() },
            DomainEvent::GitConflictDetected {
                collection: s(),
                files: Vec::new(),
            },
            DomainEvent::GitCloned {
                url: s(),
                dest: s(),
            },
            DomainEvent::GitRemoteAdded {
                collection: s(),
                name: s(),
                url: s(),
            },
            DomainEvent::GitRemoteRemoved {
                collection: s(),
                name: s(),
            },
            DomainEvent::ConsoleOutput {
                request_name: s(),
                entries: Vec::new(),
            },
            DomainEvent::TestsCompleted {
                request_name: s(),
                results: Vec::new(),
            },
            DomainEvent::ScriptError {
                request_name: s(),
                phase: s(),
                message: s(),
            },
            DomainEvent::ScriptVariableWritten {
                scope: s(),
                environment: None,
                collection: None,
                key: s(),
            },
            DomainEvent::CollectionVariableWritten {
                collection: s(),
                key: s(),
            },
        ]
    }

    /// The channel of each variant, keyed by its JSON  tag.
    const CHANNELS: [(&str, &str); 80] = [
        ("fileChanged", "collection-changed"),
        ("requestExecuted", "request-executed"),
        ("fileWatcherFailed", "file-watcher-failed"),
        ("runnerStarted", "runner-started"),
        ("runnerStepCompleted", "runner-step-completed"),
        ("runnerFinished", "runner-finished"),
        ("flowRunStarted", "flow-run-started"),
        ("flowStepStarted", "flow-step-started"),
        ("flowStepProgress", "flow-step-progress"),
        ("flowStepCompleted", "flow-step-completed"),
        ("flowRunFinished", "flow-run-finished"),
        ("acpSessionStarted", "agent-session-started"),
        ("acpSessionChunk", "agent-session-chunk"),
        ("acpThoughtChunk", "agent-session-thought"),
        ("acpSessionFinished", "agent-session-finished"),
        ("acpSessionFailed", "agent-session-failed"),
        ("grpcSessionStarted", "grpc-session-started"),
        ("grpcSessionHeaders", "grpc-session-headers"),
        ("grpcSessionMessage", "grpc-session-message"),
        ("grpcSessionFinished", "grpc-session-finished"),
        ("webSocketMessage", "ws:message"),
        ("webSocketStatus", "ws:status"),
        ("graphQlSubscriptionMessage", "graphql:subscription-message"),
        ("graphQlSubscriptionStatus", "graphql:subscription-status"),
        ("acpToolInvoked", "agent-tool-invoked"),
        ("acpProposalCreated", "agent-proposal-created"),
        ("acpProposalResolved", "agent-proposal-resolved"),
        ("acpToolActivity", "agent-session-tool-activity"),
        ("acpConfigOptionsChanged", "agent-session-config-options"),
        ("acpUsage", "agent-session-usage"),
        ("acpPermissionRequested", "agent-permission-requested"),
        ("acpPermissionResolved", "agent-permission-resolved"),
        ("assistantConversationSaved", "assistant-conversation-saved"),
        (
            "assistantConversationDeleted",
            "assistant-conversation-deleted",
        ),
        (
            "assistantConversationsCleared",
            "assistant-conversations-cleared",
        ),
        ("openApiSyncChecked", "openapi-sync-checked"),
        ("openApiSyncApplied", "openapi-sync-applied"),
        ("loadTestProgress", "load_test_progress"),
        ("loadTestComplete", "load_test_complete"),
        ("collectionCreated", "collection-changed"),
        ("collectionDeleted", "collection-changed"),
        ("collectionRenamed", "collection-changed"),
        ("requestSaved", "collection-changed"),
        ("requestDeleted", "collection-changed"),
        ("itemMoved", "collection-changed"),
        ("flowRequestPathsRewritten", "flow-request-paths-rewritten"),
        ("folderCreated", "collection-changed"),
        ("folderDeleted", "collection-changed"),
        ("itemsReordered", "collection-changed"),
        ("collectionSettingsSaved", "collection-changed"),
        ("folderVariablesSaved", "collection-changed"),
        ("folderSettingsSaved", "collection-changed"),
        ("requestVariablesSaved", "collection-changed"),
        ("collectionVariableWritten", "collection-changed"),
        ("collectionTrustChanged", "collection-trust-changed"),
        ("environmentSaved", "environment-changed"),
        ("environmentDeleted", "environment-changed"),
        ("workspaceCreated", "workspace-created"),
        ("workspaceSwitched", "workspace-switched"),
        ("workspaceRenamed", "workspace-renamed"),
        ("workspaceClosed", "workspace-closed"),
        ("workspaceDeleted", "workspace-deleted"),
        ("workspacePinned", "workspace-pinned"),
        ("workspaceUnpinned", "workspace-unpinned"),
        (
            "workspaceDescriptionUpdated",
            "workspace-description-updated",
        ),
        ("gitStatusChanged", "git-changed"),
        ("gitCommit", "git-changed"),
        ("gitPush", "git-changed"),
        ("gitPull", "git-changed"),
        ("branchSwitched", "git-changed"),
        ("branchMerged", "git-changed"),
        ("gitStashChanged", "git-changed"),
        ("gitConflictDetected", "git-changed"),
        ("gitCloned", "git-changed"),
        ("gitRemoteAdded", "git-changed"),
        ("gitRemoteRemoved", "git-changed"),
        ("consoleOutput", "script-console"),
        ("testsCompleted", "script-tests"),
        ("scriptError", "script-error"),
        ("scriptVariableWritten", "script-variable-written"),
    ];

    fn tag(event: &DomainEvent) -> String {
        let json = event.to_json().expect("event serializes");
        json["type"].as_str().expect("type tag").to_string()
    }

    #[test]
    fn one_of_each_covers_every_variant() {
        let events = one_of_each();
        assert_eq!(events.len(), 80);
        let tags: BTreeSet<String> = events.iter().map(tag).collect();
        assert_eq!(tags.len(), 80);
    }

    #[test]
    fn every_variant_keeps_its_channel() {
        for event in one_of_each() {
            let tag = tag(&event);
            let expected = CHANNELS
                .iter()
                .find(|(name, _)| *name == tag)
                .map(|(_, channel)| *channel)
                .expect("tag is in the table");
            assert_eq!(event.channel(), expected, "channel of {tag}");
        }
    }

    #[test]
    fn to_json_equals_serde_to_value() {
        for event in one_of_each() {
            let expected = serde_json::to_value(&event).expect("event serializes");
            assert_eq!(event.to_json().expect("event serializes"), expected);
        }
    }

    #[test]
    fn only_branch_events_emit_on_a_second_channel() {
        for event in one_of_each() {
            let expected: &[&str] = match event {
                DomainEvent::BranchSwitched { .. } | DomainEvent::BranchMerged { .. } => {
                    &["collection-changed"]
                }
                _ => &[],
            };
            assert_eq!(event.also_emit_on(), expected, "{}", tag(&event));
        }
    }

    #[test]
    fn written_collections_name_the_written_collections() {
        let saved = DomainEvent::RequestSaved {
            collection: "api".into(),
            path: "p".into(),
        };
        assert_eq!(saved.written_collections(), vec!["api"]);
        let moved = DomainEvent::ItemMoved {
            src_collection: "a".into(),
            src_path: "p".into(),
            dst_collection: "b".into(),
            dst_path: "q".into(),
        };
        assert_eq!(moved.written_collections(), vec!["a", "b"]);
        let renamed = DomainEvent::CollectionRenamed {
            old_name: "old".into(),
            new_name: "new".into(),
        };
        assert_eq!(renamed.written_collections(), vec!["old", "new"]);
        let changed = DomainEvent::FileChanged {
            path: "p".into(),
            event_type: FileChangeKind::Modify,
            collection: Some("api".into()),
        };
        assert!(changed.written_collections().is_empty());
        assert!(DomainEvent::EnvironmentSaved { name: "e".into() }
            .written_collections()
            .is_empty());
        assert!(DomainEvent::GitStatusChanged {
            collection: "api".into()
        }
        .written_collections()
        .is_empty());
    }
}
