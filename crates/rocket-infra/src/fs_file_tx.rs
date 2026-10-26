//! A file transaction for one collection folder, with a journal for crash recovery.
//!
//! Staged files live in `<collection>/.rocket/staging/<id>/new`. Commit writes the journal as
//! `committing`, then backs up, replaces or deletes each file in order, marking the op done.
//! A failed commit writes the journal as `rolling_back` and restores every file from
//! `staging/<id>/backup`. `recover` finishes a `committing` journal or continues a
//! `rolling_back` one, so a crash never leaves a mix of both directions.

use std::io;
use std::path::{Path, PathBuf};
#[cfg(any(test, feature = "test-support"))]
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[cfg(test)]
use rocket_collection::file_tx::OWNED_STATE_FILES;
use rocket_collection::file_tx::{
    validate_rel_path, CollectionFileTx, CollectionFileTxFactory, RecoveryOutcome,
};
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::atomic_write;

const JOURNAL_FILE: &str = "apply-journal.json";
const JOURNAL_VERSION: u32 = 1;
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Phase {
    Staging,
    Committing,
    #[serde(rename = "rolling_back")]
    RollingBack,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum OpKind {
    Put,
    Delete,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JournalOp {
    op: OpKind,
    path: String,
    backup: bool,
    done: bool,
}

impl JournalOp {
    fn is_put(&self) -> bool {
        self.op == OpKind::Put
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Journal {
    version: u32,
    id: String,
    phase: Phase,
    ops: Vec<JournalOp>,
}

#[cfg(any(test, feature = "test-support"))]
struct HookState {
    at: usize,
    count: AtomicUsize,
    crash: bool,
}

/// Fails the k-th filesystem step, in tests only. A crash hook also stops all clean-up, so the
/// test sees the disk as a real crash would leave it.
#[derive(Clone, Default)]
struct FaultHook {
    #[cfg(any(test, feature = "test-support"))]
    inner: Option<Arc<HookState>>,
}

impl FaultHook {
    fn check(&self) -> io::Result<()> {
        #[cfg(any(test, feature = "test-support"))]
        if let Some(inner) = &self.inner {
            if inner.count.fetch_add(1, Ordering::SeqCst) == inner.at {
                return Err(io::Error::other("injected fault"));
            }
        }
        Ok(())
    }

    #[cfg(any(test, feature = "test-support"))]
    fn is_crash(&self) -> bool {
        self.inner.as_ref().is_some_and(|i| i.crash)
    }

    #[cfg(not(any(test, feature = "test-support")))]
    fn is_crash(&self) -> bool {
        false
    }

    #[cfg(any(test, feature = "test-support"))]
    fn at(at: usize, crash: bool) -> Self {
        Self {
            inner: Some(Arc::new(HookState {
                at,
                count: AtomicUsize::new(0),
                crash,
            })),
        }
    }

    #[cfg(test)]
    fn steps(&self) -> usize {
        self.inner
            .as_ref()
            .map_or(0, |i| i.count.load(Ordering::SeqCst))
    }
}

/// The filesystem implementation of the transaction factory.
#[derive(Default)]
pub struct FsFileTxFactory {
    /// Hook for the forward steps (backup, replace, delete, journal marks).
    hook: FaultHook,
    /// Hook for the steps of a rollback.
    rollback_hook: FaultHook,
}

impl FsFileTxFactory {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn with_fault(step: usize) -> Self {
        Self {
            hook: FaultHook::at(step, false),
            rollback_hook: FaultHook::default(),
        }
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn with_crash(step: usize) -> Self {
        Self {
            hook: FaultHook::at(step, true),
            rollback_hook: FaultHook::default(),
        }
    }

    /// Fails forward step `step` and then rollback step `rollback_step`.
    #[cfg(test)]
    fn with_rollback_fault(step: usize, rollback_step: usize) -> Self {
        Self {
            hook: FaultHook::at(step, false),
            rollback_hook: FaultHook::at(rollback_step, false),
        }
    }

    /// Counts the forward steps and never fails.
    #[cfg(test)]
    fn counting() -> Self {
        Self::with_fault(usize::MAX)
    }
}

fn io_err(what: &str, e: io::Error) -> DomainError {
    DomainError::Internal(format!("{what}: {e}"))
}

fn rocket_dir(root: &Path) -> PathBuf {
    root.join(".rocket")
}

fn journal_path(root: &Path) -> PathBuf {
    rocket_dir(root).join(JOURNAL_FILE)
}

fn staging_root(root: &Path) -> PathBuf {
    rocket_dir(root).join("staging")
}

fn staging_dir(root: &Path, id: &str) -> PathBuf {
    staging_root(root).join(id)
}

fn staged_path(root: &Path, id: &str, rel: &str) -> PathBuf {
    staging_dir(root, id).join("new").join(rel)
}

fn backup_path(root: &Path, id: &str, rel: &str) -> PathBuf {
    staging_dir(root, id).join("backup").join(rel)
}

/// Rejects a path that goes through a symbolic link, so a link inside the collection cannot
/// send a write or a delete outside it. Parts that do not exist yet are fine.
fn guard(root: &Path, rel: &str) -> DomainResult<()> {
    let mut current = root.to_path_buf();
    for part in rel.split('/') {
        current.push(part);
        match std::fs::symlink_metadata(&current) {
            Ok(meta) if meta.file_type().is_symlink() => {
                return Err(DomainError::InvalidInput(format!(
                    "\"{rel}\" goes through a symbolic link"
                )))
            }
            Ok(_) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => break,
            Err(e) => return Err(io_err("a path could not be checked", e)),
        }
    }
    Ok(())
}

fn serialize_journal(journal: &Journal) -> DomainResult<Vec<u8>> {
    serde_json::to_vec_pretty(journal)
        .map_err(|e| DomainError::Internal(format!("{JOURNAL_FILE} could not be written: {e}")))
}

fn write_journal(root: &Path, journal: &Journal) -> DomainResult<()> {
    guard(root, ".rocket/apply-journal.json")?;
    let json = serialize_journal(journal)?;
    atomic_write(&journal_path(root), &json).map_err(|e| io_err(JOURNAL_FILE, e))
}

/// Writes the first journal of a transaction. It fails when one exists, so two processes
/// cannot both begin.
fn create_journal(root: &Path, journal: &Journal) -> DomainResult<()> {
    use std::io::Write;
    guard(root, ".rocket/apply-journal.json")?;
    let json = serialize_journal(journal)?;
    std::fs::create_dir_all(rocket_dir(root)).map_err(|e| io_err(JOURNAL_FILE, e))?;
    let mut file = match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(journal_path(root))
    {
        Ok(file) => file,
        Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
            return Err(DomainError::Conflict(
                "an earlier sync apply was interrupted; recover it first".to_string(),
            ))
        }
        Err(e) => return Err(io_err(JOURNAL_FILE, e)),
    };
    if let Err(e) = file.write_all(&json).and_then(|()| file.sync_all()) {
        let _ = std::fs::remove_file(journal_path(root));
        return Err(io_err(JOURNAL_FILE, e));
    }
    Ok(())
}

/// True when the journal file exists but is empty or is not a journal.
fn journal_is_garbage(root: &Path) -> bool {
    if guard(root, ".rocket/apply-journal.json").is_err() {
        return false;
    }
    match std::fs::read_to_string(journal_path(root)) {
        // Valid JSON of another shape may be a newer format, so it is never discarded.
        Ok(text) => {
            text.trim().is_empty() || serde_json::from_str::<serde_json::Value>(&text).is_err()
        }
        Err(_) => false,
    }
}

fn read_journal(root: &Path) -> DomainResult<Option<Journal>> {
    guard(root, ".rocket/apply-journal.json")?;
    let text = match std::fs::read_to_string(journal_path(root)) {
        Ok(text) => text,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_err("the apply journal could not be read", e)),
    };
    let value: serde_json::Value = serde_json::from_str(&text)
        .map_err(|e| DomainError::Internal(format!("{JOURNAL_FILE} cannot be parsed: {e}")))?;
    let version = value.get("version").and_then(serde_json::Value::as_u64);
    if version.is_some_and(|v| v > u64::from(JOURNAL_VERSION)) {
        return Err(DomainError::Internal(format!(
            "{JOURNAL_FILE} was written by a newer version of Rocket"
        )));
    }
    serde_json::from_value(value)
        .map(Some)
        .map_err(|e| DomainError::Internal(format!("{JOURNAL_FILE} cannot be parsed: {e}")))
}

/// Removes the journal, then the staging folder of one transaction.
fn cleanup(root: &Path, id: &str) -> DomainResult<()> {
    match std::fs::remove_file(journal_path(root)) {
        Ok(()) => {}
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(io_err("the apply journal could not be removed", e)),
    }
    remove_staging(root, id);
    Ok(())
}

/// Best effort: a leftover staging folder is cleared by the next `begin`.
fn remove_staging(root: &Path, id: &str) {
    let _ = std::fs::remove_dir_all(staging_dir(root, id));
    let _ = std::fs::remove_dir(staging_root(root));
}

/// Best effort: flushes a folder entry so a rename survives a power cut.
fn sync_dir(dir: &Path) {
    if let Ok(file) = std::fs::File::open(dir) {
        let _ = file.sync_all();
    }
}

fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(from, to)?;
    if let Some(parent) = to.parent() {
        sync_dir(parent);
    }
    if let Some(parent) = from.parent() {
        sync_dir(parent);
    }
    Ok(())
}

/// Applies the ops, in order, stopping after `limit` newly done ones when set. It checks what
/// is already on disk, so it is safe to run again after a crash.
fn apply_ops(
    root: &Path,
    journal: &mut Journal,
    hook: &FaultHook,
    limit: Option<usize>,
) -> DomainResult<usize> {
    guard(root, ".rocket/staging")?;
    let mut applied = 0;
    for idx in 0..journal.ops.len() {
        let was_done = journal.ops[idx].done;
        if !was_done && limit.is_some_and(|l| applied >= l) {
            break;
        }
        let op = journal.ops[idx].clone();
        guard(root, &op.path)?;
        let target = root.join(&op.path);
        let backup = backup_path(root, &journal.id, &op.path);
        let staged = staged_path(root, &journal.id, &op.path);
        // A put whose staged file is gone was already moved into place.
        let put_pending = op.is_put() && staged.exists();
        if (put_pending || !op.is_put()) && !backup.exists() && target.is_file() {
            hook.check().map_err(|e| io_err("backup failed", e))?;
            move_file(&target, &backup).map_err(|e| io_err("backup failed", e))?;
            journal.ops[idx].backup = true;
        }
        if put_pending {
            hook.check().map_err(|e| io_err("replace failed", e))?;
            move_file(&staged, &target).map_err(|e| io_err("replace failed", e))?;
        }
        if !was_done {
            hook.check()
                .map_err(|e| io_err("journal update failed", e))?;
            journal.ops[idx].done = true;
            write_journal(root, journal)?;
            applied += 1;
        }
    }
    Ok(applied)
}

/// Restores every file touched by the journal. Every step is decided from the disk, and a
/// replaced new file goes back to its staged place, so a run cut short can run again.
/// Returns how many ops changed something.
fn rollback(root: &Path, journal: &Journal, hook: &FaultHook) -> DomainResult<usize> {
    guard(root, ".rocket/staging")?;
    let mut changed = 0;
    for op in journal.ops.iter().rev() {
        guard(root, &op.path)?;
        let target = root.join(&op.path);
        let backup = backup_path(root, &journal.id, &op.path);
        let staged = staged_path(root, &journal.id, &op.path);
        let new_in_place = op.is_put() && !staged.exists() && target.is_file();
        if backup.exists() {
            if new_in_place {
                hook.check().map_err(|e| io_err("rollback failed", e))?;
                move_file(&target, &staged).map_err(|e| io_err("rollback failed", e))?;
            } else if target.is_file() {
                hook.check().map_err(|e| io_err("rollback failed", e))?;
                std::fs::remove_file(&target).map_err(|e| io_err("rollback failed", e))?;
            }
            hook.check().map_err(|e| io_err("rollback failed", e))?;
            move_file(&backup, &target).map_err(|e| io_err("rollback failed", e))?;
            changed += 1;
        } else if new_in_place {
            // A new file that did not exist before.
            hook.check().map_err(|e| io_err("rollback failed", e))?;
            move_file(&target, &staged).map_err(|e| io_err("rollback failed", e))?;
            changed += 1;
        }
    }
    Ok(changed)
}

/// Marks the journal `rolling_back`, then restores the files. Keeps the journal when the
/// restore fails, so the next `recover` continues it.
fn roll_back_and_clean(
    root: &Path,
    journal: &mut Journal,
    hook: &FaultHook,
) -> DomainResult<usize> {
    journal.phase = Phase::RollingBack;
    write_journal(root, journal)?;
    let files = rollback(root, journal, hook)?;
    if let Err(e) = cleanup(root, &journal.id) {
        tracing::warn!(error = %e, "the apply journal could not be cleaned up after a rollback");
    }
    Ok(files)
}

/// Collection roots that a transaction or a recovery is working on in this process.
static BUSY_ROOTS: std::sync::OnceLock<(
    std::sync::Mutex<std::collections::HashSet<PathBuf>>,
    std::sync::Condvar,
)> = std::sync::OnceLock::new();

fn busy_roots() -> &'static (
    std::sync::Mutex<std::collections::HashSet<PathBuf>>,
    std::sync::Condvar,
) {
    BUSY_ROOTS.get_or_init(Default::default)
}

/// Marks a collection root as in use until dropped, so a recovery never overlaps a running
/// transaction on the same collection.
struct RootGuard(PathBuf);

impl RootGuard {
    /// Takes the root, or `None` when another transaction or recovery holds it.
    fn try_acquire(root: &Path) -> Option<Self> {
        let (lock, _) = busy_roots();
        let mut set = lock.lock().unwrap_or_else(|e| e.into_inner());
        set.insert(root.to_path_buf())
            .then(|| Self(root.to_path_buf()))
    }

    /// Takes the root, waiting while another holder has it.
    fn acquire(root: &Path) -> Self {
        let (lock, cvar) = busy_roots();
        let mut set = lock.lock().unwrap_or_else(|e| e.into_inner());
        while set.contains(root) {
            set = cvar.wait(set).unwrap_or_else(|e| e.into_inner());
        }
        set.insert(root.to_path_buf());
        Self(root.to_path_buf())
    }
}

impl Drop for RootGuard {
    fn drop(&mut self) {
        let (lock, cvar) = busy_roots();
        lock.lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&self.0);
        cvar.notify_all();
    }
}

