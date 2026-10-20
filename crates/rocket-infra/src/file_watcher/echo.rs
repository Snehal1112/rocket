//! Remembers recent in-app writes so the watcher can drop their echoes.

use std::collections::{HashMap, VecDeque};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use rocket_shared::events::DomainEvent;

/// How far before a file change an in-app write may be and still own it.
///
/// A service publishes its event a few milliseconds after its write, and the
/// watcher sees the write a few milliseconds after it lands, in either order.
/// The window is kept below the frontend's 300 ms debounce. So a dropped
/// change always happened before the refetch that the service event starts.
pub const ECHO_WINDOW: Duration = Duration::from_millis(250);

/// Records older than this are pruned. It covers the watcher's longest batch.
const RETENTION: Duration = Duration::from_secs(5);

/// Most records kept per collection.
const MAX_PER_COLLECTION: usize = 32;

/// Thread-safe record of when each collection was last written by the app.
///
/// The event bus calls [`RecentWrites::record`] for every event it publishes.
/// The file watcher asks [`RecentWrites::owns`] before it reports a change.
#[derive(Default)]
pub struct RecentWrites {
    writes: Mutex<HashMap<String, VecDeque<Instant>>>,
}

impl RecentWrites {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the collections that `event` says the app just wrote.
    /// Events that do not describe a collection write are ignored.
    pub fn record(&self, event: &DomainEvent) {
        let now = Instant::now();
        for collection in written_collections(event) {
            self.record_at(collection, now);
        }
    }

    /// Records a write to `collection` at `at`.
    pub fn record_at(&self, collection: &str, at: Instant) {
        let mut writes = self.writes.lock().unwrap_or_else(|e| e.into_inner());
        let times = writes.entry(collection.to_string()).or_default();
        times.push_back(at);
        while times.len() > MAX_PER_COLLECTION {
            times.pop_front();
        }
        // Drop stale records of every collection so the map stays small.
        writes.retain(|_, times| {
            while times
                .front()
                .is_some_and(|t| at.saturating_duration_since(*t) > RETENTION)
            {
                times.pop_front();
            }
            !times.is_empty()
        });
    }

    /// True when the app wrote `collection` at or after `changed_at - window`.
    /// A write recorded after the change also counts, because the service
    /// event can reach the bus after the watcher saw the file.
    pub fn owns(&self, collection: &str, changed_at: Instant, window: Duration) -> bool {
        let writes = self.writes.lock().unwrap_or_else(|e| e.into_inner());
        let Some(times) = writes.get(collection) else {
            return false;
        };
        let earliest = changed_at.checked_sub(window);
        times.iter().any(|t| match earliest {
            Some(earliest) => *t >= earliest,
            None => true,
        })
    }
}

/// The collections an event reports as written by the app.
///
/// Only events that the bus also sends on the `collection-changed` channel
/// count, so a dropped watcher event is always covered by a service event.
/// Git events are left out: the frontend handles them on their own channel.
pub(crate) fn written_collections(event: &DomainEvent) -> Vec<&str> {
    match event {
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
        | DomainEvent::CollectionVariableWritten { collection, .. } => vec![collection.as_str()],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::events::FileChangeKind;

    const MS: Duration = Duration::from_millis(1);

    #[test]
    fn a_write_just_before_the_change_owns_it() {
        let writes = RecentWrites::new();
        let t0 = Instant::now();
        writes.record_at("api", t0);
        assert!(writes.owns("api", t0 + 100 * MS, ECHO_WINDOW));
        assert!(writes.owns("api", t0 + ECHO_WINDOW, ECHO_WINDOW));
    }

    #[test]
    fn a_write_recorded_after_the_change_owns_it() {
        // The service event can reach the bus after the watcher saw the file.
        let writes = RecentWrites::new();
        let t0 = Instant::now();
        writes.record_at("api", t0 + 40 * MS);
        assert!(writes.owns("api", t0, ECHO_WINDOW));
    }

    #[test]
    fn an_old_write_or_another_collection_does_not_own_it() {
        let writes = RecentWrites::new();
        let t0 = Instant::now();
        writes.record_at("api", t0);
        assert!(!writes.owns("api", t0 + ECHO_WINDOW + MS, ECHO_WINDOW));
        assert!(!writes.owns("other", t0 + MS, ECHO_WINDOW));
    }

    #[test]
    fn stale_records_are_pruned() {
        let writes = RecentWrites::new();
        let t0 = Instant::now();
        writes.record_at("old", t0);
        writes.record_at("new", t0 + RETENTION + MS);
        let map = writes.writes.lock().expect("lock");
        assert!(!map.contains_key("old"));
        assert!(map.contains_key("new"));
    }

    #[test]
    fn records_per_collection_are_capped() {
        let writes = RecentWrites::new();
        let t0 = Instant::now();
        for i in 0..(MAX_PER_COLLECTION as u32 + 10) {
            writes.record_at("api", t0 + i * MS);
        }
        let map = writes.writes.lock().expect("lock");
        assert_eq!(map["api"].len(), MAX_PER_COLLECTION);
    }

    #[test]
    fn record_reads_the_collection_of_collection_change_events() {
        let writes = RecentWrites::new();
        writes.record(&DomainEvent::RequestSaved {
            collection: "api".into(),
            path: "users/get.yml".into(),
        });
        writes.record(&DomainEvent::ItemMoved {
            src_collection: "a".into(),
            src_path: "x".into(),
            dst_collection: "b".into(),
            dst_path: "x".into(),
        });
        let now = Instant::now();
        for name in ["api", "a", "b"] {
            assert!(writes.owns(name, now, ECHO_WINDOW), "{name}");
        }
    }

    #[test]
    fn file_changes_and_other_events_are_not_writes() {
        let writes = RecentWrites::new();
        writes.record(&DomainEvent::FileChanged {
            path: "/ws/collections/api/a.yml".into(),
            event_type: FileChangeKind::Modify,
            collection: Some("api".into()),
        });
        writes.record(&DomainEvent::EnvironmentSaved { name: "api".into() });
        writes.record(&DomainEvent::GitStatusChanged {
            collection: "api".into(),
        });
        assert!(!writes.owns("api", Instant::now(), ECHO_WINDOW));
    }
}
