# Git UX Improvements Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix five UX gaps in the Rocket git panel — keyboard shortcut for git panel, merge error surfacing in BranchSelector, commit form "no files staged" hint, and discard-all confirmation dialog.

**Architecture:** All changes are pure frontend (React/TypeScript). No new Tauri commands required. Each task touches 1–2 files and is independently committable. U9 (hide visual diff toggle) is already implemented and skipped.

**Tech Stack:** React, TypeScript, Zustand, shadcn/ui (Button, AlertDialog, Tooltip), lucide-react icons.

---

## File Map

| File | Change |
|------|--------|
| `src/hooks/useKeyboardShortcuts.ts` | U1 — add `Ctrl/Cmd+Shift+G` to open git panel |
| `src/components/layout/GitToolbarButton.tsx` | U1 — add tooltip showing shortcut |
| `src/components/git/BranchSelector.tsx` | U3 — await merge, surface errors, keep popover open on failure |
| `src/components/git/GitCommitForm.tsx` | U4 — add "No files staged" hint text |
| `src/components/git/GitFileList.tsx` | U7 — add AlertDialog confirmation before discard-all |

---

## Task 1 — U1: Global `Ctrl/Cmd+Shift+G` keyboard shortcut to open git panel

**Spec ref:** U1 — No global keyboard shortcut to open the Git panel.

**Files:**
- Modify: `src/hooks/useKeyboardShortcuts.ts`
- Modify: `src/components/layout/GitToolbarButton.tsx`

### Background

`useKeyboardShortcuts.ts` registers all global `keydown` handlers via `window.addEventListener`. The git panel is opened by `GitToolbarButton.handleClick` which: reads `activeCollection` and `collectionPath` from stores, builds a `GitTab`, and calls `openTab`. We need to extract that logic so both the button click and the keyboard shortcut can invoke it.

The shortcut key: `Ctrl+Shift+G` (Windows/Linux) / `Cmd+Shift+G` (Mac). In the handler, `e.key === 'G'` (uppercase because Shift is held) and `e.shiftKey && (e.metaKey || e.ctrlKey)`.

### Step 1: Extract `openGitPanel` into a standalone async function in `GitToolbarButton.tsx`

Replace the current `GitToolbarButton.tsx` with:

```tsx
import { GitBranch } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { listCollections } from '@/lib/tauri-api';
import { useGitStore } from '@/stores/git-store';
import { usePaneStore } from '@/stores/pane-store';
import type { GitTab, LeafNode, PaneNode } from '@/types/pane-types';

/** Return the groupId of the leaf that contains a tab with the given id. */
function findTabGroupId(node: PaneNode, tabId: string): string | null {
  if (node.type === 'leaf') {
    return node.tabs.some((t) => t.id === tabId) ? node.groupId : null;
  }
  return findTabGroupId(node.children[0], tabId) ?? findTabGroupId(node.children[1], tabId);
}

/** Return the leaf node with the given groupId. */
function findLeaf(node: PaneNode, groupId: string): LeafNode | null {
  if (node.type === 'leaf') return node.groupId === groupId ? node : null;
  return findLeaf(node.children[0], groupId) ?? findLeaf(node.children[1], groupId);
}

/** Open the git panel for the active collection. Can be called from keyboard shortcuts. */
export async function openGitPanel(): Promise<void> {
  const { activeCollection, openTab, root, closeTab } = usePaneStore.getState();
  if (!activeCollection) return;

  let path = useGitStore.getState().collectionPath ?? '';
  if (!path) {
    try {
      const summaries = await listCollections();
      const match = summaries.find((s) => s.name === activeCollection);
      path = match?.path ?? '';
    } catch {
      // Fall through — GitPanel will show appropriate state.
    }
  }

  const tabId = `git:${activeCollection}`;

  if (path) {
    const groupId = findTabGroupId(root, tabId);
    if (groupId) {
      const leaf = findLeaf(root, groupId);
      const existingTab = leaf?.tabs.find((t) => t.id === tabId) as GitTab | undefined;
      if (existingTab && !existingTab.collectionPath) {
        closeTab(tabId, groupId);
      }
    }
  }

  const tab: GitTab = {
    id: tabId,
    title: 'Git UI',
    tabType: 'git',
    collectionName: activeCollection,
    collectionPath: path,
    isDirty: false,
  };
  openTab(tab);
}

export function GitToolbarButton() {
  const activeCollection = usePaneStore((s) => s.activeCollection);

  return (
    <TooltipProvider>
      <Tooltip>
        <TooltipTrigger asChild>
          <Button
            variant='ghost'
            size='icon'
            className='h-7 w-7'
            onClick={() => void openGitPanel()}
            disabled={!activeCollection}
            aria-label='Open Git panel'
          >
            <GitBranch className='h-3.5 w-3.5 text-muted-foreground' />
          </Button>
        </TooltipTrigger>
        <TooltipContent>Open Git panel (⌘⇧G)</TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}
```