struct FsFileTx {
    /// Held until the transaction ends.
    _guard: RootGuard,
    root: PathBuf,
    id: String,
    ops: Vec<JournalOp>,
    hook: FaultHook,
    rollback_hook: FaultHook,
}

impl FsFileTx {
    fn journal(&self, phase: Phase) -> Journal {
        Journal {
            version: JOURNAL_VERSION,
            id: self.id.clone(),
            phase,
            ops: self.ops.clone(),
        }
    }

    fn check_new_path(&self, rel: &str) -> DomainResult<()> {
        validate_rel_path(rel)?;
        guard(&self.root, ".rocket/staging")?;
        guard(&self.root, rel)?;
        let folded = rel.to_ascii_lowercase();
        if self
            .ops
            .iter()
            .any(|o| o.path.to_ascii_lowercase() == folded)
        {
            return Err(DomainError::InvalidInput(format!(
                "path \"{rel}\" is staged twice (paths are compared without case)"
            )));
        }
        Ok(())
    }

    /// Commits, applying at most `limit` ops when set (tests use this to simulate a crash).
    /// Checks that the staging area is still ours before anything moves. Another process may
    /// have run a recovery that discarded it. Nothing is cleaned up on failure, because the
    /// files may belong to someone else now.
    fn check_staging_intact(&self) -> DomainResult<()> {
        let lost =
            || DomainError::Conflict("the staging area was lost, nothing was changed".to_string());
        match read_journal(&self.root) {
            Ok(Some(j)) if j.id == self.id && j.phase == Phase::Staging => {}
            Err(e) => {
                // The journal cannot be read at all. Only this transaction's own staging
                // folder is cleared.
                remove_staging(&self.root, &self.id);
                return Err(e);
            }
            _ => return Err(lost()),
        }
        let missing = self
            .ops
            .iter()
            .filter(|o| o.is_put())
            .any(|o| !staged_path(&self.root, &self.id, &o.path).is_file());
        if missing {
            return Err(lost());
        }
        Ok(())
    }

