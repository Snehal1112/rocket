//! Turns raw notify events into a small set of `FileChanged` events.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};
use std::time::{Duration, Instant};

use notify::event::{ModifyKind, RenameMode};
use notify::{Event, EventKind};
use rocket_shared::events::{DomainEvent, FileChangeKind};

use super::echo::RecentWrites;
use super::ignore::is_ignored;

/// Quiet time after the last raw event before a batch is published.
pub const DEBOUNCE: Duration = Duration::from_millis(150);

/// Longest time a batch may wait while raw events keep coming.
pub const MAX_BATCH_DELAY: Duration = Duration::from_secs(1);

/// The time windows the watcher uses. Tests pass shorter ones.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Timing {
    pub debounce: Duration,
    pub max_delay: Duration,
    pub echo_window: Duration,
}

impl Default for Timing {
    fn default() -> Self {
        Self {
            debounce: DEBOUNCE,
            max_delay: MAX_BATCH_DELAY,
            echo_window: super::echo::ECHO_WINDOW,
        }
    }
}

/// Merges a later change of one path into an earlier one.
/// A remove wins, a remove then anything is a modify, a create stays a create.
pub(crate) fn merge_kind(earlier: &FileChangeKind, later: &FileChangeKind) -> FileChangeKind {
    match (earlier, later) {
        (_, FileChangeKind::Remove) => FileChangeKind::Remove,
        (FileChangeKind::Remove, _) => FileChangeKind::Modify,
        (FileChangeKind::Create, _) => FileChangeKind::Create,
        (FileChangeKind::Modify, _) => FileChangeKind::Modify,
    }
}

/// The path changes one raw event stands for.
///
/// A rename with both paths is a remove of the old path and a create of the
/// new one. A rename with an unknown side is decided by whether the path
/// still exists. Access and unknown events carry no change.
pub(crate) fn changes_of(
    event: &Event,
    exists: impl Fn(&Path) -> bool,
) -> Vec<(PathBuf, FileChangeKind)> {
    let all = |kind: FileChangeKind| -> Vec<(PathBuf, FileChangeKind)> {
        event
            .paths
            .iter()
            .map(|p| (p.clone(), kind.clone()))
            .collect()
    };
    match &event.kind {
        EventKind::Create(_) => all(FileChangeKind::Create),
        EventKind::Remove(_) => all(FileChangeKind::Remove),
        EventKind::Modify(ModifyKind::Name(RenameMode::Both)) if event.paths.len() >= 2 => vec![
            (event.paths[0].clone(), FileChangeKind::Remove),
            (event.paths[1].clone(), FileChangeKind::Create),
        ],
        EventKind::Modify(ModifyKind::Name(RenameMode::From)) => all(FileChangeKind::Remove),
        EventKind::Modify(ModifyKind::Name(RenameMode::To)) => all(FileChangeKind::Create),
        EventKind::Modify(ModifyKind::Name(_)) => event
            .paths
            .iter()
            .map(|p| {
                let kind = if exists(p) {
                    FileChangeKind::Create
                } else {
                    FileChangeKind::Remove
                };
                (p.clone(), kind)
            })
            .collect(),
        EventKind::Modify(_) => all(FileChangeKind::Modify),
        EventKind::Access(_) | EventKind::Any | EventKind::Other => Vec::new(),
    }
}

struct Entry {
    collection: Option<String>,
    kind: FileChangeKind,
    last_at: Instant,
}

/// Raw changes collected since the last publish, one entry per path.
pub(crate) struct Batch {
    root: PathBuf,
    entries: BTreeMap<PathBuf, Entry>,
    refresh_all: bool,
    first_at: Option<Instant>,
    last_at: Option<Instant>,
}

