# Git Merge Conflict Resolution

**Date:** 2026-04-02  
**Status:** Approved  
**Scope:** Full conflict resolution flow — backend pull fix, conflict editor in Git panel right pane, conflicted file routing, in-merge indicator

## Overview

When a pull or merge results in conflicts, the Git panel surfaces a conflict editor inline (in the right pane) without leaving the Git tab. The user resolves each conflicted file one at a time using the existing `ConflictResolver` component, then commits using the normal commit form.

## Affected Files

| File | Change |
|------|--------|
| `crates/rocket-git/src/git2_service.rs` | Pull returns `Ok(())` on conflicts instead of error |
| `src/components/git/GitPanel.tsx` | Add `conflict` to `RightPanelView` union, render `ConflictResolver`, add in-merge banner |
| `src/components/git/GitFileList.tsx` | Add `onConflictClick` prop, route conflicted files, render conflict icon |

## Section 1: Backend — Pull Leaves Conflict State

**File:** `crates/rocket-git/src/git2_service.rs`

In the `pull` method, when the merge step results in conflicted index entries, return `Ok(())` instead of `Err(DomainError::Internal(...))`. The repo is left in a valid merge-in-progress state (`.git/MERGE_HEAD` present, conflicted entries in the index).

The frontend detects the conflict state via the `refreshStatus()` call that already follows every pull — it will see files with `status: "conflicted"` and the conflict UI appears automatically. No new Tauri commands are needed.

**Detection logic:** After calling `repo.merge(...)`, check `repo.index()?.has_conflicts()`. If true, write the index and return `Ok(())` rather than propagating a conflict error.

## Section 2: GitPanel — Conflict Right-Panel View

**File:** `src/components/git/GitPanel.tsx`

### RightPanelView union

Add a `conflict` variant:

```ts
type RightPanelView =
  | { kind: "landing" }
  | { kind: "diff"; file: FileStatus }
  | { kind: "conflict"; conflictFile: ConflictFile }
  | { kind: "commits" }
  | { kind: "stashes" }
```

### Right panel content

Add alongside the existing `diff` render:

```tsx
{rightPanel.kind === "conflict" && (
  <ConflictResolver
    conflictState={{
      filePath: rightPanel.conflictFile.path,
      collectionPath: collectionPath,
      ours: rightPanel.conflictFile.ours,
      theirs: rightPanel.conflictFile.theirs,
      ancestor: rightPanel.conflictFile.ancestor ?? null,
    }}
  />
)}
```

### Breadcrumb label

In the breadcrumb header (visible when `rightPanel.kind !== "landing"`), add:

```tsx
{rightPanel.kind === "conflict" && rightPanel.conflictFile.path}
```

### In-merge banner

In the left panel, between the collection header and the Changes collapsible, show a banner when conflicts exist:

```tsx
{hasConflicts && (
  <div className="px-3 py-2 bg-destructive/10 border-b border-border/70 flex items-center gap-2 shrink-0">
    <AlertTriangle className="h-3.5 w-3.5 text-destructive shrink-0" />
    <span className="text-xs text-destructive flex-1">
      Merge in progress — {conflictCount} conflicted
    </span>
  </div>
)}
```

Where:
```ts
const hasConflicts = (status?.files.some(f => f.status === 'conflicted')) ?? false;
const conflictCount = status?.files.filter(f => f.status === 'conflicted').length ?? 0;
```

No Abort button in this banner — `ConflictResolver` already provides one. No "Complete Merge" button — once all conflicts are resolved, the normal commit form handles the merge commit.

### Import additions

Add to GitPanel imports:
- `ConflictFile` from `@/lib/tauri-api`
- `ConflictResolver` from `@/components/git/ConflictResolver`
- `AlertTriangle` from `lucide-react`

## Section 3: GitFileList — Conflicted File Routing

**File:** `src/components/git/GitFileList.tsx`

### New prop

Add `onConflictClick: (conflictFile: ConflictFile) => void` alongside the existing `onFileClick`.

### Click handler for conflicted files

When a file with `status === "conflicted"` is clicked:
1. Call `refreshConflicts()` from the git store
2. Find the matching `ConflictFile` from `conflicts` array by `path`
3. If found, call `onConflictClick(conflictFile)`
4. If not found (stale state), call `refreshStatus()` and do nothing further

### Visual treatment for conflicted files

Conflicted files render with:
- A red `AlertTriangle` icon instead of the normal status icon
- No stage / unstage / discard action buttons (these don't apply mid-merge)
- Same hover background as other rows

### GitPanel wiring

```tsx
<GitFileList
  onFileClick={(file) => setRightPanel({ kind: "diff", file })}
  onConflictClick={(conflictFile) => setRightPanel({ kind: "conflict", conflictFile })}
/>
```

### Import additions

Add to GitFileList imports:
- `ConflictFile` from `@/lib/tauri-api`
- `AlertTriangle` from `lucide-react`
- `useGitStore` (already imported — just use `refreshConflicts` and `conflicts` from it)

## Out of Scope

- Changes to `ConflictResolver.tsx` — it is used as-is
- Changes to `GitLandingPanel.tsx` — the in-merge banner lives in `GitPanel.tsx`
- Rebase conflict resolution — only merge conflicts are handled
- Multi-file batch resolution — user resolves one file at a time
- Conflict highlighting within file content (inline diff markers) — the existing Monaco side-by-side view is sufficient
