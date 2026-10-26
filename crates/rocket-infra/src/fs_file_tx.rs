//! A file transaction for one collection folder, with a journal for crash recovery.
//!
//! Staged files live in `<collection>/.rocket/staging/<id>/new`. Commit writes the journal as
//! `committing`, then backs up, replaces or deletes each file in order, marking the op done.
//! A failed commit restores every file from `staging/<id>/backup`. `recover` finishes or undoes
//! a commit that a crash interrupted.

use std::io;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

use rocket_collection::file_tx::{CollectionFileTx, CollectionFileTxFactory, RecoveryOutcome};
use rocket_shared::error::{DomainError, DomainResult};
use serde::{Deserialize, Serialize};

use crate::atomic_write;

const JOURNAL_FILE: &str = "apply-journal.json";
const JOURNAL_VERSION: u32 = 1;
/// Files under `.rocket` that this feature owns and a transaction may touch.
const OWNED_STATE_FILES: [&str; 4] = [
    "openapi-source.yml",
    "openapi-baseline.json",
    "openapi-pending.spec",
    "openapi-local.json",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum Phase {
    Staging,
    Committing,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Journal {
    version: u32,
    id: String,
    phase: Phase,
    ops: Vec<JournalOp>,
}

/// Fails the k-th filesystem step of a commit, in tests only.
#[derive(Clone, Default)]
struct FaultHook {
    #[cfg(test)]
    inner: Option<Arc<(usize, AtomicUsize)>>,
}

impl FaultHook {
    fn check(&self) -> io::Result<()> {
        #[cfg(test)]
        if let Some(inner) = &self.inner {
            if inner.1.fetch_add(1, Ordering::SeqCst) == inner.0 {
                return Err(io::Error::other("injected fault"));
            }
        }
        Ok(())
    }
}

/// The filesystem implementation of the transaction factory.
#[derive(Default)]
pub struct FsFileTxFactory {
    hook: FaultHook,
}

impl FsFileTxFactory {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn with_fault(step: usize) -> Self {
        Self {
            hook: FaultHook {
                inner: Some(Arc::new((step, AtomicUsize::new(0)))),
            },
        }
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

/// Checks a path given to a transaction.
fn validate_rel_path(rel: &str) -> DomainResult<()> {
    let bad = |why: &str| Err(DomainError::InvalidInput(format!("path \"{rel}\" {why}")));
    if rel.is_empty() {
        return bad("is empty");
    }
    if rel.contains('\\') || rel.contains('\0') {
        return bad("has a backslash or a NUL byte");
    }
    if rel.starts_with('/') {
        return bad("is absolute");
    }
    let parts: Vec<&str> = rel.split('/').collect();
    if parts
        .iter()
        .any(|p| p.is_empty() || *p == "." || *p == "..")
    {
        return bad("has an empty, \".\" or \"..\" part");
    }
    if parts[0] == ".rocket" && !(parts.len() == 2 && OWNED_STATE_FILES.contains(&parts[1])) {
        return bad("is inside .rocket and is not a file this feature owns");
    }
    Ok(())
}

fn write_journal(root: &Path, journal: &Journal) -> DomainResult<()> {
    let json = serde_json::to_vec_pretty(journal)
        .map_err(|e| DomainError::Internal(format!("{JOURNAL_FILE} could not be written: {e}")))?;
    atomic_write(&journal_path(root), &json).map_err(|e| io_err(JOURNAL_FILE, e))
}

fn read_journal(root: &Path) -> DomainResult<Option<Journal>> {
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

fn move_file(from: &Path, to: &Path) -> io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::rename(from, to)
}

/// Removes empty folders from `dir` upward, never the collection root itself.
fn prune_empty_parents(root: &Path, mut dir: &Path) {
    while dir != root && dir.starts_with(root) {
        if std::fs::remove_dir(dir).is_err() {
            break;
        }
        match dir.parent() {
            Some(parent) => dir = parent,
            None => break,
        }
    }
}

/// Applies the ops that are not done, in order, stopping after `limit` of them when set.
/// Safe to run again after a crash: it checks what is already in place.
fn apply_ops(
    root: &Path,
    journal: &mut Journal,
    hook: &FaultHook,
    limit: Option<usize>,
) -> DomainResult<usize> {
    let mut applied = 0;
    for idx in 0..journal.ops.len() {
        if journal.ops[idx].done {
            continue;
        }
        if limit.is_some_and(|l| applied >= l) {
            break;
        }
        let id = journal.id.clone();
        let op = journal.ops[idx].clone();
        let target = root.join(&op.path);
        let backup = backup_path(root, &id, &op.path);
        let staged = staged_path(root, &id, &op.path);
        let put_pending = op.kind_is_put() && staged.exists();
        // A put whose staged file is gone was already moved into place before a crash.
        if op.kind_is_put() && !put_pending {
            journal.ops[idx].done = true;
            write_journal(root, journal)?;
            applied += 1;
            continue;
        }
        if !backup.exists() && target.is_file() {
            hook.check().map_err(|e| io_err("backup failed", e))?;
            move_file(&target, &backup).map_err(|e| io_err("backup failed", e))?;
            journal.ops[idx].backup = true;
        }
        if put_pending {
            hook.check().map_err(|e| io_err("replace failed", e))?;
            move_file(&staged, &target).map_err(|e| io_err("replace failed", e))?;
        } else {
            // A delete: the backup move above already took the file away.
            hook.check().map_err(|e| io_err("delete failed", e))?;
        }
        journal.ops[idx].done = true;
        write_journal(root, journal)?;
        applied += 1;
    }
    Ok(applied)
}

impl JournalOp {
    fn kind_is_put(&self) -> bool {
        self.op == OpKind::Put
    }
}

/// Restores every file touched by the journal. Returns how many ops changed something.
fn rollback(root: &Path, journal: &Journal) -> DomainResult<usize> {
    let mut changed = 0;
    for op in journal.ops.iter().rev() {
        let target = root.join(&op.path);
        let backup = backup_path(root, &journal.id, &op.path);
        let staged = staged_path(root, &journal.id, &op.path);
        if backup.exists() {
            if target.exists() {
                std::fs::remove_file(&target).map_err(|e| io_err("rollback failed", e))?;
            }
            move_file(&backup, &target).map_err(|e| io_err("rollback failed", e))?;
            changed += 1;
        } else if op.kind_is_put() && !staged.exists() && target.is_file() {
            // A new file that did not exist before.
            std::fs::remove_file(&target).map_err(|e| io_err("rollback failed", e))?;
            if let Some(parent) = target.parent() {
                prune_empty_parents(root, parent);
            }
            changed += 1;
        }
    }
    Ok(changed)
}

struct FsFileTx {
    root: PathBuf,
    id: String,
    ops: Vec<JournalOp>,
    hook: FaultHook,
}

impl FsFileTx {
    fn forget(&mut self, rel: &str) {
        self.ops.retain(|o| o.path != rel);
        let _ = std::fs::remove_file(staged_path(&self.root, &self.id, rel));
    }

    fn journal(&self, phase: Phase) -> Journal {
        Journal {
            version: JOURNAL_VERSION,
            id: self.id.clone(),
            phase,
            ops: self.ops.clone(),
        }
    }

    /// Commits, applying at most `limit` ops when set (tests use this to simulate a crash).
    fn commit_inner(&mut self, limit: Option<usize>) -> DomainResult<()> {
        let mut journal = self.journal(Phase::Committing);
        write_journal(&self.root, &journal)?;
        match apply_ops(&self.root, &mut journal, &self.hook, limit) {
            Ok(_) => {
                if limit.is_none() {
                    cleanup(&self.root, &self.id)?;
                }
                Ok(())
            }
            Err(e) => match rollback(&self.root, &journal) {
                Ok(_) => {
                    cleanup(&self.root, &self.id)?;
                    Err(e)
                }
                Err(rollback_err) => Err(DomainError::Internal(format!(
                    "{e}; the files could not be restored either: {rollback_err}"
                ))),
            },
        }
    }
}

impl CollectionFileTx for FsFileTx {
    fn stage_put(&mut self, rel_path: &str, bytes: &[u8]) -> DomainResult<()> {
        validate_rel_path(rel_path)?;
        self.forget(rel_path);
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
        validate_rel_path(rel_path)?;
        self.forget(rel_path);
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

impl CollectionFileTxFactory for FsFileTxFactory {
    fn begin(&self, collection_root: &Path) -> DomainResult<Box<dyn CollectionFileTx>> {
        if journal_path(collection_root).exists() {
            return Err(DomainError::Conflict(
                "an earlier sync apply was interrupted; recover it first".to_string(),
            ));
        }
        crate::rocket_dir::ensure_gitignore(collection_root)?;
        // No journal means any staging folder is a leftover.
        let _ = std::fs::remove_dir_all(staging_root(collection_root));
        let id = uuid::Uuid::new_v4().to_string();
        std::fs::create_dir_all(staging_dir(collection_root, &id))
            .map_err(|e| io_err("the staging folder could not be created", e))?;
        let tx = FsFileTx {
            root: collection_root.to_path_buf(),
            id,
            ops: Vec::new(),
            hook: self.hook.clone(),
        };
        write_journal(collection_root, &tx.journal(Phase::Staging))?;
        Ok(Box::new(tx))
    }

    fn recover(&self, collection_root: &Path) -> DomainResult<RecoveryOutcome> {
        let Some(mut journal) = read_journal(collection_root)? else {
            return Ok(RecoveryOutcome::Nothing);
        };
        if journal.phase == Phase::Staging {
            cleanup(collection_root, &journal.id)?;
            return Ok(RecoveryOutcome::Discarded);
        }
        match apply_ops(collection_root, &mut journal, &self.hook, None) {
            Ok(files) => {
                cleanup(collection_root, &journal.id)?;
                Ok(RecoveryOutcome::Finished { files })
            }
            Err(_) => {
                let files = rollback(collection_root, &journal)?;
                cleanup(collection_root, &journal.id)?;
                Ok(RecoveryOutcome::RolledBack { files })
            }
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
        assert!(!dir.path().join(".rocket/staging").exists());
        assert!(!dir.path().join(".rocket/apply-journal.json").exists());
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
        let mut failures = 0;
        for k in 0..30 {
            let dir = tempdir().expect("tempdir");
            seed(dir.path());
            let before = snapshot(dir.path());
            let mut tx = FsFileTxFactory::with_fault(k)
                .begin(dir.path())
                .expect("begin");
            stage_six(tx.as_mut());
            if tx.commit().is_err() {
                failures += 1;
                assert_eq!(snapshot(dir.path()), before, "fault at step {k}");
                assert!(!dir.path().join(".rocket/apply-journal.json").exists());
                assert!(!dir.path().join(".rocket/staging").exists());
            }
        }
        assert!(failures >= 6, "only {failures} steps failed");
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
    fn begin_fails_when_a_journal_exists() {
        let dir = tempdir().expect("tempdir");
        let _open = factory().begin(dir.path()).expect("begin");
        let err = factory().begin(dir.path()).err().expect("conflict");
        assert!(matches!(err, DomainError::Conflict(_)), "{err}");
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
            "a\0b",
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
        let mut tx = FsFileTx {
            root: root.to_path_buf(),
            id: "crash".to_string(),
            ops: Vec::new(),
            hook: FaultHook::default(),
        };
        std::fs::create_dir_all(staging_dir(root, "crash")).expect("dir");
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
        assert!(!dir.path().join(".rocket/apply-journal.json").exists());
        assert!(!dir.path().join(".rocket/staging").exists());
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
        assert!(!dir.path().join(".rocket/apply-journal.json").exists());
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
