# Git Stash Fix Implementation Plan

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix three bugs that prevent stashes from appearing in `GitStashSection`: wrong stash flags in Rust, raw git prefix in stash messages, and silent error swallowing in the UI.

**Architecture:**
- Task 1 fixes the Rust backend (`git2_service.rs`): add `INCLUDE_UNTRACKED` flag to `stash_save` so all working-tree changes are captured, and strip the `"On <branch>: "` prefix from messages in `stash_list`.
- Task 2 fixes the frontend: `GitStashSection` reads and displays `error` from the store, and `GitPanel` calls `refreshStashes()` when the stash view is opened (same pattern as `refreshLog` for commits).

**Tech Stack:** Rust (`git2` crate), React, TypeScript, Zustand, shadcn/ui

---

## Task 1: Fix Rust stash backend

**Files:**
- Modify: `crates/rocket-git/src/git2_service.rs`

### Root cause A — `stash_save` silently ignores untracked files

`repo.stash_save(&sig, message, None)` uses `StashFlags::DEFAULT` (value 0). This means only tracked modified files are stashed. If the user has any untracked new files (new `.bru` requests), those are ignored. If the working tree has **only** untracked changes, libgit2 returns `"there is nothing to stash"` — the error is captured in the store but never shown to the user, and the stash list stays empty.

The fix: use `StashFlags::INCLUDE_UNTRACKED` (matches `git stash` CLI default behaviour).

### Root cause B — `stash_list` returns git's internal message prefix

When `stash_save` is called with `"my work"`, git stores it as `"On main: my work"`. The `stash_foreach` callback returns this full string. The UI then shows `"On main: my work"` instead of `"my work"`, which looks like a bug even when stashes do exist.

The fix: strip the `"On <branch>: "` prefix in `stash_list`.

- [ ] **Step 1: Update `stash_save` to include untracked files**

In `crates/rocket-git/src/git2_service.rs`, find:

```rust
    fn stash_save(&self, path: &str, message: &str) -> DomainResult<()> {
        let mut repo = Repository::open(path)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let sig = repo
            .signature()
            .or_else(|_| git2::Signature::now("RocketAPI User", "user@rocketapi.local"))
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        repo.stash_save(&sig, message, None)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }
```

Replace with:

```rust
    fn stash_save(&self, path: &str, message: &str) -> DomainResult<()> {
        let mut repo = Repository::open(path)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let sig = repo
            .signature()
            .or_else(|_| git2::Signature::now("RocketAPI User", "user@rocketapi.local"))
            .map_err(|e| DomainError::Internal(e.to_string()))?;

        // INCLUDE_UNTRACKED matches `git stash` CLI default — captures new
        // untracked files (e.g. newly created .bru requests) in addition to
        // tracked modified/deleted files. Without this flag, stash_save returns
        // "nothing to stash" when only untracked files exist.
        repo.stash_save(&sig, message, Some(git2::StashFlags::INCLUDE_UNTRACKED))
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        Ok(())
    }
```

- [ ] **Step 2: Strip the git prefix in `stash_list`**

Find:

```rust
    fn stash_list(&self, path: &str) -> DomainResult<Vec<StashEntry>> {
        let mut repo = Repository::open(path)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let mut entries = Vec::new();

        repo.stash_foreach(|index, message, _oid| {
            entries.push(StashEntry {
                index,
                message: message.to_string(),
                timestamp: chrono::Utc::now(),
                branch: String::new(),
            });
            true
        })
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(entries)
    }
```

Replace with:

```rust
    fn stash_list(&self, path: &str) -> DomainResult<Vec<StashEntry>> {
        let mut repo = Repository::open(path)
            .map_err(|e| DomainError::Internal(e.to_string()))?;
        let mut entries = Vec::new();

        repo.stash_foreach(|index, message, _oid| {
            // git stores stash messages as "On <branch>: <user message>".
            // Strip that prefix so the UI shows only the user-supplied label.
            let display = if let Some(pos) = message.find(": ") {
                message[pos + 2..].to_string()
            } else {
                message.to_string()
            };
            entries.push(StashEntry {
                index,
                message: display,
                timestamp: chrono::Utc::now(),
                branch: String::new(),
            });
            true
        })
        .map_err(|e| DomainError::Internal(e.to_string()))?;

        Ok(entries)
    }
```

- [ ] **Step 3: Verify the existing stash tests still pass**

```bash
cargo test -p rocket-git -- git2_service::tests::stash
```

Expected: all stash tests pass. The test asserts `stashes[0].message.contains("WIP")` — this still passes after stripping the prefix because the user message `"WIP"` is preserved.

- [ ] **Step 4: Verify the workspace compiles**

```bash
cargo check --workspace
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add crates/rocket-git/src/git2_service.rs
git commit -m "fix(git): stash_save include untracked files; strip git prefix from stash messages"
```

---

## Task 2: Fix frontend stash error display + panel refresh

**Files:**
- Modify: `frontend/src/components/git/GitStashSection.tsx`
- Modify: `frontend/src/components/git/GitPanel.tsx`

### Root cause C — `GitStashSection` never shows errors

When `saveStash` fails (e.g. "nothing to stash"), the error lands in `useGitStore().error` but `GitStashSection` doesn't read it. The user types a message, clicks Save, nothing happens — no feedback at all.

### Root cause D — stash list not refreshed when panel opens

`GitPanel` calls `refreshLog()` when the commits panel opens (via `useEffect`), but does **not** call `refreshStashes()` when the stash panel opens. If stashes were added externally (e.g., via CLI), they won't appear until the collection is re-selected.

