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
        let event_name = match &event {
            // File watcher events are collection changes — the sidebar
            // needs to refresh when files are created/modified/deleted.
            DomainEvent::FileChanged { .. } => "collection-changed",
            DomainEvent::RequestExecuted { .. } => "request-executed",
            DomainEvent::CollectionCreated { .. }
            | DomainEvent::CollectionDeleted { .. }
            | DomainEvent::CollectionRenamed { .. } => "collection-changed",
            DomainEvent::RequestSaved { .. } | DomainEvent::RequestDeleted { .. } => {
                "collection-changed"
            }
            DomainEvent::ItemMoved { .. } => "collection-changed",
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
        };
        let _ = self.app.emit(event_name, &event);
        // A branch switch replaces the working-tree content visible to the
        // collection sidebar, so also fire "collection-changed" so that any
        // expanded CollectionNode re-fetches its request tree.
        if matches!(&event, DomainEvent::BranchSwitched { .. } | DomainEvent::BranchMerged { .. }) {
            let _ = self.app.emit("collection-changed", &event);
        }
    }
}
