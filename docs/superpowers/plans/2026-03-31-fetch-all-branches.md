# Fetch All Branches Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After git fetch, show all remote branches in BranchSelector and allow one-click checkout that creates a local tracking branch.

**Architecture:** Add `checkout_remote_branch` to the Rust `GitService` trait and `Git2Service`, wire through Tauri command and TS API, refresh branches after fetch in git-store, and show remote branches in BranchSelector with auto-checkout on click.

**Tech Stack:** Rust (git2 crate), TypeScript (React, Zustand)

---

### Task 1: Add `checkout_remote_branch` to Rust backend

**Files:**
- Modify: `crates/rocket-git/src/service.rs:42-46`
- Modify: `crates/rocket-git/src/git2_service.rs:640-647`

- [ ] **Step 1: Add trait method**

In `crates/rocket-git/src/service.rs`, add after `fn switch_branch(...)` (line 43):

```rust
fn checkout_remote_branch(&self, path: &str, remote_branch: &str) -> DomainResult<()>;
```

- [ ] **Step 2: Implement `checkout_remote_branch` in Git2Service**

In `crates/rocket-git/src/git2_service.rs`, add after the `switch_branch` method (after line 647):

```rust
fn checkout_remote_branch(&self, path: &str, remote_branch: &str) -> DomainResult<()> {
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
    let branch = repo
        .branch(&local_name, &commit, false)
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
```

- [ ] **Step 3: Add test**

Add at the end of the `#[cfg(test)]` module in `git2_service.rs`:

```rust
#[test]
fn checkout_remote_branch_creates_local_tracking() {
    let (_dir, path) = setup_repo();
    let repo = Repository::open(&path).unwrap();
    let sig = git2::Signature::now("Test", "test@test.com").unwrap();

    // Create a bare remote and push main.
    let remote_dir = TempDir::new().unwrap();
    let remote_path = remote_dir.path().to_string_lossy().to_string();
    Repository::init_bare(&remote_path).unwrap();

    let mut remote = repo.remote("origin", &remote_path).unwrap();
    remote
        .push(&["refs/heads/main:refs/heads/main"], None)
        .unwrap();

    // Create a feature branch on the bare remote by pushing from a clone.
    let clone_dir = TempDir::new().unwrap();
    let clone_path = clone_dir.path().to_string_lossy().to_string();
    let clone_repo = Repository::clone(&remote_path, &clone_path).unwrap();
    let clone_head = clone_repo.head().unwrap().peel_to_commit().unwrap();
    clone_repo.branch("feature-x", &clone_head, false).unwrap();
    clone_repo
        .find_remote("origin")
        .unwrap()
        .push(&["refs/heads/feature-x:refs/heads/feature-x"], None)
        .unwrap();

    // Fetch in our original repo so we get origin/feature-x.
    let svc = Git2Service::new();
    let creds = GitCredentials::default();
    svc.fetch(&path, "origin", &creds).unwrap();

    // Checkout the remote branch.
    svc.checkout_remote_branch(&path, "origin/feature-x").unwrap();

    // Verify local branch exists and is checked out.
    let status = svc.status(&path).unwrap();
    assert_eq!(status.branch, "feature-x");

    // Verify upstream is set.
    let branches = svc.branches(&path).unwrap();
    let local = branches.local.iter().find(|b| b.name == "feature-x").unwrap();
    assert_eq!(local.upstream.as_deref(), Some("origin/feature-x"));
}
```

- [ ] **Step 4: Run tests**

Run: `cargo test -p rocket-git checkout_remote_branch`
Expected: PASS.

Run: `cargo test -p rocket-git`
Expected: All pass.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-git/src/service.rs crates/rocket-git/src/git2_service.rs
git commit -m "$(cat <<'EOF'
feat(git): add checkout_remote_branch to create local tracking branch

Creates a local branch from a remote-tracking ref, sets upstream
tracking, and switches HEAD to the new branch.
EOF
)"
```

---

### Task 2: Wire Tauri command and TypeScript API

**Files:**
- Modify: `src-tauri/src/commands/git.rs`
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Add Tauri command**

In `src-tauri/src/commands/git.rs`, add after the `git_switch_branch` command (~line 90):

```rust
#[tauri::command]
pub fn git_checkout_remote_branch(collection_path: String, name: String, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.checkout_remote_branch(&collection_path, &name)
}
```

- [ ] **Step 2: Register command in Tauri builder**

In `src-tauri/src/lib.rs`, find the `invoke_handler` macro where all git commands are registered. Add `git_checkout_remote_branch` to the list.

- [ ] **Step 3: Add TypeScript API function**

In `src/lib/tauri-api.ts`, add after `gitSwitchBranch` (~line 515):

```typescript
export const gitCheckoutRemoteBranch = (collectionPath: string, name: string) =>
  invoke<void>("git_checkout_remote_branch", { collectionPath, name });
```

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/commands/git.rs src-tauri/src/lib.rs src/lib/tauri-api.ts
git commit -m "$(cat <<'EOF'
feat: wire git_checkout_remote_branch through Tauri and TS API
EOF
)"
```

---

### Task 3: Refresh branches after fetch + add store method

**Files:**
- Modify: `src/stores/git-store.ts`

- [ ] **Step 1: Import the new API function**

In `src/stores/git-store.ts`, add `gitCheckoutRemoteBranch` to the import from `@/lib/tauri-api` (~line 2):

```typescript
import {
  // ... existing imports ...
  gitCheckoutRemoteBranch,
  // ... rest ...
} from '@/lib/tauri-api';
```

- [ ] **Step 2: Add `checkoutRemoteBranch` to the interface**

