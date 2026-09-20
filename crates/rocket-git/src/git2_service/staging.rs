use std::ops::Not;
use std::path::{Path, PathBuf};

use git2::build::CheckoutBuilder;
use git2::{Index, IndexEntry, Repository};
use rocket_shared::error::{DomainError, DomainResult};

use crate::commit::CommitInfo;

use super::helpers::{
    count_commit_files, inspect_worktree_path, open_repo, validate_batch_paths, GitRelativePath,
    InspectedWorktreePath, WorktreeLeafKind,
};

#[tracing::instrument(name = "git_stage", skip(files), fields(repo_path = %path, count = files.len()))]
pub(super) fn stage(path: &str, files: &[&str]) -> DomainResult<()> {
    let repo = open_repo(path)?;
    let inspected = inspect_batch(&repo, files)?;
    if let Some(path) = inspected.iter().find(|path| {
        matches!(
            path.leaf_kind(),
            WorktreeLeafKind::Directory | WorktreeLeafKind::Other
        )
    }) {
        return Err(DomainError::InvalidInput(format!(
            "cannot stage non-file filesystem entry {:?}",
            path.relative().as_str()
        )));
    }

    let mut index = repo
        .index()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    for path in &inspected {
        match path.leaf_kind() {
            WorktreeLeafKind::File | WorktreeLeafKind::Symlink => index
                .add_path(path.relative().as_path())
                .map_err(|e| DomainError::Internal(e.to_string()))?,
            WorktreeLeafKind::Missing => index
                .remove_path(path.relative().as_path())
                .map_err(|e| DomainError::Internal(e.to_string()))?,
            WorktreeLeafKind::Directory | WorktreeLeafKind::Other => {
                unreachable!("non-file entries are rejected above")
            }
        }
    }
    index
        .write()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}

#[tracing::instrument(name = "git_unstage", skip(files), fields(repo_path = %path, count = files.len()))]
pub(super) fn unstage(path: &str, files: &[&str]) -> DomainResult<()> {
    let paths = parse_batch(files)?;
    let repo = open_repo(path)?;
    let head = repo
        .head()
        .and_then(|reference| reference.peel_to_commit())
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let tree = head
        .tree()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let mut head_index = Index::new().map_err(|e| DomainError::Internal(e.to_string()))?;
    head_index
        .read_tree(&tree)
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    let mut index = repo
        .index()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    for path in &paths {
        match head_index.get_path(path.as_path(), 0) {
            Some(entry) => index
                .add(&entry)
                .map_err(|e| DomainError::Internal(e.to_string()))?,
            None => index
                .remove_path(path.as_path())
                .map_err(|e| DomainError::Internal(e.to_string()))?,
        }
    }
    index
        .write()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}

#[tracing::instrument(name = "git_discard", skip(files), fields(repo_path = %path, count = files.len()))]
pub(super) fn discard(path: &str, files: &[&str]) -> DomainResult<()> {
    let repo = open_repo(path)?;
    let inspected = inspect_batch(&repo, files)?;
    // Discard reverts unstaged (working-tree) changes only, so the restore
    // source is the current INDEX, not HEAD — a file with staged changes
    // must keep those staged changes, not be reverted all the way to HEAD.
    // Reading the live index (rather than HEAD's tree) also makes this work
    // correctly on an unborn repo, where there is no HEAD to read from yet.
    let index = repo
        .index()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let preflight = inspected
        .into_iter()
        .map(|path| {
            if path.leaf_kind() == WorktreeLeafKind::Directory {
                return Err(DomainError::InvalidInput(format!(
                    "discard requires individual file paths, not directory {:?}",
                    path.relative().as_str()
                )));
            }
            let indexed_entry = index.get_path(path.relative().as_path(), 0);
            let destination_exists = indexed_entry
                .as_ref()
                .map(|_| validate_replaceable_destination(path.full_path()))
                .transpose()?;
            Ok((path, indexed_entry, destination_exists))
        })
        .collect::<DomainResult<Vec<_>>>()?;

    for (path, indexed_entry, destination_exists) in preflight {
        if let Some(indexed_entry) = indexed_entry {
            checkout_exact_index_entry(
                &repo,
                &indexed_entry,
                &path,
                destination_exists.unwrap_or(false),
            )?;
        } else {
            match path.leaf_kind() {
                WorktreeLeafKind::Missing => {}
                WorktreeLeafKind::Directory => {
                    unreachable!("directory targets are rejected during preflight")
                }
                WorktreeLeafKind::File | WorktreeLeafKind::Symlink | WorktreeLeafKind::Other => {
                    std::fs::remove_file(path.full_path())
                        .map_err(|e| DomainError::Io(e.to_string()))?
                }
            }
        }
    }
    Ok(())
}

fn parse_batch(files: &[&str]) -> DomainResult<Vec<GitRelativePath>> {
    let paths = files
        .iter()
        .map(|file| GitRelativePath::parse(file))
        .collect::<DomainResult<Vec<_>>>()?;
    validate_batch_paths(&paths)?;
    Ok(paths)
}

fn inspect_batch(repo: &Repository, files: &[&str]) -> DomainResult<Vec<InspectedWorktreePath>> {
    parse_batch(files)?
        .into_iter()
        .map(|path| inspect_worktree_path(repo, path))
        .collect()
}