impl Batch {
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            entries: BTreeMap::new(),
            refresh_all: false,
            first_at: None,
            last_at: None,
        }
    }

    /// Adds every change of a raw event seen at `at`.
    pub fn push_event(&mut self, event: &Event, at: Instant, exists: impl Fn(&Path) -> bool) {
        if event.need_rescan() {
            self.push_rescan(at);
        }
        for (path, kind) in changes_of(event, exists) {
            self.push_change(path, kind, at);
        }
    }

    /// Adds one path change unless the path is ignored.
    pub fn push_change(&mut self, path: PathBuf, kind: FileChangeKind, at: Instant) {
        let rel = path.strip_prefix(&self.root).unwrap_or(&path);
        if is_ignored(rel) {
            return;
        }
        // The collection is the first directory under the watched root.
        let collection = path
            .strip_prefix(&self.root)
            .ok()
            .and_then(|rel| rel.components().next())
            .map(|c| c.as_os_str().to_string_lossy().to_string());
        self.touch(at);
        match self.entries.get_mut(&path) {
            Some(entry) => {
                entry.kind = merge_kind(&entry.kind, &kind);
                entry.last_at = at;
            }
            None => {
                self.entries.insert(
                    path,
                    Entry {
                        collection,
                        kind,
                        last_at: at,
                    },
                );
            }
        }
    }

    /// Marks that events may have been lost, so the UI must reload everything.
    pub fn push_rescan(&mut self, at: Instant) {
        self.refresh_all = true;
        self.touch(at);
    }

    fn touch(&mut self, at: Instant) {
        self.first_at.get_or_insert(at);
        self.last_at = Some(at);
    }

    /// When the batch must be published, or `None` when it is empty.
    pub fn deadline(&self, timing: &Timing) -> Option<Instant> {
        let first = self.first_at?;
        let last = self.last_at.unwrap_or(first);
        Some((last + timing.debounce).min(first + timing.max_delay))
    }

    /// Empties the batch and returns the events to publish.
    ///
    /// A lost-events batch becomes one event with no collection, which every
    /// frontend listener treats as "reload everything". Otherwise one event
    /// is returned per path, minus the changes an in-app write owns.
    pub fn take(&mut self, echo: Option<&RecentWrites>, echo_window: Duration) -> Vec<DomainEvent> {
        let entries = std::mem::take(&mut self.entries);
        let refresh_all = std::mem::replace(&mut self.refresh_all, false);
        self.first_at = None;
        self.last_at = None;

        if refresh_all {
            return vec![refresh_all_event(&self.root)];
        }
        entries
            .into_iter()
            .filter(|(path, entry)| !is_echo(&self.root, path, entry, echo, echo_window))
            .map(|(path, entry)| DomainEvent::FileChanged {
                path: path.to_string_lossy().to_string(),
                event_type: entry.kind,
                collection: entry.collection,
            })
            .collect()
    }
}

/// The event that tells the UI to reload every collection.
pub(crate) fn refresh_all_event(root: &Path) -> DomainEvent {
    DomainEvent::FileChanged {
        path: root.to_string_lossy().to_string(),
        event_type: FileChangeKind::Modify,
        collection: None,
    }
}

/// True when an in-app write owns this change.
///
/// Changes under `<collection>/environments/` are always kept: environment
/// saves publish no `collection-changed` event, and the sidebar reloads
/// environments from the watcher's path.
fn is_echo(
    root: &Path,
    path: &Path,
    entry: &Entry,
    echo: Option<&RecentWrites>,
    window: Duration,
) -> bool {
    let (Some(echo), Some(collection)) = (echo, entry.collection.as_deref()) else {
        return false;
    };
    if is_environment_path(root, path) {
        return false;
    }
    echo.owns(collection, entry.last_at, window)
}