- [ ] **Step 2: Add the shortcut to `useKeyboardShortcuts.ts`**

Add the `Ctrl/Cmd+Shift+G` handler inside the `handler` function, after the existing `Cmd/Ctrl+W` block and before the `Cmd/Ctrl+Tab` block:

```ts
// Cmd/Ctrl+Shift+G — open the git panel for the active collection.
if (e.key === 'G' && e.shiftKey) {
  e.preventDefault();
  import('@/components/layout/GitToolbarButton').then(({ openGitPanel }) => {
    void openGitPanel();
  });
  return;
}
```

The full updated `useKeyboardShortcuts.ts`:

```ts
import { useEffect } from 'react';
import { sendRequest } from '@/lib/execute-request';
import { findActiveLeaf } from '@/lib/pane-utils';
import { usePaneStore } from '@/stores/pane-store';
import { isRequestTab } from '@/types/pane-types';

// Registers global keyboard shortcuts for tab management across all pane groups.
export function useKeyboardShortcuts() {
  useEffect(() => {
    const handler = (e: KeyboardEvent) => {
      const mod = e.metaKey || e.ctrlKey;
      if (!mod) return;

      const store = usePaneStore.getState();
      const { root, activeGroupId } = store;
      const activeLeaf = findActiveLeaf(root, activeGroupId);

      // Cmd/Ctrl+Enter — send the active tab's request.
      if (e.key === 'Enter') {
        e.preventDefault();
        const tab = activeLeaf.tabs.find((t) => t.id === activeLeaf.activeTabId);
        if (tab && isRequestTab(tab)) {
          sendRequest(tab.id, tab.request);
        }
        return;
      }

      // Cmd/Ctrl+S — open save-to-collection for ephemeral tabs, else save draft.
      if (e.key === 's') {
        e.preventDefault();
        const tab = activeLeaf.tabs.find((t) => t.id === activeLeaf.activeTabId);
        if (!tab) return;
        if (isRequestTab(tab) && !tab.source) {
          window.dispatchEvent(
            new CustomEvent('rocket:save-to-collection', { detail: { tabId: tab.id } }),
          );
        } else {
          window.dispatchEvent(new CustomEvent('rocket:save-draft', { detail: { tabId: tab.id } }));
        }
        return;
      }

      // Cmd/Ctrl+W — close the active tab in the active group.
      if (e.key === 'w') {
        e.preventDefault();
        store.closeTab(activeLeaf.activeTabId, activeGroupId);
        return;
      }

      // Cmd/Ctrl+Shift+G — open the git panel for the active collection.
      if (e.key === 'G' && e.shiftKey) {
        e.preventDefault();
        import('@/components/layout/GitToolbarButton').then(({ openGitPanel }) => {
          void openGitPanel();
        });
        return;
      }

      // Cmd/Ctrl+Tab — cycle to the next tab (wrapping) in the active group.
      if (e.key === 'Tab' && !e.shiftKey) {
        e.preventDefault();
        const { tabs, activeTabId, groupId } = activeLeaf;
        if (tabs.length < 2) return;
        const idx = tabs.findIndex((t) => t.id === activeTabId);
        const next = tabs[(idx + 1) % tabs.length];
        store.setActiveTab(next.id, groupId);
        return;
      }

      // Cmd/Ctrl+Shift+Tab — cycle to the previous tab (wrapping).
      if (e.key === 'Tab' && e.shiftKey) {
        e.preventDefault();
        const { tabs, activeTabId, groupId } = activeLeaf;
        if (tabs.length < 2) return;
        const idx = tabs.findIndex((t) => t.id === activeTabId);
        const prev = tabs[(idx - 1 + tabs.length) % tabs.length];
        store.setActiveTab(prev.id, groupId);
        return;
      }

      // Cmd/Ctrl+1 through Cmd/Ctrl+9 — jump to tab by 1-based index.
      const digit = parseInt(e.key, 10);
      if (digit >= 1 && digit <= 9) {
        e.preventDefault();
        const { tabs, groupId } = activeLeaf;
        const target = tabs[digit - 1];
        if (target) {
          store.setActiveTab(target.id, groupId);
        }
      }
    };

    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, []);
}
```

