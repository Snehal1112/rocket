# Plan 12: Git High-Priority Functional Gaps

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix three high-priority functional gaps: per-file staging buttons, BranchSelector rendering in GitPanel, and abort merge capability.

**Architecture:** Per-file staging adds hover-revealed action buttons to each file row in `GitFileList`. BranchSelector is added to GitPanel's left panel header. Abort merge requires a new method across all layers (GitService trait → Git2Service → GitAppService → Tauri command → frontend store → ConflictResolver UI).

**Tech Stack:** React, TypeScript, Rust, Tauri, git2, Zustand

---

## Chunk 1: Per-file staging buttons

### Task 1: Add per-file stage/unstage/discard buttons to `GitFileList`

**Files:**
- Modify: `src/components/git/GitFileList.tsx`

- [ ] **Step 1: Add `stageFiles`, `unstageFiles`, `discardFiles` to the store destructuring**

In `src/components/git/GitFileList.tsx`, update the store connection (line 20):

```typescript
// Old
const { status, stageAll, unstageAll, discardFiles } = useGitStore();

// New
const { status, stageFiles, stageAll, unstageFiles, unstageAll, discardFiles } = useGitStore();
```

- [ ] **Step 2: Add per-file action buttons to unstaged file rows**

Replace the unstaged file row rendering (lines 79–90) with:

```tsx
{/* Unstaged file rows. */}
{unstaged.map((file) => (
  <div
    key={file.path}
    className="group flex items-center justify-between px-2 py-1 rounded-md hover:bg-muted/50 cursor-pointer"
    onClick={() => onFileClick(file)}
  >
    <span className="text-sm truncate flex-1 min-w-0">{file.path}</span>
    <div className="flex items-center gap-0.5">
      <span className={`text-xs font-medium shrink-0 ${GIT_STATUS_CONFIG[file.status].className}`}>
        {GIT_STATUS_CONFIG[file.status].label}
      </span>
      <div className="flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-5 w-5"
              onClick={(e) => { e.stopPropagation(); discardFiles([file.path]); }}
            >
              <RotateCcw className="h-3 w-3" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Discard</TooltipContent>
        </Tooltip>
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-5 w-5"
              onClick={(e) => { e.stopPropagation(); stageFiles([file.path]); }}
            >
              <Plus className="h-3 w-3" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Stage</TooltipContent>
        </Tooltip>
      </div>
    </div>
  </div>
))}
```

- [ ] **Step 3: Add per-file action buttons to staged file rows**

Replace the staged file row rendering (lines 117–128) with:

```tsx
{/* Staged file rows. */}
{staged.map((file) => (
  <div
    key={file.path}
    className="group flex items-center justify-between px-2 py-1 rounded-md hover:bg-muted/50 cursor-pointer"
    onClick={() => onFileClick(file)}
  >
    <span className="text-sm truncate flex-1 min-w-0">{file.path}</span>
    <div className="flex items-center gap-0.5">
      <span className={`text-xs font-medium shrink-0 ${GIT_STATUS_CONFIG[file.status].className}`}>
        {GIT_STATUS_CONFIG[file.status].label}
      </span>
      <div className="flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
        <Tooltip>
          <TooltipTrigger asChild>
            <Button
              variant="ghost"
              size="icon"
              className="h-5 w-5"
              onClick={(e) => { e.stopPropagation(); unstageFiles([file.path]); }}
            >
              <Minus className="h-3 w-3" />
            </Button>
          </TooltipTrigger>
          <TooltipContent>Unstage</TooltipContent>
        </Tooltip>
      </div>
    </div>
  </div>
))}
```

- [ ] **Step 4: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 5: Commit**

```bash
git add src/components/git/GitFileList.tsx
git commit -m "feat(frontend): add per-file stage/unstage/discard buttons to GitFileList"
```

## Chunk 2: BranchSelector in GitPanel

### Task 2: Render `BranchSelector` in GitPanel header

**Files:**
- Modify: `src/components/git/GitPanel.tsx`

- [ ] **Step 1: Add BranchSelector import**

In `src/components/git/GitPanel.tsx`, add the import:

```typescript
import { BranchSelector } from '@/components/git/BranchSelector';
```

- [ ] **Step 2: Add BranchSelector to the left panel header**

Find the collection name header section (~lines 108–114):

```tsx
{/* Collection name header */}
<div className="flex items-center gap-2 px-3 py-2.5 border-b border-border/70 shrink-0">
  <Package className="h-3.5 w-3.5 text-muted-foreground" />
  <span className="text-sm font-medium truncate">
    {collectionName}
  </span>
</div>
```