fn checkout_exact_index_entry(
    repo: &Repository,
    entry: &IndexEntry,
    destination: &InspectedWorktreePath,
    destination_exists: bool,
) -> DomainResult<()> {
    let mut one_entry_index = Index::new().map_err(|e| DomainError::Internal(e.to_string()))?;
    one_entry_index
        .add(entry)
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    let destination_parent = destination.full_path().parent().ok_or_else(|| {
        DomainError::InvalidInput("tracked path has no destination parent".into())
    })?;
    std::fs::create_dir_all(destination_parent).map_err(|e| DomainError::Io(e.to_string()))?;

    // A one-entry index checked out directly can remove worktree files absent
    // from that index. Materialize on the destination filesystem, then install
    // only the exact validated entry.
    let checkout_root = create_unique_directory(destination_parent, ".rocket-checkout")?;
    let mut checkout = CheckoutBuilder::new();
    checkout.force().target_dir(&checkout_root);
    if let Err(error) = repo.checkout_index(Some(&mut one_entry_index), Some(&mut checkout)) {
        cleanup_directory_best_effort(&checkout_root);
        return Err(DomainError::Internal(error.to_string()));
    }

    let source = checkout_root.join(destination.relative().as_path());
    if let Err(error) = std::fs::symlink_metadata(&source) {
        cleanup_directory_best_effort(&checkout_root);
        return Err(DomainError::Io(error.to_string()));
    }

    let backup = if destination_exists {
        let backup_root =
            match create_unique_directory(destination_parent, ".rocket-discard-backup") {
                Ok(path) => path,
                Err(error) => {
                    cleanup_directory_best_effort(&checkout_root);
                    return Err(error);
                }
            };
        let backup_path = backup_root.join("original");
        if let Err(error) = std::fs::rename(destination.full_path(), &backup_path) {
            cleanup_directory_best_effort(&checkout_root);
            cleanup_directory_best_effort(&backup_root);
            return Err(DomainError::Io(error.to_string()));
        }
        Some((backup_root, backup_path))
    } else {
        None
    };

    if let Err(install_error) = std::fs::rename(&source, destination.full_path()) {
        if let Some((backup_root, backup_path)) = &backup {
            match std::fs::rename(backup_path, destination.full_path()) {
                Ok(()) => {
                    cleanup_directory_best_effort(backup_root);
                    cleanup_directory_best_effort(&checkout_root);
                    return Err(DomainError::Io(install_error.to_string()));
                }
                Err(rollback_error) => {
                    // Preserve both roots: the backup is the only recoverable
                    // original and the checkout root still holds the replacement.
                    return Err(DomainError::Internal(format!(
                        "failed to install {:?}: {install_error}; rollback failed: {rollback_error}; original preserved at {:?}",
                        destination.relative().as_str(),
                        backup_path
                    )));
                }
            }
        }

        cleanup_directory_best_effort(&checkout_root);
        return Err(DomainError::Io(install_error.to_string()));
    }

    // Installation is complete. Cleanup is best-effort and must not turn a
    // successful discard into a reported failure.
    if let Some((backup_root, _)) = backup {
        cleanup_directory_best_effort(&backup_root);
    }
    cleanup_directory_best_effort(&checkout_root);
    Ok(())
}

fn validate_replaceable_destination(path: &Path) -> DomainResult<bool> {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            let mut entries =
                std::fs::read_dir(path).map_err(|e| DomainError::Io(e.to_string()))?;
            match entries.next() {
                Some(Ok(_)) => Err(DomainError::InvalidInput(format!(
                    "cannot replace non-empty directory at tracked path {path:?}"
                ))),
                Some(Err(error)) => Err(DomainError::Io(error.to_string())),
                None => Ok(true),
            }
        }
        Ok(_) => Ok(true),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(DomainError::Io(error.to_string())),
    }
}

fn create_unique_directory(parent: &Path, prefix: &str) -> DomainResult<PathBuf> {
    let process_id = std::process::id();
    for attempt in 0..1000_u32 {
        let path = parent.join(format!("{prefix}-{process_id}-{attempt}"));
        match std::fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(DomainError::Io(error.to_string())),
        }
    }
    Err(DomainError::Internal(format!(
        "could not allocate temporary directory with prefix {prefix:?}"
    )))
}

fn cleanup_directory_best_effort(path: &Path) {
    let _ = std::fs::remove_dir_all(path);
}

#[tracing::instrument(name = "git_commit", fields(repo_path = %path, message = %message.get(..50).unwrap_or(message)))]
pub(super) fn commit(path: &str, message: &str) -> DomainResult<CommitInfo> {
    let repo = open_repo(path)?;
    let sig = repo
        .signature()
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    let mut index = repo
        .index()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let tree_id = index
        .write_tree()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let tree = repo
        .find_tree(tree_id)
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    let head_commit = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
    let merge_commit = repo
        .find_reference("MERGE_HEAD")
        .ok()
        .and_then(|r| r.peel_to_commit().ok());

    let parents: Vec<&git2::Commit> = head_commit.iter().chain(merge_commit.iter()).collect();

    let oid = repo
        .commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    // Remove merge state files after a successful merge commit.
    if merge_commit.is_some() {
        let _ = repo.cleanup_state();
    }

    let commit_obj = repo
        .find_commit(oid)
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

    let commit_tree = commit
        .tree()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let parent_tree = commit.parent(0).ok().and_then(|p| p.tree().ok());

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
    let mut revwalk = repo
        .revwalk()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    revwalk
        .push_head()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    revwalk
        .set_sorting(git2::Sort::TIME)
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    let mut commits = Vec::new();
    for oid_result in revwalk.take(limit) {
        let oid = oid_result.map_err(|e| DomainError::Internal(e.to_string()))?;
        let commit = repo
            .find_commit(oid)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
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