In the `GitState` interface, add after `switchBranch` (~line 74):

```typescript
checkoutRemoteBranch: (name: string) => Promise<void>;
```

- [ ] **Step 3: Add `refreshBranches()` after fetch**

In the `fetch` method (~line 462), add `await get().refreshBranches();` after `await get().refreshStatus();`:

```typescript
fetch: async (remote) => {
  const { collectionPath, credentials } = get();
  if (!collectionPath) return;
  if (!credentials) { set({ showCredentialsDialog: true }); return; }
  try {
    await gitFetch(collectionPath, remote ?? 'origin', credentials);
    await get().refreshStatus();
    await get().refreshBranches();
  } catch (e) {
    set({ error: String(e) });
  }
},
```

- [ ] **Step 4: Implement `checkoutRemoteBranch` store method**

Add after the `switchBranch` method (~line 353):

```typescript
checkoutRemoteBranch: async (name) => {
  const { collectionPath } = get();
  if (!collectionPath) return;
  try {
    await gitCheckoutRemoteBranch(collectionPath, name);
    await get().refreshStatus();
    await get().refreshBranches();
  } catch (e) {
    set({ error: String(e) });
  }
},
```

- [ ] **Step 5: Commit**

```bash
git add src/stores/git-store.ts
git commit -m "$(cat <<'EOF'
feat(frontend): refresh branches after fetch, add checkoutRemoteBranch

Fetch now calls refreshBranches() so remote branches appear immediately.
New store method wraps the checkout_remote_branch Tauri command.
EOF
)"
```

---

### Task 4: Show remote branches in BranchSelector

**Files:**
- Modify: `src/components/git/BranchSelector.tsx`

- [ ] **Step 1: Read the current file**

Read `src/components/git/BranchSelector.tsx` in full.

- [ ] **Step 2: Add `checkoutRemoteBranch` to destructured store**

Update the destructuring at line 13:

```typescript
const { branches, switchBranch, createBranch, deleteBranch, mergeBranch, checkoutRemoteBranch, status } = useGitStore();
```

- [ ] **Step 3: Add filtered remote branches**

After the existing `filtered` const (~line 17), add:

```typescript
const filteredRemote = branches.remote
  .filter((b) => {
    // Exclude HEAD pointer and branches that already have a local counterpart.
    if (b.name.endsWith('/HEAD')) return false;
    const localName = b.name.split('/').slice(1).join('/');
    return (
      !branches.local.some((l) => l.name === localName) &&
      b.name.toLowerCase().includes(search.toLowerCase())
    );
  });
```

- [ ] **Step 4: Add remote branches section in the dropdown**

After the local branches `</div>` (the `max-h-48 overflow-y-auto` div, before the `<Separator />`), replace it so both local and remote branches share the scrollable area:

Find the existing scrollable div:
```tsx
<div className="max-h-48 overflow-y-auto p-1">
  {filtered.map((branch) => (
    ...
  ))}
</div>
```

Replace with:
```tsx
<div className="max-h-48 overflow-y-auto p-1">
  {filtered.map((branch) => (
    <div
      key={branch.name}
      className="group flex items-center gap-1.5 rounded px-2 py-1 hover:bg-muted/50 cursor-pointer text-sm"
      onClick={() => {
        if (!branch.isHead) switchBranch(branch.name);
        setOpen(false);
      }}
    >
      {branch.isHead && <Check className="h-3.5 w-3.5 text-primary" />}
      {!branch.isHead && <span className="w-3.5" />}
      <span className="truncate flex-1">{branch.name}</span>
      {!branch.isHead && (
        <div className="flex gap-0.5 opacity-0 group-hover:opacity-100">
          <Button
            variant="ghost"
            size="icon"
            className="h-5 w-5"
            onClick={(e) => {
              e.stopPropagation();
              mergeBranch(branch.name);
              setOpen(false);
            }}
            title="Merge into current"
          >
            <GitBranch className="h-3.5 w-3.5 text-muted-foreground" />
          </Button>
          <Button
            variant="ghost"
            size="icon"
            className="h-5 w-5 text-destructive"
            onClick={(e) => {
              e.stopPropagation();
              deleteBranch(branch.name);
            }}
            title="Delete branch"
          >
            <Trash2 className="h-3.5 w-3.5" />
          </Button>
        </div>
      )}
    </div>
  ))}
  {filteredRemote.length > 0 && (
    <>
      <div className="px-2 py-1 text-xs text-muted-foreground font-medium mt-1">
        Remote
      </div>
      {filteredRemote.map((branch) => {
        const localName = branch.name.split('/').slice(1).join('/');
        return (
          <div
            key={branch.name}
            className="flex items-center gap-1.5 rounded px-2 py-1 hover:bg-muted/50 cursor-pointer text-sm"
            onClick={() => {
              checkoutRemoteBranch(branch.name);
              setOpen(false);
            }}
          >
            <span className="w-3.5" />
            <span className="truncate flex-1 text-muted-foreground">{localName}</span>
          </div>
        );
      })}
    </>
  )}
</div>
```

- [ ] **Step 5: Verify TypeScript compiles**

Run: `yarn tsc --noEmit`
Expected: No errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/git/BranchSelector.tsx
git commit -m "$(cat <<'EOF'
feat(frontend): show remote branches in BranchSelector with auto-checkout

Remote branches appear below local branches after fetch. Clicking one
creates a local tracking branch and switches to it. Branches that
already have a local counterpart are hidden from the remote section.
EOF
)"
```

---

## Dependency Graph

```
Task 1 (Rust backend) → Task 2 (Tauri + TS API) → Task 3 (store) → Task 4 (UI)
```

All tasks are sequential.
