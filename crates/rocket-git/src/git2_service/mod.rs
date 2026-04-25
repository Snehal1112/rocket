#[cfg(test)]
use git2::Repository;
use rocket_shared::error::DomainResult;

use crate::branch::BranchList;
use crate::commit::CommitInfo;
use crate::conflict::{ConflictFile, ConflictResolution};
use crate::credentials::GitCredentials;
use crate::diff::FileDiff;
use crate::remote::{FetchResult, RemoteInfo};
use crate::service::GitService;
use crate::stash::StashEntry;
use crate::status::RepoStatus;

mod branch;
mod conflict;
mod helpers;
mod remote;
mod repo;
mod staging;
mod stash;
mod status_diff;

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

impl GitService for Git2Service {
    fn is_repo(&self, path: &str) -> bool {
        repo::is_repo(path)
    }

    fn init(&self, path: &str) -> DomainResult<()> {
        repo::init(path)
    }

    fn clone_repo(
        &self,
        url: &str,
        dest_path: &str,
        creds: &GitCredentials,
    ) -> DomainResult<()> {
        repo::clone_repo(url, dest_path, creds)
    }

    fn list_remotes(&self, path: &str) -> DomainResult<Vec<RemoteInfo>> {
        remote::list_remotes(path)
    }

    fn add_remote(&self, path: &str, name: &str, url: &str) -> DomainResult<()> {
        remote::add_remote(path, name, url)
    }

    fn remove_remote(&self, path: &str, name: &str) -> DomainResult<()> {
        remote::remove_remote(path, name)
    }

    fn set_remote_url(&self, path: &str, name: &str, url: &str) -> DomainResult<()> {
        remote::set_remote_url(path, name, url)
    }

    fn status(&self, path: &str) -> DomainResult<RepoStatus> {
        status_diff::status(path)
    }

    fn diff_file(&self, path: &str, file: &str) -> DomainResult<FileDiff> {
        status_diff::diff_file(path, file)
    }

    fn diff_staged(&self, path: &str, file: &str) -> DomainResult<FileDiff> {
        status_diff::diff_staged(path, file)
    }

    fn diff_commit(&self, path: &str, oid: &str) -> DomainResult<Vec<FileDiff>> {
        staging::diff_commit(path, oid)
    }

    fn stage(&self, path: &str, files: &[&str]) -> DomainResult<()> {
        staging::stage(path, files)
    }

    fn unstage(&self, path: &str, files: &[&str]) -> DomainResult<()> {
        staging::unstage(path, files)
    }

    fn discard(&self, path: &str, files: &[&str]) -> DomainResult<()> {
        staging::discard(path, files)
    }

    fn commit(&self, path: &str, message: &str) -> DomainResult<CommitInfo> {
        staging::commit(path, message)
    }

    fn log(&self, path: &str, limit: usize) -> DomainResult<Vec<CommitInfo>> {
        staging::log(path, limit)
    }

    fn push(&self, path: &str, remote_name: &str, creds: &GitCredentials) -> DomainResult<()> {
        remote::push(path, remote_name, creds)
    }

    fn pull(&self, path: &str, remote_name: &str, creds: &GitCredentials) -> DomainResult<()> {
        remote::pull(path, remote_name, creds)
    }

    fn fetch(&self, path: &str, remote_name: &str, creds: &GitCredentials) -> DomainResult<FetchResult> {
        remote::fetch(path, remote_name, creds)
    }

    fn branches(&self, path: &str) -> DomainResult<BranchList> {
        branch::branches(path)
    }

    fn switch_branch(&self, path: &str, name: &str) -> DomainResult<()> {
        branch::switch_branch(path, name)
    }

    fn checkout_remote_branch(&self, path: &str, remote_branch: &str) -> DomainResult<()> {
        branch::checkout_remote_branch(path, remote_branch)
    }

    fn create_branch(&self, path: &str, name: &str) -> DomainResult<()> {
        branch::create_branch(path, name)
    }

    fn delete_branch(&self, path: &str, name: &str) -> DomainResult<()> {
        branch::delete_branch(path, name)
    }

    fn merge_branch(&self, path: &str, name: &str) -> DomainResult<()> {
        branch::merge_branch(path, name)
    }

    fn stash_list(&self, path: &str) -> DomainResult<Vec<StashEntry>> {
        stash::stash_list(path)
    }

    fn stash_save(&self, path: &str, message: &str) -> DomainResult<()> {
        stash::stash_save(path, message)
    }

    fn stash_pop(&self, path: &str, index: usize) -> DomainResult<()> {
        stash::stash_pop(path, index)
    }

    fn stash_apply(&self, path: &str, index: usize) -> DomainResult<()> {
        stash::stash_apply(path, index)
    }

    fn stash_drop(&self, path: &str, index: usize) -> DomainResult<()> {
        stash::stash_drop(path, index)
    }

    fn conflicts(&self, path: &str) -> DomainResult<Vec<ConflictFile>> {
        conflict::conflicts(path)
    }

    fn resolve_conflict(
        &self,
        path: &str,
        file: &str,
        resolution: &ConflictResolution,
    ) -> DomainResult<()> {
        conflict::resolve_conflict(path, file, resolution)
    }

    fn abort_merge(&self, path: &str) -> DomainResult<()> {
        conflict::abort_merge(path)
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

    fn setup_repo_with_remote() -> (TempDir, String, TempDir, String) {
        // Create a bare "remote" repo.
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        git2::Repository::init_bare(&remote_path).unwrap();

        // Clone it locally — local file-path remotes need no credentials.
        let local_dir = TempDir::new().unwrap();
        let local_path = local_dir.path().to_string_lossy().to_string();
        git2::build::RepoBuilder::new()
            .clone(&remote_path, local_dir.path())
            .expect("clone failed");

        // Make an initial commit so the repo is non-empty.
        let repo = git2::Repository::open(&local_path).unwrap();
        let sig = git2::Signature::now("T", "t@t.com").unwrap();
        std::fs::write(local_dir.path().join("a.bru"), "content").unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(std::path::Path::new("a.bru")).unwrap();
        idx.write().unwrap();
        let tree_id = idx.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        repo.commit(Some("refs/heads/main"), &sig, &sig, "init", &tree, &[]).unwrap();
        repo.set_head("refs/heads/main").unwrap();

        (local_dir, local_path, remote_dir, remote_path)
    }

    #[test]
    fn push_succeeds_for_same_name_branch() {
        let (local_dir, local_path, _remote_dir, remote_path) = setup_repo_with_remote();
        let svc = Git2Service::new();

        // Local file-path remotes don't invoke the credential callback.
        let result = svc.push(&local_path, "origin", &crate::credentials::GitCredentials::SshAgent);
        assert!(result.is_ok(), "push failed: {:?}", result);

        // Verify the ref landed in the bare remote.
        let bare = git2::Repository::open_bare(&remote_path).unwrap();
        assert!(bare.find_reference("refs/heads/main").is_ok());

        drop(local_dir);
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
    fn stage_deleted_file() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        // Delete a tracked file.
        fs::remove_file(Path::new(&path).join("test.bru")).unwrap();
        let status = svc.status(&path).unwrap();
        assert!(status.files.iter().any(|f| f.path == "test.bru" && !f.staged));
        // Stage the deletion.
        svc.stage(&path, &["test.bru"]).unwrap();
        let status2 = svc.status(&path).unwrap();
        assert!(status2.files.iter().any(|f| f.path == "test.bru" && f.staged));
    }

    #[test]
    fn status_untracked_directory_emits_files_not_dir_entry() {
        // Reproduces: "invalid path: 'collections/e4a-rest/'; class=Index (10)"
        // An untracked directory must appear as individual file entries in
        // status, never as a single directory entry with a trailing '/'.
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        let sub = dir.path().join("collections").join("e4a-rest");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("get-users.yml"), "name: get-users").unwrap();
        let status = svc.status(&path).unwrap();
        // No entry should have a trailing '/'.
        for f in &status.files {
            assert!(
                !f.path.ends_with('/'),
                "status returned directory entry: '{}'", f.path
            );
        }
        // The file inside the directory must appear.
        assert!(
            status.files.iter().any(|f| f.path == "collections/e4a-rest/get-users.yml"),
            "expected file entry not found; got: {:?}", status.files.iter().map(|f| &f.path).collect::<Vec<_>>()
        );
    }

