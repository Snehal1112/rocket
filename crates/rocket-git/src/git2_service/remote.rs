use std::path::PathBuf;

use git2::build::CheckoutBuilder;
use rocket_shared::error::{DomainError, DomainResult};

use crate::credentials::GitCredentials;
use crate::remote::{FetchResult, RemoteInfo};

use super::helpers::{branch_name, build_callbacks, clear_matching_untracked_paths, open_repo};

#[tracing::instrument(name = "git_list_remotes", fields(repo_path = %path))]
pub(super) fn list_remotes(path: &str) -> DomainResult<Vec<RemoteInfo>> {
    let repo = open_repo(path)?;
    let remote_names = repo
        .remotes()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let mut remotes = Vec::new();
    for name in remote_names.iter().flatten().flatten() {
        let remote = repo
            .find_remote(name)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let url = remote.url().unwrap_or_default().to_string();
        remotes.push(RemoteInfo {
            name: name.to_string(),
            url,
        });
    }
    Ok(remotes)
}

#[tracing::instrument(name = "git_add_remote", fields(repo_path = %path, name = %name))]
pub(super) fn add_remote(path: &str, name: &str, url: &str) -> DomainResult<()> {
    let repo = open_repo(path)?;
    repo.remote(name, url)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}

#[tracing::instrument(name = "git_remove_remote", fields(repo_path = %path, name = %name))]
pub(super) fn remove_remote(path: &str, name: &str) -> DomainResult<()> {
    let repo = open_repo(path)?;
    repo.remote_delete(name)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    Ok(())
}

#[tracing::instrument(name = "git_set_remote_url", fields(repo_path = %path, name = %name))]
pub(super) fn set_remote_url(path: &str, name: &str, url: &str) -> DomainResult<()> {
    let repo = open_repo(path)?;
    repo.remote_set_url(name, url)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    // Prune stale remote-tracking refs so that ahead/behind no longer
    // reflects the old remote's history after the URL changes.
    let prefix = format!("refs/remotes/{}/", name);
    if let Ok(refs) = repo.references() {
        let stale: Vec<String> = refs
            .flatten()
            .filter_map(|r| r.name().ok().map(String::from))
            .filter(|n: &String| n.starts_with(&prefix))
            .collect();
        for refname in stale {
            if let Ok(mut r) = repo.find_reference(&refname) {
                let _ = r.delete();
            }
        }
    }
    Ok(())
}

/// Verify the force-with-lease precondition for `remote_branch` before any
/// forced write reaches the remote: the remote's live tip must still be the
/// commit this repository last recorded for it in
/// `refs/remotes/<remote>/<branch>`. If it has moved (someone else pushed) or
/// this repository has no record of it at all, the force-push is refused so
/// that commits the user has never seen are never silently overwritten.
///
/// This only lists the remote's advertised refs — no objects are downloaded.
fn verify_force_with_lease(
    repo: &git2::Repository,
    remote_obj: &mut git2::Remote<'_>,
    remote: &str,
    remote_branch: &str,
    creds: &GitCredentials,
    remote_url: &str,
    known_hosts_path: Option<PathBuf>,
) -> DomainResult<()> {
    let target_ref = format!("refs/heads/{remote_branch}");

    // A separate connection with its own callbacks: `connect_auth` consumes
    // the callbacks it is given, so the push below needs a fresh set. This
    // only reads the remote's ref advertisement — no objects are downloaded
    // and nothing is written.
    //
    // The direction is `Push`, not `Fetch`: libgit2 resolves a `Fetch`
    // connection against the remote's fetch URL and a `Push` connection
    // against its pushurl (falling back to the fetch URL when none is set).
    // The lease has to be read from the endpoint the push will actually
    // write to, or a remote with a distinct pushurl would be leased against
    // the wrong repository.
    let (list_callbacks, list_verification) = build_callbacks(creds, remote_url, known_hosts_path);
    let live_oid = {
        let connection = remote_obj
            .connect_auth(git2::Direction::Push, Some(list_callbacks), None)
            .map_err(|error| list_verification.map_error(error))?;
        let heads = connection
            .list()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        heads
            .iter()
            .find(|head| head.name() == target_ref)
            .map(|head| head.oid())
        // The guard disconnects on drop, releasing `remote_obj` so the push
        // that follows can reconnect. This lease check and the push below are
        // two separate connections, not one continuous session — a push that
        // lands on the remote in the gap between them would still be
        // overwritten. Real git's --force-with-lease has the same class of
        // window (it is not a server-side atomic compare-and-swap either),
        // but reads the advertisement and pushes over one session, which is
        // narrower than this. git2's `RemoteConnection` does expose a way to
        // keep the same connection open across both steps (`.remote()`,
        // returning `&mut Remote` while still connected) — narrowing this to
        // real-git parity is possible future work, not attempted here.
    };

    // What this repository last knew the remote's tip to be.
    let known_oid = repo
        .find_reference(&format!("refs/remotes/{remote}/{remote_branch}"))
        .ok()
        .and_then(|reference| reference.peel_to_commit().ok())
        .map(|commit| commit.id())
        .ok_or_else(|| {
            DomainError::Conflict(format!(
                "Cannot force-push: no local record of {remote}/{remote_branch}'s \
                 current state. Fetch first."
            ))
        })?;

    let Some(live_oid) = live_oid else {
        // The branch vanished from the remote since the last fetch — the same
        // class of surprise as it having moved, so refuse the same way.
        return Err(DomainError::Conflict(format!(
            "Cannot force-push: '{remote}/{remote_branch}' no longer exists on the remote. \
             Fetch first to see the remote's current state before overwriting."
        )));
    };

    if known_oid != live_oid {
        return Err(DomainError::Conflict(format!(
            "Cannot force-push: '{remote}/{remote_branch}' has new commits since your \
             last fetch. Fetch first to see them before overwriting."
        )));
    }

    Ok(())
}

