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
        };
        let _ = self.app.emit(event_name, &event);
    }
}
