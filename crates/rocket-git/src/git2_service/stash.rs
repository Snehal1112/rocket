use git2::Repository;
use rocket_shared::error::{DomainError, DomainResult};

use crate::stash::StashEntry;

#[tracing::instrument(name = "git_stash_list", fields(repo_path = %path))]
pub(super) fn stash_list(path: &str) -> DomainResult<Vec<StashEntry>> {
    let mut repo = Repository::open(path)
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    // Collect raw data first — stash_foreach borrows repo mutably, so we
    // can't call repo.find_commit() inside the closure.
    let mut raw: Vec<(usize, String, git2::Oid)> = Vec::new();
    repo.stash_foreach(|index, message, oid| {
        raw.push((index, message.to_string(), *oid));
        true
    })
    .map_err(|e| DomainError::Internal(e.to_string()))?;

    let mut entries = Vec::new();
    for (index, message, oid) in raw {
        // git formats stash reflog messages as "On <branch>: <user msg>"
        // or "WIP on <branch>: <user msg>". Parse both prefix forms.
        let (display, branch) = if let Some(pos) = message.find(": ") {
            let prefix = &message[..pos];
            let branch = prefix
                .strip_prefix("WIP on ")
                .or_else(|| prefix.strip_prefix("On "))
                .unwrap_or("")
                .to_string();
            (message[pos + 2..].to_string(), branch)
        } else {
            (message, String::new())
        };

        // Look up the stash commit once and derive both timestamp and diff stats.
        let commit = repo.find_commit(oid).ok();

        let timestamp = commit
            .as_ref()
            .and_then(|c| chrono::DateTime::from_timestamp(c.time().seconds(), 0))
            .unwrap_or_else(chrono::Utc::now);

        // Replicate `git stash show --stat`: diff stash^1 (HEAD at stash
        // time) vs stash^0 (working-tree commit) to get files + line stats.
        let (files_changed, insertions, deletions, changed_files) = commit
            .as_ref()
            .and_then(|sc| {
                let parent = sc.parent(0).ok()?;
                let stash_tree = sc.tree().ok()?;
                let parent_tree = parent.tree().ok()?;
                let diff = repo
                    .diff_tree_to_tree(Some(&parent_tree), Some(&stash_tree), None)
                    .ok()?;
                let stats = diff.stats().ok()?;
                let mut paths: Vec<String> = Vec::new();
                diff.foreach(
                    &mut |delta, _| {
                        let path = delta
                            .new_file()
                            .path()
                            .or_else(|| delta.old_file().path())
                            .map(|p| p.to_string_lossy().into_owned());
                        if let Some(p) = path {
                            paths.push(p);
                        }
                        true
                    },
                    None,
                    None,
                    None,
                )
                .ok()?;
                Some((stats.files_changed(), stats.insertions(), stats.deletions(), paths))
            })
            .unwrap_or_default();

        entries.push(StashEntry {
            index,
            message: display,
            timestamp,
            branch,
            files_changed,
            insertions,
            deletions,
            changed_files,
        });
    }

    Ok(entries)
}

#[tracing::instrument(name = "git_stash_save", fields(repo_path = %path))]
pub(super) fn stash_save(path: &str, message: &str) -> DomainResult<()> {
    let mut repo = Repository::open(path)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let sig = repo
        .signature()
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    // INCLUDE_UNTRACKED matches `git stash` CLI default — captures new
    // untracked files (e.g. newly created .bru requests) in addition to
    // tracked modified/deleted files. Without this flag, stash_save returns
    // "nothing to stash" when only untracked files exist.
    repo.stash_save(&sig, message, Some(git2::StashFlags::INCLUDE_UNTRACKED))
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}

#[tracing::instrument(name = "git_stash_pop", fields(repo_path = %path, index = %index))]
pub(super) fn stash_pop(path: &str, index: usize) -> DomainResult<()> {
    let mut repo = Repository::open(path)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    repo.stash_pop(index, None)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}

#[tracing::instrument(name = "git_stash_apply", fields(repo_path = %path, index = %index))]
pub(super) fn stash_apply(path: &str, index: usize) -> DomainResult<()> {
    let mut repo = Repository::open(path)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    repo.stash_apply(index, None)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}

#[tracing::instrument(name = "git_stash_drop", fields(repo_path = %path, index = %index))]
pub(super) fn stash_drop(path: &str, index: usize) -> DomainResult<()> {
    let mut repo = Repository::open(path)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    repo.stash_drop(index)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}
