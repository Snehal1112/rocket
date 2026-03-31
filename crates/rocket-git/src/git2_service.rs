use std::fs;
use std::path::Path;

use git2::{build::CheckoutBuilder, BranchType, Repository, Status};
use rocket_shared::error::{DomainError, DomainResult};

use crate::branch::{Branch, BranchList};
use crate::commit::CommitInfo;
use crate::conflict::{ConflictFile, ConflictResolution};
use crate::credentials::GitCredentials;
use crate::diff::{DiffHunk, DiffLine, FileDiff, LineType};
use crate::service::GitService;
use crate::remote::RemoteInfo;
use crate::stash::StashEntry;
use crate::status::{FileStatus, GitStatus, RepoStatus};

/// Build credential callbacks for remote operations.
fn build_callbacks(creds: &GitCredentials) -> git2::RemoteCallbacks<'_> {
    let mut callbacks = git2::RemoteCallbacks::new();
    let creds = creds.clone();
    callbacks.credentials(move |_url, username, _allowed| match &creds {
        GitCredentials::SshKey {
            private_key_path,
            passphrase,
        } => git2::Cred::ssh_key(
            username.unwrap_or("git"),
            None,
            Path::new(private_key_path),
            passphrase.as_deref(),
        ),
        GitCredentials::SshAgent => {
            git2::Cred::ssh_key_from_agent(username.unwrap_or("git"))
        }
        GitCredentials::UserPass {
            username: u,
            password,
        } => git2::Cred::userpass_plaintext(u, password),
        GitCredentials::Token { token } => {
            git2::Cred::userpass_plaintext("oauth2", token)
        }
    });
    callbacks
}

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
        url: &str,
        dest_path: &str,
        creds: &GitCredentials,
    ) -> DomainResult<()> {
        let callbacks = build_callbacks(creds);
        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);

        git2::build::RepoBuilder::new()
            .fetch_options(fetch_opts)
            .clone(url, Path::new(dest_path))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn list_remotes(&self, path: &str) -> DomainResult<Vec<RemoteInfo>> {
        let repo = open_repo(path)?;
        let remote_names = repo
            .remotes()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let mut remotes = Vec::new();
        for name in remote_names.iter().flatten() {
            let remote = repo
                .find_remote(name)
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            let url = remote.url().unwrap_or("").to_string();
            remotes.push(RemoteInfo {
                name: name.to_string(),
                url,
            });
        }
        Ok(remotes)
    }

    fn add_remote(&self, path: &str, name: &str, url: &str) -> DomainResult<()> {
        let repo = open_repo(path)?;
        repo.remote(name, url)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn remove_remote(&self, path: &str, name: &str) -> DomainResult<()> {
        let repo = open_repo(path)?;
        repo.remote_delete(name)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn set_remote_url(&self, path: &str, name: &str, url: &str) -> DomainResult<()> {
        let repo = open_repo(path)?;
        repo.remote_set_url(name, url)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
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
                    path: file_path.clone(),
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
            let mut cb = CheckoutBuilder::new();
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

    fn push(&self, path: &str, remote: &str, creds: &GitCredentials) -> DomainResult<()> {
        let repo = open_repo(path)?;
        let mut remote_obj = repo
            .find_remote(remote)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let head = repo
            .head()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let branch = head
            .shorthand()
            .unwrap_or("main");
        let refspec = format!("refs/heads/{branch}:refs/heads/{branch}");

        let callbacks = build_callbacks(creds);
        let mut push_opts = git2::PushOptions::new();
        push_opts.remote_callbacks(callbacks);

        remote_obj
            .push(&[&refspec], Some(&mut push_opts))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn pull(&self, path: &str, remote: &str, creds: &GitCredentials) -> DomainResult<()> {
        // Fetch first, then merge FETCH_HEAD.
        self.fetch(path, remote, creds)?;

        let repo = open_repo(path)?;
        let fetch_head = repo
            .find_reference("FETCH_HEAD")
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let fetch_commit = repo
            .reference_to_annotated_commit(&fetch_head)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let (analysis, _) = repo
            .merge_analysis(&[&fetch_commit])
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        if analysis.is_up_to_date() {
            return Ok(());
        }

        if analysis.is_fast_forward() {
            let ref_name = format!("refs/heads/{}", branch_name(&repo));
            let mut reference = repo
                .find_reference(&ref_name)
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            reference
                .set_target(fetch_commit.id(), "pull fast-forward")
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            repo.set_head(&ref_name)
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            repo.checkout_head(Some(&mut CheckoutBuilder::new().force()))
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            return Ok(());
        }

        // Normal merge.
        repo.merge(&[&fetch_commit], None, None)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut index = repo
            .index()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        if index.has_conflicts() {
            return Err(DomainError::Internal(
                "pull resulted in conflicts".to_string(),
            ));
        }

        let tree_id = index
            .write_tree()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let tree = repo
            .find_tree(tree_id)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let sig = repo
            .signature()
            .or_else(|_| git2::Signature::now("RocketAPI User", "user@rocketapi.local"))
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let head_commit = repo
            .head()
            .and_then(|h| h.peel_to_commit())
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let fetch_obj = repo
            .find_commit(fetch_commit.id())
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            "Merge remote changes",
            &tree,
            &[&head_commit, &fetch_obj],
        )
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        repo.cleanup_state()
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    fn fetch(&self, path: &str, remote: &str, creds: &GitCredentials) -> DomainResult<()> {
        let repo = open_repo(path)?;
        let mut remote_obj = repo
            .find_remote(remote)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let callbacks = build_callbacks(creds);
        let mut fetch_opts = git2::FetchOptions::new();
        fetch_opts.remote_callbacks(callbacks);

        remote_obj
            .fetch::<&str>(&[], Some(&mut fetch_opts), None)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn branches(&self, path: &str) -> DomainResult<BranchList> {
        let repo = open_repo(path)?;
        let current = branch_name(&repo);
        let mut local = Vec::new();
        let mut remote = Vec::new();

        let branches = repo
            .branches(None)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        for item in branches {
            let (branch, branch_type) = item.map_err(|e| DomainError::Internal(e.to_string()))?;
            let name = branch
                .name()
                .map_err(|e| DomainError::Internal(e.to_string()))?
                .unwrap_or("")
                .to_string();
            let is_head = branch.is_head();
            let upstream = branch
                .upstream()
                .ok()
                .and_then(|u| u.name().ok().flatten().map(String::from));

            let entry = Branch {
                name: name.clone(),
                is_head,
                is_remote: branch_type == BranchType::Remote,
                upstream,
            };

            match branch_type {
                BranchType::Local => local.push(entry),
                BranchType::Remote => remote.push(entry),
            }
        }

        Ok(BranchList {
            current,
            local,
            remote,
        })
    }

    fn switch_branch(&self, path: &str, name: &str) -> DomainResult<()> {
        let repo = open_repo(path)?;
        repo.set_head(&format!("refs/heads/{name}"))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        repo.checkout_head(Some(&mut CheckoutBuilder::new().force()))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn create_branch(&self, path: &str, name: &str) -> DomainResult<()> {
        let repo = open_repo(path)?;
        let head_commit = repo
            .head()
            .and_then(|h| h.peel_to_commit())
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        repo.branch(name, &head_commit, false)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn delete_branch(&self, path: &str, name: &str) -> DomainResult<()> {
        let repo = open_repo(path)?;
        let mut branch = repo
            .find_branch(name, BranchType::Local)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        branch
            .delete()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn merge_branch(&self, path: &str, name: &str) -> DomainResult<()> {
        let repo = open_repo(path)?;

        // Find the branch commit and create an annotated commit for analysis.
        let branch_ref = repo
            .find_branch(name, BranchType::Local)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let branch_commit = branch_ref
            .get()
            .peel_to_commit()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let annotated = repo
            .find_annotated_commit(branch_commit.id())
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Determine merge strategy.
        let (analysis, _preference) = repo
            .merge_analysis(&[&annotated])
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        if analysis.is_up_to_date() {
            // Nothing to do.
            return Ok(());
        }

        if analysis.is_fast_forward() {
            // Fast-forward: move the current branch ref to the target commit.
            let ref_name = format!("refs/heads/{}", branch_name(&repo));
            let mut reference = repo
                .find_reference(&ref_name)
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            reference
                .set_target(branch_commit.id(), "fast-forward merge")
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            repo.set_head(&ref_name)
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            repo.checkout_head(Some(&mut CheckoutBuilder::new().force()))
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            return Ok(());
        }

        // Normal merge: perform a real merge with a merge commit.
        repo.merge(&[&annotated], None, None)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut index = repo
            .index()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        if index.has_conflicts() {
            return Err(DomainError::Internal(
                "merge resulted in conflicts".to_string(),
            ));
        }

        let tree_id = index
            .write_tree()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let tree = repo
            .find_tree(tree_id)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let sig = repo
            .signature()
            .or_else(|_| git2::Signature::now("RocketAPI User", "user@rocketapi.local"))
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let head_commit = repo
            .head()
            .and_then(|h| h.peel_to_commit())
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let msg = format!("Merge branch '{name}'");
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &msg,
            &tree,
            &[&head_commit, &branch_commit],
        )
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        repo.cleanup_state()
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    fn stash_list(&self, path: &str) -> DomainResult<Vec<StashEntry>> {
        let mut repo = Repository::open(path)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let mut entries = Vec::new();

        repo.stash_foreach(|index, message, _oid| {
            entries.push(StashEntry {
                index,
                message: message.to_string(),
                timestamp: chrono::Utc::now(),
                branch: String::new(),
            });
            true
        })
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(entries)
    }

    fn stash_save(&self, path: &str, message: &str) -> DomainResult<()> {
        let mut repo = Repository::open(path)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let sig = repo
            .signature()
            .or_else(|_| git2::Signature::now("RocketAPI User", "user@rocketapi.local"))
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        repo.stash_save(&sig, message, None)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn stash_pop(&self, path: &str, index: usize) -> DomainResult<()> {
        let mut repo = Repository::open(path)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        repo.stash_pop(index, None)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn stash_apply(&self, path: &str, index: usize) -> DomainResult<()> {
        let mut repo = Repository::open(path)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        repo.stash_apply(index, None)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn stash_drop(&self, path: &str, index: usize) -> DomainResult<()> {
        let mut repo = Repository::open(path)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        repo.stash_drop(index)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }

    fn conflicts(&self, path: &str) -> DomainResult<Vec<ConflictFile>> {
        let repo = open_repo(path)?;
        let index = repo
            .index()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let conflicts = index
            .conflicts()
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        let mut result = Vec::new();
        for entry in conflicts {
            let entry = entry.map_err(|e| DomainError::Internal(e.to_string()))?;

            let file_path = entry
                .our
                .as_ref()
                .or(entry.their.as_ref())
                .and_then(|e| String::from_utf8(e.path.clone()).ok())
                .unwrap_or_default();

            let ours = entry
                .our
                .as_ref()
                .and_then(|e| repo.find_blob(e.id).ok())
                .and_then(|b| std::str::from_utf8(b.content()).ok().map(String::from))
                .unwrap_or_default();

            let theirs = entry
                .their
                .as_ref()
                .and_then(|e| repo.find_blob(e.id).ok())
                .and_then(|b| std::str::from_utf8(b.content()).ok().map(String::from))
                .unwrap_or_default();

            let ancestor = entry
                .ancestor
                .as_ref()
                .and_then(|e| repo.find_blob(e.id).ok())
                .and_then(|b| std::str::from_utf8(b.content()).ok().map(String::from));

            result.push(ConflictFile {
                path: file_path,
                ours,
                theirs,
                ancestor,
            });
        }

        Ok(result)
    }

    fn resolve_conflict(
        &self,
        path: &str,
        file: &str,
        resolution: &ConflictResolution,
    ) -> DomainResult<()> {
        let repo = open_repo(path)?;

        // Determine the content to write based on the resolution strategy.
        let content = match resolution {
            ConflictResolution::Ours => {
                let index = repo
                    .index()
                    .map_err(|e| DomainError::Internal(e.to_string()))?;
                let conflicts = index
                    .conflicts()
                    .map_err(|e| DomainError::Internal(e.to_string()))?;
                let mut ours_content = String::new();
                for entry in conflicts {
                    let entry =
                        entry.map_err(|e| DomainError::Internal(e.to_string()))?;
                    let entry_path = entry
                        .our
                        .as_ref()
                        .and_then(|e| String::from_utf8(e.path.clone()).ok())
                        .unwrap_or_default();
                    if entry_path == file {
                        ours_content = entry
                            .our
                            .as_ref()
                            .and_then(|e| repo.find_blob(e.id).ok())
                            .and_then(|b| {
                                std::str::from_utf8(b.content())
                                    .ok()
                                    .map(String::from)
                            })
                            .unwrap_or_default();
                        break;
                    }
                }
                ours_content
            }
            ConflictResolution::Theirs => {
                let index = repo
                    .index()
                    .map_err(|e| DomainError::Internal(e.to_string()))?;
                let conflicts = index
                    .conflicts()
                    .map_err(|e| DomainError::Internal(e.to_string()))?;
                let mut theirs_content = String::new();
                for entry in conflicts {
                    let entry =
                        entry.map_err(|e| DomainError::Internal(e.to_string()))?;
                    let entry_path = entry
                        .our
                        .as_ref()
                        .or(entry.their.as_ref())
                        .and_then(|e| String::from_utf8(e.path.clone()).ok())
                        .unwrap_or_default();
                    if entry_path == file {
                        theirs_content = entry
                            .their
                            .as_ref()
                            .and_then(|e| repo.find_blob(e.id).ok())
                            .and_then(|b| {
                                std::str::from_utf8(b.content())
                                    .ok()
                                    .map(String::from)
                            })
                            .unwrap_or_default();
                        break;
                    }
                }
                theirs_content
            }
            ConflictResolution::Custom { content } => content.clone(),
        };

        // Write the resolved content to the working directory.
        let file_path = Path::new(path).join(file);
        fs::write(&file_path, &content)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Stage the resolved file. add_path also clears the conflict marker.
        let mut index = repo
            .index()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        index
            .add_path(Path::new(file))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        index
            .write()
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
    }

    fn abort_merge(&self, path: &str) -> DomainResult<()> {
        let repo = open_repo(path)?;

        // Get HEAD commit to reset to.
        let head = repo.head()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let head_commit = head.peel_to_commit()
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Hard reset index and working directory to HEAD.
        repo.reset(
            head_commit.as_object(),
            git2::ResetType::Hard,
            None,
        )
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Clean up merge/revert/cherry-pick state files.
        repo.cleanup_state()
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(())
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
}
