# Git UI Design Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix overflow, layout, and usability issues across all Git UI components — dialogs and the main panel.

**Architecture:** Pure frontend changes confined to `src/components/git/`. No new files, no new abstractions. Each task is a targeted edit to one component applying consistent patterns: `min-w-0` for flex overflow, `truncate` + Shadcn `Tooltip` for long display text, `break-words` for error strings, `Textarea` for the commit form, and `flex-wrap` for crowded dialog footers.

**Tech Stack:** React, TypeScript, Tailwind CSS, Shadcn UI (`@/components/ui/tooltip`, `@/components/ui/textarea`)

---

## File Map

| File | Change |
|------|--------|
| `src/components/git/GitRemotesDialog.tsx` | Tooltip on URLs, min-w-0 on flex rows, max-w on names, max-h on dialog |
| `src/components/git/GitCloneDialog.tsx` | `break-words` on error paragraph |
| `src/components/git/GitCommitForm.tsx` | Replace `Input` with `Textarea`, Ctrl+Enter shortcut |
| `src/components/git/GitLandingPanel.tsx` | `flex-wrap gap-2` on both `AlertDialogFooter` instances |
| `src/components/git/GitStashSection.tsx` | Tooltip on stash message spans |

---

## Task 1: GitRemotesDialog — overflow fixes and URL tooltip

**Files:**
- Modify: `src/components/git/GitRemotesDialog.tsx`

**What changes and why:**
- `DialogContent` gets `max-h-[85vh] overflow-y-auto` — safety cap so many remotes don't push the dialog off-screen.
- Remote name spans get `max-w-[100px] truncate` — long remote names no longer push the URL off to the right.
- Remote URL span in display row gets `min-w-0` + wrapped in a `Tooltip` — hovering shows the full URL; the span truncates properly inside the flex row.
- Edit row outer div gets `min-w-0` — allows the `flex-1` Input to shrink when the name is long.
- Both Add-row Inputs get `min-w-0` — prevents their intrinsic min-width from forcing the row wider than the dialog.
- Wrap the content `div` in `TooltipProvider` (one provider for the whole dialog).

- [ ] **Step 1: Replace the full file content**

Replace `src/components/git/GitRemotesDialog.tsx` with:

