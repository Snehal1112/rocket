use git2::{build::CheckoutBuilder, BranchType};
use rocket_shared::error::{DomainError, DomainResult};

use crate::branch::{Branch, BranchList};

use super::helpers::{branch_name, open_repo, clear_matching_untracked_paths};

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

/// Returns true if any tracked file has staged or working-tree changes.
/// Untracked files (WT_NEW) are intentionally excluded — neither a branch
/// switch nor a branch reset can overwrite them.
fn has_dirty_tracked_files(repo: &git2::Repository) -> DomainResult<bool> {
    let mut status_opts = git2::StatusOptions::new();
    status_opts.include_untracked(false);
    Ok(repo
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
        }))
}

#[tracing::instrument(name = "git_switch_branch", fields(repo_path = %path, branch = %name))]
pub(super) fn switch_branch(path: &str, name: &str) -> DomainResult<()> {
    let repo = open_repo(path)?;

    // Pre-flight: refuse if any tracked file has staged or working-tree changes.
    if has_dirty_tracked_files(&repo)? {
        return Err(DomainError::InvalidInput(
            "You have uncommitted changes that would be overwritten by switching branches. \
             Please commit or stash your changes first."
                .to_string(),
        ));
    }

    // Save the current HEAD ref for rollback if checkout fails.
    let old_head = repo.head().ok().and_then(|r| r.name().ok().map(String::from));

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

/// Point an existing local branch at `commit`, the tip of `remote_branch`.
///
/// This discards any local commits the branch carried that are absent from
/// that remote, so it is only ever reachable through the opt-in `force` flag
/// of [`checkout_remote_branch`]. The working tree and index are touched only
/// when the branch is the one currently checked out; otherwise nothing on
/// disk needs to change and only the ref and its upstream move.
fn reset_local_branch_to_commit<'repo>(
    repo: &'repo git2::Repository,
    mut existing: git2::Branch<'repo>,
    local_name: &str,
    commit: &git2::Commit<'repo>,
    tree: &git2::Tree<'repo>,
    remote_branch: &str,
) -> DomainResult<()> {
    let is_current = repo
        .head()
        .ok()
        .and_then(|h| h.shorthand().ok().map(String::from))
        == Some(local_name.to_string());

    if is_current {
        // Second safety layer, independent of any confirmation the UI showed.
        // That confirmation only warns about losing *committed* history; this
        // guard protects *uncommitted* working-tree changes, which must never
        // be discarded silently.
        if has_dirty_tracked_files(repo)? {
            return Err(DomainError::InvalidInput(
                "You have uncommitted changes that would be overwritten by resetting this \
                 branch. Please commit or stash your changes first."
                    .to_string(),
            ));
        }

        // Adopt any untracked file whose content already matches the target,
        // so checkout doesn't reject a harmless pre-existing copy.
        clear_matching_untracked_paths(repo, tree)?;

        // Preflight: a safe (never forced) checkout of the target tree must
        // succeed BEFORE the branch ref moves, so a rejected checkout leaves
        // refs, the index, and the worktree completely untouched.
        let mut checkout = git2::build::CheckoutBuilder::new();
        checkout.safe();
        repo.checkout_tree(commit.as_object(), Some(&mut checkout))
            .map_err(|e| {
                DomainError::Conflict(format!(
                    "cannot check out '{remote_branch}': {e}. \
                     Resolve the conflicting local file(s) first."
                ))
            })?;
    }

    // Retarget the branch ref first, then its upstream. This applies whether
    // or not the branch is checked out; when it is, HEAD follows it
    // symbolically. Order matters: if the upstream write below failed after
    // the ref move, the branch would still be left consistent (pointing at
    // real, valid content) rather than in a half-updated intermediate state.
    existing
        .get_mut()
        .set_target(commit.id(), "reset to remote branch")
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    existing
        .set_upstream(Some(remote_branch))
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    if is_current {
        // Sync the index to the tree that was just checked out.
        let mut index = repo
            .index()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        index
            .read_tree(tree)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        index
            .write()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
    }

    Ok(())
}

#[tracing::instrument(name = "git_checkout_remote_branch", fields(repo_path = %path, remote_branch = %remote_branch, force = %force))]
pub(super) fn checkout_remote_branch(
    path: &str,
    remote_branch: &str,
    force: bool,
) -> DomainResult<()> {
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

    let existing_local = repo.find_branch(&local_name, git2::BranchType::Local).ok();

    // Default behaviour: reject up front if the local branch already exists,
    // before anything else is touched. Only an explicit force opts into
    // resetting that branch to the remote's content.
    if existing_local.is_some() && !force {
        return Err(DomainError::InvalidInput(format!(
            "local branch '{local_name}' already exists"
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
    let tree = commit
        .tree()
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    if let Some(existing) = existing_local {
        // Only reachable with force == true; the check above already returned
        // otherwise.
        return reset_local_branch_to_commit(
            &repo,
            existing,
            &local_name,
            &commit,
            &tree,
            remote_branch,
        );
    }

    // Adopt any untracked file whose content already matches the target,
    // so checkout doesn't reject a harmless pre-existing copy.
    clear_matching_untracked_paths(&repo, &tree)?;

    // Preflight: verify a safe (non-forced) checkout of the target tree
    // succeeds BEFORE creating the local branch or moving HEAD, so a
    // rejected checkout (e.g. a colliding untracked file) leaves refs, the
    // index, and the worktree completely untouched. libgit2's safe checkout
    // detects all such conflicts before applying any change.
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(commit.as_object(), Some(&mut checkout))
        .map_err(|e| {
            DomainError::Conflict(format!(
                "cannot check out '{remote_branch}': {e}. \
                 Resolve the conflicting local file(s) first."
            ))
        })?;

    // Checkout succeeded — now safe to create the local branch, track
    // upstream, sync the index to the checked-out tree, and move HEAD.
    repo.branch(&local_name, &commit, false)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let mut local_branch = repo
        .find_branch(&local_name, git2::BranchType::Local)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    local_branch
        .set_upstream(Some(remote_branch))
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    let mut index = repo
        .index()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    index
        .read_tree(&tree)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    index
        .write()
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    repo.set_head(&format!("refs/heads/{local_name}"))
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
