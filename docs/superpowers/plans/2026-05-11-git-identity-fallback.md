# Git Identity Fallback Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the hardcoded `"RocketAPI User"` git commit author fallback with a one-time dialog that prompts the user for their name and email, writes it to the repo-local `.git/config`, and never appears again for that repo.

**Architecture:** Two new Tauri commands (`git_get_identity`, `git_set_identity`) read and write `user.name`/`user.email` via `git2::Config`. Before committing, `GitCommitForm` checks identity and shows `GitIdentityDialog` if missing. The backend `commit()` / merge / stash fallback is removed entirely.

**Tech Stack:** Rust / git2, Tauri IPC, React / TypeScript, shadcn/ui Dialog + Input + Label + Button, lucide-react

---

## File Map

| File | Action |
|---|---|
| `src-tauri/src/commands/git.rs` | Add `GitIdentity` struct + `git_get_identity` + `git_set_identity` commands |
| `src-tauri/src/lib.rs` | Register the two new commands in `invoke_handler` |
| `crates/rocket-git/src/git2_service/staging.rs` | Remove `"RocketAPI User"` fallback in `commit()` |
| `crates/rocket-git/src/git2_service/branch.rs` | Remove `"RocketAPI User"` fallback in merge commit |
| `crates/rocket-git/src/git2_service/stash.rs` | Remove `"RocketAPI User"` fallback in `stash_save()` |
| `src/lib/tauri-api.ts` | Add `GitIdentity` type + `gitGetIdentity` + `gitSetIdentity` wrappers |
| `src/components/git/GitIdentityDialog.tsx` | New dialog component |
| `src/components/git/GitCommitForm.tsx` | Add identity check before commit |

---

## Task 1: Add Tauri commands for reading and writing git identity

**Files:**
- Modify: `src-tauri/src/commands/git.rs`

- [ ] **Step 1: Add the `GitIdentity` struct and two commands**

  Open `src-tauri/src/commands/git.rs`. Add directly after the `use` block at the top (before line 10, after line 8):

  ```rust
  #[derive(Debug, Serialize, Deserialize)]
  #[serde(rename_all = "camelCase")]
  pub struct GitIdentity {
      pub name: String,
      pub email: String,
  }
  ```

  Then add these two functions anywhere after the existing command functions (e.g. after `load_git_credentials` at the end of the file):

  ```rust
  /// Read user.name and user.email from the repo's git config (local → global → system).
  /// Returns empty strings when the values are unset — never errors on a missing entry.
  #[tauri::command]
  pub fn git_get_identity(path: String) -> Result<GitIdentity, DomainError> {
      let repo = git2::Repository::open(&path)
          .map_err(|e| DomainError::Internal(e.to_string()))?;
      let cfg = repo.config()
          .map_err(|e| DomainError::Internal(e.to_string()))?;
      let name = cfg.get_string("user.name").unwrap_or_default();
      let email = cfg.get_string("user.email").unwrap_or_default();
      Ok(GitIdentity { name, email })
  }

  /// Write user.name and user.email to the repo-local .git/config.
  #[tauri::command]
  pub fn git_set_identity(path: String, name: String, email: String) -> Result<(), DomainError> {
      let repo = git2::Repository::open(&path)
          .map_err(|e| DomainError::Internal(e.to_string()))?;
      let mut cfg = repo.config()
          .map_err(|e| DomainError::Internal(e.to_string()))?;
      // open_level gives us the repo-local config only, so we never write to ~/.gitconfig.
      let mut local = cfg.open_level(git2::ConfigLevel::Local)
          .map_err(|e| DomainError::Internal(e.to_string()))?;
      local.set_str("user.name", &name)
          .map_err(|e| DomainError::Internal(e.to_string()))?;
      local.set_str("user.email", &email)
          .map_err(|e| DomainError::Internal(e.to_string()))?;
      Ok(())
  }
  ```

  Note: `git2` is already a dependency of `rocket-git`; in `src-tauri` it is not a direct dep but `rocket-git` re-exports the repo type transitively. If `cargo check` reports `git2` not in scope, add `use git2;` or use `rocket_git`'s re-export. Check with `cargo check -p src-tauri` first.

