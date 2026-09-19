use rocket_shared::error::{DomainError, DomainResult};
use std::fs;

use crate::conflict::{ConflictFile, ConflictResolution};

use super::helpers::{inspect_worktree_path, open_repo, GitRelativePath, WorktreeLeafKind};

#[tracing::instrument(name = "git_conflicts", fields(repo_path = %path))]
pub(super) fn conflicts(path: &str) -> DomainResult<Vec<ConflictFile>> {
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
            .or(entry.ancestor.as_ref())
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

#[tracing::instrument(name = "git_resolve_conflict", skip(resolution), fields(repo_path = %path, file = %file))]
pub(super) fn resolve_conflict(
    path: &str,
    file: &str,
    resolution: &ConflictResolution,
) -> DomainResult<()> {
    let repo = open_repo(path)?;
    let inspected = inspect_worktree_path(&repo, GitRelativePath::parse(file)?)?;
    if inspected.leaf_kind() == WorktreeLeafKind::Symlink {
        return Err(DomainError::InvalidInput(format!(
            "cannot resolve conflict through symlink leaf {:?}",
            inspected.relative().as_str()
        )));
    }

    // Keep one live index and prove this exact validated path is conflicted
    // before changing the worktree.
    let mut index = repo
        .index()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let conflict = {
        let conflicts = index
            .conflicts()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let mut matching = None;
        for conflict in conflicts {
            let conflict = conflict.map_err(|e| DomainError::Internal(e.to_string()))?;
            let matches = conflict
                .ancestor
                .iter()
                .chain(conflict.our.iter())
                .chain(conflict.their.iter())
                .any(|entry| entry.path == inspected.relative().as_str().as_bytes());
            if matches {
                matching = Some(conflict);
                break;
            }
        }
        matching
    }
    .ok_or_else(|| {
        DomainError::Conflict(format!(
            "path {:?} is not currently conflicted",
            inspected.relative().as_str()
        ))
    })?;

    let selected_entry = match resolution {
        ConflictResolution::Ours => conflict.our.as_ref(),
        ConflictResolution::Theirs => conflict.their.as_ref(),
        ConflictResolution::Custom { .. } => None,
    };
    let selected_content = match resolution {
        ConflictResolution::Custom { content } => Some(content.as_bytes().to_vec()),
        ConflictResolution::Ours | ConflictResolution::Theirs => selected_entry
            .map(|entry| {
                repo.find_blob(entry.id)
                    .map(|blob| blob.content().to_vec())
                    .map_err(|e| DomainError::Internal(e.to_string()))
            })
            .transpose()?,
    };

    match selected_content {
        Some(content) => {
            if matches!(
                inspected.leaf_kind(),
                WorktreeLeafKind::Directory | WorktreeLeafKind::Other
            ) {
                return Err(DomainError::InvalidInput(format!(
                    "cannot replace non-file path {:?} during conflict resolution",
                    inspected.relative().as_str()
                )));
            }
            fs::write(inspected.full_path(), content)
                .map_err(|e| DomainError::Io(e.to_string()))?;
            index
                .add_path(inspected.relative().as_path())
                .map_err(|e| DomainError::Internal(e.to_string()))?;
        }
        None => {
            match inspected.leaf_kind() {
                WorktreeLeafKind::Missing => {}
                WorktreeLeafKind::File => fs::remove_file(inspected.full_path())
                    .map_err(|e| DomainError::Io(e.to_string()))?,
                WorktreeLeafKind::Directory | WorktreeLeafKind::Other => {
                    return Err(DomainError::InvalidInput(format!(
                        "cannot delete non-file path {:?} during conflict resolution",
                        inspected.relative().as_str()
                    )));
                }
                WorktreeLeafKind::Symlink => {
                    return Err(DomainError::InvalidInput(format!(
                        "cannot resolve conflict through symlink leaf {:?}",
                        inspected.relative().as_str()
                    )));
                }
            }
            index
                .remove_path(inspected.relative().as_path())
                .map_err(|e| DomainError::Internal(e.to_string()))?;
        }
    }

    index
        .write()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}

#[tracing::instrument(name = "git_abort_merge", fields(repo_path = %path))]
pub(super) fn abort_merge(path: &str) -> DomainResult<()> {
    let repo = open_repo(path)?;

    // Get HEAD commit to reset to.
    let head = repo
        .head()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let head_commit = head
        .peel_to_commit()
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    // Hard reset index and working directory to HEAD.
    repo.reset(head_commit.as_object(), git2::ResetType::Hard, None)
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    // Clean up merge/revert/cherry-pick state files.
    repo.cleanup_state()
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    Ok(())
}
