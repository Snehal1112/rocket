use std::fs;
use std::path::Path;

use git2::{Repository, Status};
use rocket_shared::error::{DomainError, DomainResult};

use crate::branch::BranchList;
use crate::commit::CommitInfo;
use crate::conflict::{ConflictFile, ConflictResolution};
use crate::credentials::GitCredentials;
use crate::diff::{DiffHunk, DiffLine, FileDiff, LineType};
use crate::service::GitService;
use crate::stash::StashEntry;
use crate::status::{FileStatus, GitStatus, RepoStatus};

/// Git service backed by libgit2.
pub struct Git2Service;

impl Git2Service {
    pub fn new() -> Self {
        Git2Service
    }
}

impl Default for Git2Service {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Open a git repository at the given path.
fn open_repo(path: &str) -> DomainResult<Repository> {
    Repository::open(path).map_err(|e| DomainError::Internal(e.to_string()))
}

/// Map a git2 status bitflag to a (GitStatus, staged) pair.
fn map_git2_status(status: Status) -> (GitStatus, bool) {
    if status.contains(Status::CONFLICTED) {
        return (GitStatus::Conflicted, false);
    }

    // Index (staged) flags take priority when present.
    if status.contains(Status::INDEX_NEW) {
        return (GitStatus::Added, true);
    }
    if status.contains(Status::INDEX_MODIFIED) {
        return (GitStatus::Modified, true);
    }
    if status.contains(Status::INDEX_DELETED) {
        return (GitStatus::Deleted, true);
    }
    if status.contains(Status::INDEX_RENAMED) {
        return (GitStatus::Renamed, true);
    }

    // Work-tree (unstaged) flags.
    if status.contains(Status::WT_NEW) {
        return (GitStatus::Untracked, false);
    }
    if status.contains(Status::WT_MODIFIED) {
        return (GitStatus::Modified, false);
    }
    if status.contains(Status::WT_DELETED) {
        return (GitStatus::Deleted, false);
    }
    if status.contains(Status::WT_RENAMED) {
        return (GitStatus::Renamed, false);
    }

    (GitStatus::Unchanged, false)
}

/// Read the content of a file from the HEAD commit tree.
fn get_head_content(repo: &Repository, file: &str) -> Option<String> {
    let head = repo.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    let tree = commit.tree().ok()?;
    let entry = tree.get_path(Path::new(file)).ok()?;
    let blob = repo.find_blob(entry.id()).ok()?;
    std::str::from_utf8(blob.content()).ok().map(String::from)
}

/// Read the content of a file from the staging index.
fn get_index_content(repo: &Repository, file: &str) -> Option<String> {
    let index = repo.index().ok()?;
    let entry = index.get_path(Path::new(file), 0)?;
    let blob = repo.find_blob(entry.id).ok()?;
    std::str::from_utf8(blob.content()).ok().map(String::from)
}

/// Build a simple line-by-line diff producing hunks.
///
/// This is intentionally simplistic: all old lines are marked as removals and
/// all new lines as additions inside a single hunk. A real diff algorithm is
/// not required at this stage.
fn build_simple_diff(old: &Option<String>, new: &Option<String>) -> Vec<DiffHunk> {
    let old_lines: Vec<&str> = old.as_deref().map(|s| s.lines().collect()).unwrap_or_default();
    let new_lines: Vec<&str> = new.as_deref().map(|s| s.lines().collect()).unwrap_or_default();

    if old_lines == new_lines {
        return Vec::new();
    }

    let mut lines = Vec::new();
    for l in &old_lines {
        lines.push(DiffLine {
            content: l.to_string(),
            line_type: LineType::Remove,
        });
    }
    for l in &new_lines {
        lines.push(DiffLine {
            content: l.to_string(),
            line_type: LineType::Add,
        });
    }

    vec![DiffHunk {
        old_start: 1,
        old_lines: old_lines.len() as u32,
        new_start: 1,
        new_lines: new_lines.len() as u32,
        lines,
    }]
}

/// Extract the current branch name from the repository HEAD.
fn branch_name(repo: &Repository) -> String {
    repo.head()
        .ok()
        .and_then(|r| r.shorthand().map(String::from))
        .unwrap_or_else(|| "main".to_string())
}

/// Compute how many commits the local branch is ahead/behind the upstream.
fn ahead_behind(repo: &Repository) -> (usize, usize) {
    let head = match repo.head() {
        Ok(r) => r,
        Err(_) => return (0, 0),
    };

    let local_oid = match head.target() {
        Some(oid) => oid,
        None => return (0, 0),
    };

    let branch = match repo.find_branch(
        head.shorthand().unwrap_or("main"),
        git2::BranchType::Local,
    ) {
        Ok(b) => b,
        Err(_) => return (0, 0),
    };

    let upstream = match branch.upstream() {
        Ok(u) => u,
        Err(_) => return (0, 0),
    };

    let upstream_oid = match upstream.get().target() {
        Some(oid) => oid,
        None => return (0, 0),
    };

    repo.graph_ahead_behind(local_oid, upstream_oid)
        .unwrap_or((0, 0))
}

// ---------------------------------------------------------------------------
// GitService implementation
// ---------------------------------------------------------------------------

impl GitService for Git2Service {
    fn is_repo(&self, path: &str) -> bool {
        Repository::open(path).is_ok()
    }

