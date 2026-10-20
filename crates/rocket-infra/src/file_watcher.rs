//! Watches the collections directory and reports changes as `FileChanged`.
//!
//! Raw notify events go to a worker thread. It drops noise paths, merges the
//! changes of one path, waits until the burst is quiet, and drops changes that
//! an in-app write already reported. See `batch`, `ignore` and `echo`.

mod batch;
mod echo;
mod ignore;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use notify::{recommended_watcher, Event, RecommendedWatcher, RecursiveMode, Watcher};

use rocket_shared::events::EventPublisher;

pub use batch::{DEBOUNCE, MAX_BATCH_DELAY};
pub use echo::{RecentWrites, ECHO_WINDOW};

use batch::{Batch, Timing};

/// A message for the worker thread.
enum Message {
    Raw(notify::Result<Event>),
    Stop,
}

/// A started watcher and its worker thread.
struct Running {
    watcher: RecommendedWatcher,
    sender: Sender<Message>,
    worker: JoinHandle<()>,
}

pub struct NotifyFileWatcher {
    running: Mutex<Option<Running>>,
    echo: Option<Arc<RecentWrites>>,
    timing: Timing,
    workers_alive: Arc<AtomicUsize>,
}

impl NotifyFileWatcher {
    pub fn new() -> Self {
        Self::build(None, Timing::default())
    }

    /// A watcher that drops the echo of writes recorded in `writes`.
    pub fn with_echo_guard(writes: Arc<RecentWrites>) -> Self {
        Self::build(Some(writes), Timing::default())
    }

    fn build(echo: Option<Arc<RecentWrites>>, timing: Timing) -> Self {
        Self {
            running: Mutex::new(None),
            echo,
            timing,
            workers_alive: Arc::new(AtomicUsize::new(0)),
        }
    }

    /// Start watching `collections_dir` recursively, publishing `FileChanged`
    /// events via `publisher`. A running watch is stopped first.
    pub fn start(
        &self,
        collections_dir: PathBuf,
        publisher: Arc<dyn EventPublisher>,
    ) -> Result<(), String> {
        self.stop();

        let (sender, receiver) = mpsc::channel();
        let raw_sender = sender.clone();
        let mut watcher = recommended_watcher(move |result: notify::Result<Event>| {
            // The worker is gone after a stop, so a late event is dropped.
            let _ = raw_sender.send(Message::Raw(result));
        })
        .map_err(|e| e.to_string())?;
        watcher
            .watch(&collections_dir, RecursiveMode::Recursive)
            .map_err(|e| e.to_string())?;

        let worker = Worker {
            root: collections_dir,
            publisher,
            echo: self.echo.clone(),
            timing: self.timing,
        };
        let alive = Arc::clone(&self.workers_alive);
        alive.fetch_add(1, Ordering::SeqCst);
        let spawned = std::thread::Builder::new()
            .name("file-watcher".into())
            .spawn(move || {
                let _guard = AliveGuard(alive);
                worker.run(receiver);
            });
        let worker = match spawned {
            Ok(handle) => handle,
            Err(e) => {
                // The closure never ran, so its guard did not count down.
                self.workers_alive.fetch_sub(1, Ordering::SeqCst);
                return Err(format!("could not start the file watcher thread: {e}"));
            }
        };

        *self.running.lock().unwrap_or_else(|e| e.into_inner()) = Some(Running {
            watcher,
            sender,
            worker,
        });
        Ok(())
    }

    /// Stop watching. Pending changes are dropped and the worker is joined.
    pub fn stop(&self) {
        let running = self
            .running
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let Some(running) = running else { return };
        // Drop the watcher first so no new raw events arrive.
        drop(running.watcher);
        let _ = running.sender.send(Message::Stop);
        if running.worker.join().is_err() {
            tracing::warn!("file watcher thread panicked");
        }
    }

    /// Number of worker threads that have not exited yet.
    #[cfg(test)]
    fn workers_alive(&self) -> usize {
        self.workers_alive.load(Ordering::SeqCst)
    }
}