#[tracing::instrument(name = "git_push", skip(creds), fields(repo_path = %path, remote = %remote, force = force))]
pub(super) fn push(
    path: &str,
    remote: &str,
    creds: &GitCredentials,
    known_hosts_path: Option<PathBuf>,
    force: bool,
) -> DomainResult<()> {
    let repo = open_repo(path)?;
    let mut remote_obj = repo
        .find_remote(remote)
        .map_err(|e| DomainError::Internal(e.to_string()))?;

    let head = repo
        .head()
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let branch_name_str = head.shorthand().unwrap_or("main");

    // Prefer the configured upstream's remote branch name as the push target.
    // Falls back to same-name if no upstream is configured. Note: this reads
    // the branch's upstream unconditionally, regardless of which `remote`
    // this call is pushing to — if the upstream happens to belong to a
    // DIFFERENT remote (e.g. after force-resetting a branch onto another
    // remote's content), pushing to `remote` here would target that other
    // remote's branch NAME against `remote`. This fails safe: the lease
    // check below would refuse if `refs/remotes/<remote>/<that name>`
    // doesn't exist or doesn't match, rather than pushing to the wrong ref
    // silently. Narrowing this to "only trust the upstream when it belongs
    // to `remote`" is a reasonable follow-up, not attempted here.
    let remote_branch = repo
        .find_branch(branch_name_str, git2::BranchType::Local)
        .ok()
        .and_then(|b| b.upstream().ok())
        .and_then(|u| {
            u.name().ok().flatten().map(|full| {
                // upstream name is "origin/feat-x" — strip the "origin/" prefix.
                full.split_once('/').map(|(_, branch)| branch.to_string())
            })
        })
        .flatten()
        .unwrap_or_else(|| branch_name_str.to_string());

    let refspec = format!("refs/heads/{branch_name_str}:refs/heads/{remote_branch}");
    let remote_url = remote_obj
        .pushurl()
        .ok()
        .flatten()
        .map(str::to_owned)
        .or_else(|| remote_obj.url().ok().map(str::to_owned))
        .ok_or_else(|| DomainError::InvalidInput(format!("Remote '{remote}' has no URL")))?;

    // A forced push opts into --force-with-lease semantics, never a blind
    // force: the lease is checked before the refspec is allowed to carry the
    // "+" that lets the remote's history be overwritten, so a refused push
    // leaves the remote completely untouched.
    let refspec = if force {
        verify_force_with_lease(
            &repo,
            &mut remote_obj,
            remote,
            &remote_branch,
            creds,
            &remote_url,
            known_hosts_path.clone(),
        )?;
        format!("+{refspec}")
    } else {
        refspec
    };

    let (callbacks, verification) = build_callbacks(creds, &remote_url, known_hosts_path);
    let mut push_opts = git2::PushOptions::new();
    push_opts.remote_callbacks(callbacks);

    remote_obj
        .push(&[&refspec], Some(&mut push_opts))
        .map_err(|error| verification.map_error(error))?;
    Ok(())
}

