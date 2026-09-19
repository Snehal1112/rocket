use git2::Status;
use rocket_shared::error::{DomainError, DomainResult};
use std::fs;

use crate::diff::FileDiff;
use crate::status::{FileStatus, RepoStatus};

use super::helpers::{
    ahead_behind, branch_name, build_simple_diff, get_head_content, get_index_content,
    inspect_worktree_path, map_git2_status, open_repo, GitRelativePath, WorktreeLeafKind,
};

#[tracing::instrument(name = "git_status", fields(repo_path = %path))]
pub(super) fn status(path: &str) -> DomainResult<RepoStatus> {
    let repo = open_repo(path)?;
    let branch = branch_name(&repo);
    let (ahead, behind) = ahead_behind(&repo);

    // Recurse into untracked directories so each file is reported
    // individually rather than as a single directory entry with a
    // trailing '/'. Without this, staging an untracked folder fails
    // because index.add_path() rejects directory paths.
    let mut status_opts = git2::StatusOptions::new();
    status_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true);
    let statuses = repo
        .statuses(Some(&mut status_opts))
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

#[tracing::instrument(name = "git_diff_file", fields(repo_path = %path, file = %file))]
pub(super) fn diff_file(path: &str, file: &str) -> DomainResult<FileDiff> {
    let repo = open_repo(path)?;
    let inspected = inspect_worktree_path(&repo, GitRelativePath::parse(file)?)?;
    if inspected.leaf_kind() == WorktreeLeafKind::Symlink {
        return Err(DomainError::InvalidInput(format!(
            "cannot diff symlink leaf {:?}",
            inspected.relative().as_str()
        )));
    }

    let old_content = get_head_content(&repo, inspected.relative());
    let new_content = match inspected.leaf_kind() {
        WorktreeLeafKind::File => fs::read_to_string(inspected.full_path()).ok(),
        WorktreeLeafKind::Missing | WorktreeLeafKind::Directory | WorktreeLeafKind::Other => None,
        WorktreeLeafKind::Symlink => unreachable!("symlink leaves are rejected above"),
    };
    let hunks = build_simple_diff(&old_content, &new_content);

    Ok(FileDiff {
        path: inspected.relative().as_str().to_string(),
        old_content,
        new_content,
        hunks,
    })
}

#[tracing::instrument(name = "git_diff_staged", fields(repo_path = %path, file = %file))]
pub(super) fn diff_staged(path: &str, file: &str) -> DomainResult<FileDiff> {
    let repo = open_repo(path)?;
    let file = GitRelativePath::parse(file)?;
    let old_content = get_head_content(&repo, &file);
    let new_content = get_index_content(&repo, &file);
    let hunks = build_simple_diff(&old_content, &new_content);

    Ok(FileDiff {
        path: file.as_str().to_string(),
        old_content,
        new_content,
        hunks,
    })
}