- [ ] **Step 3: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Lint check**

```bash
yarn check
```

Expected: clean.

- [ ] **Step 5: Commit**

```bash
git add src/hooks/useKeyboardShortcuts.ts src/components/layout/GitToolbarButton.tsx
git commit -m "feat(git): add Ctrl/Cmd+Shift+G keyboard shortcut to open git panel"
```

---

## Task 2 — U3: `mergeBranch` in BranchSelector must await, surface errors, keep popover open on failure

**Spec ref:** U3 — Merge errors silently dropped, popover closes before merge completes.

**Files:**
- Modify: `src/components/git/BranchSelector.tsx`

### Background

Current merge handler (lines 150–155):
```tsx
onClick={(e) => {
  e.stopPropagation();
  mergeBranch(branch.name);  // no await
  setOpen(false);             // closes immediately
}}
```

Fix: Add a `handleMerge` async function mirroring `handleSwitch` — await the store action, check if a new error was set, keep popover open on failure (show in `switchError`), close on success. If the merge produced conflicts (error includes "conflict"), the popover should close so the user can see the conflict resolver that appears in the right panel.

- [ ] **Step 1: Add `handleMerge` and update the merge button's onClick**

Replace the full `BranchSelector.tsx` content:

```tsx
import { AlertCircle, Check, GitBranch, GitMerge, Plus, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { Separator } from '@/components/ui/separator';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { useGitStore } from '@/stores/git-store';

export function BranchSelector() {
  const [open, setOpen] = useState(false);
  const [search, setSearch] = useState('');
  const [newBranchName, setNewBranchName] = useState('');
  const [createError, setCreateError] = useState<string | null>(null);
  const [switchError, setSwitchError] = useState<string | null>(null);
  const {
    branches,
    switchBranch,
    createBranch,
    deleteBranch,
    mergeBranch,
    checkoutRemoteBranch,
    status,
  } = useGitStore();

  if (!branches) return null;

  const filtered = branches.local.filter((b) =>
    b.name.toLowerCase().includes(search.toLowerCase()),
  );

  const filteredRemote = branches.remote.filter((b) => {
    if (b.name.endsWith('/HEAD')) return false;
    const localName = b.name.split('/').slice(1).join('/');
    return (
      !branches.local.some((l) => l.name === localName) &&
      b.name.toLowerCase().includes(search.toLowerCase())
    );
  });

  const handleCreate = async () => {
    if (!newBranchName.trim()) return;
    setCreateError(null);
    const prevError = useGitStore.getState().error;
    await createBranch(newBranchName.trim());
    const nextError = useGitStore.getState().error;
    if (nextError && nextError !== prevError) {
      setCreateError(nextError);
    } else {
      setNewBranchName('');
    }
  };

  const handleSwitch = async (name: string) => {
    setSwitchError(null);
    const prevError = useGitStore.getState().error;
    await switchBranch(name);
    const nextError = useGitStore.getState().error;
    if (nextError && nextError !== prevError) {
      setSwitchError(nextError);
    } else {
      setOpen(false);
    }
  };

  const handleCheckoutRemote = async (name: string) => {
    setSwitchError(null);
    const prevError = useGitStore.getState().error;
    await checkoutRemoteBranch(name);
    const nextError = useGitStore.getState().error;
    if (nextError && nextError !== prevError) {
      setSwitchError(nextError);
    } else {
      setOpen(false);
    }
  };

  // Await merge, then surface the result:
  // - On success: close the popover.
  // - On conflict: close the popover so the conflict resolver is visible.
  // - On other error: keep the popover open and show the error inline.
  const handleMerge = async (name: string) => {
    setSwitchError(null);
    const prevError = useGitStore.getState().error;
    await mergeBranch(name);
    const nextError = useGitStore.getState().error;
    if (nextError && nextError !== prevError) {
      // Conflict errors should close the popover — the conflict resolver will appear
      // in the right panel and the error is also visible in GitLandingPanel.
      if (nextError.toLowerCase().includes('conflict')) {
        setOpen(false);
      } else {
        setSwitchError(nextError);
      }
    } else {
      setOpen(false);
    }
  };

  return (
    <Popover
      open={open}
      onOpenChange={(v) => {
        setOpen(v);
        if (!v) setSwitchError(null);
      }}
    >
      <PopoverTrigger asChild>
        <Button variant='ghost' size='sm' className='h-6 gap-1 text-sm'>
          <GitBranch className='h-3.5 w-3.5' />
          {status?.branch ?? 'main'}
        </Button>
      </PopoverTrigger>
      <PopoverContent className='w-64 p-0' align='start'>
        {switchError && (
          <div className='flex items-start gap-1.5 px-2 py-1.5 text-xs text-destructive border-b border-border/70'>
            <AlertCircle className='h-3 w-3 shrink-0 mt-0.5' />
            <span className='wrap-break-word'>{switchError}</span>
          </div>
        )}
        <div className='p-2'>
          <Input
            placeholder='Search branches...'
            value={search}
            onChange={(e) => {
              setSearch(e.target.value);
              setSwitchError(null);
            }}
            className='h-7 text-sm'
            aria-label='Search branches'
          />
        </div>
        <Separator />
        <div className='max-h-48 overflow-y-auto p-1'>
          {filtered.map((branch) => (
            // biome-ignore lint/a11y/useSemanticElements: outer <button> nesting inner <button> is invalid HTML; WebKitGTK reparses it and breaks hover tracking.
            <div
              key={branch.name}
              role='button'
              tabIndex={0}
              className='branch-row flex w-full items-center gap-1.5 rounded px-2 py-1 hover:bg-muted/50 cursor-pointer text-sm text-left'
              onClick={() => {
                if (!branch.isHead) void handleSwitch(branch.name);
                else setOpen(false);
              }}
              onKeyDown={(e) => {
                if (e.key === 'Enter' || e.key === ' ') {
                  if (!branch.isHead) void handleSwitch(branch.name);
                  else setOpen(false);
                }
              }}
            >
              {branch.isHead && <Check className='h-3.5 w-3.5 text-primary' />}
              {!branch.isHead && <span className='w-3' />}
              <span className='truncate flex-1'>{branch.name}</span>
              {!branch.isHead && (
                <TooltipProvider delayDuration={300}>
                  <div className='branch-row-actions gap-0.5'>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          onClick={(e) => {
                            e.stopPropagation();
                            void handleMerge(branch.name);
                          }}
                        >
                          <GitMerge className='h-3.5 w-3.5 text-muted-foreground' />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Merge into current</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5 text-destructive'
                          onClick={(e) => {
                            e.stopPropagation();
                            deleteBranch(branch.name);
                          }}
                        >
                          <Trash2 className='h-3.5 w-3.5' />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Delete branch</TooltipContent>
                    </Tooltip>
                  </div>
                </TooltipProvider>
              )}
            </div>
          ))}
          {filteredRemote.length > 0 && (
            <>
              <div className='px-2 py-1 text-xs text-muted-foreground font-medium mt-1'>Remote</div>
              {filteredRemote.map((branch) => {
                const localName = branch.name.split('/').slice(1).join('/');
                return (
                  <button
                    key={branch.name}
                    type='button'
                    className='flex w-full items-center gap-1.5 rounded px-2 py-1 hover:bg-muted/50 cursor-pointer text-sm text-left'
                    onClick={() => {
                      void handleCheckoutRemote(branch.name);
                    }}
                  >
                    <span className='w-3.5' />
                    <span className='truncate flex-1 text-muted-foreground'>{localName}</span>
                  </button>
                );
              })}
            </>
          )}
        </div>
        <Separator />
        <div className='flex flex-col gap-1 p-2'>
          <div className='flex gap-1'>
            <Input
              placeholder='New branch...'
              value={newBranchName}
              onChange={(e) => {
                setNewBranchName(e.target.value);
                setCreateError(null);
              }}
              className='h-7 text-sm'
              onKeyDown={(e) => e.key === 'Enter' && handleCreate()}
              aria-label='New branch name'
            />
            <Button
              variant='outline'
              size='sm'
              className='h-7 shrink-0'
              onClick={handleCreate}
              disabled={!newBranchName.trim()}
              aria-label='Create branch'
            >
              <Plus className='h-3.5 w-3.5' />
            </Button>
          </div>
          {createError && (
            <div className='flex items-start gap-1.5 text-xs text-destructive'>
              <AlertCircle className='h-3 w-3 shrink-0 mt-0.5' />
              <span className='wrap-break-word'>{createError}</span>
            </div>
          )}
        </div>
      </PopoverContent>
    </Popover>
  );
}
```

