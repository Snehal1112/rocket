use rocket_shared::error::DomainResult;
use crate::{
    status::RepoStatus, diff::FileDiff, branch::BranchList,
    commit::CommitInfo, stash::StashEntry,
    conflict::{ConflictFile, ConflictResolution},
    credentials::GitCredentials,
    remote::RemoteInfo,
};

pub trait GitService: Send + Sync {
    // Repository
    fn is_repo(&self, path: &str) -> bool;
    fn init(&self, path: &str) -> DomainResult<()>;
    fn clone_repo(&self, url: &str, dest_path: &str, creds: &GitCredentials) -> DomainResult<()>;

    // Remotes
    fn list_remotes(&self, path: &str) -> DomainResult<Vec<RemoteInfo>>;
    fn add_remote(&self, path: &str, name: &str, url: &str) -> DomainResult<()>;
    fn remove_remote(&self, path: &str, name: &str) -> DomainResult<()>;
    fn set_remote_url(&self, path: &str, name: &str, url: &str) -> DomainResult<()>;

    // Status + diff
    fn status(&self, path: &str) -> DomainResult<RepoStatus>;
    fn diff_file(&self, path: &str, file: &str) -> DomainResult<FileDiff>;
    fn diff_staged(&self, path: &str, file: &str) -> DomainResult<FileDiff>;

    // Staging
    fn stage(&self, path: &str, files: &[&str]) -> DomainResult<()>;
    fn unstage(&self, path: &str, files: &[&str]) -> DomainResult<()>;
    fn discard(&self, path: &str, files: &[&str]) -> DomainResult<()>;

    // Commit
    fn commit(&self, path: &str, message: &str) -> DomainResult<CommitInfo>;
    fn log(&self, path: &str, limit: usize) -> DomainResult<Vec<CommitInfo>>;

    // Remote
    fn push(&self, path: &str, remote: &str, creds: &GitCredentials) -> DomainResult<()>;
    fn pull(&self, path: &str, remote: &str, creds: &GitCredentials) -> DomainResult<()>;
    fn fetch(&self, path: &str, remote: &str, creds: &GitCredentials) -> DomainResult<()>;

    // Branches
    fn branches(&self, path: &str) -> DomainResult<BranchList>;
    fn switch_branch(&self, path: &str, name: &str) -> DomainResult<()>;
    fn create_branch(&self, path: &str, name: &str) -> DomainResult<()>;
    fn delete_branch(&self, path: &str, name: &str) -> DomainResult<()>;
    fn merge_branch(&self, path: &str, name: &str) -> DomainResult<()>;

    // Stash
    fn stash_list(&self, path: &str) -> DomainResult<Vec<StashEntry>>;
    fn stash_save(&self, path: &str, message: &str) -> DomainResult<()>;
    fn stash_pop(&self, path: &str, index: usize) -> DomainResult<()>;
    fn stash_apply(&self, path: &str, index: usize) -> DomainResult<()>;
    fn stash_drop(&self, path: &str, index: usize) -> DomainResult<()>;

    // Conflicts
    fn conflicts(&self, path: &str) -> DomainResult<Vec<ConflictFile>>;
    fn resolve_conflict(&self, path: &str, file: &str, resolution: &ConflictResolution) -> DomainResult<()>;
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn trait_is_object_safe() { fn _assert(_: Box<dyn GitService>) {} }
}