- [ ] **Step 2: Verify it compiles**

  ```bash
  cargo check -p src-tauri
  ```

  Expected: no errors. If `git2` is not in scope in `src-tauri`, add to `src-tauri/Cargo.toml`:
  ```toml
  git2 = { version = "*", features = ["ssh"] }
  ```
  and re-run.

- [ ] **Step 3: Register the new commands in `invoke_handler`**

  Open `src-tauri/src/lib.rs`. Find line 358 (`commands::git::load_git_credentials,`) and add after it:

  ```rust
  commands::git::git_get_identity,
  commands::git::git_set_identity,
  ```

- [ ] **Step 4: Verify compilation after registration**

  ```bash
  cargo check -p src-tauri
  ```

  Expected: no errors.

- [ ] **Step 5: Commit**

  ```bash
  git add src-tauri/src/commands/git.rs src-tauri/src/lib.rs
  git commit -m "feat(git): add git_get_identity and git_set_identity Tauri commands"
  ```

---

## Task 2: Remove the "RocketAPI User" fallback from the Rust backend

**Files:**
- Modify: `crates/rocket-git/src/git2_service/staging.rs:95-97`
- Modify: `crates/rocket-git/src/git2_service/branch.rs:277-280`
- Modify: `crates/rocket-git/src/git2_service/stash.rs:97-100`

- [ ] **Step 1: Remove fallback in `staging.rs` (commit path)**

  In `crates/rocket-git/src/git2_service/staging.rs`, replace lines 95–97:

  ```rust
  // BEFORE
  let sig = repo.signature().or_else(|_|
      git2::Signature::now("RocketAPI User", "user@rocketapi.local")
  ).map_err(|e| DomainError::Internal(e.to_string()))?;
  ```

  With:

  ```rust
  // AFTER
  let sig = repo.signature()
      .map_err(|e| DomainError::Internal(e.to_string()))?;
  ```

- [ ] **Step 2: Remove fallback in `branch.rs` (merge commit path)**

  In `crates/rocket-git/src/git2_service/branch.rs`, replace lines 277–280:

  ```rust
  // BEFORE
  let sig = repo
      .signature()
      .or_else(|_| git2::Signature::now("RocketAPI User", "user@rocketapi.local"))
      .map_err(|e| DomainError::Internal(e.to_string()))?;
  ```

  With:

  ```rust
  // AFTER
  let sig = repo
      .signature()
      .map_err(|e| DomainError::Internal(e.to_string()))?;
  ```

- [ ] **Step 3: Remove fallback in `stash.rs`**

  In `crates/rocket-git/src/git2_service/stash.rs`, replace lines 97–100:

  ```rust
  // BEFORE
  let sig = repo
      .signature()
      .or_else(|_| git2::Signature::now("RocketAPI User", "user@rocketapi.local"))
      .map_err(|e| DomainError::Internal(e.to_string()))?;
  ```

  With:

  ```rust
  // AFTER
  let sig = repo
      .signature()
      .map_err(|e| DomainError::Internal(e.to_string()))?;
  ```

- [ ] **Step 4: Verify**

  ```bash
  cargo check -p rocket-git
  ```

  Expected: no errors.

- [ ] **Step 5: Commit**

  ```bash
  git add crates/rocket-git/src/git2_service/staging.rs \
          crates/rocket-git/src/git2_service/branch.rs \
          crates/rocket-git/src/git2_service/stash.rs
  git commit -m "fix(git): remove hardcoded RocketAPI User fallback signature"
  ```

---

## Task 3: Add Rust tests for commit with and without identity

**Files:**
- Modify: `crates/rocket-git/src/git2_service/mod.rs` (test module)

