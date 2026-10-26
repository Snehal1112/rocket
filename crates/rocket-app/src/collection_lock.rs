//! The process-wide lock of one collection.
//!
//! Lock order: the collection lock is taken first, then the file transaction's own busy-root
//! guard (`FsFileTx`). The sync apply holds both and never calls back into `CollectionService`.
//! `CollectionService` takes only this lock, once per public method, for the whole
//! load-modify-write. A holder never awaits while it holds the lock.

use std::path::Path;
use std::sync::Arc;

type LockMap =
    std::sync::Mutex<std::collections::HashMap<std::path::PathBuf, Arc<std::sync::Mutex<()>>>>;

static COLLECTION_LOCKS: std::sync::OnceLock<LockMap> = std::sync::OnceLock::new();

/// The lock of one collection, shared by every service in the process (commands build a
/// sync service per call). A std mutex is not reentrant, so a holder must not lock twice.
pub(crate) fn collection_lock(root: &Path) -> Arc<std::sync::Mutex<()>> {
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
    let _guard = lock.lock().unwrap_or_else(|e| e.into_inner());
    f()
}
