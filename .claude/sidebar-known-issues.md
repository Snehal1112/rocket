# Collection sidebar — known issues

Three deferred bugs from the Apr 6 2026 sidebar audit. Lower priority than the
rename double-fire and stuck-on-error bugs that were fixed in the same session.
Pick these up when working on sidebar UX improvements or when a user reports
related friction.

Line numbers verified against `main` on 2026-04-11.

## Issue 1: No user-facing error feedback

All sidebar operations (rename, move, duplicate, delete) only log errors to
`console.error`. Users see no toast, banner, or inline message when an
operation fails.

Affected files:
- `src/components/collections/CollectionNode.tsx` — `handleRename` catch block (~line 128).
- `src/components/collections/FolderNode.tsx` — `handleRename` catch block (~line 93).
- `src/components/collections/RequestNode.tsx` — `handleRename` catch block at line 82.
- `src/components/layout/CollectionsSidebar.tsx` — `handleMove` (~line 170), `handleDuplicate` (~line 228), `confirmDelete` (~line 112).

Fix approach: add a toast/notification system (e.g. sonner or radix toast).
Wrap each catch block to show the error message. Alternatively, add inline
error state per-component.

## Issue 2: "Move to..." always targets collection root

When using "Move to..." on a request, the destination path is hardcoded to
`''` (empty string), so requests always land at the root of the target
collection regardless of source folder structure.

Affected files:
- `src/components/collections/RequestNode.tsx` — line 171: `onClick={() => void onMove(collectionName, path, s.name, '')}`.
- Same pattern in the context menu further down the file.

Fix approaches:
- A) Add a folder picker sub-menu inside "Move to..." that lets the user choose a destination folder within the target collection.
- B) Preserve the relative folder path from source when moving (e.g. `subfolder/req.yml` → `subfolder/req.yml` in the destination).
- C) Keep the current behavior but rename the label to "Move to root of...".

## Issue 3: Stale `renameValue` if props change mid-edit

`renameValue` is initialized via `useState(name)` and only re-synced when the
user clicks "Rename" (which calls `setRenameValue(name)`). If an external
refresh updates the `name` prop while the rename input is already open, the
input shows the old value.

Affected files:
- `src/components/collections/RequestNode.tsx` — line 68: `useState(name)`.
- `src/components/collections/FolderNode.tsx` — `useState(name)` (~line 70).
- `src/components/collections/CollectionNode.tsx` — `useState(summary.name)` (~line 65).

Fix approach: add a `useEffect` to sync `renameValue` when `name` prop changes
and `isRenaming` is true.

```tsx
useEffect(() => {
  if (!isRenaming) setRenameValue(name);
}, [name, isRenaming]);
```

This is a minor edge case — the "Rename" menu click already resets the value,
so it only matters if the rename dialog is open during an external tree
refresh.