    fn init(&self, path: &str) -> DomainResult<()> {
        Repository::init(path).map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn clone_repo(
        &self,
        _url: &str,
        _dest_path: &str,
        _creds: &GitCredentials,
    ) -> DomainResult<()> {
        todo!()
    }

    fn status(&self, path: &str) -> DomainResult<RepoStatus> {
        let repo = open_repo(path)?;
        let branch = branch_name(&repo);
        let (ahead, behind) = ahead_behind(&repo);

        let statuses = repo
            .statuses(None)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut files = Vec::new();
        for entry in statuses.iter() {
            let file_path = entry.path().unwrap_or("").to_string();
            let raw = entry.status();

            // A file can have both staged and unstaged changes. Emit separate
            // FileStatus entries when that is the case.
            let has_index = raw.intersects(
                Status::INDEX_NEW
                    | Status::INDEX_MODIFIED
                    | Status::INDEX_DELETED
                    | Status::INDEX_RENAMED,
            );
            let has_wt = raw.intersects(
                Status::WT_NEW
                    | Status::WT_MODIFIED
                    | Status::WT_DELETED
                    | Status::WT_RENAMED
                    | Status::CONFLICTED,
            );

            if has_index {
                let (gs, _) = map_git2_status(
                    raw & (Status::INDEX_NEW
                        | Status::INDEX_MODIFIED
                        | Status::INDEX_DELETED
                        | Status::INDEX_RENAMED),
                );
                files.push(FileStatus {
                    path: file_path.clone(),
                    status: gs,
                    staged: true,
                });
            }

            if has_wt {
                let (gs, _staged) = map_git2_status(
                    raw & (Status::WT_NEW
                        | Status::WT_MODIFIED
                        | Status::WT_DELETED
                        | Status::WT_RENAMED
                        | Status::CONFLICTED),
                );
                files.push(FileStatus {
                    path: file_path,
                    status: gs,
                    staged: false,
                });
            } else if !has_index && !raw.is_empty() {
                // Fallback: neither index nor wt flags matched.
                let (gs, staged) = map_git2_status(raw);
                files.push(FileStatus {
                    path: file_path,
                    status: gs,
                    staged,
                });
            }
        }

        let is_clean = files.is_empty();

        Ok(RepoStatus {
            branch,
            files,
            ahead,
            behind,
            is_clean,
        })
    }

    fn diff_file(&self, path: &str, file: &str) -> DomainResult<FileDiff> {
        let repo = open_repo(path)?;
        let old_content = get_head_content(&repo, file);
        let file_path = Path::new(path).join(file);
        let new_content = fs::read_to_string(&file_path).ok();
        let hunks = build_simple_diff(&old_content, &new_content);

        Ok(FileDiff {
            path: file.to_string(),
            old_content,
            new_content,
            hunks,
        })
    }

    fn diff_staged(&self, path: &str, file: &str) -> DomainResult<FileDiff> {
        let repo = open_repo(path)?;
        let old_content = get_head_content(&repo, file);
        let new_content = get_index_content(&repo, file);
        let hunks = build_simple_diff(&old_content, &new_content);

        Ok(FileDiff {
            path: file.to_string(),
            old_content,
            new_content,
            hunks,
        })
    }

    fn stage(&self, path: &str, files: &[&str]) -> DomainResult<()> {
        let repo = open_repo(path)?;
        let mut index = repo.index().map_err(|e| DomainError::Internal(e.to_string()))?;
        for file in files {
            index
                .add_path(Path::new(file))
                .map_err(|e| DomainError::Internal(e.to_string()))?;
        }
        index.write().map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn unstage(&self, path: &str, files: &[&str]) -> DomainResult<()> {
        let repo = open_repo(path)?;
        let head = repo
            .head()
            .and_then(|r| r.peel(git2::ObjectType::Commit))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let paths: Vec<&str> = files.to_vec();
        repo.reset_default(Some(&head), paths)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn discard(&self, path: &str, files: &[&str]) -> DomainResult<()> {
        let repo = open_repo(path)?;
        for file in files {
            let mut cb = git2::build::CheckoutBuilder::new();
            cb.path(*file).force();
            repo.checkout_head(Some(&mut cb))
                .map_err(|e| DomainError::Internal(e.to_string()))?;
        }
        Ok(())
    }

    fn commit(&self, path: &str, message: &str) -> DomainResult<CommitInfo> {
        let repo = open_repo(path)?;
        let sig = repo.signature().or_else(|_|
            git2::Signature::now("RocketAPI User", "user@rocketapi.local")
        ).map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut index = repo.index().map_err(|e| DomainError::Internal(e.to_string()))?;
        let tree_id = index.write_tree().map_err(|e| DomainError::Internal(e.to_string()))?;
        let tree = repo.find_tree(tree_id).map_err(|e| DomainError::Internal(e.to_string()))?;

        let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let parents: Vec<&git2::Commit> = parent.iter().collect();

        let oid = repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(CommitInfo {
            id: oid.to_string()[..7].to_string(),
            full_id: oid.to_string(),
            message: message.to_string(),
            author: sig.name().unwrap_or("").to_string(),
            author_email: sig.email().unwrap_or("").to_string(),
            timestamp: chrono::Utc::now(),
            files_changed: 0,
        })
    }

    fn log(&self, path: &str, limit: usize) -> DomainResult<Vec<CommitInfo>> {
        let repo = open_repo(path)?;
        let mut revwalk = repo.revwalk().map_err(|e| DomainError::Internal(e.to_string()))?;
        revwalk.push_head().map_err(|e| DomainError::Internal(e.to_string()))?;
        revwalk.set_sorting(git2::Sort::TIME).map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut commits = Vec::new();
        for oid_result in revwalk.take(limit) {
            let oid = oid_result.map_err(|e| DomainError::Internal(e.to_string()))?;
            let commit = repo.find_commit(oid).map_err(|e| DomainError::Internal(e.to_string()))?;
            let time = commit.time();
            let timestamp = chrono::DateTime::from_timestamp(time.seconds(), 0)
                .unwrap_or_default()
                .with_timezone(&chrono::Utc);

            commits.push(CommitInfo {
                id: oid.to_string()[..7].to_string(),
                full_id: oid.to_string(),
                message: commit.message().unwrap_or("").to_string(),
                author: commit.author().name().unwrap_or("").to_string(),
                author_email: commit.author().email().unwrap_or("").to_string(),
                timestamp,
                files_changed: 0,
            });
        }
        Ok(commits)
    }

    fn push(&self, _path: &str, _remote: &str, _creds: &GitCredentials) -> DomainResult<()> {
        todo!()
    }

    fn pull(&self, _path: &str, _remote: &str, _creds: &GitCredentials) -> DomainResult<()> {
        todo!()
    }

    fn fetch(&self, _path: &str, _remote: &str, _creds: &GitCredentials) -> DomainResult<()> {
        todo!()
    }

    fn branches(&self, _path: &str) -> DomainResult<BranchList> {
        todo!()
    }

    fn switch_branch(&self, _path: &str, _name: &str) -> DomainResult<()> {
        todo!()
    }

    fn create_branch(&self, _path: &str, _name: &str) -> DomainResult<()> {
        todo!()
    }

    fn delete_branch(&self, _path: &str, _name: &str) -> DomainResult<()> {
        todo!()
    }

    fn merge_branch(&self, _path: &str, _name: &str) -> DomainResult<()> {
        todo!()
    }

    fn stash_list(&self, _path: &str) -> DomainResult<Vec<StashEntry>> {
        todo!()
    }

    fn stash_save(&self, _path: &str, _message: &str) -> DomainResult<()> {
        todo!()
    }

    fn stash_pop(&self, _path: &str, _index: usize) -> DomainResult<()> {
        todo!()
    }

    fn stash_apply(&self, _path: &str, _index: usize) -> DomainResult<()> {
        todo!()
    }

    fn stash_drop(&self, _path: &str, _index: usize) -> DomainResult<()> {
        todo!()
    }

    fn conflicts(&self, _path: &str) -> DomainResult<Vec<ConflictFile>> {
        todo!()
    }

    fn resolve_conflict(
        &self,
        _path: &str,
        _file: &str,
        _resolution: &ConflictResolution,
    ) -> DomainResult<()> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::service::GitService;
    use crate::status::GitStatus;
    use std::fs;
    use std::path::Path;
    use tempfile::TempDir;

    fn setup_repo() -> (TempDir, String) {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let repo = git2::Repository::init(&path).unwrap();

        // Ensure the default branch is "main" regardless of system git config.
        repo.set_head("refs/heads/main").ok();

        let sig = git2::Signature::now("Test", "test@test.com").unwrap();
        fs::write(dir.path().join("test.bru"), "meta { name: Test }").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.bru")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("refs/heads/main"), &sig, &sig, "initial", &tree, &[]).unwrap();

        // Point HEAD at the main branch.
        repo.set_head("refs/heads/main").unwrap();

        (dir, path)
    }

    #[test]
    fn is_repo_true() {
        let (_dir, path) = setup_repo();
        assert!(Git2Service::new().is_repo(&path));
    }

    #[test]
    fn is_repo_false() {
        let dir = TempDir::new().unwrap();
        assert!(!Git2Service::new().is_repo(&dir.path().to_string_lossy()));
    }

    #[test]
    fn status_modified_file() {
        let (dir, path) = setup_repo();
        fs::write(dir.path().join("test.bru"), "meta { name: Changed }").unwrap();
        let status = Git2Service::new().status(&path).unwrap();
        assert_eq!(status.branch, "main");
        assert!(
            status
                .files
                .iter()
                .any(|f| f.path == "test.bru" && f.status == GitStatus::Modified)
        );
    }

    #[test]
    fn status_untracked_file() {
        let (dir, path) = setup_repo();
        fs::write(dir.path().join("new.bru"), "new").unwrap();
        let status = Git2Service::new().status(&path).unwrap();
        assert!(
            status
                .files
                .iter()
                .any(|f| f.path == "new.bru" && f.status == GitStatus::Untracked)
        );
    }

    #[test]
    fn diff_file_shows_changes() {
        let (dir, path) = setup_repo();
        fs::write(dir.path().join("test.bru"), "meta { name: Changed }").unwrap();
        let diff = Git2Service::new().diff_file(&path, "test.bru").unwrap();
        assert_eq!(diff.path, "test.bru");
        assert!(diff.old_content.is_some());
        assert!(diff.new_content.is_some());
        assert_ne!(diff.old_content, diff.new_content);
    }

    #[test]
    fn stage_and_unstage_file() {
        let (dir, path) = setup_repo();
        fs::write(dir.path().join("test.bru"), "changed").unwrap();
        let svc = Git2Service::new();
        svc.stage(&path, &["test.bru"]).unwrap();
        let status = svc.status(&path).unwrap();
        assert!(status.files.iter().any(|f| f.path == "test.bru" && f.staged));
        svc.unstage(&path, &["test.bru"]).unwrap();
        let status2 = svc.status(&path).unwrap();
        assert!(status2.files.iter().any(|f| f.path == "test.bru" && !f.staged));
    }

    #[test]
    fn discard_reverts_changes() {
        let (dir, path) = setup_repo();
        fs::write(dir.path().join("test.bru"), "changed").unwrap();
        let svc = Git2Service::new();
        svc.discard(&path, &["test.bru"]).unwrap();
        let content = fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content, "meta { name: Test }"); // original content
    }

    #[test]
    fn commit_and_log() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        fs::write(dir.path().join("new.bru"), "content").unwrap();
        svc.stage(&path, &["new.bru"]).unwrap();
        let info = svc.commit(&path, "add new request").unwrap();
        assert!(!info.id.is_empty());
        assert_eq!(info.message, "add new request");

        let log = svc.log(&path, 10).unwrap();
        assert!(log.len() >= 2);
        assert_eq!(log[0].message, "add new request");
    }

    #[test]
    fn log_respects_limit() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        for i in 0..5 {
            fs::write(dir.path().join(format!("f{}.bru", i)), format!("content {}", i)).unwrap();
            svc.stage(&path, &[&format!("f{}.bru", i)]).unwrap();
            svc.commit(&path, &format!("commit {}", i)).unwrap();
        }
        let log = svc.log(&path, 3).unwrap();
        assert_eq!(log.len(), 3);
    }
}