    fn commit_inner(&mut self, limit: Option<usize>) -> DomainResult<()> {
        self.check_staging_intact()?;
        let mut journal = self.journal(Phase::Committing);
        if let Err(e) = write_journal(&self.root, &journal) {
            // Nothing was applied yet.
            remove_staging(&self.root, &self.id);
            let _ = std::fs::remove_file(journal_path(&self.root));
            return Err(e);
        }
        match apply_ops(&self.root, &mut journal, &self.hook, limit) {
            Ok(_) => {
                if limit.is_none() {
                    // The commit worked. A leftover journal only makes the next recover a no-op.
                    if let Err(e) = cleanup(&self.root, &self.id) {
                        tracing::warn!(error = %e, "the apply journal could not be removed after a commit");
                    }
                }
                Ok(())
            }
            Err(e) => {
                if self.hook.is_crash() {
                    return Err(e);
                }
                match roll_back_and_clean(&self.root, &mut journal, &self.rollback_hook) {
                    Ok(_) => Err(e),
                    Err(rollback_err) => Err(DomainError::Internal(format!(
                        "{e}; the files could not be restored yet, so the apply may be finished or restored by the next recovery: {rollback_err}"
                    ))),
                }
            }
        }
    }
}

impl CollectionFileTx for FsFileTx {
    fn stage_put(&mut self, rel_path: &str, bytes: &[u8]) -> DomainResult<()> {
        self.check_new_path(rel_path)?;
        if std::fs::symlink_metadata(self.root.join(rel_path)).is_ok_and(|m| m.is_dir()) {
            return Err(DomainError::InvalidInput(format!(
                "path \"{rel_path}\" is a folder"
            )));
        }
        atomic_write(&staged_path(&self.root, &self.id, rel_path), bytes)
            .map_err(|e| io_err("a file could not be staged", e))?;
        self.ops.push(JournalOp {
            op: OpKind::Put,
            path: rel_path.to_string(),
            backup: false,
            done: false,
        });
        Ok(())
    }

