//! The port for changing many collection files as one unit. The filesystem implementation
//! lives in `rocket-infra`.

use crate::RelPath;
use rocket_shared::error::{DomainError, DomainResult};
use std::path::Path;

/// Files under `.rocket` that this feature owns and a transaction may touch.
pub const OWNED_STATE_FILES: [&str; 6] = [
    "openapi-source.yml",
    "openapi-baseline.json",
    "openapi-pending.spec",
    "openapi-local.json",
    "openapi-applied.spec",
    "openapi-reset-backups.json",
];

/// Checks a path given to a transaction. Code that builds such paths calls this before
/// returning them, so one bad path cannot fail a whole apply.
pub fn validate_rel_path(rel: &str) -> DomainResult<()> {
    RelPath::parse(rel).map(|_| ())
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
    /// Like `recover`, but gives up with `None` when a transaction is running on the
    /// collection, instead of waiting for it. The default waits like `recover`.
    fn recover_if_idle(&self, collection_root: &Path) -> DomainResult<Option<RecoveryOutcome>> {
        self.recover(collection_root).map(Some)
    }
    /// Every file of the collection as a path relative to its root, with `/` separators.
    /// The `.rocket` and `.git` folders are left out. The default refuses, so a factory that
    /// cannot list files never lets an apply pick names blindly.
    fn list_files(
        &self,
        _collection_root: &Path,
    ) -> DomainResult<std::collections::HashSet<String>> {
        Err(DomainError::Internal(
            "listing the collection files is not available".into(),
        ))
    }
}

#[cfg(test)]
mod owned_files_tests {
    use super::*;

    #[test]
    fn owned_state_files_lists_the_backups_file() {
        assert_eq!(OWNED_STATE_FILES.len(), 6);
        assert!(OWNED_STATE_FILES.contains(&"openapi-reset-backups.json"));
    }
}