- [ ] **Step 2: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Lint check**

```bash
yarn check
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/components/git/BranchSelector.tsx
git commit -m "fix(git): await mergeBranch in BranchSelector and surface errors inline"
```

---

## Task 3 — U4: Add "No files staged" hint in GitCommitForm

**Spec ref:** U4 — No staged-files guard with user feedback.

**Files:**
- Modify: `src/components/git/GitCommitForm.tsx`

### Background

The commit button is already disabled when `stagedCount === 0` (line 40). The missing piece is a visible hint explaining *why* it's disabled. Add a `<p>` element below the textarea that shows "No files staged" when `stagedCount === 0` and the message is non-empty (so it only appears when the user has actually tried to compose a commit message but has no staged files).

- [ ] **Step 1: Add the staged-files hint**

Replace the entire `GitCommitForm.tsx` with:

```tsx
import { Check, Loader2 } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Textarea } from '@/components/ui/textarea';
import { useGitStore } from '@/stores/git-store';

export function GitCommitForm() {
  const [message, setMessage] = useState('');
  const [committing, setCommitting] = useState(false);
  const { status, commitChanges } = useGitStore();

  const stagedCount = status?.files.filter((f) => f.staged).length ?? 0;

  const handleCommit = async () => {
    if (!message.trim() || stagedCount === 0) return;
    setCommitting(true);
    try {
      await commitChanges(message.trim());
      setMessage('');
    } finally {
      setCommitting(false);
    }
  };

  return (
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
  );
}
```

