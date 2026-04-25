use std::fs;
use std::path::Path;

use rocket_shared::error::{DomainError, DomainResult};

use crate::conflict::{ConflictFile, ConflictResolution};

use super::helpers::open_repo;

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

#[tracing::instrument(name = "git_abort_merge", fields(repo_path = %path))]
pub(super) fn abort_merge(path: &str) -> DomainResult<()> {
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
