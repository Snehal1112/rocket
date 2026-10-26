//! The port for changing many collection files as one unit. The filesystem implementation
//! lives in `rocket-infra`.

use rocket_shared::error::{DomainError, DomainResult};
use std::path::Path;

/// Files under `.rocket` that this feature owns and a transaction may touch.
pub const OWNED_STATE_FILES: [&str; 4] = [
    "openapi-source.yml",
    "openapi-baseline.json",
    "openapi-pending.spec",
    "openapi-local.json",
];

/// Names that Windows treats as devices, with or without an extension.
const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// Checks a path given to a transaction. Code that builds such paths calls this before
/// returning them, so one bad path cannot fail a whole apply.
pub fn validate_rel_path(rel: &str) -> DomainResult<()> {
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
    for part in &parts {
        if part.contains(':') {
            return bad("has a part with a colon");
        }
        let stem = part.split('.').next().unwrap_or("").trim_end();
        if RESERVED_NAMES
            .iter()
            .any(|name| name.eq_ignore_ascii_case(stem))
        {
            return bad("has a part that Windows reserves as a device name");
        }
    }
    if parts[0].eq_ignore_ascii_case(".rocket")
        && !(parts[0] == ".rocket" && parts.len() == 2 && OWNED_STATE_FILES.contains(&parts[1]))
    {
        return bad("is inside .rocket and is not a file this feature owns");
    }
    Ok(())
}

/// One open transaction. Nothing reaches the collection until `commit`.
pub trait CollectionFileTx: Send {
    /// Stages a file to create or replace. `rel_path` is relative to the collection root.
    fn stage_put(&mut self, rel_path: &str, bytes: &[u8]) -> DomainResult<()>;
    /// Stages a file to delete. A file that does not exist is not an error.
    fn stage_delete(&mut self, rel_path: &str) -> DomainResult<()>;
    /// Applies every staged change, or none of them when a step fails.
    fn commit(self: Box<Self>) -> DomainResult<()>;
    /// Drops the staged changes.
    fn abort(self: Box<Self>);
}

/// What `recover` found and did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryOutcome {
    /// There was no unfinished transaction.
    Nothing,
    /// An interrupted commit was finished.
    Finished { files: usize },
    /// An interrupted commit could not be finished and was undone.
    RolledBack { files: usize },
    /// A transaction that was still staging was thrown away.
    Discarded,
}

pub trait CollectionFileTxFactory: Send + Sync {
    /// Opens a transaction. Fails with `Conflict` while an unfinished one exists.
    fn begin(&self, collection_root: &Path) -> DomainResult<Box<dyn CollectionFileTx>>;
    /// Finishes or undoes a transaction that a crash left behind.
    fn recover(&self, collection_root: &Path) -> DomainResult<RecoveryOutcome>;
}
