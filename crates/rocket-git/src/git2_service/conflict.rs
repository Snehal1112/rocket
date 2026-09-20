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
            // fs::write always creates a regular, non-executable file —
            // apply the winning side's mode (e.g. the executable bit) so a
            // resolved script doesn't silently lose it.
            if let Some(entry) = selected_entry {
                apply_index_entry_mode(inspected.full_path(), entry.mode)?;
            }
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

    // Refuse outside an actual merge: an unconditional hard reset would
    // silently discard unrelated uncommitted work if called by mistake.
    if repo.state() != git2::RepositoryState::Merge {
        return Err(DomainError::InvalidInput(
            "no merge is in progress to abort".into(),
        ));
    }

    let head = repo
        .head()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let head_commit = head
        .peel_to_commit()
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    // NOTE: a plain safe (non-forced) checkout was tried here to preserve
    // pre-merge uncommitted work unrelated to the merge, but libgit2's
    // checkout unconditionally refuses whenever the index holds ANY
    // unresolved conflict entries — regardless of which paths they're on —
    // so it cannot be used directly against the still-conflicted merge
    // index. Reaching real git's `reset --merge` behavior (which does
    // distinguish "touched by the merge" from "unrelated") would need
    // explicit conflict-aware checkout strategy flags; deferred rather than
    // risk a subtly wrong reimplementation. Hard reset is scoped by the
    // merge-state guard above, so it can no longer fire outside a merge.
    repo.reset(head_commit.as_object(), git2::ResetType::Hard, None)
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    // Clean up merge/revert/cherry-pick state files.
    repo.cleanup_state()
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    Ok(())
}

/// Apply a git index entry's mode (e.g. the executable bit) to a file just
/// written to disk. `fs::write` always creates a regular, non-executable
/// file, so this is needed after writing resolved conflict content whose
/// winning side (`Ours`/`Theirs`) may have been executable.
#[cfg(unix)]
fn apply_index_entry_mode(path: &std::path::Path, mode: u32) -> DomainResult<()> {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = fs::metadata(path)
        .map_err(|e| DomainError::Io(e.to_string()))?
        .permissions();
    let mut bits = permissions.mode();
    if mode & 0o111 != 0 {
        bits |= 0o111;
    } else {
        bits &= !0o111;
    }
    permissions.set_mode(bits);
    fs::set_permissions(path, permissions).map_err(|e| DomainError::Io(e.to_string()))
}

#[cfg(not(unix))]
fn apply_index_entry_mode(_path: &std::path::Path, _mode: u32) -> DomainResult<()> {
    Ok(())
}