- [ ] **Step 1: Write the failing test — commit fails without identity**

  Open `crates/rocket-git/src/git2_service/mod.rs`. In the `#[cfg(test)]` block, add this test after the existing tests:

  ```rust
  #[test]
  fn commit_fails_without_identity() {
      // Create a repo with NO user.name/user.email in its config.
      let dir = TempDir::new().unwrap();
      let path = dir.path().to_string_lossy().to_string();
      let repo = git2::Repository::init(&path).unwrap();
      repo.set_head("refs/heads/main").ok();

      // Make an initial commit using an explicit signature (bypassing repo.signature()).
      let sig = git2::Signature::now("Setup", "setup@test.com").unwrap();
      fs::write(dir.path().join("a.bru"), "v1").unwrap();
      let mut idx = repo.index().unwrap();
      idx.add_path(std::path::Path::new("a.bru")).unwrap();
      idx.write().unwrap();
      let tree_id = idx.write_tree().unwrap();
      let tree = repo.find_tree(tree_id).unwrap();
      repo.commit(Some("refs/heads/main"), &sig, &sig, "initial", &tree, &[]).unwrap();
      repo.set_head("refs/heads/main").unwrap();
      drop(repo);

      // Stage a change.
      fs::write(dir.path().join("a.bru"), "v2").unwrap();
      Git2Service::new().stage(&path, &["a.bru"]).unwrap();

      // Commit should fail — no identity in git config, no fallback.
      let result = Git2Service::new().commit(&path, "second commit");
      assert!(result.is_err(), "expected error when identity is missing, got: {:?}", result);
  }
  ```

- [ ] **Step 2: Run the test to verify it fails (before the fix lands)**

  ```bash
  cargo test -p rocket-git commit_fails_without_identity -- --nocapture
  ```

  Expected: FAIL — because `setup_repo()` sets a signature via `git2::Signature::now` directly in the test, not via `repo.config()`. The test repo has no `user.name` in config. After Task 2's removal of the fallback, this test should pass. If Task 2 is already done, the test should now **pass** — which is what we want.

- [ ] **Step 3: Write the passing test — commit succeeds with identity**

  In the same test block, add:

  ```rust
  #[test]
  fn commit_succeeds_with_identity() {
      let dir = TempDir::new().unwrap();
      let path = dir.path().to_string_lossy().to_string();
      let repo = git2::Repository::init(&path).unwrap();
      repo.set_head("refs/heads/main").ok();

      // Write identity into the repo-local config.
      let mut cfg = repo.config().unwrap();
      let mut local = cfg.open_level(git2::ConfigLevel::Local).unwrap();
      local.set_str("user.name", "Alice").unwrap();
      local.set_str("user.email", "alice@example.com").unwrap();
      drop(local);
      drop(cfg);

      // Initial commit via explicit sig (identity not needed for this step).
      let sig = git2::Signature::now("Setup", "setup@test.com").unwrap();
      fs::write(dir.path().join("a.bru"), "v1").unwrap();
      let mut idx = repo.index().unwrap();
      idx.add_path(std::path::Path::new("a.bru")).unwrap();
      idx.write().unwrap();
      let tree_id = idx.write_tree().unwrap();
      let tree = repo.find_tree(tree_id).unwrap();
      repo.commit(Some("refs/heads/main"), &sig, &sig, "initial", &tree, &[]).unwrap();
      repo.set_head("refs/heads/main").unwrap();
      drop(repo);

      // Stage a change and commit via the service — should use config identity.
      fs::write(dir.path().join("a.bru"), "v2").unwrap();
      Git2Service::new().stage(&path, &["a.bru"]).unwrap();
      let result = Git2Service::new().commit(&path, "second commit");
      assert!(result.is_ok(), "commit failed: {:?}", result);
      let info = result.unwrap();
      assert_eq!(info.author, "Alice");
      assert_eq!(info.author_email, "alice@example.com");
  }
  ```

- [ ] **Step 4: Run both new tests**

  ```bash
  cargo test -p rocket-git commit_fails_without_identity commit_succeeds_with_identity -- --nocapture
  ```

  Expected: both PASS.

- [ ] **Step 5: Commit**

  ```bash
  git add crates/rocket-git/src/git2_service/mod.rs
  git commit -m "test(git): verify commit requires identity and uses config when present"
  ```

---