impl Default for NotifyFileWatcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for NotifyFileWatcher {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Counts a worker thread down when it exits, even by panic.
struct AliveGuard(Arc<AtomicUsize>);

impl Drop for AliveGuard {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

/// The worker thread's state.
struct Worker {
    root: PathBuf,
    publisher: Arc<dyn EventPublisher>,
    echo: Option<Arc<RecentWrites>>,
    timing: Timing,
}

impl Worker {
    fn run(self, receiver: Receiver<Message>) {
        let mut batch = Batch::new(self.root.clone());
        loop {
            let message = match batch.deadline(&self.timing) {
                Some(deadline) => {
                    let wait = deadline.saturating_duration_since(Instant::now());
                    receiver.recv_timeout(wait)
                }
                None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match message {
                Ok(Message::Raw(raw)) => self.accept(&mut batch, raw),
                Ok(Message::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => {}
            }
            let due = batch
                .deadline(&self.timing)
                .is_some_and(|deadline| Instant::now() >= deadline);
            if due {
                for event in batch.take(self.echo.as_deref(), self.timing.echo_window) {
                    self.publisher.publish(event);
                }
            }
        }
    }

    fn accept(&self, batch: &mut Batch, raw: notify::Result<Event>) {
        let now = Instant::now();
        match raw {
            Ok(event) => {
                if event.need_rescan() {
                    tracing::warn!(
                        dir = %self.root.display(),
                        "file watcher lost events, reloading all collections"
                    );
                }
                batch.push_event(&event, now, Path::exists);
            }
            Err(error) => {
                // Changes may have been missed, so the UI reloads everything.
                let paths: Vec<String> = error
                    .paths
                    .iter()
                    .map(|p| p.display().to_string())
                    .collect();
                tracing::warn!(
                    dir = %self.root.display(),
                    paths = ?paths,
                    error = %error,
                    "file watcher error, reloading all collections"
                );
                batch.push_rescan(now);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rocket_shared::events::{DomainEvent, FileChangeKind, NullEventPublisher};
    use std::time::Duration;
    use tempfile::TempDir;

    /// Records published events.
    #[derive(Default)]
    struct Recorder(Mutex<Vec<DomainEvent>>);

    impl EventPublisher for Recorder {
        fn publish(&self, event: DomainEvent) {
            self.0.lock().expect("lock").push(event);
        }
    }

    impl Recorder {
        fn paths(&self) -> Vec<(String, String)> {
            self.0
                .lock()
                .expect("lock")
                .iter()
                .filter_map(|e| match e {
                    DomainEvent::FileChanged {
                        path, event_type, ..
                    } => Some((path.clone(), format!("{event_type:?}"))),
                    _ => None,
                })
                .collect()
        }
    }

    fn fast_timing() -> Timing {
        Timing {
            debounce: Duration::from_millis(60),
            max_delay: Duration::from_millis(400),
            echo_window: ECHO_WINDOW,
        }
    }

    /// Waits up to two seconds for `done`, checking every 10 ms.
    fn wait_for(done: impl Fn() -> bool) -> bool {
        let until = Instant::now() + Duration::from_secs(2);
        while Instant::now() < until {
            if done() {
                return true;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        done()
    }

    #[test]
    fn start_and_stop_without_error() {
        let dir = TempDir::new().expect("temp dir");
        let watcher = NotifyFileWatcher::new();
        let publisher = Arc::new(NullEventPublisher);

        watcher
            .start(dir.path().to_path_buf(), publisher)
            .expect("watcher should start");

        watcher.stop();
    }

    #[test]
    fn start_on_a_missing_directory_returns_an_error() {
        let dir = TempDir::new().expect("temp dir");
        let watcher = NotifyFileWatcher::new();
        let publisher = Arc::new(NullEventPublisher);

        let result = watcher.start(dir.path().join("missing"), publisher);

        assert!(result.is_err());
        assert_eq!(watcher.workers_alive(), 0);
    }

    #[test]
    fn stop_before_start_is_safe() {
        let watcher = NotifyFileWatcher::new();
        watcher.stop(); // Should not panic.
    }

    #[test]
    fn start_stop_and_restart_leave_no_thread_running() {
        let dir = TempDir::new().expect("temp dir");
        let other = TempDir::new().expect("temp dir");
        let watcher = NotifyFileWatcher::new();
        let publisher: Arc<dyn EventPublisher> = Arc::new(NullEventPublisher);

        watcher
            .start(dir.path().to_path_buf(), Arc::clone(&publisher))
            .expect("start");
        assert_eq!(watcher.workers_alive(), 1);

        // A second start replaces the first worker.
        watcher
            .start(other.path().to_path_buf(), Arc::clone(&publisher))
            .expect("restart");
        assert_eq!(watcher.workers_alive(), 1);

        watcher.stop();
        assert_eq!(watcher.workers_alive(), 0);

        watcher
            .start(dir.path().to_path_buf(), Arc::clone(&publisher))
            .expect("start again");
        let alive = Arc::clone(&watcher.workers_alive);
        drop(watcher);
        assert_eq!(alive.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn a_burst_of_writes_is_published_once() {
        let dir = TempDir::new().expect("temp dir");
        let root = dir.path().canonicalize().expect("canonical");
        std::fs::create_dir(root.join("api")).expect("mkdir");
        let recorder = Arc::new(Recorder::default());
        let watcher = NotifyFileWatcher::build(None, fast_timing());
        watcher
            .start(root.clone(), recorder.clone())
            .expect("start");

        let file = root.join("api").join("a.yml");
        for i in 0..10 {
            std::fs::write(&file, format!("n: {i}\n")).expect("write");
        }
        std::fs::write(root.join("api").join("a.yml.swp"), "x").expect("write swp");
        std::fs::create_dir_all(root.join("api").join(".git")).expect("mkdir git");
        std::fs::write(root.join("api").join(".git").join("index"), "x").expect("write git");

        let expected = file.to_string_lossy().to_string();
        assert!(wait_for(|| !recorder.paths().is_empty()), "no event");
        // Let one more debounce pass so a stray second batch would show up.
        std::thread::sleep(Duration::from_millis(150));
        watcher.stop();

        let paths = recorder.paths();
        assert_eq!(paths, vec![(expected, "Create".to_string())], "{paths:?}");
    }

    #[test]
    fn an_echoed_save_is_dropped_and_an_external_edit_is_kept() {
        let dir = TempDir::new().expect("temp dir");
        let root = dir.path().canonicalize().expect("canonical");
        std::fs::create_dir(root.join("api")).expect("mkdir");
        std::fs::create_dir(root.join("other")).expect("mkdir");
        let writes = Arc::new(RecentWrites::new());
        let recorder = Arc::new(Recorder::default());
        let watcher = NotifyFileWatcher::build(Some(Arc::clone(&writes)), fast_timing());
        watcher
            .start(root.clone(), recorder.clone())
            .expect("start");

        // An in-app save: the file is written, then the service event is recorded.
        std::fs::write(root.join("api").join("a.yml"), "a: 1\n").expect("write");
        writes.record(&DomainEvent::RequestSaved {
            collection: "api".into(),
            path: "a.yml".into(),
        });
        // An external edit in another collection at the same time.
        let external = root.join("other").join("b.yml");
        std::fs::write(&external, "b: 1\n").expect("write");

        assert!(wait_for(|| !recorder.paths().is_empty()), "no event");
        std::thread::sleep(Duration::from_millis(150));
        watcher.stop();

        let paths = recorder.paths();
        assert_eq!(
            paths,
            vec![(external.to_string_lossy().to_string(), "Create".to_string())],
            "{paths:?}"
        );
    }

    #[test]
    fn a_watcher_error_publishes_one_reload_event() {
        let (sender, receiver) = mpsc::channel();
        let recorder = Arc::new(Recorder::default());
        let worker = Worker {
            root: PathBuf::from("/ws/collections"),
            publisher: recorder.clone(),
            echo: None,
            timing: fast_timing(),
        };
        let handle = std::thread::spawn(move || worker.run(receiver));
        for _ in 0..3 {
            sender
                .send(Message::Raw(Err(notify::Error::generic("queue overflow"))))
                .expect("send");
        }
        assert!(wait_for(|| !recorder.paths().is_empty()), "no event");
        sender.send(Message::Stop).expect("send stop");
        handle.join().expect("join");

        let events = recorder.0.lock().expect("lock");
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
    fn a_rename_on_disk_reports_both_paths() {
        let dir = TempDir::new().expect("temp dir");
        let root = dir.path().canonicalize().expect("canonical");
        std::fs::create_dir(root.join("api")).expect("mkdir");
        let old = root.join("api").join("old.yml");
        std::fs::write(&old, "a: 1\n").expect("write");
        let recorder = Arc::new(Recorder::default());
        let watcher = NotifyFileWatcher::build(None, fast_timing());
        watcher
            .start(root.clone(), recorder.clone())
            .expect("start");

        let new = root.join("api").join("new.yml");
        std::fs::rename(&old, &new).expect("rename");

        assert!(wait_for(|| recorder.paths().len() >= 2), "no events");
        std::thread::sleep(Duration::from_millis(150));
        watcher.stop();

        let mut paths = recorder.paths();
        paths.sort();
        assert_eq!(
            paths,
            vec![
                (new.to_string_lossy().to_string(), "Create".to_string()),
                (old.to_string_lossy().to_string(), "Remove".to_string()),
            ]
        );
    }
}