```tsx
import { useEffect, useState } from 'react';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Separator } from '@/components/ui/separator';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { Pencil, Trash2, Check, X, Plus } from 'lucide-react';
import { useGitStore } from '@/stores/git-store';

interface Props {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}

export function GitRemotesDialog({ open, onOpenChange }: Props) {
  const { remotes, addRemote, removeRemote, setRemoteUrl, refreshRemotes } = useGitStore();

  const [newName, setNewName] = useState('');
  const [newUrl, setNewUrl] = useState('');
  const [editingRemote, setEditingRemote] = useState<string | null>(null);
  const [editUrl, setEditUrl] = useState('');
  const [deletingRemote, setDeletingRemote] = useState<string | null>(null);

  // Refresh the remote list each time the dialog opens.
  useEffect(() => {
    if (open) {
      refreshRemotes();
    }
  }, [open, refreshRemotes]);

  const canAdd =
    newName.trim().length > 0 &&
    !newName.includes(' ') &&
    newUrl.trim().length > 0 &&
    !remotes.some((r) => r.name === newName.trim());

  const handleAdd = async () => {
    await addRemote(newName.trim(), newUrl.trim());
    setNewName('');
    setNewUrl('');
  };

  const handleSaveEdit = async () => {
    if (!editingRemote) return;
    await setRemoteUrl(editingRemote, editUrl.trim());
    setEditingRemote(null);
  };

  const handleConfirmDelete = async () => {
    if (!deletingRemote) return;
    await removeRemote(deletingRemote);
    setDeletingRemote(null);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-md max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>Manage Remotes</DialogTitle>
        </DialogHeader>

        <TooltipProvider delayDuration={300}>
          <div className="space-y-3">
            {remotes.length === 0 ? (
              <p className="text-sm text-muted-foreground text-center py-4">
                No remotes configured.
              </p>
            ) : (
              <div className="space-y-1">
                {remotes.map((remote) => {
                  if (deletingRemote === remote.name) {
                    return (
                      <div key={remote.name} className="flex items-center gap-2 px-2 py-1.5 rounded-md bg-destructive/10">
                        <span className="text-sm flex-1">
                          Remove <span className="font-mono font-semibold">{remote.name}</span>?
                        </span>
                        <Button
                          size="sm"
                          variant="destructive"
                          className="h-7 text-xs"
                          onClick={handleConfirmDelete}
                        >
                          Remove
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          className="h-7 text-xs"
                          onClick={() => setDeletingRemote(null)}
                        >
                          Cancel
                        </Button>
                      </div>
                    );
                  }

                  if (editingRemote === remote.name) {
                    return (
                      <div key={remote.name} className="flex items-center gap-2 px-2 py-1 min-w-0">
                        <span className="font-mono font-semibold text-sm shrink-0 max-w-[100px] truncate">{remote.name}</span>
                        <Input
                          value={editUrl}
                          onChange={(e) => setEditUrl(e.target.value)}
                          className="h-7 text-sm flex-1 min-w-0"
                          autoFocus
                          onKeyDown={(e) => {
                            if (e.key === 'Enter') handleSaveEdit();
                            if (e.key === 'Escape') setEditingRemote(null);
                          }}
                        />
                        <Button
                          size="sm"
                          variant="ghost"
                          className="h-7 w-7 p-0 shrink-0"
                          onClick={handleSaveEdit}
                        >
                          <Check className="h-3.5 w-3.5" />
                        </Button>
                        <Button
                          size="sm"
                          variant="ghost"
                          className="h-7 w-7 p-0 shrink-0"
                          onClick={() => setEditingRemote(null)}
                        >
                          <X className="h-3.5 w-3.5" />
                        </Button>
                      </div>
                    );
                  }

                  return (
                    <div key={remote.name} className="group flex items-center gap-2 px-2 py-1 rounded-md hover:bg-muted/50">
                      <span className="font-mono font-semibold text-sm shrink-0 max-w-[100px] truncate">{remote.name}</span>
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <span className="text-sm text-muted-foreground truncate flex-1 min-w-0 cursor-default">{remote.url}</span>
                        </TooltipTrigger>
                        <TooltipContent><p>{remote.url}</p></TooltipContent>
                      </Tooltip>
                      <Button
                        size="sm"
                        variant="ghost"
                        className="h-7 w-7 p-0 shrink-0 opacity-0 group-hover:opacity-100"
                        onClick={() => {
                          setEditingRemote(remote.name);
                          setEditUrl(remote.url);
                        }}
                      >
                        <Pencil className="h-3.5 w-3.5" />
                      </Button>
                      <Button
                        size="sm"
                        variant="ghost"
                        className="h-7 w-7 p-0 shrink-0 opacity-0 group-hover:opacity-100 text-destructive hover:text-destructive"
                        onClick={() => setDeletingRemote(remote.name)}
                      >
                        <Trash2 className="h-3.5 w-3.5" />
                      </Button>
                    </div>
                  );
                })}
              </div>
            )}

            <Separator />

            <div className="flex items-center gap-2">
              <Input
                placeholder="name"
                value={newName}
                onChange={(e) => setNewName(e.target.value)}
                className="h-8 text-sm flex-[2] min-w-0"
                onKeyDown={(e) => { if (e.key === 'Enter' && canAdd) handleAdd(); }}
              />
              <Input
                placeholder="https://github.com/..."
                value={newUrl}
                onChange={(e) => setNewUrl(e.target.value)}
                className="h-8 text-sm flex-[5] min-w-0"
                onKeyDown={(e) => { if (e.key === 'Enter' && canAdd) handleAdd(); }}
              />
              <Button
                size="sm"
                className="h-8 shrink-0"
                disabled={!canAdd}
                onClick={handleAdd}
              >
                <Plus className="h-3.5 w-3.5 mr-1" /> Add
              </Button>
            </div>
          </div>
        </TooltipProvider>
      </DialogContent>
    </Dialog>
  );
}
```