## Task 4: Add Rust tests for `git_get_identity` and `git_set_identity`

**Files:**
- Modify: `src-tauri/src/commands/git.rs` (add `#[cfg(test)]` block)

- [ ] **Step 1: Write tests**

  At the end of `src-tauri/src/commands/git.rs`, add:

  ```rust
  #[cfg(test)]
  mod tests {
      use super::*;
      use tempfile::TempDir;

      fn init_repo(dir: &TempDir) -> String {
          let path = dir.path().to_string_lossy().to_string();
          git2::Repository::init(&path).unwrap();
          path
      }

      #[test]
      fn get_identity_returns_empty_when_unset() {
          let dir = TempDir::new().unwrap();
          let path = init_repo(&dir);
          let identity = git_get_identity(path).unwrap();
          assert_eq!(identity.name, "");
          assert_eq!(identity.email, "");
      }

      #[test]
      fn set_and_get_identity_roundtrip() {
          let dir = TempDir::new().unwrap();
          let path = init_repo(&dir);
          git_set_identity(path.clone(), "Bob".into(), "bob@example.com".into()).unwrap();
          let identity = git_get_identity(path).unwrap();
          assert_eq!(identity.name, "Bob");
          assert_eq!(identity.email, "bob@example.com");
      }

      #[test]
      fn set_identity_writes_to_local_config_only() {
          let dir = TempDir::new().unwrap();
          let path = init_repo(&dir);
          git_set_identity(path.clone(), "Local".into(), "local@test.com".into()).unwrap();
          // Read config at local level only to confirm it's there.
          let repo = git2::Repository::open(&path).unwrap();
          let cfg = repo.config().unwrap();
          let local = cfg.open_level(git2::ConfigLevel::Local).unwrap();
          assert_eq!(local.get_string("user.name").unwrap(), "Local");
          assert_eq!(local.get_string("user.email").unwrap(), "local@test.com");
      }
  }
  ```

  Note: `src-tauri/Cargo.toml` must have `tempfile` as a dev-dependency. Check with:
  ```bash
  grep "tempfile" src-tauri/Cargo.toml
  ```
  If missing, add to `[dev-dependencies]`:
  ```toml
  tempfile = "3"
  ```

- [ ] **Step 2: Run tests**

  ```bash
  cargo test -p src-tauri get_identity set_and_get_identity set_identity_writes -- --nocapture
  ```

  Expected: all three PASS.

- [ ] **Step 3: Commit**

  ```bash
  git add src-tauri/src/commands/git.rs src-tauri/Cargo.toml
  git commit -m "test(git): add unit tests for git_get_identity and git_set_identity"
  ```

---

## Task 5: Add TypeScript wrappers in `tauri-api.ts`

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Add `GitIdentity` type**

  In `src/lib/tauri-api.ts`, find the block of `interface` / type definitions (around line 268 where `CommitInfo` is defined). Add after `CommitInfo`:

  ```typescript
  export interface GitIdentity {
    name: string;
    email: string;
  }
  ```

- [ ] **Step 2: Add the two wrapper functions**

  Find `gitSetRemoteUrl` (around line 724). Add after it:

  ```typescript
  export const gitGetIdentity = (collectionPath: string) =>
    invoke<GitIdentity>('git_get_identity', { path: collectionPath });

  export const gitSetIdentity = (collectionPath: string, name: string, email: string) =>
    invoke<void>('git_set_identity', { path: collectionPath, name, email });
  ```

  Note: the Tauri command parameter is `path: String` (not `collectionPath`) — the invoke arg key must match the Rust function parameter name exactly.

- [ ] **Step 3: TypeScript check**

  ```bash
  yarn tsc --noEmit
  ```

  Expected: no errors.

- [ ] **Step 4: Commit**

  ```bash
  git add src/lib/tauri-api.ts
  git commit -m "feat(git): add gitGetIdentity and gitSetIdentity TypeScript wrappers"
  ```

---

## Task 6: Build `GitIdentityDialog` component

**Files:**
- Create: `src/components/git/GitIdentityDialog.tsx`