Replace with:

```tsx
{/* Collection name header with branch selector. */}
<div className="flex items-center gap-2 px-3 py-2.5 border-b border-border/70 shrink-0">
  <Package className="h-3.5 w-3.5 text-muted-foreground" />
  <span className="text-sm font-medium truncate flex-1">
    {collectionName}
  </span>
  <BranchSelector />
</div>
```

Note: The key changes are adding `flex-1` to the collection name span so it takes available space, and appending `<BranchSelector />` which renders as a compact button showing the current branch name.

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add src/components/git/GitPanel.tsx
git commit -m "feat(frontend): render BranchSelector in GitPanel header"
```

## Chunk 3: Abort merge

### Task 3: Add `abort_merge` to the Rust backend

**Files:**
- Modify: `crates/rocket-git/src/service.rs`
- Modify: `crates/rocket-git/src/git2_service.rs`

- [ ] **Step 1: Add `abort_merge` to the `GitService` trait**

In `crates/rocket-git/src/service.rs`, add a new method to the `GitService` trait, after `resolve_conflict`:

```rust
    fn abort_merge(&self, path: &str) -> DomainResult<()>;
```

- [ ] **Step 2: Implement `abort_merge` in `Git2Service`**

In `crates/rocket-git/src/git2_service.rs`, add the implementation after the `resolve_conflict` method. The implementation should:
1. Open the repo
2. Check if the repo is in a merge/conflict state
3. Reset the index and working directory to HEAD
4. Clean up the merge state

```rust
    fn abort_merge(&self, path: &str) -> DomainResult<()> {
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
```

- [ ] **Step 3: Verify Rust compiles**

Run: `cargo check -p rocket-git`
Expected: no errors

- [ ] **Step 4: Add a test for `abort_merge`**

In `crates/rocket-git/src/git2_service.rs`, add a test in the `#[cfg(test)]` module:

```rust
    #[test]
    fn abort_merge_resets_to_head() {
        let (dir, svc) = setup_repo();
        let path = dir.path().to_str().unwrap();

        // Create a branch with a conflicting change.
        svc.create_branch(path, "conflict-branch").unwrap();
        svc.switch_branch(path, "conflict-branch").unwrap();
        std::fs::write(dir.path().join("file.txt"), "conflict content").unwrap();
        svc.stage(path, &["file.txt"]).unwrap();
        svc.commit(path, "conflict commit").unwrap();

        // Switch back to main and make a different change to the same file.
        svc.switch_branch(path, "main").unwrap();
        std::fs::write(dir.path().join("file.txt"), "main content").unwrap();
        svc.stage(path, &["file.txt"]).unwrap();
        svc.commit(path, "main commit").unwrap();

        // Attempt merge — this may leave the repo in a conflicted state.
        let _ = svc.merge_branch(path, "conflict-branch");

        // Abort the merge.
        svc.abort_merge(path).unwrap();

        // Verify the repo is clean and on main.
        let status = svc.status(path).unwrap();
        assert!(status.is_clean, "Repo should be clean after abort");
        assert_eq!(status.branch, "main");
    }
```

- [ ] **Step 5: Run the test**

Run: `cargo test -p rocket-git abort_merge`
Expected: PASS

- [ ] **Step 6: Commit**

```bash
git add crates/rocket-git/src/service.rs crates/rocket-git/src/git2_service.rs
git commit -m "feat(backend): add abort_merge to GitService trait and Git2Service"
```

### Task 4: Wire `abort_merge` through app service and Tauri command

**Files:**
- Modify: `crates/rocket-app/src/git_service.rs`
- Modify: `src-tauri/src/commands/git.rs`
- Modify: `src-tauri/src/lib.rs`

- [ ] **Step 1: Add `abort_merge` to `GitAppService`**

In `crates/rocket-app/src/git_service.rs`, add after the `resolve_conflict` method (~line 222):

```rust
    pub fn abort_merge(&self, path: &str) -> DomainResult<()> {
        self.git.abort_merge(path)?;
        self.events.publish(DomainEvent::GitStatusChanged {
            collection: path.to_string(),
        });
        Ok(())
    }
```

- [ ] **Step 2: Add the Tauri command**

In `src-tauri/src/commands/git.rs`, add after the `git_resolve_conflict` command:

```rust
#[tauri::command]
pub fn git_abort_merge(collection_path: String, svc: State<'_, GitAppService>) -> Result<(), DomainError> {
    svc.abort_merge(&collection_path)
}
```

- [ ] **Step 3: Register the command in `lib.rs`**

In `src-tauri/src/lib.rs`, find the `invoke_handler` macro call and add `commands::git::git_abort_merge` after `commands::git::git_resolve_conflict`:

```rust
            commands::git::git_resolve_conflict,
            commands::git::git_abort_merge,
```

- [ ] **Step 4: Verify Rust compiles**

Run: `cargo check`
Expected: no errors

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-app/src/git_service.rs src-tauri/src/commands/git.rs src-tauri/src/lib.rs
git commit -m "feat(backend): wire abort_merge through app service and Tauri command"
```

### Task 5: Add abort merge to frontend store and UI

**Files:**
- Modify: `src/lib/tauri-api.ts`
- Modify: `src/stores/git-store.ts`
- Modify: `src/components/git/ConflictResolver.tsx`

- [ ] **Step 1: Add `gitAbortMerge` to `tauri-api.ts`**

In `src/lib/tauri-api.ts`, add after the `gitResolveConflict` export:

```typescript
export const gitAbortMerge = (collectionPath: string) =>
  invoke<void>("git_abort_merge", { collectionPath });
```

- [ ] **Step 2: Add `abortMerge` to the git store**

In `src/stores/git-store.ts`, add the import:

```typescript
// Add gitAbortMerge to the existing import from tauri-api
import {
  // ... existing imports ...
  gitAbortMerge,  // ADD THIS
} from '@/lib/tauri-api';
```

Add the method to the `GitState` interface:

```typescript
  abortMerge: () => Promise<void>;
```

Add the implementation in the store body, after the `resolveConflict` method:

```typescript
  // Abort a merge and reset to HEAD.
  abortMerge: async () => {
    const { collectionPath } = get();
    if (!collectionPath) return;
    try {
      await gitAbortMerge(collectionPath);
      await get().refreshStatus();
      await get().refreshConflicts();
    } catch (e) {
      set({ error: String(e) });
    }
  },
```

- [ ] **Step 3: Add "Abort Merge" button to `ConflictResolver`**

In `src/components/git/ConflictResolver.tsx`, add the import:

```typescript
import { useGitStore } from '@/stores/git-store';
import { usePaneStore } from '@/stores/pane-store';
```

Note: `useGitStore` is already imported. Add `usePaneStore` for closing the conflict tab after abort.

In the `ConflictResolver` component, add the abort handler and store connection. Update the component to:

1. Get `abortMerge` from the git store (already have `refreshStatus`)
2. Get `closeTab` from the pane store
3. Add an abort handler

After the existing `const { refreshStatus } = useGitStore();` line, add:

```typescript
  const { refreshStatus, abortMerge } = useGitStore();
```

(Replace the existing `refreshStatus`-only destructuring.)

Add the abort handler:

```typescript
  const handleAbort = async () => {
    await abortMerge();
  };
```

Add the "Abort Merge" button in the non-manual-mode header bar (the `div` with `flex items-center gap-2 border-b px-3 py-1.5` at ~line 61). Add it after the file path span:

```tsx
<div className="flex items-center gap-2 border-b px-3 py-1.5">
  <Badge variant="destructive" className="text-[9px]">Conflict</Badge>
  <span className="font-mono text-sm truncate">{conflictState.filePath}</span>
  <div className="ml-auto">
    <Button variant="outline" size="sm" className="h-6 text-sm text-destructive" onClick={handleAbort}>
      Abort Merge
    </Button>
  </div>
</div>
```

Also add the same button in the manual-mode header bar (~line 35):

```tsx
<div className="flex items-center gap-2 border-b px-3 py-1.5">
  <Badge variant="destructive" className="text-[9px]">Conflict</Badge>
  <span className="font-mono text-sm truncate">{conflictState.filePath}</span>
  <div className="ml-auto flex gap-1">
    <Button variant="outline" size="sm" className="h-6 text-sm text-destructive" onClick={handleAbort}>
      Abort Merge
    </Button>
    <Button variant="outline" size="sm" className="h-6 text-sm" onClick={() => setManualMode(false)}>
      Back
    </Button>
    <Button size="sm" className="h-6 text-sm" onClick={() => handleResolve('custom', manualContent)}>
      Save Resolution
    </Button>
  </div>
</div>
```

- [ ] **Step 4: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 5: Verify the app builds**

Run: `yarn build`
Expected: builds successfully

- [ ] **Step 6: Verify lint**

Run: `yarn lint`
Expected: no new errors

- [ ] **Step 7: Commit**

```bash
git add src/lib/tauri-api.ts src/stores/git-store.ts src/components/git/ConflictResolver.tsx
git commit -m "feat(frontend): add abort merge to store and ConflictResolver UI"
```