- [ ] **Step 2: Check TypeScript**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/git/GitRemotesDialog.tsx
git commit -m "fix: manage remotes dialog overflow and URL tooltip"
```

---

## Task 2: GitCloneDialog — error text wrapping

**Files:**
- Modify: `src/components/git/GitCloneDialog.tsx`

**What changes:** The error `<p>` currently has `className="text-sm text-destructive"`. Long Rust error strings containing file paths or URLs are one unbreakable token — they overflow horizontally past the dialog edge. Adding `break-words` forces them to wrap.

- [ ] **Step 1: Edit the error paragraph**

In `src/components/git/GitCloneDialog.tsx`, find:

```tsx
{error && <p className="text-sm text-destructive">{error}</p>}
```

Replace with:

```tsx
{error && <p className="text-sm text-destructive break-words">{error}</p>}
```

- [ ] **Step 2: Check TypeScript**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/git/GitCloneDialog.tsx
git commit -m "fix: wrap long error strings in clone dialog"
```

---

## Task 3: GitCommitForm — multi-line Textarea

**Files:**
- Modify: `src/components/git/GitCommitForm.tsx`

**What changes:**
- Import `Textarea` from `@/components/ui/textarea` instead of `Input` from `@/components/ui/input`.
- The `Textarea` Shadcn component already has `min-h-[60px]` built in via its default className, but we override with `min-h-[60px] resize-none` to lock height and prevent manual resizing (which would break the panel layout).
- Change the `onKeyDown` handler from `Enter` → `Ctrl+Enter` / `Cmd+Enter` since plain Enter now inserts newlines.
- Update placeholder to hint the new shortcut.

- [ ] **Step 1: Replace the full file content**

Replace `src/components/git/GitCommitForm.tsx` with:

```tsx
import { useState } from 'react';
import { Check } from 'lucide-react';
import { Textarea } from '@/components/ui/textarea';
import { Button } from '@/components/ui/button';
import { useGitStore } from '@/stores/git-store';

export function GitCommitForm() {
  const [message, setMessage] = useState('');
  const { status, commitChanges } = useGitStore();

  const stagedCount = status?.files.filter((f) => f.staged).length ?? 0;

  const handleCommit = async () => {
    if (!message.trim() || stagedCount === 0) return;
    await commitChanges(message.trim());
    setMessage('');
  };

  return (
    <div className="space-y-2">
      <Textarea
        placeholder="Commit message... (Ctrl+Enter to commit)"
        value={message}
        onChange={(e) => setMessage(e.target.value)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) void handleCommit();
        }}
        className="text-sm min-h-[60px] resize-none"
      />
      <Button
        onClick={handleCommit}
        disabled={!message.trim() || stagedCount === 0}
        className="w-full"
        size="sm"
      >
        <Check className="h-3.5 w-3.5" />
        Commit Changes
      </Button>
    </div>
  );
}
```

- [ ] **Step 2: Check TypeScript**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/git/GitCommitForm.tsx
git commit -m "feat: multi-line textarea for commit message (Ctrl+Enter to commit)"
```

---

## Task 4: GitLandingPanel — AlertDialog footer wrapping

**Files:**
- Modify: `src/components/git/GitLandingPanel.tsx`

**What changes:** Both `AlertDialog` instances (stash dialog and fetch-before-push dialog) have three buttons in their footer. Shadcn's `AlertDialogFooter` uses `sm:flex-row sm:space-x-2` — on a narrow panel the buttons overflow instead of wrapping. Adding `className="flex-wrap gap-2"` to both footers lets them wrap to a second line gracefully.

- [ ] **Step 1: Update the stash AlertDialog footer**

In `src/components/git/GitLandingPanel.tsx`, find:

```tsx
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handlePullAnyway}>Pull Anyway</AlertDialogAction>
            <AlertDialogAction onClick={handleStashAndPull}>Stash & Pull</AlertDialogAction>
          </AlertDialogFooter>