- [ ] **Step 1: Create the component**

  ```tsx
  import { useState } from 'react';
  import { Button } from '@/components/ui/button';
  import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
  } from '@/components/ui/dialog';
  import { Input } from '@/components/ui/input';
  import { Label } from '@/components/ui/label';

  interface Props {
    open: boolean;
    onConfirm: (name: string, email: string) => void;
    onCancel: () => void;
  }

  export function GitIdentityDialog({ open, onConfirm, onCancel }: Props) {
    const [name, setName] = useState('');
    const [email, setEmail] = useState('');

    const isValid = name.trim().length > 0 && email.includes('@');

    const handleConfirm = () => {
      if (!isValid) return;
      onConfirm(name.trim(), email.trim());
    };

    const handleOpenChange = (nextOpen: boolean) => {
      if (!nextOpen) onCancel();
    };

    return (
      <Dialog open={open} onOpenChange={handleOpenChange}>
        <DialogContent className='w-auto min-w-[22rem] max-w-[min(90vw,_36rem)]'>
          <DialogHeader>
            <DialogTitle>Git Author Identity</DialogTitle>
          </DialogHeader>

          <p className='text-sm text-muted-foreground'>
            Git needs your name and email to record commit authorship.
          </p>

          <div className='space-y-3'>
            <div>
              <Label htmlFor='git-identity-name' className='text-sm'>
                Name
              </Label>
              <Input
                id='git-identity-name'
                value={name}
                onChange={(e) => setName(e.target.value)}
                className='h-8 text-sm'
                placeholder='Your Name'
                autoComplete='name'
                autoFocus
              />
            </div>

            <div>
              <Label htmlFor='git-identity-email' className='text-sm'>
                Email
              </Label>
              <Input
                id='git-identity-email'
                type='email'
                value={email}
                onChange={(e) => setEmail(e.target.value)}
                className='h-8 text-sm'
                placeholder='you@example.com'
                autoComplete='email'
                onKeyDown={(e) => {
                  if (e.key === 'Enter' && isValid) handleConfirm();
                }}
              />
            </div>

            <div className='flex gap-2'>
              <Button
                onClick={handleConfirm}
                disabled={!isValid}
                className='flex-1'
                size='sm'
              >
                Save &amp; Commit
              </Button>
              <Button
                onClick={onCancel}
                variant='outline'
                className='flex-1'
                size='sm'
              >
                Cancel
              </Button>
            </div>
          </div>
        </DialogContent>
      </Dialog>
    );
  }
  ```

- [ ] **Step 2: TypeScript check**

  ```bash
  yarn tsc --noEmit
  ```

  Expected: no errors.

- [ ] **Step 3: Commit**

  ```bash
  git add src/components/git/GitIdentityDialog.tsx
  git commit -m "feat(git): add GitIdentityDialog for one-time author identity prompt"
  ```

---

## Task 7: Wire identity check into `GitCommitForm`

**Files:**
- Modify: `src/components/git/GitCommitForm.tsx`