fn is_environment_path(root: &Path, path: &Path) -> bool {
    let Ok(rel) = path.strip_prefix(root) else {
        return false;
    };
    matches!(
        rel.components().nth(1),
        Some(Component::Normal(name)) if name == "environments"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use notify::event::{CreateKind, DataChange, Flag, RemoveKind};

    const MS: Duration = Duration::from_millis(1);

    fn root() -> PathBuf {
        PathBuf::from("/ws/collections")
    }

    fn timing() -> Timing {
        Timing::default()
    }

    fn event(kind: EventKind, paths: &[&str]) -> Event {
        paths
            .iter()
            .fold(Event::new(kind), |e, p| e.add_path(root().join(p)))
    }

    fn summary(events: &[DomainEvent]) -> Vec<(String, String, Option<String>)> {
        events
            .iter()
            .map(|e| match e {
                DomainEvent::FileChanged {
                    path,
                    event_type,
                    collection,
                } => (
                    path.trim_start_matches("/ws/collections/").to_string(),
                    format!("{event_type:?}"),
                    collection.clone(),
                ),
                other => panic!("unexpected event {other:?}"),
            })
            .collect()
    }

    fn row(path: &str, kind: &str, collection: &str) -> (String, String, Option<String>) {
        (path.into(), kind.into(), Some(collection.into()))
    }

    #[test]
    fn kind_merging() {
        use FileChangeKind::{Create, Modify, Remove};
        let cases = [
            (Create, Modify, Create),
            (Create, Create, Create),
            (Create, Remove, Remove),
            (Modify, Modify, Modify),
            (Modify, Create, Modify),
            (Modify, Remove, Remove),
            (Remove, Create, Modify),
            (Remove, Modify, Modify),
            (Remove, Remove, Remove),
        ];
        for (earlier, later, want) in cases {
            let got = merge_kind(&earlier, &later);
            assert_eq!(
                format!("{got:?}"),
                format!("{want:?}"),
                "{earlier:?} then {later:?}"
            );
        }
    }

    #[test]
    fn many_raw_events_for_one_file_become_one() {
        let mut batch = Batch::new(root());
        let t0 = Instant::now();
        batch.push_event(
            &event(EventKind::Create(CreateKind::File), &["api/a.yml"]),
            t0,
            |_| true,
        );
        for i in 1..20 {
            batch.push_event(
                &event(
                    EventKind::Modify(ModifyKind::Data(DataChange::Content)),
                    &["api/a.yml"],
                ),
                t0 + i * MS,
                |_| true,
            );
        }
        let events = batch.take(None, timing().echo_window);
        assert_eq!(summary(&events), vec![row("api/a.yml", "Create", "api")]);
        assert!(batch.take(None, timing().echo_window).is_empty());
    }

    #[test]
    fn an_atomic_save_reports_only_the_target_file() {
        // atomic_write: create and write the temp file, then rename it over the target.
        let tmp = "api/a.yml.tmp.4242_0badcafe";
        let mut batch = Batch::new(root());
        let t0 = Instant::now();
        batch.push_event(
            &event(EventKind::Create(CreateKind::File), &[tmp]),
            t0,
            |_| true,
        );
        batch.push_event(
            &event(EventKind::Modify(ModifyKind::Data(DataChange::Any)), &[tmp]),
            t0,
            |_| true,
        );
        batch.push_event(
            &event(
                EventKind::Modify(ModifyKind::Name(RenameMode::From)),
                &[tmp],
            ),
            t0,
            |_| false,
        );
        batch.push_event(
            &event(
                EventKind::Modify(ModifyKind::Name(RenameMode::To)),
                &["api/a.yml"],
            ),
            t0,
            |_| true,
        );
        batch.push_event(
            &event(
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                &[tmp, "api/a.yml"],
            ),
            t0,
            |_| true,
        );
        let events = batch.take(None, timing().echo_window);
        assert_eq!(summary(&events), vec![row("api/a.yml", "Create", "api")]);
    }

    #[test]
    fn ignored_paths_never_start_a_batch() {
        let mut batch = Batch::new(root());
        let t0 = Instant::now();
        for p in ["api/.git/index", "api/node_modules/x.js", "api/a.yml.swp"] {
            batch.push_event(
                &event(EventKind::Create(CreateKind::File), &[p]),
                t0,
                |_| true,
            );
        }
        assert!(batch.deadline(&timing()).is_none());
        assert!(batch.take(None, timing().echo_window).is_empty());
    }

    #[test]
    fn a_rename_reports_the_old_and_the_new_path() {
        let mut batch = Batch::new(root());
        batch.push_event(
            &event(
                EventKind::Modify(ModifyKind::Name(RenameMode::Both)),
                &["api/old.yml", "api/new.yml"],
            ),
            Instant::now(),
            |_| true,
        );
        let events = batch.take(None, timing().echo_window);
        assert_eq!(
            summary(&events),
            vec![
                row("api/new.yml", "Create", "api"),
                row("api/old.yml", "Remove", "api"),
            ]
        );
    }

    #[test]
    fn rename_halves_and_unknown_renames() {
        let from = event(
            EventKind::Modify(ModifyKind::Name(RenameMode::From)),
            &["api/gone.yml"],
        );
        let to = event(
            EventKind::Modify(ModifyKind::Name(RenameMode::To)),
            &["api/came.yml"],
        );
        let any = event(
            EventKind::Modify(ModifyKind::Name(RenameMode::Any)),
            &["api/here.yml", "api/away.yml"],
        );
        let exists = |p: &Path| p.ends_with("here.yml");
        let kinds = |e: &Event| -> Vec<String> {
            changes_of(e, exists)
                .into_iter()
                .map(|(_, k)| format!("{k:?}"))
                .collect()
        };
        assert_eq!(kinds(&from), ["Remove"]);
        assert_eq!(kinds(&to), ["Create"]);
        assert_eq!(kinds(&any), ["Create", "Remove"]);
        assert!(kinds(&Event::new(EventKind::Access(
            notify::event::AccessKind::Any
        )))
        .is_empty());
        assert_eq!(
            kinds(&event(EventKind::Remove(RemoveKind::File), &["api/x.yml"])),
            ["Remove"]
        );
    }

    #[test]
    fn a_rescan_replaces_the_batch_with_one_reload_event() {
        let mut batch = Batch::new(root());
        let t0 = Instant::now();
        batch.push_event(
            &event(EventKind::Create(CreateKind::File), &["api/a.yml"]),
            t0,
            |_| true,
        );
        batch.push_event(
            &Event::new(EventKind::Other).set_flag(Flag::Rescan),
            t0,
            |_| true,
        );
        let events = batch.take(None, timing().echo_window);
        assert_eq!(events.len(), 1);
        match &events[0] {
            DomainEvent::FileChanged {
                path,
                event_type,
                collection,
            } => {
                assert_eq!(path, "/ws/collections");
                assert!(matches!(event_type, FileChangeKind::Modify));
                assert!(collection.is_none());
            }
            other => panic!("unexpected event {other:?}"),
        }
    }

    #[test]
    fn deadline_is_debounced_and_capped() {
        let t = Timing {
            debounce: 100 * MS,
            max_delay: 300 * MS,
            echo_window: 250 * MS,
        };
        let mut batch = Batch::new(root());
        let t0 = Instant::now();
        let modify = event(
            EventKind::Modify(ModifyKind::Data(DataChange::Any)),
            &["api/a.yml"],
        );
        batch.push_event(&modify, t0, |_| true);
        assert_eq!(batch.deadline(&t), Some(t0 + 100 * MS));
        batch.push_event(&modify, t0 + 50 * MS, |_| true);
        assert_eq!(batch.deadline(&t), Some(t0 + 150 * MS));
        batch.push_event(&modify, t0 + 280 * MS, |_| true);
        assert_eq!(batch.deadline(&t), Some(t0 + 300 * MS));
    }

    #[test]
    fn echo_suppression() {
        let writes = RecentWrites::new();
        let t0 = Instant::now();
        writes.record_at("api", t0);
        let window = timing().echo_window;
        let mut batch = Batch::new(root());
        let modify = |p: &str| event(EventKind::Modify(ModifyKind::Data(DataChange::Any)), &[p]);
        // Within the window, same collection: dropped.
        batch.push_event(&modify("api/a.yml"), t0 + 20 * MS, |_| true);
        // Same collection, environment file: kept.
        batch.push_event(&modify("api/environments/prod.yml"), t0 + 20 * MS, |_| true);
        // Another collection: kept.
        batch.push_event(&modify("other/a.yml"), t0 + 20 * MS, |_| true);
        let events = batch.take(Some(&writes), window);
        assert_eq!(
            summary(&events),
            vec![
                row("api/environments/prod.yml", "Modify", "api"),
                row("other/a.yml", "Modify", "other"),
            ]
        );

        // Outside the window: kept.
        batch.push_event(&modify("api/b.yml"), t0 + window + 50 * MS, |_| true);
        let events = batch.take(Some(&writes), window);
        assert_eq!(summary(&events), vec![row("api/b.yml", "Modify", "api")]);
    }

    #[test]
    fn a_service_event_after_the_file_change_still_owns_it() {
        // The watcher thread can see the write before the service publishes.
        let writes = RecentWrites::new();
        let t0 = Instant::now();
        let mut batch = Batch::new(root());
        batch.push_event(
            &event(
                EventKind::Modify(ModifyKind::Data(DataChange::Any)),
                &["api/a.yml"],
            ),
            t0,
            |_| true,
        );
        // The service event arrives during the debounce, before the flush.
        writes.record_at("api", t0 + 30 * MS);
        assert!(batch.take(Some(&writes), timing().echo_window).is_empty());
    }

    #[test]
    fn a_reload_event_is_never_suppressed() {
        let writes = RecentWrites::new();
        let t0 = Instant::now();
        writes.record_at("api", t0);
        let mut batch = Batch::new(root());
        batch.push_rescan(t0);
        assert_eq!(batch.take(Some(&writes), timing().echo_window).len(), 1);
    }
}