- [ ] **Step 2: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/git/GitCommitForm.tsx
git commit -m "feat(git): show 'No files staged' hint in commit form when nothing is staged"
```

---

## Task 4 — U7: Add confirmation dialog before "Discard all" in GitFileList

**Spec ref:** U7 — "Discard all" has no confirmation — data-loss risk.

**Files:**
- Modify: `src/components/git/GitFileList.tsx`

### Background

`handleDiscardAll` currently calls `discardFiles()` immediately with no confirmation. Discarded changes cannot be recovered. The fix: add an `AlertDialog` (shadcn/ui) that shows the count of files to be discarded and requires the user to click "Discard" to confirm. Individual-file discard buttons remain unchanged (low blast radius).

Pattern: mirror the `AlertDialog` usage in `src/components/git/GitLandingPanel.tsx` for stash confirmation.

- [ ] **Step 1: Add AlertDialog to GitFileList.tsx**

Replace the full `GitFileList.tsx`:

```tsx
import { AlertTriangle, Minus, Plus, Trash2 } from 'lucide-react';
import { useState } from 'react';
import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { GIT_STATUS_CONFIG } from '@/lib/colors';
import type { ConflictFile, FileStatus } from '@/lib/tauri-api';
import { useGitStore } from '@/stores/git-store';