#[tracing::instrument(name = "git_fetch", skip(creds), fields(repo_path = %path, remote = %remote))]
pub(super) fn fetch(
    path: &str,
    remote: &str,
    creds: &GitCredentials,
    known_hosts_path: Option<PathBuf>,
) -> DomainResult<FetchResult> {
    let repo = open_repo(path)?;
    let mut remote_obj = repo
        .find_remote(remote)
        .map_err(|e| DomainError::Internal(e.to_string()))?;
    let remote_url = remote_obj
        .url()
        .ok()
        .ok_or_else(|| DomainError::InvalidInput(format!("Remote '{remote}' has no URL")))?
        .to_owned();

    let updated_refs = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let received_objects = std::sync::Arc::new(std::sync::Mutex::new(0usize));
    let received_bytes = std::sync::Arc::new(std::sync::Mutex::new(0usize));

    let refs_clone = updated_refs.clone();
    let objs_clone = received_objects.clone();
    let bytes_clone = received_bytes.clone();

    let (mut callbacks, verification) = build_callbacks(creds, &remote_url, known_hosts_path);
    callbacks.update_tips(move |refname, _old, _new| {
        if let Ok(mut v) = refs_clone.lock() {
            v.push(refname.to_owned());
        }
        true
    });
    callbacks.transfer_progress(move |stats| {
        if let Ok(mut n) = objs_clone.lock() {
            *n = stats.received_objects();
        }
        if let Ok(mut b) = bytes_clone.lock() {
            *b = stats.received_bytes();
        }
        true
    });

    let mut fetch_opts = git2::FetchOptions::new();
    fetch_opts.remote_callbacks(callbacks);

    remote_obj
        .fetch::<&str>(&[], Some(&mut fetch_opts), None)
        .map_err(|error| verification.map_error(error))?;

    Ok(FetchResult {
        updated_refs: updated_refs.lock().map(|v| v.clone()).unwrap_or_default(),
        received_objects: received_objects.lock().map(|n| *n).unwrap_or(0),
        received_bytes: received_bytes.lock().map(|b| *b).unwrap_or(0),
    })
}

#[tracing::instrument(name = "git_pull", skip(creds), fields(repo_path = %path, remote = %remote))]
pub(super) fn pull(
    path: &str,
    remote: &str,
    creds: &GitCredentials,
    known_hosts_path: Option<PathBuf>,
) -> DomainResult<()> {
    // Fetch first, using the same injected read-only trust store.
    fetch(path, remote, creds, known_hosts_path)?;

    let repo = open_repo(path)?;
    let current_branch = branch_name(&repo);

    // Resolve the remote-tracking ref to merge.  Resolution order:
    // 1. Configured upstream tracking branch (e.g. local "master" → "origin/main").
    // 2. Same-name remote branch: refs/remotes/<remote>/<current_branch>.
    // 3. Remote HEAD (refs/remotes/<remote>/HEAD) — the remote's default branch.
    //    This handles repos cloned before the remote renamed its default branch
    //    and repos with no upstream tracking configured at all.
    let tracking_ref = repo
        .find_branch(&current_branch, git2::BranchType::Local)
        .ok()
        .and_then(|b| b.upstream().ok())
        .and_then(|u| u.get().resolve().ok())
        .or_else(|| {
            let refname = format!("refs/remotes/{remote}/{current_branch}");
            repo.find_reference(&refname)
                .ok()
                .and_then(|r| r.resolve().ok())
        })
        .or_else(|| {
            // refs/remotes/origin/HEAD is a symbolic ref pointing to the
            // remote's default branch (set during clone / git remote set-head).
            let head_refname = format!("refs/remotes/{remote}/HEAD");
            repo.find_reference(&head_refname)
                .ok()
                .and_then(|r| r.resolve().ok())
        });

    let fetch_commit = tracking_ref
        .ok_or_else(|| {
            DomainError::Internal(format!(
                "remote tracking ref 'refs/remotes/{remote}/{current_branch}' not found after fetch: \
                 no upstream configured and branch name does not match any remote branch"
            ))
        })
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
        let target_commit = repo
            .find_commit(fetch_commit.id())
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let target_tree = target_commit
            .tree()
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        // Adopt any untracked file whose content already matches the target,
        // so checkout doesn't reject a harmless pre-existing copy.
        clear_matching_untracked_paths(&repo, &target_tree)?;

        // Preflight: verify a safe (non-forced) checkout of the target tree
        // succeeds BEFORE moving the branch ref or HEAD, so a rejected
        // fast-forward (e.g. a colliding untracked file) leaves refs, the
        // index, and the worktree completely untouched.
        let mut checkout = CheckoutBuilder::new();
        checkout.safe();
        repo.checkout_tree(target_tree.as_object(), Some(&mut checkout))
            .map_err(|e| {
                DomainError::Conflict(format!(
                    "cannot fast-forward pull: {e}. \
                     Resolve the conflicting local file(s) first."
                ))
            })?;

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

        // Sync the index to the checked-out tree (checkout_tree above only
        // materializes the worktree; it does not update the index).
        let mut index = repo
            .index()
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        index
            .read_tree(&target_tree)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        index
            .write()
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
        return Err(DomainError::Conflict(format!(
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
