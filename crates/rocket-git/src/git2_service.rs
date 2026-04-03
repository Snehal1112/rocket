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
///
/// The callback includes a one-shot guard: if libgit2 calls it more than once
/// (which happens when credentials are rejected and it retries), we return an
/// error on the second call so the operation fails fast instead of looping.
fn build_callbacks(creds: &GitCredentials) -> git2::RemoteCallbacks<'_> {
    let mut callbacks = git2::RemoteCallbacks::new();
    let creds = creds.clone();
    let mut used = false;
    callbacks.credentials(move |_url, username, _allowed| {
        if used {
            return Err(git2::Error::from_str("authentication failed: check credentials and remote URL"));
        }
        used = true;
        match &creds {
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

    let branch_name = head.shorthand().unwrap_or("main");

    let branch = match repo.find_branch(branch_name, git2::BranchType::Local) {
        Ok(b) => b,
        Err(_) => return (0, 0),
    };

    // Try the configured upstream first.
    let upstream_oid = branch
        .upstream()
        .ok()
        .and_then(|u| u.get().target())
        // Fall back to refs/remotes/<remote>/<branch> for each configured remote.
        .or_else(|| {
            let remotes = repo.remotes().ok()?;
            remotes.iter().flatten().find_map(|remote_name| {
                let refname = format!("refs/remotes/{}/{}", remote_name, branch_name);
                repo.find_reference(&refname).ok().and_then(|r| r.target())
            })
        });

    match upstream_oid {
        Some(oid) => repo.graph_ahead_behind(local_oid, oid).unwrap_or((0, 0)),
        None => (0, 0),
    }
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
        // Prune stale remote-tracking refs so that ahead/behind no longer
        // reflects the old remote's history after the URL changes.
        let prefix = format!("refs/remotes/{}/", name);
        if let Ok(refs) = repo.references() {
            let stale: Vec<String> = refs
                .flatten()
                .filter_map(|r| r.name().map(String::from))
                .filter(|n| n.starts_with(&prefix))
                .collect();
            for refname in stale {
                if let Ok(mut r) = repo.find_reference(&refname) {
                    let _ = r.delete();
                }
            }
        }
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
        let workdir = repo.workdir()
            .ok_or_else(|| DomainError::Internal("No working directory".into()))?;
        for file in files {
            let file_path = workdir.join(file);
            if file_path.exists() {
                // File exists — add its current content to the index.
                index.add_path(Path::new(file))
                    .map_err(|e| DomainError::Internal(e.to_string()))?;
            } else {
                // File was deleted — remove it from the index.
                index.remove_path(Path::new(file))
                    .map_err(|e| DomainError::Internal(e.to_string()))?;
            }
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

        let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
        let merge_commit = repo
            .find_reference("MERGE_HEAD")
            .ok()
            .and_then(|r| r.peel_to_commit().ok());

        let parents: Vec<&git2::Commit> = head_commit.iter()
            .chain(merge_commit.iter())
            .collect();

        let oid = repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Remove merge state files after a successful merge commit.
        if merge_commit.is_some() {
            let _ = repo.cleanup_state();
        }

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
        // Fetch first.
        self.fetch(path, remote, creds)?;

        let repo = open_repo(path)?;
        let current_branch = branch_name(&repo);

        // Resolve the remote-tracking ref for the current branch directly
        // (e.g. refs/remotes/origin/main).  This avoids the FETCH_HEAD
        // pitfall: when the repo fetches multiple branches, FETCH_HEAD's first
        // line may point to a different branch (e.g. feature/database-migration)
        // causing the wrong branch to be merged into main.
        let remote_ref_name = format!("refs/remotes/{remote}/{current_branch}");
        let fetch_commit = repo
            .find_reference(&remote_ref_name)
            .map_err(|e| DomainError::Internal(format!(
                "remote tracking ref '{remote_ref_name}' not found after fetch: {e}"
            )))
            .and_then(|r| {
                repo.reference_to_annotated_commit(&r)
                    .map_err(|e| DomainError::Internal(e.to_string()))
            })?;

        let (analysis, _) = repo
            .merge_analysis(&[&fetch_commit])
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        if analysis.is_up_to_date() {
            return Ok(());
        }

        if analysis.is_fast_forward() {
            let ref_name = format!("refs/heads/{}", branch_name(&repo));
            // In an unborn repo (fresh `git init`, no commits yet) the local
            // branch ref (`refs/heads/main`) does not exist — create it
            // instead of trying to update a non-existent reference.
            match repo.find_reference(&ref_name) {
                Ok(mut reference) => {
                    reference
                        .set_target(fetch_commit.id(), "pull fast-forward")
                        .map_err(|e| DomainError::Internal(e.to_string()))?;
                }
                Err(_) => {
                    repo.reference(
                        &ref_name,
                        fetch_commit.id(),
                        false,
                        "pull: initial checkout into unborn branch",
                    )
                    .map_err(|e| DomainError::Internal(e.to_string()))?;
                }
            }
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
            // Persist the conflicted index so the frontend can enumerate the
            // conflict files.  Return an error so the caller knows it must
            // surface the conflict UI rather than treating the pull as done.
            // The merge-in-progress state (MERGE_HEAD) is intentionally left
            // so the user can resolve conflicts and complete the merge via the
            // conflict-resolution panel.
            index
                .write()
                .map_err(|e| DomainError::Internal(e.to_string()))?;
            let conflicted: Vec<String> = index
                .conflicts()
                .map(|iter| {
                    iter.flatten()
                        .filter_map(|c| {
                            c.our
                                .or(c.their)
                                .or(c.ancestor)
                                .and_then(|e| String::from_utf8(e.path).ok())
                        })
                        .collect()
                })
                .unwrap_or_default();
            let file_list = if conflicted.is_empty() {
                "unknown files".to_string()
            } else {
                conflicted.join(", ")
            };
            return Err(DomainError::Internal(format!(
                "merge conflict: resolve conflicts in {file_list} and commit to complete the pull"
            )));
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

    fn checkout_remote_branch(&self, path: &str, remote_branch: &str) -> DomainResult<()> {
        let repo = open_repo(path)?;

        // remote_branch is e.g. "origin/feature-x".
        let local_name = remote_branch
            .split('/')
            .skip(1)
            .collect::<Vec<_>>()
            .join("/");

        if local_name.is_empty() {
            return Err(DomainError::InvalidInput(format!(
                "Invalid remote branch name: {remote_branch}"
            )));
        }

        // Resolve the remote-tracking ref to a commit.
        let remote_ref = format!("refs/remotes/{remote_branch}");
        let reference = repo
            .find_reference(&remote_ref)
            .map_err(|e| DomainError::Internal(format!("Remote branch not found: {e}")))?;
        let commit = reference
            .peel_to_commit()
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Create a local branch pointing at the same commit.
        repo.branch(&local_name, &commit, false)
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Set upstream tracking.
        let mut local_branch = repo
            .find_branch(&local_name, git2::BranchType::Local)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        local_branch
            .set_upstream(Some(remote_branch))
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Switch HEAD to the new local branch.
        repo.set_head(&format!("refs/heads/{local_name}"))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        repo.checkout_head(Some(&mut git2::build::CheckoutBuilder::new().force()))
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
        assert!(svc.status(&path).is_ok());
    }

    #[test]
    fn init_on_existing_repo_succeeds() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        // Calling init on an already-initialised repo must be idempotent —
        // it must succeed AND leave the repo state intact.
        assert!(svc.init(&path).is_ok());
        let st = svc.status(&path).unwrap();
        assert_eq!(st.branch, "main", "branch must be unchanged after re-init");
        assert!(st.is_clean, "repo must still be clean after re-init");
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

    // -----------------------------------------------------------------------
    // push
    // -----------------------------------------------------------------------

    #[test]
    fn push_to_bare_remote_succeeds() {
        let (_dir, path) = setup_repo();
        let repo = Repository::open(&path).unwrap();

        let remote_dir = TempDir::new().unwrap();
        let remote_path = remote_dir.path().to_string_lossy().to_string();
        Repository::init_bare(&remote_path).unwrap();
        repo.remote("origin", &remote_path).unwrap();

        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass { username: String::new(), password: String::new() };
        svc.push(&path, "origin", &creds).unwrap();

        // Verify the bare remote received main.
        let bare = Repository::open(&remote_path).unwrap();
        assert!(bare.find_reference("refs/heads/main").is_ok());
    }

    #[test]
    fn push_nonexistent_remote_fails() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        let creds = GitCredentials::UserPass { username: String::new(), password: String::new() };
        let result = svc.push(&path, "doesnotexist", &creds);
        assert!(result.is_err());
    }

    // -----------------------------------------------------------------------
    // diff_staged
    // -----------------------------------------------------------------------

    #[test]
    fn diff_staged_shows_staged_changes() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        fs::write(dir.path().join("test.bru"), "meta { name: Staged }").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        let diff = svc.diff_staged(&path, "test.bru").unwrap();
        assert_eq!(diff.path, "test.bru");
        assert!(diff.old_content.is_some(), "old_content should be the HEAD version");
        assert!(diff.new_content.is_some(), "new_content should be the staged version");
        assert_ne!(diff.old_content, diff.new_content);
    }

    #[test]
    fn diff_staged_new_file_has_no_old_content() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        fs::write(dir.path().join("brand_new.bru"), "meta { name: New }").unwrap();
        svc.stage(&path, &["brand_new.bru"]).unwrap();
        let diff = svc.diff_staged(&path, "brand_new.bru").unwrap();
        assert!(diff.old_content.is_none(), "new file must have no old HEAD content");
        assert!(diff.new_content.is_some());
    }

    #[test]
    fn diff_file_new_untracked_has_no_old_content() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        fs::write(dir.path().join("new_file.bru"), "fresh content").unwrap();
        let diff = svc.diff_file(&path, "new_file.bru").unwrap();
        assert!(diff.old_content.is_none(), "untracked file has no HEAD content");
        assert!(diff.new_content.is_some());
    }

    // -----------------------------------------------------------------------
    // status — additional coverage
    // -----------------------------------------------------------------------

    #[test]
    fn status_clean_repo_reports_is_clean() {
        let (_dir, path) = setup_repo();
        let status = Git2Service::new().status(&path).unwrap();
        assert!(status.is_clean, "freshly committed repo must be clean");
        assert!(status.files.is_empty());
    }

    #[test]
    fn status_staged_file_has_staged_true() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        fs::write(dir.path().join("test.bru"), "staged change").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        let status = svc.status(&path).unwrap();
        assert!(
            status.files.iter().any(|f| f.path == "test.bru" && f.staged),
            "staged modification must appear with staged=true"
        );
        assert!(!status.is_clean);
    }

    #[test]
    fn status_deleted_file_appears_as_deleted() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        fs::remove_file(Path::new(&path).join("test.bru")).unwrap();
        let status = svc.status(&path).unwrap();
        assert!(
            status.files.iter().any(|f| f.path == "test.bru" && f.status == GitStatus::Deleted),
            "deleted tracked file must appear as Deleted"
        );
    }

    // -----------------------------------------------------------------------
    // stash_drop
    // -----------------------------------------------------------------------

    #[test]
    fn stash_drop_removes_entry() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();
        fs::write(dir.path().join("test.bru"), "stash me").unwrap();
        svc.stash_save(&path, "to drop").unwrap();
        assert_eq!(svc.stash_list(&path).unwrap().len(), 1);
        svc.stash_drop(&path, 0).unwrap();
        assert!(svc.stash_list(&path).unwrap().is_empty(), "stash must be empty after drop");
    }

    // -----------------------------------------------------------------------
    // conflicts() + resolve_conflict
    // -----------------------------------------------------------------------

    /// Helper: create a repo in merge-conflict state and return (TempDir, path).
    fn setup_conflicted_repo() -> (TempDir, String) {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Branch A: modify test.bru
        svc.create_branch(&path, "branch-a").unwrap();
        svc.switch_branch(&path, "branch-a").unwrap();
        fs::write(dir.path().join("test.bru"), "branch-a content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "branch-a commit").unwrap();

        // Main: independent modification of test.bru
        svc.switch_branch(&path, "main").unwrap();
        fs::write(dir.path().join("test.bru"), "main content").unwrap();
        svc.stage(&path, &["test.bru"]).unwrap();
        svc.commit(&path, "main commit").unwrap();

        // Start merge — leaves MERGE_HEAD + conflict
        let _ = svc.merge_branch(&path, "branch-a");

        assert!(
            dir.path().join(".git/MERGE_HEAD").exists(),
            "setup_conflicted_repo: MERGE_HEAD must exist"
        );
        (dir, path)
    }

    #[test]
    fn conflicts_returns_conflicted_files() {
        let (_dir, path) = setup_conflicted_repo();
        let svc = Git2Service::new();
        let conflicts = svc.conflicts(&path).unwrap();
        assert!(!conflicts.is_empty(), "conflicts() must return at least one file");
        let c = conflicts.iter().find(|f| f.path == "test.bru").unwrap();
        assert!(!c.ours.is_empty(), "ours must be non-empty");
        assert!(!c.theirs.is_empty(), "theirs must be non-empty");
    }

    #[test]
    fn resolve_conflict_ours_writes_our_content() {
        let (dir, path) = setup_conflicted_repo();
        let svc = Git2Service::new();
        svc.resolve_conflict(&path, "test.bru", &ConflictResolution::Ours).unwrap();
        let content = fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content, "main content", "resolving Ours must write main (HEAD) content");
        // Conflict marker must be gone — test.bru must not appear as Conflicted.
        let status = svc.status(&path).unwrap();
        assert!(
            !status.files.iter().any(|f| f.path == "test.bru" && f.status == GitStatus::Conflicted),
            "conflict must be cleared after Ours resolution"
        );
        // Note: when Ours == HEAD, no staged diff shows — that is correct git behaviour.
    }

    #[test]
    fn resolve_conflict_theirs_writes_their_content() {
        let (dir, path) = setup_conflicted_repo();
        let svc = Git2Service::new();
        svc.resolve_conflict(&path, "test.bru", &ConflictResolution::Theirs).unwrap();
        let content = fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content, "branch-a content", "resolving Theirs must write the incoming content");
        let status = svc.status(&path).unwrap();
        assert!(
            status.files.iter().any(|f| f.path == "test.bru" && f.staged),
            "resolved file must be staged"
        );
    }

    #[test]
    fn resolve_conflict_custom_writes_custom_content() {
        let (dir, path) = setup_conflicted_repo();
        let svc = Git2Service::new();
        let custom = "custom resolved content".to_string();
        svc.resolve_conflict(
            &path,
            "test.bru",
            &ConflictResolution::Custom { content: custom.clone() },
        )
        .unwrap();
        let content = fs::read_to_string(dir.path().join("test.bru")).unwrap();
        assert_eq!(content, custom);
    }

    // -----------------------------------------------------------------------
    // merge_branch — non-fast-forward and conflict paths
    // -----------------------------------------------------------------------

    #[test]
    fn merge_branch_normal_merge_creates_merge_commit() {
        let (dir, path) = setup_repo();
        let svc = Git2Service::new();

        // Create a branch with a unique file.
        svc.create_branch(&path, "feature").unwrap();
        svc.switch_branch(&path, "feature").unwrap();
        fs::write(dir.path().join("feature.bru"), "feature").unwrap();
        svc.stage(&path, &["feature.bru"]).unwrap();
        svc.commit(&path, "feature commit").unwrap();

        // Back to main: add a diverging commit so it's a true merge (not FF).
        svc.switch_branch(&path, "main").unwrap();
        fs::write(dir.path().join("main_extra.bru"), "main extra").unwrap();
        svc.stage(&path, &["main_extra.bru"]).unwrap();
        svc.commit(&path, "main diverge commit").unwrap();

        svc.merge_branch(&path, "feature").unwrap();

        // The merge commit must have 2 parents.
        let repo = Repository::open(&path).unwrap();
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        assert_eq!(head_commit.parent_count(), 2, "non-FF merge must produce a 2-parent commit");
        // Both files must exist.
        assert!(dir.path().join("feature.bru").exists());
        assert!(dir.path().join("main_extra.bru").exists());
    }

    #[test]
    fn merge_branch_conflict_returns_error() {
        let (_dir, path) = setup_conflicted_repo();
        // setup_conflicted_repo already attempted merge_branch and left MERGE_HEAD.
        // Abort, then try again cleanly to verify error is returned.
        let svc = Git2Service::new();
        svc.abort_merge(&path).unwrap();

        // Re-introduce both diverging commits.
        // (The repo is already in the right diverged state from setup_conflicted_repo;
        // abort_merge reset to main. Just re-attempt the merge which must error.)
        let result = svc.merge_branch(&path, "branch-a");
        assert!(result.is_err(), "merge with conflict must return an error");
    }

    // -----------------------------------------------------------------------
    // branch error paths
    // -----------------------------------------------------------------------

    #[test]
    fn create_duplicate_branch_fails() {
        let (_dir, path) = setup_repo();
        let svc = Git2Service::new();
        svc.create_branch(&path, "dup").unwrap();
        let result = svc.create_branch(&path, "dup");
        assert!(result.is_err(), "creating duplicate branch must fail");
    }

    #[test]
    fn delete_nonexistent_branch_fails() {
        let (_dir, path) = setup_repo();
        let result = Git2Service::new().delete_branch(&path, "ghost");
        assert!(result.is_err());
    }

    #[test]
    fn switch_to_nonexistent_branch_fails() {
        let (_dir, path) = setup_repo();
        let result = Git2Service::new().switch_branch(&path, "no-such-branch");
        assert!(result.is_err());
    }

    #[test]
    fn checkout_remote_branch_invalid_name_fails() {
        let (_dir, path) = setup_repo();
        let result = Git2Service::new().checkout_remote_branch(&path, "not-a-remote-branch");
        assert!(result.is_err(), "missing slash in name must return InvalidInput error");
    }
}
