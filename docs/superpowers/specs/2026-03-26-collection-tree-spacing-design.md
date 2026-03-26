# Collection Tree Spacing Enhancement

**Date:** 2026-03-26
**Branch:** feat/ux-workflows
**Goal:** Make the collection tree compact and consistent — VS Code-style tight rows with uniform indentation guides.

## Problem

The tree has three node levels (collection, folder, request) with inconsistent spacing across each:

- Vertical padding differs: `py-1.5` on collection rows vs `py-1` on folder/request rows.
- Icon-to-label gaps differ: `gap-1.5` on collection, `gap-1` on folder, `gap-1.5` on request.
- `FolderNode` and `RequestNode` both mix `px-2 pl-4` — the `pl-4` overrides the left side of `px-2`, which is a code smell and does not scale with depth.
- The indentation guide containers use different offsets: `ml-3 pl-2` in `CollectionNode` vs `ml-4 pl-3` in `FolderNode`, so guide lines do not align across levels.

## Design

Apply a single consistent spacing token set across all three node files. No structural changes.

### Row classes (all three nodes)

| Property | Before | After |
|---|---|---|
| Vertical padding | `py-1.5` (collection) or `py-1` (folder/request) | `py-1` everywhere |
| Icon-to-label gap | `gap-1.5` or `gap-1` | `gap-1` everywhere |
| Left padding | `px-2 pl-4` (folder/request) or `px-2` (collection) | `px-2` everywhere |

### Indentation guide container (folder children)

`FolderNode` children wrapper changes from `pl-3 border-l border-border/30 ml-4` to `pl-2 border-l border-border/30 ml-3` — matching the container already used in `CollectionNode`. Every depth level now increments by the same `ml-3 pl-2` offset.

## Files Changed

| File | Change |
|---|---|
| `src/components/collections/CollectionNode.tsx` | `py-1.5 gap-1.5` → `py-1 gap-1` on `TreeItemContent` |
| `src/components/collections/FolderNode.tsx` | Remove `pl-4`, fix `gap-1` (already correct), fix children wrapper offset |
| `src/components/collections/RequestNode.tsx` | Remove `pl-4`, `gap-1.5` → `gap-1` on `TreeItemContent` |

## Out of Scope

- Icon sizes
- Font sizes
- Hover/active background colours
- Any structural refactoring of `tree.tsx`
