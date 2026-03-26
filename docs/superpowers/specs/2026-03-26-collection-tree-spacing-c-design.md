# Collection Tree Spacing — Approach C (Full VS Code-style Pass)

**Date:** 2026-03-26
**Branch:** feat/ux-workflows
**Goal:** Push the tree to full VS Code-style compactness: tighter rows and uniform icon sizing on top of the Approach A baseline.

## Baseline (Approach A — already committed)

- All three nodes use `py-1 gap-1 px-2` on `TreeItemContent`
- `pl-4` removed from `FolderNode` and `RequestNode` rows
- Both indent guide containers use `ml-3 pl-2` (12 px per depth level)

## Delta — Approach C adds

### 1. Tighter rows: `py-1` → `py-0.5`

Apply to all three nodes' `TreeItemContent` className.

| File | Current | After |
|---|---|---|
| `CollectionNode.tsx` | `py-1` | `py-0.5` |
| `FolderNode.tsx` | `py-1` | `py-0.5` |
| `RequestNode.tsx` | `py-1` | `py-0.5` |

### 2. Uniform folder icons: → `h-3 w-3`

| File | Element | Current | After |
|---|---|---|---|
| `CollectionNode.tsx` | `FolderOpen` / `Folder` inline icon | `h-4 w-4` | `h-3 w-3` |
| `FolderNode.tsx` | `FolderOpen` / `Folder` inline icon | `h-3.5 w-3.5` | `h-3 w-3` |
| `FolderNode.tsx` | `Folder` in DragOverlay | `h-3.5 w-3.5` | `h-3 w-3` |

`RequestNode.tsx` has no folder icons — only the method `Badge` and already-`h-3 w-3` `GripVertical`/`MoreHorizontal` icons.

### 3. Indent — no change

`ml-3 pl-2` per level is already consistent 12 px after Approach A. Nothing to touch.

## Files Changed

| File | Changes |
|---|---|
| `src/components/collections/CollectionNode.tsx` | `py-1` → `py-0.5`; `FolderOpen`/`Folder` `h-4 w-4` → `h-3 w-3` |
| `src/components/collections/FolderNode.tsx` | `py-1` → `py-0.5`; `FolderOpen`/`Folder` inline + DragOverlay `h-3.5 w-3.5` → `h-3 w-3` |
| `src/components/collections/RequestNode.tsx` | `py-1` → `py-0.5` only |

## Out of Scope

- Dropdown menu icon sizes
- Font sizes
- Hover/active background colours
- Any structural refactoring of `tree.tsx`
- Badge sizing