- [ ] **Step 1: Update `GitCommitForm` to check identity before committing**

  Replace the entire contents of `src/components/git/GitCommitForm.tsx` with:

  ```tsx
  import { Check, Loader2 } from 'lucide-react';
  import { useState } from 'react';
  import { Button } from '@/components/ui/button';
  import { Textarea } from '@/components/ui/textarea';
  import { gitGetIdentity, gitSetIdentity } from '@/lib/tauri-api';
  import { useGitStore } from '@/stores/git-store';
  import { GitIdentityDialog } from './GitIdentityDialog';

  export function GitCommitForm() {
    const [message, setMessage] = useState('');
    const [committing, setCommitting] = useState(false);
    const [showIdentityDialog, setShowIdentityDialog] = useState(false);
    const { status, commitChanges, collectionPath } = useGitStore();

    const stagedCount = status?.files.filter((f) => f.staged).length ?? 0;

    const doCommit = async () => {
      setCommitting(true);
      try {
        await commitChanges(message.trim());
        setMessage('');
      } finally {
        setCommitting(false);
      }
    };

    const handleCommit = async () => {
      if (!message.trim() || stagedCount === 0) return;
      if (!collectionPath) return;

      // Check identity; treat any error as "identity unknown" — show dialog.
      let identityMissing = false;
      try {
        const identity = await gitGetIdentity(collectionPath);
        identityMissing = !identity.name.trim() || !identity.email.trim();
      } catch {
        identityMissing = true;
      }

      if (identityMissing) {
        setShowIdentityDialog(true);
        return;
      }

      await doCommit();
    };

    const handleIdentityConfirm = async (name: string, email: string) => {
      setShowIdentityDialog(false);
      if (!collectionPath) return;
      try {
        await gitSetIdentity(collectionPath, name, email);
      } catch (e) {
        useGitStore.setState({ error: `Failed to save git identity: ${String(e)}` });
        return;
      }
      await doCommit();
    };

    const handleIdentityCancel = () => {
      setShowIdentityDialog(false);
    };

    return (
      <>
        <GitIdentityDialog
          open={showIdentityDialog}
          onConfirm={handleIdentityConfirm}
          onCancel={handleIdentityCancel}
        />

        <div className='space-y-2'>
          <Textarea
            placeholder='Commit message... (Ctrl+Enter to commit)'
            value={message}
            onChange={(e) => setMessage(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) void handleCommit();
            }}
            className='text-sm min-h-[60px] resize-none'
            disabled={committing}
            aria-label='Commit message'
          />
          {stagedCount === 0 && message.trim().length > 0 && (
            <p className='text-xs text-muted-foreground/70'>No files staged</p>
          )}
          <Button
            onClick={handleCommit}
            disabled={!message.trim() || stagedCount === 0 || committing}
            className='w-full'
            size='sm'
          >
            {committing ? (
              <Loader2 className='h-3.5 w-3.5 animate-spin' />
            ) : (
              <Check className='h-3.5 w-3.5' />
            )}
            {committing
              ? 'Committing...'
              : `Commit${stagedCount > 0 ? ` ${stagedCount} file${stagedCount !== 1 ? 's' : ''}` : ''}`}
          </Button>
        </div>
      </>
    );
  }
  ```

  Key changes from original:
  - Imports `gitGetIdentity`, `gitSetIdentity` from `tauri-api`
  - Imports `GitIdentityDialog`
  - Reads `collectionPath` from store (it was already in the store at line 46)
  - `handleCommit` checks identity first, opens dialog if missing
  - `handleIdentityConfirm` writes identity then calls `doCommit`

- [ ] **Step 2: TypeScript check**

  ```bash
  yarn tsc --noEmit
  ```

  Expected: no errors.

- [ ] **Step 3: Biome lint**

  ```bash
  yarn check
  ```

  Expected: no errors. If lint errors, run `yarn lint` to auto-fix, then re-check.

- [ ] **Step 4: Commit**

  ```bash
  git add src/components/git/GitCommitForm.tsx
  git commit -m "feat(git): prompt for author identity before committing when git config is unset"
  ```

---

## Task 8: Full verification pass

- [ ] **Step 1: Run all Rust tests**

  ```bash
  cargo test -p rocket-git
  cargo test -p src-tauri
  ```

  Expected: all tests pass.

- [ ] **Step 2: Full TypeScript + lint check**

  ```bash
  yarn tsc --noEmit && yarn check
  ```

  Expected: no errors.

- [ ] **Step 3: Manual smoke test**

  1. Run `yarn tauri dev`
  2. Open a collection that is a git repo
  3. In a terminal: `git config --unset user.name` and `git config --unset user.email` inside that collection's `.git/` (or use a fresh cloned repo with no local config and no global config)
  4. Stage a file and click Commit — the `GitIdentityDialog` should appear
  5. Enter a name and email, click "Save & Commit" — commit should succeed with correct author
  6. Make another change, stage it, click Commit — dialog should NOT appear again

- [ ] **Step 4: Final commit if any fixups needed**

  If any fixups were needed during smoke test, commit them:

  ```bash
  git add -p
  git commit -m "fix(git): <describe fixup>"
  ```
