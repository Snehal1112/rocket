//! The port for changing many collection files as one unit. The filesystem implementation
//! lives in `rocket-infra`.

use rocket_shared::error::DomainResult;
use std::path::Path;

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