    #[test]
    fn stage_untracked_directory_succeeds() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        let sub = dir.path().join("collections").join("e4a-rest");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("get-users.yml"), "name: get-users").unwrap();
        let status = svc.status(&path).unwrap();
        let paths: Vec<&str> = status.files.iter().map(|f| f.path.as_str()).collect();
        svc.stage(&path, &paths).unwrap();
        let status2 = svc.status(&path).unwrap();
        assert!(
            status2.files.iter().any(|f| f.path.contains("get-users.yml") && f.staged)
        );
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

    #[test]
    fn branch_create_switch_delete() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.create_branch(&path, "feature-x").unwrap();
        let branches = svc.branches(&path).unwrap();
        assert!(branches.local.iter().any(|b| b.name == "feature-x"));
        svc.switch_branch(&path, "feature-x").unwrap();
        assert_eq!(svc.status(&path).unwrap().branch, "feature-x");
        svc.switch_branch(&path, "main").unwrap();
        svc.delete_branch(&path, "feature-x").unwrap();
        let branches2 = svc.branches(&path).unwrap();
        assert!(!branches2.local.iter().any(|b| b.name == "feature-x"));
    }

    #[test]
    fn merge_branch_fast_forward() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.create_branch(&path, "feature").unwrap();
        svc.switch_branch(&path, "feature").unwrap();
        fs::write(dir.path().join("new.bru"), "content").unwrap();
        svc.stage(&path, &["new.bru"]).unwrap();
        svc.commit(&path, "feature commit").unwrap();
        svc.switch_branch(&path, "main").unwrap();
        svc.merge_branch(&path, "feature").unwrap();
        let log = svc.log(&path, 5).unwrap();
        assert!(log.iter().any(|c| c.message == "feature commit"));
    }

    #[test]
    fn stash_save_and_pop() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        fs::write(dir.path().join("test.bru"), "changed for stash").unwrap();
        svc.stash_save(&path, "WIP").unwrap();
        let content = fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content, "meta { name: Test }"); // reverted
        let stashes = svc.stash_list(&path).unwrap();
        assert_eq!(stashes.len(), 1);
        assert!(stashes[0].message.contains("WIP"));
        svc.stash_pop(&path, 0).unwrap();
        let content2 = fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content2, "changed for stash"); // restored
    }

    #[test]
    fn stash_save_captures_untracked_files() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Create a brand-new file that has never been staged or committed.
        let new_file = dir.path().join("untracked.bru");
        fs::write(&new_file, "new request content").unwrap();
        assert!(new_file.exists(), "precondition: untracked file should exist before stash");

        // Stash should capture the untracked file.
        svc.stash_save(&path, "capture untracked").unwrap();

        // After stash, the untracked file should be gone from the working tree.
        assert!(
            !new_file.exists(),
            "untracked file should be removed from working tree after stash"
        );

        // Pop restores the untracked file.
        svc.stash_pop(&path, 0).unwrap();
        assert!(
            new_file.exists(),
            "untracked file should be restored after stash pop"
        );
    }

    #[test]
    fn stash_apply_keeps_stash() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        fs::write(dir.path().join("test.bru"), "stash this").unwrap();
        svc.stash_save(&path, "keep me").unwrap();
        svc.stash_apply(&path, 0).unwrap();
        let stashes = svc.stash_list(&path).unwrap();
        assert_eq!(stashes.len(), 1); // still there
        let content = fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content, "stash this"); // restored
    }

    #[test]
    fn list_remotes_empty_for_fresh_repo() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        let remotes = svc.list_remotes(&path).unwrap();
        assert!(remotes.is_empty());
    }

    #[test]
    fn add_and_list_remote() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.add_remote(&path, "origin", "https://github.com/user/repo.git").unwrap();
        let remotes = svc.list_remotes(&path).unwrap();
        assert_eq!(remotes.len(), 1);
        assert_eq!(remotes[0].name, "origin");
        assert_eq!(remotes[0].url, "https://github.com/user/repo.git");
    }

    #[test]
    fn add_multiple_remotes() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.add_remote(&path, "origin", "https://github.com/user/repo.git").unwrap();
        svc.add_remote(&path, "upstream", "https://github.com/upstream/repo.git").unwrap();
        let remotes = svc.list_remotes(&path).unwrap();
        assert_eq!(remotes.len(), 2);
        let names: Vec<&str> = remotes.iter().map(|r| r.name.as_str()).collect();
        assert!(names.contains(&"origin"));
        assert!(names.contains(&"upstream"));
    }

    #[test]
    fn remove_remote() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.add_remote(&path, "origin", "https://github.com/user/repo.git").unwrap();
        svc.remove_remote(&path, "origin").unwrap();
        let remotes = svc.list_remotes(&path).unwrap();
        assert!(remotes.is_empty());
    }

    #[test]
    fn set_remote_url() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.add_remote(&path, "origin", "https://github.com/user/old.git").unwrap();
        svc.set_remote_url(&path, "origin", "https://github.com/user/new.git").unwrap();
        let remotes = svc.list_remotes(&path).unwrap();
        assert_eq!(remotes.len(), 1);
        assert_eq!(remotes[0].url, "https://github.com/user/new.git");
    }

    #[test]
    fn add_duplicate_remote_fails() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.add_remote(&path, "origin", "https://github.com/user/repo.git").unwrap();
        let result = svc.add_remote(&path, "origin", "https://github.com/user/other.git");
        assert!(result.is_err());
    }

    #[test]
    fn remove_nonexistent_remote_fails() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        let result = svc.remove_remote(&path, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn status_ahead_behind_with_remote() {
        let (_dir, path) = setup_repo();
        let repo = Repository::open(&path).unwrap();
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();

        // Create a bare remote to push to.
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        Repository::init_bare(&remote_path).unwrap();

        // Add the bare repo as "origin" and push main.
        let mut remote = repo.remote("origin", &remote_path).unwrap();
        remote
            .push(&["refs/heads/main:refs/heads/main"], None)
            .unwrap();

        // Make one more local commit (ahead by 1).
        fs::write(Path::new(&path).join("extra.txt"), "extra").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("extra.txt")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(
            Some("refs/heads/main"),
            &sig,
            &sig,
            "second",
            &tree,
            &[&head_commit],
        )
        .unwrap();

        // No upstream tracking configured — falls back to refs/remotes/origin/main.
        let svc = Git2Service::new();
        let status = svc.status(&path).unwrap();
        assert_eq!(status.ahead, 1, "should be 1 commit ahead");
        assert_eq!(status.behind, 0, "should be 0 commits behind");
    }

    #[test]
    fn checkout_remote_branch_creates_local_tracking() {
        let (_dir, path) = setup_repo();
        let repo = Repository::open(&path).unwrap();

        // Create a bare remote and push main.
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        Repository::init_bare(&remote_path).unwrap();

        let mut remote = repo.remote("origin", &remote_path).unwrap();
        remote
            .push(&["refs/heads/main:refs/heads/main"], None)
            .unwrap();

        // Create a feature branch on the bare remote by pushing from a clone.
        let clone_dir = TempDir::new().unwrap();
        let clone_path = clone_dir.path().to_string_lossy().to_string();
        let clone_repo = Repository::clone(&remote_path, &clone_path).unwrap();
        let clone_head = clone_repo.head().unwrap().peel_to_commit().unwrap();
        clone_repo.branch("feature-x", &clone_head, false).unwrap();
        clone_repo
            .find_remote("origin")
            .unwrap()
            .push(&["refs/heads/feature-x:refs/heads/feature-x"], None)
            .unwrap();

        // Fetch in our original repo so we get origin/feature-x.
        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass { username: String::new(), password: String::new() };
        svc.fetch(&path, "origin", &creds).unwrap();

        // Checkout the remote branch.
        svc.checkout_remote_branch(&path, "origin/feature-x").unwrap();

        // Verify local branch exists and is checked out.
        let status = svc.status(&path).unwrap();
        assert_eq!(status.branch, "feature-x");

        // Verify upstream is set.
        let branches = svc.branches(&path).unwrap();
        let local = branches.local.iter().find(|b| b.name == "feature-x").unwrap();
        assert_eq!(local.upstream.as_deref(), Some("origin/feature-x"));
    }

    #[test]
    fn commit_creates_merge_commit_when_merge_in_progress() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Create a branch with a change to the same file (will conflict with main).
        svc.create_branch(&path, "conflict-branch").unwrap();
        svc.switch_branch(&path, "conflict-branch").unwrap();
        fs::write(dir.path().join("test.bru"), "branch content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "branch commit").unwrap();

        // Switch back to main and make a conflicting change.
        svc.switch_branch(&path, "main").unwrap();
        fs::write(dir.path().join("test.bru"), "main content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        let main_tip = svc.commit(&path, "main commit").unwrap();

        // Start the merge — this leaves the repo in conflict state (MERGE_HEAD set).
        let _ = svc.merge_branch(&path, "conflict-branch");

        // Verify we are actually in a merge-in-progress state before proceeding.
        assert!(
            dir.path().join(".git/MERGE_HEAD").exists(),
            "MERGE_HEAD must exist to simulate merge-in-progress state"
        );

        // Resolve the conflict by staging a resolved version of the file.
        fs::write(dir.path().join("test.bru"), "resolved content").unwrap();
        let repo = Repository::open(&path).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("test.bru")).unwrap();
        index.write().unwrap();

        // Commit via the service — must produce a two-parent merge commit.
        let info = svc.commit(&path, "merge: resolve conflicts").unwrap();

        // The new commit must have exactly 2 parents.
        let oid = git2::Oid::from_str(&info.full_id).unwrap();
        let verify_repo = Repository::open(&path).unwrap();
        let commit = verify_repo.find_commit(oid).unwrap();
        assert_eq!(commit.parent_count(), 2, "merge commit must have 2 parents");

        // First parent must be the main tip before the merge.
        assert_eq!(
            commit.parent(0).unwrap().id().to_string()[..7].to_string(),
            main_tip.id,
            "first parent must be the main branch tip"
        );

        // MERGE_HEAD must be cleaned up after the commit.
        assert!(
            !dir.path().join(".git/MERGE_HEAD").exists(),
            "MERGE_HEAD must be removed after a successful merge commit"
        );
    }

    #[test]
    fn pull_fast_forward_updates_status() {
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();

        // Create bare remote with an initial commit.
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        let _remote_repo = Repository::init_bare(&remote_path).unwrap();

        // Create local repo, make initial commit, push to remote.
        let local_dir = TempDir::new().unwrap();
        let local_path = local_dir.path().to_string_lossy().to_string();
        let local_repo = Repository::init(&local_path).unwrap();
        local_repo.set_head("refs/heads/main").ok();
        fs::write(local_dir.path().join("a.txt"), "a").unwrap();
        let mut idx = local_repo.index().unwrap();
        idx.add_path(Path::new("a.txt")).unwrap();
        idx.write().unwrap();
        let tid = idx.write_tree().unwrap();
        let tree = local_repo.find_tree(tid).unwrap();
        local_repo.commit(Some("refs/heads/main"), &sig, &sig, "init", &tree, &[]).unwrap();
        drop(tree);
        let mut r = local_repo.remote("origin", &remote_path).unwrap();
        r.push(&["refs/heads/main:refs/heads/main"], None).unwrap();
        drop(r);
        drop(local_repo);

        // Push a new commit from a separate clone → remote is now 1 ahead of local.
        let other_dir = TempDir::new().unwrap();
        let other_repo = Repository::clone(&remote_path, other_dir.path()).unwrap();
        fs::write(other_dir.path().join("b.txt"), "b").unwrap();
        let mut oi = other_repo.index().unwrap();
        oi.add_path(Path::new("b.txt")).unwrap();
        oi.write().unwrap();
        let otid = oi.write_tree().unwrap();
        let ohead = other_repo.head().unwrap().peel_to_commit().unwrap();
        {
            let otree = other_repo.find_tree(otid).unwrap();
            other_repo.commit(Some("refs/heads/main"), &sig, &sig, "remote commit", &otree, &[&ohead]).unwrap();
        }
        other_repo.find_remote("origin").unwrap()
            .push(&["refs/heads/main:refs/heads/main"], None).unwrap();

        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass { username: String::new(), password: String::new() };

        // Fetch so ahead_behind has fresh tracking data.
        svc.fetch(&local_path, "origin", &creds).unwrap();
        let before = svc.status(&local_path).unwrap();
        assert_eq!(before.behind, 1, "should be 1 behind before pull");

        // Pull — should fast-forward local branch.
        svc.pull(&local_path, "origin", &creds).unwrap();

        let after = svc.status(&local_path).unwrap();
        assert_eq!(after.behind, 0, "behind should be 0 after pull");
        assert_eq!(after.ahead, 0, "ahead should be 0 after pull");
    }

    #[test]
    fn pull_fast_forward_without_prior_fetch() {
        // Simulates the user's scenario: behind:N shown, user clicks pull directly
        // without having done an explicit fetch first.
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();

        // Build bare remote + local (in sync), then add remote commit.
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        let _remote_repo = Repository::init_bare(&remote_path).unwrap();

        let local_dir = TempDir::new().unwrap();
        let local_path = local_dir.path().to_string_lossy().to_string();
        let local_repo = Repository::init(&local_path).unwrap();
        local_repo.set_head("refs/heads/main").ok();
        fs::write(local_dir.path().join("a.txt"), "a").unwrap();
        let mut idx = local_repo.index().unwrap();
        idx.add_path(Path::new("a.txt")).unwrap();
        idx.write().unwrap();
        let tid = idx.write_tree().unwrap();
        {
            let t = local_repo.find_tree(tid).unwrap();
            local_repo.commit(Some("refs/heads/main"), &sig, &sig, "init", &t, &[]).unwrap();
        }
        let mut r = local_repo.remote("origin", &remote_path).unwrap();
        r.push(&["refs/heads/main:refs/heads/main"], None).unwrap();
        drop(r);
        drop(local_repo);

        // Push extra commit from another clone (local is now 1 behind).
        let other_dir = TempDir::new().unwrap();
        let other_repo = Repository::clone(&remote_path, other_dir.path()).unwrap();
        fs::write(other_dir.path().join("b.txt"), "b").unwrap();
        let mut oi = other_repo.index().unwrap();
        oi.add_path(Path::new("b.txt")).unwrap();
        oi.write().unwrap();
        let otid = oi.write_tree().unwrap();
        let ohead = other_repo.head().unwrap().peel_to_commit().unwrap();
        {
            let otree = other_repo.find_tree(otid).unwrap();
            other_repo.commit(Some("refs/heads/main"), &sig, &sig, "remote commit", &otree, &[&ohead]).unwrap();
        }
        other_repo.find_remote("origin").unwrap()
            .push(&["refs/heads/main:refs/heads/main"], None).unwrap();

        // NOTE: no explicit svc.fetch call here — simulates user clicking pull directly.
        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass { username: String::new(), password: String::new() };

        // Pull should succeed without a prior explicit fetch.
        let result = svc.pull(&local_path, "origin", &creds);
        assert!(result.is_ok(), "pull should succeed: {:?}", result.err());

        let after = svc.status(&local_path).unwrap();
        assert_eq!(after.behind, 0, "behind should be 0 after pull");
        assert_eq!(after.ahead, 0, "ahead should be 0 after pull");
    }

    /// Regression test for the "Sage Network" bug:
    /// Remote has multiple branches (main + feature/database-migration).
    /// When fetch returns both, FETCH_HEAD's first line is the feature branch.
    /// pull() must merge the CURRENT branch (main) not FETCH_HEAD's first entry.
    #[test]
    fn pull_uses_current_branch_not_fetch_head_first_line() {
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();

        // Build bare remote with TWO branches: main and feature.
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        Repository::init_bare(&remote_path).unwrap();

        let seed_dir = TempDir::new().unwrap();
        let base_oid = {
            let seed_repo = Repository::clone(&remote_path, seed_dir.path()).unwrap();
            seed_repo.set_head("refs/heads/main").ok();

            // Shared base commit.
            fs::write(seed_dir.path().join("base.txt"), "base").unwrap();
            let mut si = seed_repo.index().unwrap();
            si.add_path(Path::new("base.txt")).unwrap();
            si.write().unwrap();
            let base_oid = {
                let tid = si.write_tree().unwrap();
                let t = seed_repo.find_tree(tid).unwrap();
                seed_repo.commit(Some("refs/heads/main"), &sig, &sig, "base", &t, &[]).unwrap()
            };

            // Push main's base to remote.
            seed_repo.find_remote("origin").unwrap()
                .push(&["refs/heads/main:refs/heads/main"], None).unwrap();

            // Create feature branch from base and push it.
            {
                let base_commit = seed_repo.find_commit(base_oid).unwrap();
                seed_repo.branch("feature/database-migration", &base_commit, false).unwrap();
            }
            fs::write(seed_dir.path().join("feature.txt"), "feature work").unwrap();
            let mut fi = seed_repo.index().unwrap();
            fi.add_path(Path::new("feature.txt")).unwrap();
            fi.write().unwrap();
            seed_repo.set_head("refs/heads/feature/database-migration").ok();
            {
                let tid = fi.write_tree().unwrap();
                let t = seed_repo.find_tree(tid).unwrap();
                let base_c = seed_repo.find_commit(base_oid).unwrap();
                seed_repo.commit(
                    Some("refs/heads/feature/database-migration"),
                    &sig, &sig, "feature commit", &t, &[&base_c],
                ).unwrap();
            }
            seed_repo.find_remote("origin").unwrap()
                .push(&["refs/heads/feature/database-migration:refs/heads/feature/database-migration"], None).unwrap();

            base_oid
        }; // seed_repo dropped here

        // Push 2 extra commits onto remote main via a FRESH clone so the index
        // is clean (no feature.txt contamination from the seed_repo's index).
        {
            let other_dir = TempDir::new().unwrap();
            let other_repo = Repository::clone(&remote_path, other_dir.path()).unwrap();
            for i in 1..=2u32 {
                fs::write(other_dir.path().join(format!("remote{i}.txt")), "remote").unwrap();
                let mut ri = other_repo.index().unwrap();
                ri.add_path(Path::new(&format!("remote{i}.txt"))).unwrap();
                ri.write().unwrap();
                let tid = ri.write_tree().unwrap();
                let t = other_repo.find_tree(tid).unwrap();
                let h = other_repo.head().unwrap().peel_to_commit().unwrap();
                other_repo.commit(
                    Some("refs/heads/main"), &sig, &sig,
                    &format!("remote main {i}"), &t, &[&h],
                ).unwrap();
            }
            other_repo.find_remote("origin").unwrap()
                .push(&["refs/heads/main:refs/heads/main"], None).unwrap();
        }

        // Local: clone, reset to base (1 behind main), add local commit (ahead).
        let local_dir = TempDir::new().unwrap();
        let local_path = local_dir.path().to_string_lossy().to_string();
        {
            let local_repo = Repository::clone(&remote_path, &local_path).unwrap();
            local_repo.set_head("refs/heads/main").ok();
            {
                let base_c = local_repo.find_commit(base_oid).unwrap();
                local_repo.reset(base_c.as_object(), git2::ResetType::Hard, None).unwrap();
            }
            fs::write(local_dir.path().join("local.txt"), "local").unwrap();
            let mut li = local_repo.index().unwrap();
            li.add_path(Path::new("local.txt")).unwrap();
            li.write().unwrap();
            {
                let tid = li.write_tree().unwrap();
                let t = local_repo.find_tree(tid).unwrap();
                let h = local_repo.head().unwrap().peel_to_commit().unwrap();
                local_repo.commit(Some("refs/heads/main"), &sig, &sig, "local commit", &t, &[&h]).unwrap();
            }
        } // local_repo dropped here

        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass { username: String::new(), password: String::new() };

        // Fetch — populates FETCH_HEAD with both branches.
        svc.fetch(&local_path, "origin", &creds).unwrap();

        let before = svc.status(&local_path).unwrap();
        println!("before pull: ahead={} behind={}", before.ahead, before.behind);
        assert!(before.behind > 0, "should be behind main before pull");

        // Pull must merge origin/main, NOT origin/feature/database-migration.
        let result = svc.pull(&local_path, "origin", &creds);
        assert!(result.is_ok(), "pull must succeed: {:?}", result.err());

        let after = svc.status(&local_path).unwrap();
        println!("after pull: ahead={} behind={}", after.ahead, after.behind);
        assert_eq!(after.behind, 0, "behind must be 0 — pull must have merged origin/main");

        // remote main files must be present; feature file must NOT be.
        assert!(local_dir.path().join("remote1.txt").exists(), "remote1.txt from origin/main must be present");
        assert!(!local_dir.path().join("feature.txt").exists(), "feature.txt from wrong branch must NOT be present");
    }

    #[test]
    fn pull_with_diverged_history_merges_and_clears_behind() {
        // Exact user scenario: ahead:2, behind:8 → pull → behind:0
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();

        // Create bare remote.
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        let _remote = Repository::init_bare(&remote_path).unwrap();

        // Local: 1 initial commit shared with remote.
        let local_dir = TempDir::new().unwrap();
        let local_path = local_dir.path().to_string_lossy().to_string();
        let local_repo = Repository::init(&local_path).unwrap();
        local_repo.set_head("refs/heads/main").ok();
        fs::write(local_dir.path().join("base.txt"), "base").unwrap();
        let mut idx = local_repo.index().unwrap();
        idx.add_path(Path::new("base.txt")).unwrap();
        idx.write().unwrap();
        {
            let tid = idx.write_tree().unwrap();
            let t = local_repo.find_tree(tid).unwrap();
            local_repo.commit(Some("refs/heads/main"), &sig, &sig, "base", &t, &[]).unwrap();
        }
        // Push base to remote.
        let mut r = local_repo.remote("origin", &remote_path).unwrap();
        r.push(&["refs/heads/main:refs/heads/main"], None).unwrap();
        drop(r);

        // Add 2 LOCAL commits (ahead of remote).
        for i in 1..=2 {
            fs::write(local_dir.path().join(format!("local{i}.txt")), "local").unwrap();
            let mut idx = local_repo.index().unwrap();
            idx.add_path(Path::new(&format!("local{i}.txt"))).unwrap();
            idx.write().unwrap();
            let tid = idx.write_tree().unwrap();
            let t = local_repo.find_tree(tid).unwrap();
            let head = local_repo.head().unwrap().peel_to_commit().unwrap();
            local_repo.commit(Some("refs/heads/main"), &sig, &sig, &format!("local {i}"), &t, &[&head]).unwrap();
        }
        drop(local_repo);

        // Add 8 REMOTE commits via a separate clone (remote is now ahead of base by 8).
        let other_dir = TempDir::new().unwrap();
        let other_repo = Repository::clone(&remote_path, other_dir.path()).unwrap();
        for i in 1..=8 {
            fs::write(other_dir.path().join(format!("remote{i}.txt")), "remote").unwrap();
            let mut oi = other_repo.index().unwrap();
            oi.add_path(Path::new(&format!("remote{i}.txt"))).unwrap();
            oi.write().unwrap();
            let otid = oi.write_tree().unwrap();
            let otree = other_repo.find_tree(otid).unwrap();
            let ohead = other_repo.head().unwrap().peel_to_commit().unwrap();
            other_repo.commit(Some("refs/heads/main"), &sig, &sig, &format!("remote {i}"), &otree, &[&ohead]).unwrap();
        }
        other_repo.find_remote("origin").unwrap()
            .push(&["refs/heads/main:refs/heads/main"], None).unwrap();

        // Fetch to establish refs/remotes/origin/main so status shows ahead:2, behind:8.
        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass { username: String::new(), password: String::new() };
        svc.fetch(&local_path, "origin", &creds).unwrap();

        let before = svc.status(&local_path).unwrap();
        assert_eq!(before.ahead, 2, "should be 2 ahead before pull");
        assert_eq!(before.behind, 8, "should be 8 behind before pull");

        // Pull should do a real merge (diverged history, no fast-forward).
        let result = svc.pull(&local_path, "origin", &creds);
        assert!(result.is_ok(), "pull should succeed: {:?}", result.err());

        let after = svc.status(&local_path).unwrap();
        println!("after pull: ahead={}, behind={}", after.ahead, after.behind);
        assert_eq!(after.behind, 0, "behind must be 0 after pull");
    }

    /// Reproduces the test-42 scenario: local repo has workspace.yml as its
    /// initial commit (no common ancestor with remote), remote also has
    /// workspace.yml. Pull must return a DomainError reporting the conflict,
    /// and the repo must be left in merge-in-progress state so the conflict
    /// resolution UI can show.
    #[test]
    fn pull_with_unrelated_histories_returns_conflict_error() {
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();

        // Bare remote with its own independent workspace.yml.
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        Repository::init_bare(&remote_path).unwrap();

        let seed_dir = TempDir::new().unwrap();
        let seed_repo = Repository::clone(&remote_path, seed_dir.path()).unwrap();
        seed_repo.set_head("refs/heads/main").ok();
        fs::write(seed_dir.path().join("workspace.yml"), "name: remote-workspace\n").unwrap();
        let mut si = seed_repo.index().unwrap();
        si.add_path(Path::new("workspace.yml")).unwrap();
        si.write().unwrap();
        let stid = si.write_tree().unwrap();
        {
            let t = seed_repo.find_tree(stid).unwrap();
            seed_repo
                .commit(Some("refs/heads/main"), &sig, &sig, "remote initial", &t, &[])
                .unwrap();
        }
        seed_repo
            .find_remote("origin")
            .unwrap()
            .push(&["refs/heads/main:refs/heads/main"], None)
            .unwrap();
        drop(seed_repo);

        // Local: independent initial commit with workspace.yml (different content, NO common ancestor).
        let local_dir = TempDir::new().unwrap();
        let local_path = local_dir.path().to_string_lossy().to_string();
        let local_repo = Repository::init(&local_path).unwrap();
        local_repo.set_head("refs/heads/main").ok();
        fs::write(local_dir.path().join("workspace.yml"), "name: local-workspace\n").unwrap();
        let mut li = local_repo.index().unwrap();
        li.add_path(Path::new("workspace.yml")).unwrap();
        li.write().unwrap();
        let ltid = li.write_tree().unwrap();
        {
            let t = local_repo.find_tree(ltid).unwrap();
            local_repo
                .commit(Some("refs/heads/main"), &sig, &sig, "local initial", &t, &[])
                .unwrap();
        }
        drop(local_repo);

        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass { username: String::new(), password: String::new() };

        svc.add_remote(&local_path, "origin", &remote_path).unwrap();
        svc.fetch(&local_path, "origin", &creds).unwrap();

        let before = svc.status(&local_path).unwrap();
        assert!(before.behind > 0, "should be behind before pull");

        // Pull MUST return an error because workspace.yml has a merge conflict
        // (both sides added it independently with no common ancestor).
        let result = svc.pull(&local_path, "origin", &creds);
        assert!(result.is_err(), "pull must return error when there are merge conflicts");
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("merge conflict"),
            "error must mention 'merge conflict'; got: {err_msg}"
        );
        assert!(
            err_msg.contains("workspace.yml"),
            "error must name the conflicted file; got: {err_msg}"
        );

        // Repo must be in merge-in-progress state so conflict resolution UI can work.
        assert!(
            local_dir.path().join(".git/MERGE_HEAD").exists(),
            "MERGE_HEAD must exist after a conflicting pull"
        );
    }

    #[test]
    fn remove_and_readd_remote_leaves_stale_tracking_refs() {
        // Verify that remove_remote + add_remote does NOT clear refs/remotes/<name>/*.
        // This identifies the leaked-refs bug in the remove→re-add workflow.
        let (_dir, path) = setup_repo();
        let repo = Repository::open(&path).unwrap();

        // Manually plant a stale tracking ref (simulates what a prior fetch would do).
        let head_oid = repo.head().unwrap().target().unwrap();
        repo.reference("refs/remotes/origin/main", head_oid, false, "planted").unwrap();
        assert!(repo.find_reference("refs/remotes/origin/main").is_ok());

        let svc = Git2Service::new();
        svc.add_remote(&path, "origin", "https://example.com/repo.git").unwrap();
        svc.remove_remote(&path, "origin").unwrap();
        svc.add_remote(&path, "origin", "https://example.com/new-repo.git").unwrap();

        // The stale ref should be GONE after remove — currently it is NOT (bug).
        let repo2 = Repository::open(&path).unwrap();
        let ref_exists = repo2.find_reference("refs/remotes/origin/main").is_ok();
        println!("stale tracking ref still exists after remove+readd: {ref_exists}");
        // This assertion currently FAILS if remove_remote doesn't prune refs.
        assert!(!ref_exists, "stale refs/remotes/origin/* must be deleted by remove_remote");
    }

    /// End-to-end integration test against the real GitHub remote used in bug
    /// reports. Requires SSH agent with a key authorised for Snehal1112/test-42.
    /// Marked `#[ignore]` so it does not run in CI; run explicitly with:
    ///   cargo test -p rocket-git pull_unborn_real_github -- --ignored
    #[test]
    #[ignore]
    fn pull_unborn_real_github_with_untracked_workspace_yml() {
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();

        let local_dir = TempDir::new().unwrap();
        let local_path = local_dir.path().to_string_lossy().to_string();

        // Replicate exact user scenario:
        // 1. git init
        let local_repo = git2::Repository::init(&local_path).unwrap();
        local_repo.set_head("refs/heads/main").ok();

        // 2. workspace.yml is created by the app (shows as dirty in Git UI)
        fs::write(local_dir.path().join("workspace.yml"), "name: test-workspace\n").unwrap();

        // 3. User stages and commits workspace.yml (it's shown as dirty, they commit it)
        let mut idx = local_repo.index().unwrap();
        idx.add_path(Path::new("workspace.yml")).unwrap();
        idx.write().unwrap();
        let tid = idx.write_tree().unwrap();
        let tree = local_repo.find_tree(tid).unwrap();
        local_repo
            .commit(Some("refs/heads/main"), &sig, &sig, "initial: workspace.yml", &tree, &[])
            .unwrap();
        drop(tree);
        drop(local_repo);

        let svc = Git2Service::new();
        // SSH agent credentials — no passphrase required.
        let creds = GitCredentials::SshAgent;

        // 4. Add remote and fetch.
        svc.add_remote(&local_path, "origin", "git@github.com:Snehal1112/test-42.git")
            .unwrap();
        svc.fetch(&local_path, "origin", &creds).unwrap();

        let before = svc.status(&local_path).unwrap();
        println!("before pull: ahead={} behind={}", before.ahead, before.behind);
        assert!(before.behind > 0, "should be behind before pull; got behind={}", before.behind);

        // Pull will produce a merge conflict because local committed workspace.yml
        // from an unrelated history (no common ancestor with remote).  The
        // expected behavior is a DomainError naming the conflicted file.
        let result = svc.pull(&local_path, "origin", &creds);
        println!("pull result: {:?}", result);
        match result {
            Ok(()) => {
                // Pull succeeded cleanly (remote's workspace.yml happened to be
                // compatible).  Status must show behind=0.
                let after = svc.status(&local_path).unwrap();
                println!("clean pull: ahead={} behind={}", after.ahead, after.behind);
                assert_eq!(after.behind, 0, "behind must be 0 after clean pull");
            }
            Err(ref e) => {
                // Pull produced merge conflicts — the expected case for test-42
                // (unrelated histories, same workspace.yml file).
                let msg = e.to_string();
                assert!(
                    msg.contains("merge conflict"),
                    "error must mention 'merge conflict'; got: {msg}"
                );
                // Repo must be in MERGE_IN_PROGRESS state for conflict resolution UI.
                assert!(
                    local_dir.path().join(".git/MERGE_HEAD").exists(),
                    "MERGE_HEAD must exist after a conflicting pull"
                );
                println!("conflict pull (expected for test-42): {msg}");
            }
        }
        assert!(
            local_dir.path().join("workspace.yml").exists(),
            "workspace.yml must still exist after pull"
        );
    }

    /// Reproduces the exact real-world scenario with test-42:
    /// - Remote has workspace.yml as a committed file
    /// - Local is a fresh `git init` (unborn HEAD) with an untracked workspace.yml
    /// - Pull must succeed even though workspace.yml exists locally as untracked
    #[test]
    fn pull_into_unborn_repo_with_conflicting_untracked_file() {
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();

        // Bare remote.
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        Repository::init_bare(&remote_path).unwrap();

        // Seed clone — commits workspace.yml (same as test-42 remote has it).
        let seed_dir = TempDir::new().unwrap();
        let seed_repo = Repository::clone(&remote_path, seed_dir.path()).unwrap();
        seed_repo.set_head("refs/heads/main").ok();
        fs::write(seed_dir.path().join("workspace.yml"), "name: test-workspace\nversion: 1\n").unwrap();
        fs::write(seed_dir.path().join("request.bru"), "meta { name: Ping }").unwrap();
        let mut idx = seed_repo.index().unwrap();
        idx.add_path(Path::new("workspace.yml")).unwrap();
        idx.add_path(Path::new("request.bru")).unwrap();
        idx.write().unwrap();
        let tid = idx.write_tree().unwrap();
        {
            let tree = seed_repo.find_tree(tid).unwrap();
            seed_repo
                .commit(Some("refs/heads/main"), &sig, &sig, "initial", &tree, &[])
                .unwrap();
        }
        seed_repo
            .find_remote("origin")
            .unwrap()
            .push(&["refs/heads/main:refs/heads/main"], None)
            .unwrap();
        drop(seed_repo);

        // Local: fresh git init — NO commits, HEAD is unborn.
        let local_dir = TempDir::new().unwrap();
        let local_path = local_dir.path().to_string_lossy().to_string();
        let local_repo = Repository::init(&local_path).unwrap();
        local_repo.set_head("refs/heads/main").ok();

        // workspace.yml exists locally as UNTRACKED — this is the rocket app file
        // the user already has before initialising git.
        fs::write(
            local_dir.path().join("workspace.yml"),
            "name: test-workspace\nversion: 1\n",
        )
        .unwrap();
        drop(local_repo);

        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass { username: String::new(), password: String::new() };

        svc.add_remote(&local_path, "origin", &remote_path).unwrap();
        svc.fetch(&local_path, "origin", &creds).unwrap();

        // This is the key assertion: pull must NOT fail with
        // "untracked file would be overwritten" or "reference not found".
        let result = svc.pull(&local_path, "origin", &creds);
        assert!(
            result.is_ok(),
            "pull into unborn repo with untracked workspace.yml must succeed: {:?}",
            result.err()
        );

        let after = svc.status(&local_path).unwrap();
        assert_eq!(after.behind, 0, "behind must be 0 after pull");
        assert_eq!(after.ahead, 0, "ahead must be 0 after pull");
        // The remote workspace.yml should now be the tracked version.
        assert!(
            local_dir.path().join("workspace.yml").exists(),
            "workspace.yml must exist after pull"
        );
        assert!(
            local_dir.path().join("request.bru").exists(),
            "request.bru from remote must be checked out"
        );
    }

    /// Reproduces the exact real-world scenario with test-42:
    /// fresh `git init` (no commits, unborn HEAD) → add remote → fetch → pull
    /// should complete successfully and leave status.behind == 0.
    #[test]
    fn pull_into_unborn_repo_succeeds_and_clears_behind() {
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();

        // Build a bare remote repo with 3 commits (simulates test-42 with N commits).
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        let remote_repo = Repository::init_bare(&remote_path).unwrap();

        // The bare repo needs an initial commit — work via a seed clone.
        let seed_dir = TempDir::new().unwrap();
        let seed_repo = Repository::clone(&remote_path, seed_dir.path()).unwrap();
        seed_repo.set_head("refs/heads/main").ok();
        for i in 1..=3u32 {
            let file = seed_dir.path().join(format!("file{i}.txt"));
            fs::write(&file, format!("content {i}")).unwrap();
            let mut idx = seed_repo.index().unwrap();
            idx.add_path(Path::new(&format!("file{i}.txt"))).unwrap();
            idx.write().unwrap();
            let tid = idx.write_tree().unwrap();
            let tree = seed_repo.find_tree(tid).unwrap();
            let parents: Vec<git2::Commit> = if i == 1 {
                vec![]
            } else {
                vec![seed_repo.head().unwrap().peel_to_commit().unwrap()]
            };
            let parent_refs: Vec<&git2::Commit> = parents.iter().collect();
            seed_repo
                .commit(Some("refs/heads/main"), &sig, &sig, &format!("commit {i}"), &tree, &parent_refs)
                .unwrap();
        }
        seed_repo
            .find_remote("origin")
            .unwrap()
            .push(&["refs/heads/main:refs/heads/main"], None)
            .unwrap();
        drop(seed_repo);
        drop(remote_repo);

        // Fresh local repo: git init only — no commits, HEAD is unborn.
        let local_dir = TempDir::new().unwrap();
        let local_path = local_dir.path().to_string_lossy().to_string();
        let local_repo = Repository::init(&local_path).unwrap();
        local_repo.set_head("refs/heads/main").ok();

        // workspace.yml is present but NOT committed (mimics the user's scenario).
        fs::write(local_dir.path().join("workspace.yml"), "name: test-workspace\n").unwrap();

        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass { username: String::new(), password: String::new() };

        // Add remote and fetch.
        svc.add_remote(&local_path, "origin", &remote_path).unwrap();
        svc.fetch(&local_path, "origin", &creds).unwrap();

        // Status after fetch: unborn HEAD means ahead_behind returns (0,0).
        // Verify the repo is treated as a valid repo.
        let status_before = svc.status(&local_path).unwrap();
        assert_eq!(status_before.branch, "main");
        // workspace.yml should appear as untracked.
        assert!(
            status_before.files.iter().any(|f| f.path == "workspace.yml"),
            "workspace.yml should be listed as untracked"
        );

        // Pull must succeed even though HEAD is unborn (no local commits yet).
        let result = svc.pull(&local_path, "origin", &creds);
        assert!(result.is_ok(), "pull into unborn repo must succeed: {:?}", result.err());

        // After pull, status.behind must be 0 and the remote files must be checked out.
        let status_after = svc.status(&local_path).unwrap();
        assert_eq!(status_after.behind, 0, "behind must be 0 after pull into unborn repo");
        assert_eq!(status_after.ahead, 0, "ahead must be 0 after pull into unborn repo");
        assert!(
            local_dir.path().join("file3.txt").exists(),
            "remote files must be checked out after pull"
        );

        // workspace.yml (which was untracked) must still be present.
        assert!(
            local_dir.path().join("workspace.yml").exists(),
            "untracked workspace.yml must survive the pull"
        );
    }

    #[test]
    fn abort_merge_resets_to_head() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Create a branch with a conflicting change.
        svc.create_branch(&path, "conflict-branch").unwrap();
        svc.switch_branch(&path, "conflict-branch").unwrap();
        fs::write(dir.path().join("test.bru"), "conflict content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "conflict commit").unwrap();

        // Switch back to main and make a different change to the same file.
        svc.switch_branch(&path, "main").unwrap();
        fs::write(dir.path().join("test.bru"), "main content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "main commit").unwrap();

        // Attempt merge — this may leave the repo in a conflicted state.
        let _ = svc.merge_branch(&path, "conflict-branch");

        // Abort the merge.
        svc.abort_merge(&path).unwrap();

        // Verify the repo is clean and on main.
        let status = svc.status(&path).unwrap();
        assert!(status.is_clean, "Repo should be clean after abort");
        assert_eq!(status.branch, "main");
    }

    #[test]
    fn init_creates_git_repo() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().to_string_lossy().to_string();
        let svc = Git2Service::new();
        svc.init(&path).unwrap();
        assert!(svc.is_repo(&path));
        // No commits yet — status() must handle unborn HEAD without panicking.
        assert!(svc.status(&path).is_ok());
    }

    #[test]
    fn init_on_existing_repo_succeeds() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        // Calling init on an already-initialised repo must be idempotent.
        assert!(svc.init(&path).is_ok());
    }

    #[test]
    fn clone_fails_on_invalid_url() {
        let dest_dir = TempDir::new().unwrap();
        let dest_path = dest_dir.path().to_string_lossy().to_string();
        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass {
            username: String::new(),
            password: String::new(),
        };
        let result = svc.clone_repo("not-a-valid-url", &dest_path, &creds);
        assert!(result.is_err(), "clone with invalid url must fail");
    }

    #[test]
    fn clone_fails_on_non_empty_directory() {
        let dest_dir = TempDir::new().unwrap();
        // Put a file in the directory so it's non-empty.
        fs::write(dest_dir.path().join("existing.txt"), "data").unwrap();
        let dest_path = dest_dir.path().to_string_lossy().to_string();
        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass {
            username: String::new(),
            password: String::new(),
        };
        let result = svc.clone_repo("https://example.com/repo.git", &dest_path, &creds);
        assert!(result.is_err(), "clone into non-empty dir must fail");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("not empty"),
            "error should mention non-empty directory, got: {err}"
        );
    }

    #[test]
    fn diff_staged_shows_staged_changes() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        fs::write(dir.path().join("test.bru"), "modified content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        let diff = svc.diff_staged(&path, "test.bru").unwrap();
        assert_eq!(diff.path, "test.bru");
        assert!(diff.old_content.is_some());
        assert!(diff.new_content.is_some());
        assert_ne!(diff.old_content, diff.new_content);
    }

    #[test]
    fn diff_file_clean_returns_empty_hunks() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        // test.bru is tracked and unmodified — diff must be empty.
        let diff = svc.diff_file(&path, "test.bru").unwrap();
        assert!(diff.hunks.is_empty(), "clean file must have no diff hunks");
    }

    #[test]
    fn push_advances_remote_head() {
        let (dir, path) = setup_repo();

        // Set up a bare remote and push the initial commit.
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        Repository::init_bare(&remote_path).unwrap();
        let repo = Repository::open(&path).unwrap();
        let mut origin = repo.remote("origin", &remote_path).unwrap();
        origin.push(&["refs/heads/main:refs/heads/main"], None).unwrap();
        drop(origin);
        drop(repo);

        // Make a new local commit via the service.
        let svc = Git2Service::new();
        fs::write(dir.path().join("new.bru"), "pushed content").unwrap();
        svc.stage(&path, &["new.bru"]).unwrap();
        let commit_info = svc.commit(&path, "new commit").unwrap();

        // Push via the service.
        let creds = GitCredentials::UserPass { username: String::new(), password: String::new() };
        svc.push(&path, "origin", &creds).unwrap();

        // Verify the bare remote HEAD now matches the new local commit.
        let remote_repo = Repository::open(&remote_path).unwrap();
        let remote_head = remote_repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(
            remote_head.id().to_string(),
            commit_info.full_id,
            "remote HEAD must match the pushed commit"
        );
    }

    #[test]
    fn push_fails_with_non_fast_forward() {
        let sig = git2::Signature::now("Test", "test@test.com").unwrap();

        // Shared bare remote.
        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        Repository::init_bare(&remote_path).unwrap();

        // Clone A: push initial commit.
        let dir_a = TempDir::new().unwrap();
        let path_a = dir_a.path().to_string_lossy().to_string();
        let repo_a = Repository::init(&path_a).unwrap();
        repo_a.set_head("refs/heads/main").ok();
        fs::write(dir_a.path().join("base.txt"), "base").unwrap();
        let mut idx = repo_a.index().unwrap();
        idx.add_path(Path::new("base.txt")).unwrap();
        idx.write().unwrap();
        let tid = idx.write_tree().unwrap();
        let tree = repo_a.find_tree(tid).unwrap();
        repo_a.commit(Some("refs/heads/main"), &sig, &sig, "base", &tree, &[]).unwrap();
        drop(tree);
        repo_a.remote("origin", &remote_path).unwrap()
            .push(&["refs/heads/main:refs/heads/main"], None).unwrap();

        // Clone B: starts from the same base.
        let dir_b = TempDir::new().unwrap();
        let path_b = dir_b.path().to_string_lossy().to_string();
        Repository::clone(&remote_path, &path_b).unwrap();

        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass { username: String::new(), password: String::new() };

        // Clone A pushes a second commit — remote is now 1 ahead of B's base.
        let repo_a2 = Repository::open(&path_a).unwrap();
        fs::write(dir_a.path().join("a_extra.txt"), "from A").unwrap();
        let mut idx2 = repo_a2.index().unwrap();
        idx2.add_path(Path::new("a_extra.txt")).unwrap();
        idx2.write().unwrap();
        let tid2 = idx2.write_tree().unwrap();
        let tree2 = repo_a2.find_tree(tid2).unwrap();
        let head2 = repo_a2.head().unwrap().peel_to_commit().unwrap();
        repo_a2.commit(Some("refs/heads/main"), &sig, &sig, "A second", &tree2, &[&head2]).unwrap();
        svc.push(&path_a, "origin", &creds).unwrap();

        // Clone B makes a commit on its stale base and tries to push — must fail.
        fs::write(dir_b.path().join("b_extra.txt"), "from B").unwrap();
        svc.stage(&path_b, &["b_extra.txt"]).unwrap();
        svc.commit(&path_b, "B commit on stale base").unwrap();

        let result = svc.push(&path_b, "origin", &creds);
        assert!(result.is_err(), "non-fast-forward push must return Err");
    }

    #[test]
    fn stash_drop_removes_entry_at_index() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        fs::write(dir.path().join("test.bru"), "stash this").unwrap();
        svc.stash_save(&path, "drop me").unwrap();
        assert_eq!(svc.stash_list(&path).unwrap().len(), 1, "stash must exist before drop");
        svc.stash_drop(&path, 0).unwrap();
        assert!(
            svc.stash_list(&path).unwrap().is_empty(),
            "stash list must be empty after drop"
        );
    }

    #[test]
    fn stash_drop_out_of_range_fails() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        // No stashes — index 99 must error.
        let result = svc.stash_drop(&path, 99);
        assert!(result.is_err(), "stash_drop with out-of-range index must fail");
    }

    #[test]
    fn conflicts_listed_after_merge_conflict() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Branch writes "branch content" to test.bru.
        svc.create_branch(&path, "conflict-branch").unwrap();
        svc.switch_branch(&path, "conflict-branch").unwrap();
        fs::write(dir.path().join("test.bru"), "branch content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "branch commit").unwrap();

        // Main writes "main content" — guaranteed conflict.
        svc.switch_branch(&path, "main").unwrap();
        fs::write(dir.path().join("test.bru"), "main content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "main commit").unwrap();

        // Start merge without aborting — leaves repo in conflict state.
        let _ = svc.merge_branch(&path, "conflict-branch");

        let conflicts = svc.conflicts(&path).unwrap();
        assert!(!conflicts.is_empty(), "conflicts must be non-empty after a conflicting merge");
        assert!(
            conflicts.iter().any(|c| c.path == "test.bru"),
            "test.bru must appear in the conflict list"
        );
    }

    #[test]
    fn resolve_conflict_ours_writes_local_content() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Branch content = "theirs content"; main content = "ours content".
        svc.create_branch(&path, "conflict-branch").unwrap();
        svc.switch_branch(&path, "conflict-branch").unwrap();
        fs::write(dir.path().join("test.bru"), "theirs content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "branch commit").unwrap();

        svc.switch_branch(&path, "main").unwrap();
        fs::write(dir.path().join("test.bru"), "ours content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "main commit").unwrap();

        let _ = svc.merge_branch(&path, "conflict-branch");

        svc.resolve_conflict(&path, "test.bru", &ConflictResolution::Ours).unwrap();

        let content = fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content, "ours content", "Ours resolution must keep main branch content");
    }

    #[test]
    fn resolve_conflict_theirs_writes_remote_content() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Branch content = "theirs content"; main content = "ours content".
        svc.create_branch(&path, "conflict-branch").unwrap();
        svc.switch_branch(&path, "conflict-branch").unwrap();
        fs::write(dir.path().join("test.bru"), "theirs content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "branch commit").unwrap();

        svc.switch_branch(&path, "main").unwrap();
        fs::write(dir.path().join("test.bru"), "ours content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "main commit").unwrap();

        let _ = svc.merge_branch(&path, "conflict-branch");

        svc.resolve_conflict(&path, "test.bru", &ConflictResolution::Theirs).unwrap();

        let content = fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content, "theirs content", "Theirs resolution must keep incoming branch content");
    }

    #[test]
    fn delete_checked_out_branch_fails() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.create_branch(&path, "feature-x").unwrap();
        svc.switch_branch(&path, "feature-x").unwrap();
        // feature-x is now checked out — deleting it must fail.
        let result = svc.delete_branch(&path, "feature-x");
        assert!(result.is_err(), "deleting the currently checked-out branch must fail");
    }

    #[test]
    fn switch_branch_refuses_when_dirty() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Create a second branch to switch to.
        svc.create_branch(&path, "other").unwrap();
        // Switch back to main first (create_branch switches HEAD).
        let repo = git2::Repository::open(&path).unwrap();
        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(&mut git2::build::CheckoutBuilder::new().force())).unwrap();

        // Now dirty the working tree on main.
        std::fs::write(dir.path().join("test.bru"), "dirty content").unwrap();

        // Attempting to switch to 'other' must fail with InvalidInput, not silently discard the change.
        let result = svc.switch_branch(&path, "other");
        assert!(
            matches!(result, Err(rocket_shared::error::DomainError::InvalidInput(_))),
            "expected InvalidInput when dirty, got: {:?}", result
        );

        // The dirty file must still be there — not discarded.
        let content = std::fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content, "dirty content");
    }

    #[test]
    fn switch_branch_refuses_when_staged_changes_exist() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        svc.create_branch(&path, "other").unwrap();
        let repo = git2::Repository::open(&path).unwrap();
        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(&mut git2::build::CheckoutBuilder::new().force())).unwrap();

        // Stage a new file without committing.
        std::fs::write(dir.path().join("staged.bru"), "staged content").unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(std::path::Path::new("staged.bru")).unwrap();
        idx.write().unwrap();

        let result = svc.switch_branch(&path, "other");
        assert!(
            matches!(result, Err(rocket_shared::error::DomainError::InvalidInput(_))),
            "expected InvalidInput for staged changes, got: {:?}", result
        );
    }

    #[test]
    fn merge_branch_with_conflicts_returns_conflict_error_and_writes_index() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Create 'feature' branch and commit a change to test.bru.
        svc.create_branch(&path, "feature").unwrap();
        std::fs::write(dir.path().join("test.bru"), "feature version").unwrap();
        let repo = git2::Repository::open(&path).unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(std::path::Path::new("test.bru")).unwrap();
        idx.write().unwrap();
        let sig = git2::Signature::now("T", "t@t.com").unwrap();
        let tree_id = idx.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "feature commit", &tree, &[&head]).unwrap();

        // Switch back to main and make a conflicting change to the same file.
        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(&mut git2::build::CheckoutBuilder::new().force())).unwrap();
        std::fs::write(dir.path().join("test.bru"), "main version").unwrap();
        let mut idx2 = repo.index().unwrap();
        idx2.add_path(std::path::Path::new("test.bru")).unwrap();
        idx2.write().unwrap();
        let tree_id2 = idx2.write_tree().unwrap();
        let tree2 = repo.find_tree(tree_id2).unwrap();
        let head2 = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "main conflicting commit", &tree2, &[&head2]).unwrap();

        // Now try to merge 'feature' into main — must conflict.
        let result = svc.merge_branch(&path, "feature");
        assert!(
            matches!(result, Err(rocket_shared::error::DomainError::Conflict(_))),
            "expected Conflict error, got: {:?}", result
        );

        // Conflicts must be readable after the call (index was written).
        let conflicts = svc.conflicts(&path).unwrap();
        assert!(!conflicts.is_empty(), "expected at least one conflict file in index");
        assert!(conflicts.iter().any(|c| c.path == "test.bru"));
    }

    #[test]
    fn commit_returns_files_changed_count() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Stage a new file (second commit, so has a parent).
        std::fs::write(dir.path().join("new.bru"), "new request").unwrap();
        let repo = git2::Repository::open(&path).unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(std::path::Path::new("new.bru")).unwrap();
        idx.write().unwrap();

        let info = svc.commit(&path, "add new.bru").unwrap();
        assert_eq!(info.files_changed, 1, "expected 1 file changed, got {}", info.files_changed);
    }

    #[test]
    fn log_returns_files_changed_count() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        let log = svc.log(&path, 10).unwrap();
        // The initial commit in setup_repo() adds test.bru — files_changed should be 1.
        assert!(!log.is_empty());
        assert_eq!(log[0].files_changed, 1, "expected 1 file in initial commit, got {}", log[0].files_changed);
    }

    #[test]
    fn pull_fast_forward_updates_branch() {
        let (local_dir, local_path, _remote_dir, remote_path) = setup_repo_with_remote();
        let svc = Git2Service::new();
        let creds = GitCredentials::SshAgent;

        // First push local main to remote so remote has a commit.
        svc.push(&local_path, "origin", &creds).unwrap();

        // Add a commit directly to the bare remote via a second clone.
        let clone2_dir = TempDir::new().unwrap();
        git2::build::RepoBuilder::new()
            .clone(&remote_path, clone2_dir.path())
            .unwrap();
        let clone2 = git2::Repository::open(clone2_dir.path()).unwrap();
        // Get the tip commit from the remote tracking branch.
        let origin_main = clone2.find_reference("refs/remotes/origin/main").unwrap();
        let tip_commit = origin_main.peel_to_commit().unwrap();
        // Create a local main branch pointing at the tip commit.
        clone2.branch("main", &tip_commit, false).unwrap();
        clone2.set_head("refs/heads/main").unwrap();
        clone2.checkout_head(Some(&mut git2::build::CheckoutBuilder::new().force())).unwrap();
        let sig = git2::Signature::now("T", "t@t.com").unwrap();
        std::fs::write(clone2_dir.path().join("remote_change.bru"), "from remote").unwrap();
        let mut idx = clone2.index().unwrap();
        idx.add_path(std::path::Path::new("remote_change.bru")).unwrap();
        idx.write().unwrap();
        let tree_id = idx.write_tree().unwrap();
        let tree = clone2.find_tree(tree_id).unwrap();
        let head = clone2.head().unwrap().peel_to_commit().unwrap();
        clone2.commit(Some("HEAD"), &sig, &sig, "remote commit", &tree, &[&head]).unwrap();
        clone2.find_remote("origin").unwrap()
            .push(&["refs/heads/main:refs/heads/main"], None).unwrap();

        // Pull into the original local repo — should fast-forward.
        let result = svc.pull(&local_path, "origin", &creds);
        assert!(result.is_ok(), "fast-forward pull failed: {:?}", result);

        // The new file from the remote commit must now exist locally.
        assert!(
            local_dir.path().join("remote_change.bru").exists(),
            "pulled file not present after fast-forward pull"
        );

        drop(local_dir);
        drop(clone2_dir);
    }

    #[test]
    fn push_and_pull_roundtrip() {
        let (local_dir, local_path, _remote_dir, remote_path) = setup_repo_with_remote();
        let svc = Git2Service::new();
        let creds = GitCredentials::SshAgent;

        // Push local main to remote.
        svc.push(&local_path, "origin", &creds).unwrap();

        // Clone the remote into a second local dir.
        let clone2_dir = TempDir::new().unwrap();
        git2::build::RepoBuilder::new()
            .clone(&remote_path, clone2_dir.path())
            .unwrap();

        // Add a commit in clone2 and push it.
        let clone2 = git2::Repository::open(clone2_dir.path()).unwrap();
        // Create a local main branch from the remote tracking ref and check it out.
        let origin_main = clone2.find_reference("refs/remotes/origin/main").unwrap();
        let tip_commit = origin_main.peel_to_commit().unwrap();
        clone2.branch("main", &tip_commit, false).unwrap();
        clone2.set_head("refs/heads/main").unwrap();
        clone2.checkout_head(Some(&mut git2::build::CheckoutBuilder::new().force())).unwrap();
        let sig = git2::Signature::now("T", "t@t.com").unwrap();
        std::fs::write(clone2_dir.path().join("roundtrip.bru"), "roundtrip").unwrap();
        let mut idx = clone2.index().unwrap();
        idx.add_path(std::path::Path::new("roundtrip.bru")).unwrap();
        idx.write().unwrap();
        let tree_id = idx.write_tree().unwrap();
        let tree = clone2.find_tree(tree_id).unwrap();
        let head = clone2.head().unwrap().peel_to_commit().unwrap();
        clone2.commit(Some("HEAD"), &sig, &sig, "roundtrip commit", &tree, &[&head]).unwrap();
        clone2.find_remote("origin").unwrap()
            .push(&["refs/heads/main:refs/heads/main"], None).unwrap();

        // Pull in local1 and verify the file arrived.
        svc.pull(&local_path, "origin", &creds).unwrap();
        assert!(
            local_dir.path().join("roundtrip.bru").exists(),
            "roundtrip file not present after pull"
        );

        drop(local_dir);
        drop(clone2_dir);
    }

    #[test]
    fn resolve_conflict_ours_stages_local_version() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Create conflicting branches.
        svc.create_branch(&path, "feature").unwrap();
        std::fs::write(dir.path().join("test.bru"), "feature version").unwrap();
        let repo = git2::Repository::open(&path).unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(std::path::Path::new("test.bru")).unwrap();
        idx.write().unwrap();
        let sig = git2::Signature::now("T", "t@t.com").unwrap();
        let tree_id = idx.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "feature", &tree, &[&head]).unwrap();

        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(&mut git2::build::CheckoutBuilder::new().force())).unwrap();
        std::fs::write(dir.path().join("test.bru"), "main version").unwrap();
        let mut idx2 = repo.index().unwrap();
        idx2.add_path(std::path::Path::new("test.bru")).unwrap();
        idx2.write().unwrap();
        let tree_id2 = idx2.write_tree().unwrap();
        let tree2 = repo.find_tree(tree_id2).unwrap();
        let head2 = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "main", &tree2, &[&head2]).unwrap();
        svc.merge_branch(&path, "feature").unwrap_err(); // produces conflict

        // Resolve using Ours strategy.
        svc.resolve_conflict(&path, "test.bru", &ConflictResolution::Ours).unwrap();

        // File on disk must contain the local (main) version.
        let content = std::fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content.trim(), "main version", "expected 'main version', got: {content}");

        // File must be staged (no longer in conflict list).
        let conflicts = svc.conflicts(&path).unwrap();
        assert!(
            !conflicts.iter().any(|c| c.path == "test.bru"),
            "test.bru still in conflicts after Ours resolution"
        );
    }

    #[test]
    fn resolve_conflict_theirs_stages_remote_version() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        svc.create_branch(&path, "feature").unwrap();
        std::fs::write(dir.path().join("test.bru"), "feature version").unwrap();
        let repo = git2::Repository::open(&path).unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(std::path::Path::new("test.bru")).unwrap();
        idx.write().unwrap();
        let sig = git2::Signature::now("T", "t@t.com").unwrap();
        let tree_id = idx.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "feature", &tree, &[&head]).unwrap();

        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(&mut git2::build::CheckoutBuilder::new().force())).unwrap();
        std::fs::write(dir.path().join("test.bru"), "main version").unwrap();
        let mut idx2 = repo.index().unwrap();
        idx2.add_path(std::path::Path::new("test.bru")).unwrap();
        idx2.write().unwrap();
        let tree_id2 = idx2.write_tree().unwrap();
        let tree2 = repo.find_tree(tree_id2).unwrap();
        let head2 = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "main", &tree2, &[&head2]).unwrap();
        svc.merge_branch(&path, "feature").unwrap_err();

        svc.resolve_conflict(&path, "test.bru", &ConflictResolution::Theirs).unwrap();

        let content = std::fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content.trim(), "feature version", "expected 'feature version', got: {content}");

        let conflicts = svc.conflicts(&path).unwrap();
        assert!(
            !conflicts.iter().any(|c| c.path == "test.bru"),
            "test.bru still in conflicts after Theirs resolution"
        );
    }

    #[test]
    fn abort_merge_resets_file_to_head_content() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        svc.create_branch(&path, "feature").unwrap();
        std::fs::write(dir.path().join("test.bru"), "feature version").unwrap();
        let repo = git2::Repository::open(&path).unwrap();
        let mut idx = repo.index().unwrap();
        idx.add_path(std::path::Path::new("test.bru")).unwrap();
        idx.write().unwrap();
        let sig = git2::Signature::now("T", "t@t.com").unwrap();
        let tree_id = idx.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let head = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "feature", &tree, &[&head]).unwrap();

        repo.set_head("refs/heads/main").unwrap();
        repo.checkout_head(Some(&mut git2::build::CheckoutBuilder::new().force())).unwrap();
        std::fs::write(dir.path().join("test.bru"), "main version").unwrap();
        let mut idx2 = repo.index().unwrap();
        idx2.add_path(std::path::Path::new("test.bru")).unwrap();
        idx2.write().unwrap();
        let tree_id2 = idx2.write_tree().unwrap();
        let tree2 = repo.find_tree(tree_id2).unwrap();
        let head2 = repo.head().unwrap().peel_to_commit().unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "main", &tree2, &[&head2]).unwrap();
        svc.merge_branch(&path, "feature").unwrap_err();

        svc.abort_merge(&path).unwrap();

        let conflicts = svc.conflicts(&path).unwrap();
        assert!(conflicts.is_empty(), "conflicts should be empty after abort");

        let content = std::fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content.trim(), "main version", "file not restored after abort");
    }
}
