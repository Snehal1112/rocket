---
title: Git Identity Fallback — Commit Author Resolution
date: 2026-05-11
status: approved
---

## Problem

When a user commits via Rocket's git feature, the commit author falls back to
`"RocketAPI User" <user@rocketapi.local>` whenever `repo.signature()` cannot
resolve a name and email. This happens when neither the repo-local `.git/config`
nor `~/.gitconfig` has `user.name`/`user.email` set. The hardcoded placeholder
is wrong: it appears on GitHub and in git log with a fake identity.

The root cause is a missing `user.name`/`user.email` in git config — not an
SSH key mismatch. SSH keys control push authentication; commit authorship is
entirely separate.

## Goal

When git config has no identity for a repo, prompt the user once for their
name and email, write it to the repo-local `.git/config`, then proceed with the
commit. On every subsequent commit (same repo, any session), `repo.signature()`
succeeds and the prompt never appears again.

## Approach

**Frontend-driven identity check (Approach A)**

Before submitting a commit, the frontend calls a new `git_get_identity` command.
If name or email is empty, it shows `GitIdentityDialog`. On confirm, it calls
`git_set_identity` to write the repo-local config, then calls `git_commit`.
The backend `commit()` function is simplified — the `"RocketAPI User"` fallback
is removed entirely.

## Architecture

### New Tauri commands — `src-tauri/src/commands/git.rs`

```rust
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitIdentity {
    pub name: String,
    pub email: String,
}

// Returns name + email from git config (local → global → system).
// Returns empty strings (not an error) if unset.
pub fn git_get_identity(path: String) -> Result<GitIdentity, DomainError>

// Writes user.name and user.email to the repo-local .git/config.
pub fn git_set_identity(path: String, name: String, email: String) -> Result<(), DomainError>
```

Both commands operate directly via `git2::Config` — no new service method
needed, as they are thin config reads/writes with no domain logic.

### Backend change — `crates/rocket-git/src/git2_service/staging.rs`

Remove the `.or_else(|_| git2::Signature::now("RocketAPI User", "user@rocketapi.local"))`
fallback in `commit()`. Let `repo.signature()` propagate its error as
`DomainError::Internal`. Apply the same removal to the merge-commit path in
`branch.rs` and the stash path in `stash.rs`.

### New frontend component — `src/components/git/GitIdentityDialog.tsx`

A shadcn `Dialog` with:
- **Name** field (required)
- **Email** field (required, must contain `@`)
- **"Save & Commit"** button — disabled until both fields are non-empty
- **Cancel** button — aborts commit, no side effects
- One-line explanation: *"Git needs your name and email to record commit authorship."*

No "save globally" option — repo-local only.

### Frontend change — `src/components/git/GitCommitForm.tsx`

Before calling `git_commit`, check identity:

```
if git_get_identity returns empty name or email:
  open GitIdentityDialog
  on cancel: abort
  on confirm: call git_set_identity, then git_commit
else:
  call git_commit directly
```

## Data Flow

```
User clicks "Commit"
  │
  ▼
GitCommitForm → git_get_identity(collectionPath)
  │
  ├─ name & email present ──────────────────────► git_commit → done
  │
  └─ name or email empty
       ▼
     GitIdentityDialog
       ├─ cancel → abort, close dialog
       └─ confirm
            ▼
          git_set_identity(path, name, email)
          writes user.name + user.email to .git/config
            ▼
          git_commit(path, message)
          repo.signature() succeeds (reads .git/config)
            ▼
          CommitInfo returned → UI updates
```

`git_get_identity` reads config in standard git2 resolution order:
repo-local `.git/config` → `~/.gitconfig` → system config. Users with a
global gitconfig never see the dialog.

## Error Handling

| Scenario | Behaviour |
|---|---|
| `git_get_identity` fails (repo unreadable) | Treat as "identity unknown", show dialog (safe degradation) |
| `git_set_identity` fails (read-only repo) | Surface as toast, abort commit |
| `repo.signature()` still fails after write | Surface as commit error toast (pathological case) |
| User cancels dialog | Abort commit silently, form remains ready |

## Testing

- **`rocket-git` unit tests** (`staging.rs`): one test confirming `commit()` succeeds after identity is written; one confirming it returns `DomainError::Internal` when config has no identity (no fallback).
- **`git_get_identity` / `git_set_identity`**: tested with `tempfile::TempDir` repos following existing test patterns in `git2_service.rs`.
- **Frontend**: manual verification — trigger a commit on a repo with no `user.name`/`user.email` in `.git/config`, confirm dialog appears, confirm identity is persisted, confirm subsequent commit skips dialog.

## Files Changed

| File | Change |
|---|---|
| `src-tauri/src/commands/git.rs` | Add `GitIdentity`, `git_get_identity`, `git_set_identity` |
| `src-tauri/src/lib.rs` | Register the two new commands in `invoke_handler` |
| `crates/rocket-git/src/git2_service/staging.rs` | Remove `"RocketAPI User"` fallback in `commit()` |
| `crates/rocket-git/src/git2_service/branch.rs` | Remove `"RocketAPI User"` fallback in merge commit |
| `crates/rocket-git/src/git2_service/stash.rs` | Remove `"RocketAPI User"` fallback in stash |
| `src/components/git/GitIdentityDialog.tsx` | New dialog component |
| `src/components/git/GitCommitForm.tsx` | Add identity check before commit |
