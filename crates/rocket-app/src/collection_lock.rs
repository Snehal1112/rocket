//! The process-wide lock of one collection.
//!
//! Lock order: the collection lock is taken first, then the file transaction's own busy-root
//! guard (`FsFileTx`). The sync apply holds both and never calls back into `CollectionService`.
//! `CollectionService` takes only this lock, once per public method, for the whole
//! load-modify-write. A holder never awaits while it holds the lock.
//!
//! The lock is fair: waiters are served in the order they asked (a ticket lock). Editor saves
//! run concurrently on the async runtime, and a plain mutex could let an older save win the
//! lock after a newer one and overwrite the newer edit. The lock is not reentrant.

use std::path::Path;
use std::sync::{Arc, Condvar, Mutex};

#[derive(Default)]
struct Tickets {
    /// The next ticket to hand out.
    next: u64,
    /// The ticket that may hold the lock now.
    serving: u64,
}

/// A fair FIFO lock for one collection root.
#[derive(Default)]
pub(crate) struct CollectionLock {
    tickets: Mutex<Tickets>,
    turn: Condvar,
}

/// Holds the collection lock and gives it to the next waiter on drop, also while unwinding.
pub(crate) struct CollectionGuard<'a> {
    lock: &'a CollectionLock,
}

impl CollectionLock {
    fn tickets(&self) -> std::sync::MutexGuard<'_, Tickets> {
        self.tickets.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Waits for every earlier caller, then returns the guard.
    pub(crate) fn lock(&self) -> CollectionGuard<'_> {
        let mut tickets = self.tickets();
        let mine = tickets.next;
        tickets.next += 1;
        while tickets.serving != mine {
            tickets = self.turn.wait(tickets).unwrap_or_else(|e| e.into_inner());
        }
        CollectionGuard { lock: self }
    }

    /// The guard when nobody holds or awaits the lock, else `None`.
    #[cfg(test)]
    pub(crate) fn try_lock(&self) -> Option<CollectionGuard<'_>> {
        let mut tickets = self.tickets();
        if tickets.next != tickets.serving {
            return None;
        }
        tickets.next += 1;
        Some(CollectionGuard { lock: self })
    }

    /// How many callers hold or await the lock.
    #[cfg(test)]
    pub(crate) fn queued(&self) -> u64 {
        let tickets = self.tickets();
        tickets.next - tickets.serving
    }
}

impl Drop for CollectionGuard<'_> {
    fn drop(&mut self) {
        self.lock.tickets().serving += 1;
        self.lock.turn.notify_all();
    }
}

type LockMap = Mutex<std::collections::HashMap<std::path::PathBuf, Arc<CollectionLock>>>;

static COLLECTION_LOCKS: std::sync::OnceLock<LockMap> = std::sync::OnceLock::new();

/// The lock of one collection, shared by every service in the process (commands build a
/// sync service per call). It is not reentrant, so a holder must not lock twice.
pub(crate) fn collection_lock(root: &Path) -> Arc<CollectionLock> {
    let key = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let mut map = COLLECTION_LOCKS
        .get_or_init(Default::default)
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    Arc::clone(map.entry(key).or_default())
}

/// Runs `f` while holding the lock of `collection`, found through `repo`. When the repository
/// has no root for it (a test double, or a missing collection) `f` runs without the lock and
/// reports its own error. `f` must not take the lock again and must not await.
pub(crate) fn with_collection_lock<T>(
    repo: &dyn rocket_collection::CollectionRepository,
    collection: &str,
    f: impl FnOnce() -> T,
) -> T {
    let Ok(root) = repo.collection_root_path(collection) else {
        return f();
    };
    let lock = collection_lock(&root);
    let _guard = lock.lock();
    f()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc::channel;
    use std::time::Duration;

    fn wait_until(cond: impl Fn() -> bool) {
        let start = std::time::Instant::now();
        while !cond() {
            assert!(start.elapsed() < Duration::from_secs(10), "timed out");
            std::thread::sleep(Duration::from_millis(1));
        }
    }

    #[test]
    fn waiters_are_served_in_the_order_they_queued() {
        let lock = Arc::new(CollectionLock::default());
        let held = lock.lock();
        let (tx, rx) = channel();
        let mut workers = Vec::new();
        for i in 0..8u64 {
            let queue = Arc::clone(&lock);
            let tx = tx.clone();
            workers.push(std::thread::spawn(move || {
                let _guard = queue.lock();
                tx.send(i).expect("send");
            }));
            // The next thread starts only after this one holds its ticket.
            wait_until(|| lock.queued() == i + 2);
        }
        drop(held);
        let order: Vec<u64> = (0..8)
            .map(|_| rx.recv_timeout(Duration::from_secs(10)).expect("served"))
            .collect();
        for w in workers {
            w.join().expect("join");
        }
        assert_eq!(order, (0..8).collect::<Vec<_>>());
    }

    #[test]
    fn a_panicking_holder_still_releases_the_lock() {
        let lock = Arc::new(CollectionLock::default());
        let holder = {
            let lock = Arc::clone(&lock);
            std::thread::spawn(move || {
                let _guard = lock.lock();
                panic!("holder failed");
            })
        };
        assert!(holder.join().is_err());
        assert!(lock.try_lock().is_some(), "the queue is not wedged");
        // A later caller is served as usual.
        drop(lock.lock());
    }

    #[test]
    fn try_lock_fails_while_held_or_awaited() {
        let lock = CollectionLock::default();
        let guard = lock.lock();
        assert!(lock.try_lock().is_none());
        drop(guard);
        assert!(lock.try_lock().is_some());
    }
}