interface GitFileListProps {
  onFileClick: (file: FileStatus) => void;
  onConflictClick: (conflictFile: ConflictFile) => void;
}

export function GitFileList({ onFileClick, onConflictClick }: GitFileListProps) {
  const {
    status,
    refreshConflicts,
    refreshStatus,
    stageFiles,
    stageAll,
    unstageFiles,
    unstageAll,
    discardFiles,
  } = useGitStore();

  const [showDiscardAllDialog, setShowDiscardAllDialog] = useState(false);

  const staged = status?.files.filter((f) => f.staged) ?? [];
  const unstaged = status?.files.filter((f) => !f.staged && f.status !== 'unchanged') ?? [];
  const discardableFiles = unstaged.filter((f) => f.status !== 'conflicted');

  const handleDiscardAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    if (discardableFiles.length === 0) return;
    setShowDiscardAllDialog(true);
  };

  const handleConfirmDiscardAll = () => {
    discardFiles(discardableFiles.map((f) => f.path));
    setShowDiscardAllDialog(false);
  };

  const handleStageAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    stageAll();
  };

  const handleUnstageAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    unstageAll();
  };

  const handleConflictClick = async (file: FileStatus) => {
    await refreshConflicts();
    // Read fresh state — the `conflicts` binding captured at render-time is stale after the await.
    const conflictFile = useGitStore.getState().conflicts.find((c) => c.path === file.path);
    if (conflictFile) {
      onConflictClick(conflictFile);
    } else {
      await refreshStatus();
    }
  };

  return (
    <TooltipProvider>
      <div className='overflow-y-auto flex-1'>
        <div className='p-3 space-y-1'>
          {/* Staged section */}
          {staged.length > 0 && (
            <>
              <div className='flex items-center justify-between px-2 py-1'>
                <span className='text-xs font-medium text-muted-foreground'>Staged Changes</span>
                <div className='flex items-center gap-1.5'>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant='ghost'
                        size='icon'
                        className='h-5 w-5'
                        onClick={handleUnstageAll}
                      >
                        <Minus className='h-3.5 w-3.5' />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>Unstage all</TooltipContent>
                  </Tooltip>
                  <span className='text-xs text-muted-foreground'>{staged.length}</span>
                </div>
              </div>

              {staged.map((file) => (
                // biome-ignore lint/a11y/useSemanticElements: outer <button> nesting inner <button> is invalid HTML; WebKitGTK reparses it and breaks mouseleave tracking.
                <div
                  key={file.path}
                  role='button'
                  tabIndex={0}
                  className='git-file-row flex w-full items-center px-2 py-1 rounded-md hover:bg-muted/50 cursor-pointer gap-1.5 text-left'
                  onClick={() => onFileClick(file)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') onFileClick(file);
                  }}
                >
                  <span className='text-sm truncate flex-1 min-w-0'>{file.path}</span>
                  <span
                    className={`text-xs font-medium shrink-0 ${GIT_STATUS_CONFIG[file.status].className}`}
                  >
                    {GIT_STATUS_CONFIG[file.status].label}
                  </span>
                  <div className='git-row-actions gap-0.5 shrink-0'>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          onClick={(e) => {
                            e.stopPropagation();
                            unstageFiles([file.path]);
                          }}
                        >
                          <Minus className='h-3.5 w-3.5' />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Unstage</TooltipContent>
                    </Tooltip>
                  </div>
                </div>
              ))}
              <Separator className='my-2' />
            </>
          )}

          {/* Unstaged section header */}
          <div className='flex items-center justify-between px-2 py-1'>
            <span className='text-xs font-medium text-muted-foreground'>Unstaged Changes</span>
            <div className='flex items-center gap-1.5'>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant='ghost'
                    size='icon'
                    className='h-5 w-5'
                    onClick={handleDiscardAll}
                    disabled={discardableFiles.length === 0}
                  >
                    <Trash2 className='h-3.5 w-3.5' />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Discard all unstaged</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant='ghost' size='icon' className='h-5 w-5' onClick={handleStageAll}>
                    <Plus className='h-3.5 w-3.5' />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Stage all</TooltipContent>
              </Tooltip>
              <span className='text-xs text-muted-foreground'>{unstaged.length}</span>
            </div>
          </div>

          {/* Empty state */}
          {unstaged.length === 0 && (
            <p className='px-2 py-1 text-xs text-muted-foreground/60'>
              {staged.length > 0 ? 'All changes staged.' : 'Working tree clean.'}
            </p>
          )}

          {/* Unstaged file rows */}
          {unstaged.map((file) => {
            const isConflicted = file.status === 'conflicted';
            return (
              // biome-ignore lint/a11y/useSemanticElements: outer <button> nesting inner <button> is invalid HTML; WebKitGTK reparses it and breaks mouseleave tracking.
              <div
                key={file.path}
                role='button'
                tabIndex={0}
                className='git-file-row flex w-full items-center px-2 py-1 rounded-md hover:bg-muted/50 cursor-pointer gap-1.5 text-left'
                onClick={() => {
                  if (isConflicted) {
                    void handleConflictClick(file);
                  } else {
                    onFileClick(file);
                  }
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    if (isConflicted) {
                      void handleConflictClick(file);
                    } else {
                      onFileClick(file);
                    }
                  }
                }}
              >
                {isConflicted && (
                  <AlertTriangle className='h-3.5 w-3.5 text-destructive shrink-0' />
                )}
                <span className='text-sm truncate flex-1 min-w-0'>{file.path}</span>
                <span
                  className={`text-xs font-medium shrink-0 ${GIT_STATUS_CONFIG[file.status].className}`}
                >
                  {GIT_STATUS_CONFIG[file.status].label}
                </span>
                {!isConflicted && (
                  <div className='git-row-actions gap-0.5 shrink-0'>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          onClick={(e) => {
                            e.stopPropagation();
                            discardFiles([file.path]);
                          }}
                        >
                          <Trash2 className='h-3.5 w-3.5' />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Discard</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          onClick={(e) => {
                            e.stopPropagation();
                            stageFiles([file.path]);
                          }}
                        >
                          <Plus className='h-3.5 w-3.5' />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Stage</TooltipContent>
                    </Tooltip>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>

      {/* Discard-all confirmation dialog */}
      <AlertDialog open={showDiscardAllDialog} onOpenChange={setShowDiscardAllDialog}>
        <AlertDialogContent>
          <AlertDialogHeader>
            <AlertDialogTitle>Discard All Changes?</AlertDialogTitle>
            <AlertDialogDescription>
              This will permanently discard all {discardableFiles.length} unstaged{' '}
              {discardableFiles.length === 1 ? 'change' : 'changes'}. This cannot be undone.
            </AlertDialogDescription>
          </AlertDialogHeader>
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handleConfirmDiscardAll}>Discard</AlertDialogAction>
          </AlertDialogFooter>
        </AlertDialogContent>
      </AlertDialog>
    </TooltipProvider>
  );
}
```

- [ ] **Step 2: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Lint check**

```bash
yarn check
```

Expected: clean.

- [ ] **Step 4: Commit**

```bash
git add src/components/git/GitFileList.tsx
git commit -m "feat(git): add confirmation dialog before discard-all in file list"
```

---

## Final Verification

- [ ] **TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Biome lint**

```bash
yarn check
```

Expected: clean (pre-existing errors in unrelated files are acceptable).
