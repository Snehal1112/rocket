use git2::{build::CheckoutBuilder, BranchType};
use rocket_shared::error::{DomainError, DomainResult};

use crate::branch::{Branch, BranchList};

use super::helpers::{branch_name, open_repo};

#[tracing::instrument(name = "git_branches", fields(repo_path = %path))]
pub(super) fn branches(path: &str) -> DomainResult<BranchList> {
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

#[tracing::instrument(name = "git_switch_branch", fields(repo_path = %path, branch = %name))]
pub(super) fn switch_branch(path: &str, name: &str) -> DomainResult<()> {
    let repo = open_repo(path)?;

    // Pre-flight: refuse if any tracked file has staged or working-tree changes.
    // Untracked files (WT_NEW) are intentionally excluded — a branch switch
    // cannot overwrite them.
    let mut status_opts = git2::StatusOptions::new();
    status_opts.include_untracked(false);
    let dirty = repo
        .statuses(Some(&mut status_opts))
        .map_err(|e| DomainError::Internal(e.to_string()))?
        .iter()
        .any(|e| {
            e.status().intersects(
                git2::Status::INDEX_NEW
                    | git2::Status::INDEX_MODIFIED
                    | git2::Status::INDEX_DELETED
                    | git2::Status::INDEX_RENAMED
                    | git2::Status::INDEX_TYPECHANGE
                    | git2::Status::WT_MODIFIED
                    | git2::Status::WT_DELETED
                    | git2::Status::WT_RENAMED
                    | git2::Status::WT_TYPECHANGE,
            )
        });
    if dirty {
        return Err(DomainError::InvalidInput(
            "You have uncommitted changes that would be overwritten by switching branches. \
             Please commit or stash your changes first."
                .to_string(),
        ));
    }

    // Save the current HEAD ref for rollback if checkout fails.
    let old_head = repo
        .head()
        .ok()
        .and_then(|r| r.name().map(String::from));

    repo.set_head(&format!("refs/heads/{name}"))
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    // Safe checkout as a second-layer guard (TOCTOU window defence).
    repo.checkout_head(Some(git2::build::CheckoutBuilder::new().safe()))
        .map_err(|e| {
            // Best-effort rollback — restore HEAD to its previous ref.
            if let Some(ref original) = old_head {
                let _ = repo.set_head(original);
            }
            DomainError::Internal(e.to_string())
        })?;

    Ok(())
}

#[tracing::instrument(name = "git_checkout_remote_branch", fields(repo_path = %path, remote_branch = %remote_branch))]
pub(super) fn checkout_remote_branch(path: &str, remote_branch: &str) -> DomainResult<()> {
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

#[tracing::instrument(name = "git_create_branch", fields(repo_path = %path, name = %name))]
pub(super) fn create_branch(path: &str, name: &str) -> DomainResult<()> {
    let repo = open_repo(path)?;

    // HEAD must point to a commit; an unborn HEAD (no commits yet) cannot
    // be used as a branch base.
    let head_commit = repo.head().and_then(|h| h.peel_to_commit()).map_err(|_| {
        DomainError::InvalidInput(
            "Cannot create a branch: the repository has no commits yet. \
             Make an initial commit first."
                .to_string(),
        )
    })?;

    repo.branch(name, &head_commit, false)
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    // Switch HEAD to the new branch immediately after creating it.
    repo.set_head(&format!("refs/heads/{name}"))
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    repo.checkout_head(Some(&mut CheckoutBuilder::new().force()))
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    Ok(())
}

#[tracing::instrument(name = "git_delete_branch", fields(repo_path = %path, name = %name))]
pub(super) fn delete_branch(path: &str, name: &str) -> DomainResult<()> {
    let repo = open_repo(path)?;
    let mut branch = repo
        .find_branch(name, BranchType::Local)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    branch
        .delete()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}

#[tracing::instrument(name = "git_merge_branch", fields(repo_path = %path, name = %name))]
pub(super) fn merge_branch(path: &str, name: &str) -> DomainResult<()> {
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
        // Write the conflicted index so git_conflicts() can enumerate the files.
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
        return Err(DomainError::Conflict(format!(
            "merge conflict: resolve conflicts in {file_list} and commit to complete the merge"
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
