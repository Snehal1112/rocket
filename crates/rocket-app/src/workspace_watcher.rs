//! Points the file watcher at the active workspace's collections folder, and tells
//! the UI when it cannot be started.

use std::path::Path;
use std::sync::Arc;

use rocket_collection::CollectionEvent;

use rocket_shared::events::EventPublisher;
use rocket_workspace::FileWatcher;

pub struct WorkspaceWatcher {
    watcher: Arc<dyn FileWatcher>,
    publisher: Arc<dyn EventPublisher>,
}

impl WorkspaceWatcher {
    pub fn new(watcher: Arc<dyn FileWatcher>, publisher: Arc<dyn EventPublisher>) -> Self {
        Self { watcher, publisher }
    }

    /// Watches `<workspace>/collections`, replacing any running watch. A failure is
    /// logged and published as `FileWatcherFailed` (`<folder>: <reason>`); the app keeps
    /// working without live refresh. Returns whether the watcher is running.
    pub fn restart(&self, workspace: &Path) -> bool {
        let collections_dir = workspace.join("collections");
        self.watcher.stop();
        match self
            .watcher
            .start(collections_dir.clone(), Arc::clone(&self.publisher))
        {
            Ok(()) => true,
            Err(e) => {
                tracing::error!(
                    error = %e,
                    dir = %collections_dir.display(),
                    "file watcher failed to start"
                );
                self.publisher.publish(&CollectionEvent::FileWatcherFailed {
                    message: format!("{}: {e}", collections_dir.display()),
                });
                false
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_doubles::RecordingPublisher;
    use std::path::PathBuf;
    use std::sync::Mutex;

    /// Records the calls it gets, in order.
    struct FakeWatcher {
        calls: Mutex<Vec<String>>,
        start_result: Result<(), String>,
    }

    impl FakeWatcher {
        fn ok() -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(Vec::new()),
                start_result: Ok(()),
            })
        }
        fn failing(reason: &str) -> Arc<Self> {
            Arc::new(Self {
                calls: Mutex::new(Vec::new()),
                start_result: Err(reason.to_string()),
            })
        }
        fn calls(&self) -> Vec<String> {
            self.calls.lock().expect("lock").clone()
        }
    }

    impl FileWatcher for FakeWatcher {
        fn start(
            &self,
            collections_dir: PathBuf,
            _publisher: Arc<dyn EventPublisher>,
        ) -> Result<(), String> {
            self.calls
                .lock()
                .expect("lock")
                .push(format!("start:{}", collections_dir.display()));
            self.start_result.clone()
        }

        fn stop(&self) {
            self.calls.lock().expect("lock").push("stop".to_string());
        }
    }

    #[test]
    fn restart_starts_the_watcher_on_the_collections_dir() {
        let fake = FakeWatcher::ok();
        let service = WorkspaceWatcher::new(fake.clone(), RecordingPublisher::new());

        assert!(service.restart(Path::new("/ws/main")));

        assert!(fake
            .calls()
            .contains(&"start:/ws/main/collections".to_string()));
    }

    #[test]
    fn restart_stops_before_starting() {
        let fake = FakeWatcher::ok();
        let service = WorkspaceWatcher::new(fake.clone(), RecordingPublisher::new());

        service.restart(Path::new("/ws/main"));

        assert_eq!(
            fake.calls(),
            vec!["stop".to_string(), "start:/ws/main/collections".to_string()]
        );
    }

    #[test]
    fn failed_start_publishes_one_event_with_dir_and_reason() {
        let fake = FakeWatcher::failing("permission denied");
        let events = RecordingPublisher::new();
        let service = WorkspaceWatcher::new(fake, events.clone());

        assert!(!service.restart(Path::new("/ws/main")));

        let failures: Vec<String> = events
            .collection_events()
            .into_iter()
            .filter_map(|e| match e {
                CollectionEvent::FileWatcherFailed { message } => Some(message),
                _ => None,
            })
            .collect();
        assert_eq!(
            failures,
            vec!["/ws/main/collections: permission denied".to_string()]
        );
    }

    #[test]
    fn successful_start_publishes_nothing() {
        let events = RecordingPublisher::new();
        let service = WorkspaceWatcher::new(FakeWatcher::ok(), events.clone());

        service.restart(Path::new("/ws/main"));

        assert!(events.collection_events().is_empty());
    }
}