- [ ] **Step 1: Add error display and `isSaving` state to `GitStashSection`**

Replace the entire contents of `frontend/src/components/git/GitStashSection.tsx` with:

```tsx
import { AlertCircle, Archive, MoreHorizontal } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { useGitStore } from '@/stores/git-store';

export function GitStashSection() {
  const [message, setMessage] = useState('');
  const [isSaving, setIsSaving] = useState(false);
  const { stashes, saveStash, popStash, applyStash, dropStash, error, clearError } = useGitStore();

  const handleSave = async () => {
    if (!message.trim()) return;
    clearError();
    setIsSaving(true);
    try {
      await saveStash(message.trim());
      // Only clear the input if the save succeeded (no error in store after await).
      if (!useGitStore.getState().error) {
        setMessage('');
      }
    } finally {
      setIsSaving(false);
    }
  };

  // Show stash-related errors only — not unrelated git errors.
  const stashError = error?.toLowerCase().includes('stash') ? error : null;

  return (
    <div>
      {/* Section header */}
      <div className='flex items-center gap-1.5 px-2 py-1.5'>
        <Archive className='h-3.5 w-3.5 text-muted-foreground' />
        <span className='text-[11px] uppercase tracking-[0.06em] font-semibold text-muted-foreground'>
          Stash
          <span className='ml-1.5 font-mono normal-case tracking-normal opacity-70'>
            {stashes.length}
          </span>
        </span>
      </div>

      {/* Save input */}
      <div className='flex gap-1 px-2 pb-1'>
        <Input
          placeholder='Stash message...'
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          className='h-7 text-sm'
          onKeyDown={(e) => e.key === 'Enter' && !isSaving && handleSave()}
          disabled={isSaving}
        />
        <Button
          variant='outline'
          size='sm'
          className='h-7 text-sm shrink-0'
          onClick={handleSave}
          disabled={!message.trim() || isSaving}
        >
          {isSaving ? 'Saving…' : 'Save'}
        </Button>
      </div>

      {/* Error banner — shown when stash operation fails */}
      {stashError && (
        <div className='mx-2 mb-1.5 flex items-start gap-1.5 rounded-md border border-destructive/30 bg-destructive/10 px-2 py-1.5 text-xs text-destructive'>
          <AlertCircle className='mt-px h-3 w-3 shrink-0' />
          <span className='break-all'>{stashError}</span>
        </div>
      )}

      {/* Stash list */}
      {stashes.length === 0 && (
        <p className='px-2 py-1 text-xs text-muted-foreground'>No stashes yet.</p>
      )}
      {stashes.map((stash) => (
        <div
          key={stash.index}
          className='group flex items-center gap-1.5 px-2 py-[3px] hover:bg-muted/50 text-[13px]'
        >
          <TooltipProvider delayDuration={300}>
            <Tooltip>
              <TooltipTrigger asChild>
                <span className='truncate font-mono text-[13px] cursor-default flex-1'>
                  {stash.message}
                </span>
              </TooltipTrigger>
              <TooltipContent>
                <p>{stash.message}</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>

          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant='ghost'
                size='icon'
                className='h-5 w-5 opacity-0 group-hover:opacity-100 shrink-0'
              >
                <MoreHorizontal className='h-3.5 w-3.5' />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align='end'>
              <DropdownMenuItem onClick={() => popStash(stash.index)}>
                Pop (apply + remove)
              </DropdownMenuItem>
              <DropdownMenuItem onClick={() => applyStash(stash.index)}>
                Apply (keep stash)
              </DropdownMenuItem>
              <DropdownMenuItem
                onClick={() => dropStash(stash.index)}
                className='text-destructive'
              >
                Drop
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      ))}
    </div>
  );
}
```

- [ ] **Step 2: Refresh stashes when the stash panel opens in `GitPanel`**

In `frontend/src/components/git/GitPanel.tsx`, find the existing commit log refresh effect:

```tsx
  // Load the commit log when the commits view is opened.
  useEffect(() => {
    if (rightPanel.kind === 'commits') void refreshLog();
  }, [rightPanel.kind, refreshLog]);
```

Add a new effect immediately after it:

```tsx
  // Refresh the stash list when the stash view is opened.
  useEffect(() => {
    if (rightPanel.kind === 'stashes') void refreshStashes();
  }, [rightPanel.kind, refreshStashes]);
```

Update the store destructure at the top of `GitPanel` to include `refreshStashes`:

```tsx
  // Find this line (exact wording may vary slightly):
  const { showCredentialsDialog, setCollection, refreshLog, status } = useGitStore();

  // Replace with:
  const { showCredentialsDialog, setCollection, refreshLog, refreshStashes, status } = useGitStore();
```

- [ ] **Step 3: Verify TypeScript compiles cleanly**

```bash
cd frontend
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Smoke-test the full flow**

```bash
yarn tauri dev
```

Verify:
- Modify a tracked `.bru` file in a collection that has git initialized
- Open Git panel → click "Stashes" in the Links section
- Type a message and click Save
- The stash appears in the list with just the user message (no `"On main: "` prefix)
- The working-tree changes are reverted (the modified file returns to its committed state)
- Click the stash's `⋯` menu → "Pop" → changes are restored

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/git/GitStashSection.tsx frontend/src/components/git/GitPanel.tsx
git commit -m "fix(git): show stash errors in UI; refresh stash list when panel opens"
```