    fn stage_delete(&mut self, rel_path: &str) -> DomainResult<()> {
        self.check_new_path(rel_path)?;
        if std::fs::symlink_metadata(self.root.join(rel_path)).is_ok_and(|m| m.is_dir()) {
            return Err(DomainError::InvalidInput(format!(
                "path \"{rel_path}\" is a folder and cannot be deleted here"
            )));
        }
        self.ops.push(JournalOp {
            op: OpKind::Delete,
            path: rel_path.to_string(),
            backup: false,
            done: false,
        });
        Ok(())
    }

    fn commit(mut self: Box<Self>) -> DomainResult<()> {
        self.commit_inner(None)
    }

    fn abort(self: Box<Self>) {
        let _ = cleanup(&self.root, &self.id);
    }
}

/// Collects the files under `dir` without following links. Folders that cannot be read fail
/// the listing, so a half-known collection is never used.
fn walk_files(
    root: &Path,
    dir: &Path,
    out: &mut std::collections::HashSet<String>,
) -> DomainResult<()> {
    let entries = std::fs::read_dir(dir).map_err(|e| io_err("a folder could not be listed", e))?;
    for entry in entries {
        let entry = entry.map_err(|e| io_err("a folder could not be listed", e))?;
        let path = entry.path();
        let Ok(rel) = path.strip_prefix(root) else {
            continue;
        };
        let rel = rel
            .components()
            .map(|c| c.as_os_str().to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("/");
        if rel == ".rocket" || rel == ".git" {
            continue;
        }
        let kind = entry
            .file_type()
            .map_err(|e| io_err("a file could not be inspected", e))?;
        if kind.is_dir() {
            walk_files(root, &path, out)?;
        } else {
            out.insert(rel);
        }
    }
    Ok(())
}

impl CollectionFileTxFactory for FsFileTxFactory {
    fn list_files(
        &self,
        collection_root: &Path,
    ) -> DomainResult<std::collections::HashSet<String>> {
        let mut out = std::collections::HashSet::new();
        walk_files(collection_root, collection_root, &mut out)?;
        Ok(out)
    }

    fn begin(&self, collection_root: &Path) -> DomainResult<Box<dyn CollectionFileTx>> {
        guard(collection_root, ".rocket")?;
        let Some(root_guard) = RootGuard::try_acquire(collection_root) else {
            return Err(DomainError::Conflict(
                "another sync apply is running on this collection".to_string(),
            ));
        };
        crate::rocket_dir::ensure_gitignore(collection_root)?;
        let id = uuid::Uuid::new_v4().to_string();
        let tx = FsFileTx {
            _guard: root_guard,
            root: collection_root.to_path_buf(),
            id,
            ops: Vec::new(),
            hook: self.hook.clone(),
            rollback_hook: self.rollback_hook.clone(),
        };
        // Taking the journal first makes this process the only one that may touch staging.
        create_journal(collection_root, &tx.journal(Phase::Staging))?;
        guard(collection_root, ".rocket/staging")?;
        // No other journal existed, so any staging folder is a leftover.
        let _ = std::fs::remove_dir_all(staging_root(collection_root));
        if let Err(e) = std::fs::create_dir_all(staging_dir(collection_root, &tx.id)) {
            let _ = std::fs::remove_file(journal_path(collection_root));
            return Err(io_err("the staging folder could not be created", e));
        }
        Ok(Box::new(tx))
    }

    fn recover(&self, collection_root: &Path) -> DomainResult<RecoveryOutcome> {
        // Waits for a running transaction, so a staging journal is never taken for a leftover.
        let _root = RootGuard::acquire(collection_root);
        let journal = match read_journal(collection_root) {
            Ok(journal) => journal,
            Err(e) => {
                // An empty or unreadable journal cannot be continued. It is dropped, so one
                // bad file does not block every later sync call.
                if !journal_is_garbage(collection_root) {
                    return Err(e);
                }
                tracing::warn!("an unreadable apply journal was discarded");
                let _ = std::fs::remove_file(journal_path(collection_root));
                let _ = std::fs::remove_dir_all(staging_root(collection_root));
                return Ok(RecoveryOutcome::Discarded);
            }
        };
        let Some(mut journal) = journal else {
            return Ok(RecoveryOutcome::Nothing);
        };
        match journal.phase {
            Phase::Staging => {
                cleanup(collection_root, &journal.id)?;
                Ok(RecoveryOutcome::Discarded)
            }
            Phase::RollingBack => {
                let files = rollback(collection_root, &journal, &self.rollback_hook)?;
                cleanup(collection_root, &journal.id)?;
                Ok(RecoveryOutcome::RolledBack { files })
            }
            Phase::Committing => match apply_ops(collection_root, &mut journal, &self.hook, None) {
                Ok(files) => {
                    cleanup(collection_root, &journal.id)?;
                    Ok(RecoveryOutcome::Finished { files })
                }
                Err(e) => {
                    if self.hook.is_crash() {
                        return Err(e);
                    }
                    tracing::warn!(error = %e, "finishing an interrupted sync apply failed, rolling it back");
                    let files =
                        roll_back_and_clean(collection_root, &mut journal, &self.rollback_hook)?;
                    Ok(RecoveryOutcome::RolledBack { files })
                }
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tempfile::tempdir;

    /// Every file under the root as relative path to bytes, without the gitignore.
    fn snapshot(root: &Path) -> BTreeMap<String, Vec<u8>> {
        fn walk(root: &Path, dir: &Path, out: &mut BTreeMap<String, Vec<u8>>) {
            for entry in std::fs::read_dir(dir).expect("read_dir") {
                let path = entry.expect("entry").path();
                if path.is_dir() {
                    walk(root, &path, out);
                } else {
                    let rel = path.strip_prefix(root).expect("prefix");
                    let rel = rel.to_string_lossy().replace('\\', "/");
                    if rel != ".rocket/.gitignore" {
                        out.insert(rel, std::fs::read(&path).expect("read"));
                    }
                }
            }
        }
        let mut out = BTreeMap::new();
        walk(root, root, &mut out);
        out
    }

    fn write(root: &Path, rel: &str, text: &str) {
        let path = root.join(rel);
        std::fs::create_dir_all(path.parent().expect("parent")).expect("dirs");
        std::fs::write(path, text).expect("write");
    }

    fn read(root: &Path, rel: &str) -> String {
        std::fs::read_to_string(root.join(rel)).expect("read")
    }

    fn factory() -> FsFileTxFactory {
        FsFileTxFactory::new()
    }

    /// Stages 2 puts over existing files, 2 new puts and 2 deletes (one of a missing file).
    fn stage_six(tx: &mut dyn CollectionFileTx) {
        tx.stage_put("a.yml", b"A2").expect("put");
        tx.stage_put("dir/b.yml", b"B2").expect("put");
        tx.stage_put("new/deep/c.yml", b"C").expect("put");
        tx.stage_put(".rocket/openapi-baseline.json", b"{}")
            .expect("put");
        tx.stage_delete("gone.yml").expect("delete");
        tx.stage_delete("missing.yml").expect("delete");
    }

    fn seed(root: &Path) {
        write(root, "a.yml", "A1");
        write(root, "dir/b.yml", "B1");
        write(root, "gone.yml", "G1");
        write(root, "keep.yml", "K1");
    }

    /// The tree a successful six-op commit leaves.
    fn final_tree() -> BTreeMap<String, Vec<u8>> {
        let dir = tempdir().expect("tempdir");
        seed(dir.path());
        let mut tx = factory().begin(dir.path()).expect("begin");
        stage_six(tx.as_mut());
        tx.commit().expect("commit");
        snapshot(dir.path())
    }

    /// How many hook steps a successful six-op commit takes.
    fn six_op_steps() -> usize {
        let dir = tempdir().expect("tempdir");
        seed(dir.path());
        let counting = FsFileTxFactory::counting();
        let mut tx = counting.begin(dir.path()).expect("begin");
        stage_six(tx.as_mut());
        tx.commit().expect("commit");
        counting.hook.steps()
    }

    fn no_leftovers(root: &Path) {
        assert!(!root.join(".rocket/apply-journal.json").exists());
        assert!(!root.join(".rocket/staging").exists());
    }

    #[test]
    fn commit_applies_puts_and_deletes() {
        let dir = tempdir().expect("tempdir");
        seed(dir.path());
        let mut tx = factory().begin(dir.path()).expect("begin");
        stage_six(tx.as_mut());
        tx.commit().expect("commit");
        assert_eq!(read(dir.path(), "a.yml"), "A2");
        assert_eq!(read(dir.path(), "dir/b.yml"), "B2");
        assert_eq!(read(dir.path(), "new/deep/c.yml"), "C");
        assert_eq!(read(dir.path(), "keep.yml"), "K1");
        assert!(!dir.path().join("gone.yml").exists());
    }

    #[test]
    fn a_put_over_an_existing_file_keeps_nothing_behind() {
        let dir = tempdir().expect("tempdir");
        seed(dir.path());
        let mut tx = factory().begin(dir.path()).expect("begin");
        tx.stage_put("a.yml", b"A2").expect("put");
        tx.commit().expect("commit");
        no_leftovers(dir.path());
    }

    #[test]
    fn new_files_get_parent_folders() {
        let dir = tempdir().expect("tempdir");
        let mut tx = factory().begin(dir.path()).expect("begin");
        tx.stage_put("x/y/z.yml", b"Z").expect("put");
        tx.commit().expect("commit");
        assert_eq!(read(dir.path(), "x/y/z.yml"), "Z");
    }

    #[test]
    fn delete_of_a_missing_file_is_ok() {
        let dir = tempdir().expect("tempdir");
        let mut tx = factory().begin(dir.path()).expect("begin");
        tx.stage_delete("nope.yml").expect("delete");
        tx.commit().expect("commit");
    }

    #[test]
    fn failure_at_each_step_restores_every_file_byte_for_byte() {
        let steps = six_op_steps();
        assert_eq!(steps, 13, "the step count of the six-op commit changed");
        for k in 0..=steps {
            let dir = tempdir().expect("tempdir");
            seed(dir.path());
            let before = snapshot(dir.path());
            let mut tx = FsFileTxFactory::with_fault(k)
                .begin(dir.path())
                .expect("begin");
            stage_six(tx.as_mut());
            let result = tx.commit();
            if k < steps {
                assert!(result.is_err(), "fault at step {k} did not fail the commit");
                assert_eq!(snapshot(dir.path()), before, "fault at step {k}");
            } else {
                result.expect("a fault past the last step changes nothing");
                assert_eq!(snapshot(dir.path()), final_tree());
            }
            no_leftovers(dir.path());
        }
    }

    #[test]
    fn a_crash_at_each_step_is_finished_by_recover() {
        let steps = six_op_steps();
        let expected = final_tree();
        for k in 0..steps {
            let dir = tempdir().expect("tempdir");
            seed(dir.path());
            let mut tx = FsFileTxFactory::with_crash(k)
                .begin(dir.path())
                .expect("begin");
            stage_six(tx.as_mut());
            assert!(tx.commit().is_err(), "crash at step {k}");
            let outcome = factory().recover(dir.path()).expect("recover");
            assert!(
                matches!(outcome, RecoveryOutcome::Finished { .. }),
                "step {k}: {outcome:?}"
            );
            assert_eq!(snapshot(dir.path()), expected, "crash at step {k}");
            no_leftovers(dir.path());
        }
    }

    #[test]
    fn a_failed_rollback_is_continued_by_recover() {
        let steps = six_op_steps();
        let mut rolled_back = 0;
        for k in 0..steps {
            for j in 0..4 {
                let dir = tempdir().expect("tempdir");
                seed(dir.path());
                let before = snapshot(dir.path());
                let mut tx = FsFileTxFactory::with_rollback_fault(k, j)
                    .begin(dir.path())
                    .expect("begin");
                stage_six(tx.as_mut());
                assert!(tx.commit().is_err());
                let outcome = factory().recover(dir.path()).expect("recover");
                if matches!(outcome, RecoveryOutcome::RolledBack { .. }) {
                    rolled_back += 1;
                }
                assert_eq!(snapshot(dir.path()), before, "faults at {k} and {j}");
                no_leftovers(dir.path());
            }
        }
        assert!(rolled_back > 0, "no rollback was ever cut short");
    }

    #[test]
    fn recovery_that_crashes_is_run_again() {
        let steps = six_op_steps();
        let expected = final_tree();
        for k in 0..steps {
            let dir = tempdir().expect("tempdir");
            seed(dir.path());
            let mut tx = FsFileTxFactory::with_crash(steps / 2)
                .begin(dir.path())
                .expect("begin");
            stage_six(tx.as_mut());
            assert!(tx.commit().is_err());
            // The first recovery crashes too (at step k, or finishes when k is past its work).
            let _ = FsFileTxFactory::with_crash(k).recover(dir.path());
            factory().recover(dir.path()).expect("second recover");
            assert_eq!(snapshot(dir.path()), expected, "recovery crash at {k}");
            no_leftovers(dir.path());
        }
    }

    #[test]
    fn a_rollback_started_by_recovery_and_cut_short_is_continued() {
        let dir = tempdir().expect("tempdir");
        seed(dir.path());
        let before = snapshot(dir.path());
        crashed_after(dir.path(), 2);
        // The roll-forward fails at step 2, and then the rollback fails at its first step.
        assert!(FsFileTxFactory::with_rollback_fault(2, 0)
            .recover(dir.path())
            .is_err());
        assert_eq!(
            factory().recover(dir.path()).expect("recover"),
            RecoveryOutcome::RolledBack { files: 3 }
        );
        assert_eq!(snapshot(dir.path()), before);
        no_leftovers(dir.path());
    }

    #[test]
    fn a_journal_that_cannot_be_written_changes_nothing() {
        let dir = tempdir().expect("tempdir");
        seed(dir.path());
        let before = snapshot(dir.path());
        let mut tx = factory().begin(dir.path()).expect("begin");
        std::fs::remove_file(dir.path().join(".rocket/apply-journal.json")).expect("remove");
        std::fs::create_dir(dir.path().join(".rocket/apply-journal.json")).expect("dir");
        stage_six(tx.as_mut());
        assert!(tx.commit().is_err());
        assert_eq!(snapshot(dir.path()), before);
    }

    #[test]
    fn abort_removes_the_staging_area() {
        let dir = tempdir().expect("tempdir");
        seed(dir.path());
        let before = snapshot(dir.path());
        let mut tx = factory().begin(dir.path()).expect("begin");
        tx.stage_put("a.yml", b"A2").expect("put");
        tx.abort();
        assert_eq!(snapshot(dir.path()), before);
        assert!(!dir.path().join(".rocket/staging").exists());
    }

    #[test]
    fn recover_waits_for_a_running_transaction_and_keeps_its_journal() {
        let dir = tempdir().expect("tempdir");
        let mut tx = factory().begin(dir.path()).expect("begin");
        tx.stage_put("a.yml", b"A").expect("put");
        let root = dir.path().to_path_buf();
        let handle = std::thread::spawn(move || factory().recover(&root));
        std::thread::sleep(std::time::Duration::from_millis(300));
        assert!(
            !handle.is_finished(),
            "recover must wait for the transaction"
        );
        assert!(dir.path().join(".rocket/apply-journal.json").exists());
        tx.commit().expect("commit");
        let outcome = handle.join().expect("thread").expect("recover");
        assert_eq!(outcome, RecoveryOutcome::Nothing);
        assert_eq!(read(dir.path(), "a.yml"), "A");
    }

    #[test]
    fn commit_refuses_when_the_journal_was_taken_over() {
        let dir = tempdir().expect("tempdir");
        write(dir.path(), "a.yml", "A1");
        let mut tx = factory().begin(dir.path()).expect("begin");
        tx.stage_put("a.yml", b"A2").expect("put");
        // Another process recovered, discarded this staging area and began its own.
        std::fs::remove_file(dir.path().join(".rocket/apply-journal.json")).expect("remove");
        let other = Journal {
            version: JOURNAL_VERSION,
            id: "other".to_string(),
            phase: Phase::Staging,
            ops: Vec::new(),
        };
        create_journal(dir.path(), &other).expect("journal");
        let err = tx.commit().expect_err("lost");
        assert!(matches!(err, DomainError::Conflict(_)), "{err}");
        assert_eq!(read(dir.path(), "a.yml"), "A1");
        // The other transaction's journal is left alone.
        assert!(dir.path().join(".rocket/apply-journal.json").exists());
    }

    #[test]
    fn commit_refuses_when_a_staged_file_is_gone() {
        let dir = tempdir().expect("tempdir");
        write(dir.path(), "a.yml", "A1");
        let mut tx = factory().begin(dir.path()).expect("begin");
        tx.stage_put("a.yml", b"A2").expect("put");
        let staging = dir.path().join(".rocket/staging");
        std::fs::remove_dir_all(&staging).expect("remove");
        let err = tx.commit().expect_err("lost");
        assert!(matches!(err, DomainError::Conflict(_)), "{err}");
        assert_eq!(read(dir.path(), "a.yml"), "A1");
    }

    #[test]
    fn an_empty_or_unparseable_journal_is_discarded() {
        for text in ["", "  \n", "{not json"] {
            let dir = tempdir().expect("tempdir");
            write(dir.path(), ".rocket/apply-journal.json", text);
            std::fs::create_dir_all(dir.path().join(".rocket/staging/x")).expect("dir");
            assert_eq!(
                factory().recover(dir.path()).expect("recover"),
                RecoveryOutcome::Discarded
            );
            assert!(!dir.path().join(".rocket/apply-journal.json").exists());
            assert!(!dir.path().join(".rocket/staging").exists());
            factory()
                .begin(dir.path())
                .expect("begin works again")
                .abort();
        }
    }

    #[test]
    fn a_journal_of_a_newer_format_is_an_error_and_is_kept() {
        let dir = tempdir().expect("tempdir");
        let text = r#"{"version":99,"phase":"future","steps":[]}"#;
        write(dir.path(), ".rocket/apply-journal.json", text);
        assert!(factory().recover(dir.path()).is_err());
        assert_eq!(read(dir.path(), ".rocket/apply-journal.json"), text);
    }

    #[test]
    fn begin_fails_when_a_journal_exists() {
        let dir = tempdir().expect("tempdir");
        let _open = factory().begin(dir.path()).expect("begin");
        let err = factory().begin(dir.path()).err().expect("conflict");
        assert!(matches!(err, DomainError::Conflict(_)), "{err}");
        // The refused begin leaves the first transaction's staging alone.
        assert!(dir.path().join(".rocket/staging").exists());
    }

    #[test]
    fn rejected_paths() {
        let dir = tempdir().expect("tempdir");
        let mut tx = factory().begin(dir.path()).expect("begin");
        for bad in [
            "",
            "/etc/x",
            "../x",
            "a/../../x",
            "a\\b",
            ".rocket/staging/x",
            ".rocket/other.txt",
            ".ROCKET/openapi-baseline.json",
            ".Rocket/Staging/x",
            "a\0b",
            "c:/x",
            "a/b:stream",
            "CON",
            "nul.txt",
            "dir/Com1.yml",
            "lpt9",
        ] {
            assert!(tx.stage_put(bad, b"x").is_err(), "{bad:?} was accepted");
            assert!(tx.stage_delete(bad).is_err(), "{bad:?} was accepted");
        }
    }

    #[test]
    fn allowed_state_files_under_dot_rocket() {
        let dir = tempdir().expect("tempdir");
        let mut tx = factory().begin(dir.path()).expect("begin");
        for ok in OWNED_STATE_FILES {
            tx.stage_put(&format!(".rocket/{ok}"), b"x").expect(ok);
        }
    }

    #[test]
    fn a_path_staged_twice_is_rejected_even_with_another_case() {
        let dir = tempdir().expect("tempdir");
        let mut tx = factory().begin(dir.path()).expect("begin");
        tx.stage_put("Users/A.yml", b"1").expect("put");
        assert!(tx.stage_put("Users/A.yml", b"2").is_err());
        assert!(tx.stage_put("users/a.YML", b"2").is_err());
        assert!(tx.stage_delete("USERS/a.yml").is_err());
        tx.stage_put("users/b.yml", b"3").expect("other path");
    }

    #[test]
    fn a_folder_cannot_be_deleted_or_replaced() {
        let dir = tempdir().expect("tempdir");
        write(dir.path(), "dir/b.yml", "B");
        let mut tx = factory().begin(dir.path()).expect("begin");
        let err = tx.stage_delete("dir").expect_err("folder");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err}");
        assert!(tx.stage_put("dir", b"x").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn symbolic_links_are_refused() {
        use std::os::unix::fs::symlink;
        let outside = tempdir().expect("outside");
        let dir = tempdir().expect("tempdir");
        symlink(outside.path(), dir.path().join("link")).expect("symlink");
        let mut tx = factory().begin(dir.path()).expect("begin");
        assert!(tx.stage_put("link/x.yml", b"x").is_err());
        assert!(tx.stage_delete("link/x.yml").is_err());
        assert!(tx.stage_put("link", b"x").is_err());
        assert_eq!(std::fs::read_dir(outside.path()).expect("dir").count(), 0);

        // A journal that names a linked path is refused when it is applied.
        let other = tempdir().expect("other");
        let linked_rocket = tempdir().expect("linked");
        symlink(linked_rocket.path(), other.path().join(".rocket")).expect("symlink");
        let err = factory().begin(other.path()).err().expect("refused");
        assert!(matches!(err, DomainError::InvalidInput(_)), "{err}");
        assert_eq!(
            std::fs::read_dir(linked_rocket.path())
                .expect("dir")
                .count(),
            0
        );
        assert!(factory().recover(other.path()).is_err());
    }

    #[test]
    fn crash_while_staging_is_discarded() {
        let dir = tempdir().expect("tempdir");
        seed(dir.path());
        let before = snapshot(dir.path());
        let mut tx = factory().begin(dir.path()).expect("begin");
        tx.stage_put("a.yml", b"A2").expect("put");
        drop(tx);
        assert_eq!(
            factory().recover(dir.path()).expect("recover"),
            RecoveryOutcome::Discarded
        );
        assert_eq!(snapshot(dir.path()), before);
        assert!(!dir.path().join(".rocket/staging").exists());
    }

    /// Builds the staged state of a transaction that crashed after `after` ops.
    fn crashed_after(root: &Path, after: usize) {
        let id = "crash".to_string();
        let mut tx = FsFileTx {
            _guard: RootGuard::acquire(root),
            root: root.to_path_buf(),
            id: id.clone(),
            ops: Vec::new(),
            hook: FaultHook::default(),
            rollback_hook: FaultHook::default(),
        };
        create_journal(root, &tx.journal(Phase::Staging)).expect("journal");
        std::fs::create_dir_all(staging_dir(root, &id)).expect("dir");
        tx.stage_put("a.yml", b"A2").expect("put");
        tx.stage_put("dir/b.yml", b"B2").expect("put");
        tx.stage_put("new/c.yml", b"C").expect("put");
        tx.stage_delete("gone.yml").expect("delete");
        tx.stage_put("d.yml", b"D").expect("put");
        tx.commit_inner(Some(after)).expect("partial commit");
    }

    #[test]
    fn crash_mid_commit_rolls_forward() {
        let dir = tempdir().expect("tempdir");
        seed(dir.path());
        crashed_after(dir.path(), 2);
        assert_eq!(read(dir.path(), "a.yml"), "A2");
        assert_eq!(read(dir.path(), "dir/b.yml"), "B2");
        assert_eq!(
            factory().recover(dir.path()).expect("recover"),
            RecoveryOutcome::Finished { files: 3 }
        );
        assert_eq!(read(dir.path(), "new/c.yml"), "C");
        assert_eq!(read(dir.path(), "d.yml"), "D");
        assert!(!dir.path().join("gone.yml").exists());
        no_leftovers(dir.path());
    }

    #[test]
    fn recovery_reapplies_a_done_put_whose_staged_file_is_still_there() {
        let dir = tempdir().expect("tempdir");
        seed(dir.path());
        crashed_after(dir.path(), 1);
        // Put the first op's staged file back, as if its replace had not reached the disk.
        std::fs::rename(
            dir.path().join("a.yml"),
            staged_path(dir.path(), "crash", "a.yml"),
        )
        .expect("undo the replace");
        write(dir.path(), "a.yml", "A1");
        std::fs::remove_file(backup_path(dir.path(), "crash", "a.yml")).expect("no backup");
        factory().recover(dir.path()).expect("recover");
        assert_eq!(read(dir.path(), "a.yml"), "A2");
    }

    #[test]
    fn roll_forward_failure_rolls_back() {
        let dir = tempdir().expect("tempdir");
        seed(dir.path());
        let before = snapshot(dir.path());
        crashed_after(dir.path(), 2);
        let outcome = FsFileTxFactory::with_fault(2)
            .recover(dir.path())
            .expect("recover");
        assert!(
            matches!(outcome, RecoveryOutcome::RolledBack { .. }),
            "{outcome:?}"
        );
        assert_eq!(snapshot(dir.path()), before);
        no_leftovers(dir.path());
    }

    #[test]
    fn recover_without_a_journal_is_nothing() {
        let dir = tempdir().expect("tempdir");
        assert_eq!(
            factory().recover(dir.path()).expect("recover"),
            RecoveryOutcome::Nothing
        );
    }

    #[test]
    fn recover_is_idempotent() {
        let dir = tempdir().expect("tempdir");
        seed(dir.path());
        crashed_after(dir.path(), 1);
        factory().recover(dir.path()).expect("first");
        let after = snapshot(dir.path());
        assert_eq!(
            factory().recover(dir.path()).expect("second"),
            RecoveryOutcome::Nothing
        );
        assert_eq!(snapshot(dir.path()), after);
    }
}