```

Replace with:

```tsx
          <AlertDialogFooter className="flex-wrap gap-2">
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handlePullAnyway}>Pull Anyway</AlertDialogAction>
            <AlertDialogAction onClick={handleStashAndPull}>Stash & Pull</AlertDialogAction>
          </AlertDialogFooter>
```

- [ ] **Step 2: Update the fetch-before-push AlertDialog footer**

Find:

```tsx
          <AlertDialogFooter>
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handlePushAnyway}>Push Anyway</AlertDialogAction>
            <AlertDialogAction onClick={handleFetchAndPush}>Fetch & Push</AlertDialogAction>
          </AlertDialogFooter>
```

Replace with:

```tsx
          <AlertDialogFooter className="flex-wrap gap-2">
            <AlertDialogCancel>Cancel</AlertDialogCancel>
            <AlertDialogAction onClick={handlePushAnyway}>Push Anyway</AlertDialogAction>
            <AlertDialogAction onClick={handleFetchAndPush}>Fetch & Push</AlertDialogAction>
          </AlertDialogFooter>
```

- [ ] **Step 3: Check TypeScript**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/git/GitLandingPanel.tsx
git commit -m "fix: wrap three-button AlertDialog footers in git landing panel"
```

---

## Task 5: GitStashSection — stash message tooltip

**Files:**
- Modify: `src/components/git/GitStashSection.tsx`

**What changes:** Stash messages truncate with `...` but there's no way to see the full message. Wrap each stash message `span` in a Shadcn `Tooltip` (same pattern as `GitCommitLog` uses for commit hashes). Add `TooltipProvider` per item, matching the pattern already used in `GitCommitLog.tsx`.

- [ ] **Step 1: Add tooltip imports**

In `src/components/git/GitStashSection.tsx`, find the import block:

```tsx
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Archive, MoreHorizontal } from 'lucide-react';
import { useGitStore } from '@/stores/git-store';
```

Replace with:

```tsx
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { Archive, MoreHorizontal } from 'lucide-react';
import { useGitStore } from '@/stores/git-store';
```

- [ ] **Step 2: Wrap stash message span in Tooltip**

Find:

```tsx
          <span className="truncate font-mono text-[13px]">{stash.message}</span>
```

Replace with:

```tsx
          <TooltipProvider delayDuration={300}>
            <Tooltip>
              <TooltipTrigger asChild>
                <span className="truncate font-mono text-[13px] cursor-default">{stash.message}</span>
              </TooltipTrigger>
              <TooltipContent><p>{stash.message}</p></TooltipContent>
            </Tooltip>
          </TooltipProvider>
```

- [ ] **Step 3: Check TypeScript**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/git/GitStashSection.tsx
git commit -m "fix: tooltip on truncated stash messages"
```

---

## Task 6: Final validation

- [ ] **Step 1: Run full TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 2: Run linter**

```bash
yarn lint
```

Expected: no new errors introduced.

- [ ] **Step 3: Run frontend tests**

```bash
yarn test
```

Expected: all existing tests pass (no git component tests exist — these are visual changes).

- [ ] **Step 4: Manual smoke test checklist**

Launch `yarn tauri dev` and verify:

| Check | Where |
|-------|-------|
| Open Manage Remotes — add a remote with a very long URL | Git panel → Links → Remotes |
| Verify URL truncates in the list and full URL shows on hover | Display row |
| Edit a remote — verify name truncates, input doesn't overflow | Edit row |
| Open Manage Remotes with many remotes (6+) — verify dialog scrolls at 85vh | Manage Remotes |
| Clone a repo with a bad URL — verify long error wraps inside dialog | Clone Repository |
| Commit form shows textarea, Enter inserts newline, Ctrl+Enter commits | Left panel Changes section |
| Stash a change, verify message truncates and tooltip shows full text | Git panel → Links → Stashes |
| Pull with uncommitted changes — verify 3-button footer wraps on narrow panel | Landing panel alert dialog |
