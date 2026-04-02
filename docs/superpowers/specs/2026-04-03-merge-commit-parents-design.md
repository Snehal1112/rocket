# Merge Commit Parent Fix

**Date:** 2026-04-03  
**Status:** Approved  
**Scope:** Single method change in `crates/rocket-git/src/git2_service.rs`

## Overview

When a pull results in conflicts, the repo is left in merge-in-progress state (`.git/MERGE_HEAD` present). The user resolves each file and commits via the normal commit form. The current `commit()` implementation only uses the current `HEAD` as the parent, ignoring `MERGE_HEAD`. This produces a plain single-parent commit that does not incorporate the remote's commits into the ancestry, causing the local and remote branches to diverge. A subsequent push fails with `NotFastForward`.

The fix: check for `MERGE_HEAD` when building the parent list, and call `repo.cleanup_state()` after a merge commit to remove the merge state files.

## Affected Files

| File | Change |
|------|--------|
| `crates/rocket-git/src/git2_service.rs` | `commit()`: include `MERGE_HEAD` as second parent when present; call `cleanup_state()` after merge commit |

## Change

**File:** `crates/rocket-git/src/git2_service.rs`, `commit()` method (line 430)

**Before:**
```rust
let parent = repo.head().ok().and_then(|h| h.peel_to_commit().ok());
let parents: Vec<&git2::Commit> = parent.iter().collect();

let oid = repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parents)
    .map_err(|e| DomainError::Internal(e.to_string()))?;
```

**After:**
```rust
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
```

## Why This Works

A merge commit with `MERGE_HEAD` as its second parent makes the remote's `HEAD` an ancestor of the new local `HEAD`. The remote can then fast-forward to the new local commit, so a normal (non-force) push succeeds.

`cleanup_state()` removes `.git/MERGE_HEAD`, `.git/MERGE_MSG`, and related files — the same call used in `abort_merge()`. This leaves the repo in a clean non-merging state after the commit.

## Out of Scope

- No frontend changes needed.
- No changes to `push()` or force-push behavior.
- `merge_branch()` conflict handling is a separate concern and not addressed here.
