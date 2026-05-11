use std::ops::Not;
use std::path::Path;

use git2::build::CheckoutBuilder;
use rocket_shared::error::{DomainError, DomainResult};

use crate::commit::CommitInfo;

use super::helpers::{count_commit_files, open_repo};

#[tracing::instrument(name = "git_stage", skip(files), fields(repo_path = %path, count = files.len()))]
pub(super) fn stage(path: &str, files: &[&str]) -> DomainResult<()> {
    let repo = open_repo(path)?;
    let mut index = repo.index().map_err(|e| DomainError::Internal(e.to_string()))?;
    let workdir = repo.workdir()
        .ok_or_else(|| DomainError::Internal("No working directory".into()))?;
    // Collect directory paths separately — index.add_path() rejects them.
    // Use index.add_all() with the directory as a pathspec instead.
    let mut dir_specs: Vec<&str> = Vec::new();
    for file in files {
        if file.ends_with('/') {
            dir_specs.push(file);
            continue;
        }
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
    if !dir_specs.is_empty() {
        // Stage all files under each directory pathspec.
        index.add_all(dir_specs.iter().copied(), git2::IndexAddOption::DEFAULT, None)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
    }
    index.write().map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}

#[tracing::instrument(name = "git_unstage", skip(files), fields(repo_path = %path, count = files.len()))]
pub(super) fn unstage(path: &str, files: &[&str]) -> DomainResult<()> {
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

#[tracing::instrument(name = "git_discard", skip(files), fields(repo_path = %path, count = files.len()))]
pub(super) fn discard(path: &str, files: &[&str]) -> DomainResult<()> {
    let repo = open_repo(path)?;
    for file in files {
        // Check if the file exists in HEAD (i.e. it is a tracked file with a committed version).
        let in_head = repo
            .head()
            .ok()
            .and_then(|h| h.peel_to_commit().ok())
            .and_then(|c| c.tree().ok())
            .and_then(|tree| tree.get_path(std::path::Path::new(file)).ok())
            .is_some();

        if in_head {
            // Restore the committed version from HEAD.
            let mut cb = CheckoutBuilder::new();
            cb.path(*file).force();
            repo.checkout_head(Some(&mut cb))
                .map_err(|e| DomainError::Internal(e.to_string()))?;
        } else {
            // Untracked or new file — delete it from disk.
            let full_path = std::path::Path::new(path).join(file);
            if full_path.is_dir() {
                std::fs::remove_dir_all(&full_path)
                    .map_err(|e| DomainError::Internal(e.to_string()))?;
            } else if full_path.exists() {
                std::fs::remove_file(&full_path)
                    .map_err(|e| DomainError::Internal(e.to_string()))?;
            }
        }
    }
    Ok(())
}

#[tracing::instrument(name = "git_commit", fields(repo_path = %path, message = %message.get(..50).unwrap_or(message)))]
pub(super) fn commit(path: &str, message: &str) -> DomainResult<CommitInfo> {
    let repo = open_repo(path)?;
    let sig = repo.signature()
        .map_err(|e| DomainError::Internal(e.to_string()))?;

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

    let commit_obj = repo.find_commit(oid)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let files_changed = count_commit_files(&repo, &commit_obj);

    Ok(CommitInfo {
        id: oid.to_string()[..7].to_string(),
        full_id: oid.to_string(),
        message: message.to_string(),
        author: sig.name().unwrap_or("").to_string(),
        author_email: sig.email().unwrap_or("").to_string(),
        timestamp: chrono::Utc::now(),
        files_changed,
    })
}

#[tracing::instrument(name = "git_diff_commit", skip_all, fields(repo_path = %path, oid = %oid))]
pub(super) fn diff_commit(path: &str, oid: &str) -> DomainResult<Vec<crate::diff::FileDiff>> {
    use crate::diff::FileDiff;
    let repo = open_repo(path)?;
    let obj = repo
        .revparse_single(oid)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let commit = obj
        .peel_to_commit()
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    let commit_tree = commit.tree().map_err(|e| DomainError::Internal(e.to_string()))?;
    let parent_tree = commit
        .parent(0)
        .ok()
        .and_then(|p| p.tree().ok());

    let diff = repo
        .diff_tree_to_tree(parent_tree.as_ref(), Some(&commit_tree), None)
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    let mut results: Vec<FileDiff> = Vec::new();

    diff.foreach(
        &mut |delta, _| {
            let file_path = delta
                .new_file()
                .path()
                .or_else(|| delta.old_file().path())
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();

            let old_content = delta
                .old_file()
                .id()
                .is_zero()
                .not()
                .then(|| {
                    repo.find_blob(delta.old_file().id())
                        .ok()
                        .and_then(|b| std::str::from_utf8(b.content()).ok().map(String::from))
                })
                .flatten();

            let new_content = delta
                .new_file()
                .id()
                .is_zero()
                .not()
                .then(|| {
                    repo.find_blob(delta.new_file().id())
                        .ok()
                        .and_then(|b| std::str::from_utf8(b.content()).ok().map(String::from))
                })
                .flatten();

            let hunks = super::helpers::build_simple_diff(&old_content, &new_content);

            results.push(FileDiff {
                path: file_path,
                old_content,
                new_content,
                hunks,
            });
            true
        },
        None,
        None,
        None,
    )
    .map_err(|e| DomainError::Internal(e.to_string()))?;

    Ok(results)
}

#[tracing::instrument(name = "git_log", fields(repo_path = %path, limit = %limit))]
pub(super) fn log(path: &str, limit: usize) -> DomainResult<Vec<CommitInfo>> {
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

        let files_changed = count_commit_files(&repo, &commit);
        commits.push(CommitInfo {
            id: oid.to_string()[..7].to_string(),
            full_id: oid.to_string(),
            message: commit.message().unwrap_or("").to_string(),
            author: commit.author().name().unwrap_or("").to_string(),
            author_email: commit.author().email().unwrap_or("").to_string(),
            timestamp,
            files_changed,
        });
    }
    Ok(commits)
}
