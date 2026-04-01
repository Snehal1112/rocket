use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use notify::{recommended_watcher, Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use rocket_shared::events::{DomainEvent, EventPublisher, FileChangeKind};

pub struct NotifyFileWatcher {
    watcher: Mutex<Option<RecommendedWatcher>>,
}

impl NotifyFileWatcher {
    pub fn new() -> Self {
        Self {
            watcher: Mutex::new(None),
        }
    }

    /// Start watching `collections_dir` recursively, publishing `FileChanged`
    /// events via `publisher` for each create, modify, or remove event.
    pub fn start(
        &self,
        collections_dir: PathBuf,
        publisher: Arc<dyn EventPublisher>,
    ) -> Result<(), String> {
        let dir_for_closure = collections_dir.clone();

        let mut watcher = recommended_watcher(move |result: notify::Result<Event>| {
            let Ok(event) = result else { return };

            let kind = match event.kind {
                EventKind::Create(_) => FileChangeKind::Create,
                EventKind::Modify(_) => FileChangeKind::Modify,
                EventKind::Remove(_) => FileChangeKind::Remove,
                _ => return,
            };

            for path in &event.paths {
                let path_str = path.to_string_lossy().to_string();

                // Derive the collection name from the top-level subdir under the watched dir.
                let collection = path
                    .strip_prefix(&dir_for_closure)
                    .ok()
                    .and_then(|rel| rel.components().next())
                    .map(|c| c.as_os_str().to_string_lossy().to_string());

                publisher.publish(DomainEvent::FileChanged {
                    path: path_str,
                    event_type: kind.clone(),
                    collection,
                });
            }
        })
        .map_err(|e| e.to_string())?;

        watcher
            .watch(&collections_dir, RecursiveMode::Recursive)
            .map_err(|e| e.to_string())?;

        *self.watcher.lock().expect("file watcher lock poisoned") = Some(watcher);
        Ok(())
    }

    /// Stop watching by dropping the underlying watcher.
    pub fn stop(&self) {
        *self.watcher.lock().expect("file watcher lock poisoned") = None;
    }
}

impl Default for NotifyFileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::events::NullEventPublisher;
    use tempfile::TempDir;

    #[test]
    fn start_and_stop_without_error() {
        let dir = TempDir::new().unwrap();
        let watcher = NotifyFileWatcher::new();
        let publisher = Arc::new(NullEventPublisher);

        watcher
            .start(dir.path().to_path_buf(), publisher)
            .expect("watcher should start");

        watcher.stop();
    }

    #[test]
    fn stop_before_start_is_safe() {
        let watcher = NotifyFileWatcher::new();
        watcher.stop(); // Should not panic.
    }
}
