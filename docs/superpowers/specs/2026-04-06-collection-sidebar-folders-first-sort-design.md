# Collection Sidebar: Folders-First Sort

**Date:** 2026-04-06  
**Status:** Approved

## Problem

The collection sidebar currently renders items in whatever order the backend returns them. Folders and requests are interleaved with no guaranteed ordering. Bruno sorts folders before requests, with each group sorted alphabetically by name. Rocket should match this behaviour.

## Goal

In the collection tree (both at collection root level and inside any folder), render:
1. All folders first, sorted A–Z by name (case-insensitive).
2. All requests after, sorted A–Z by name (case-insensitive).

This applies at every level of nesting.

## Approach: Shared sort utility

Extract one pure function `sortItemsFoldersFirst(items: CollectionItem[]): CollectionItem[]` into `src/lib/collection-utils.ts`. Both `CollectionNode` and `FolderNode` call it on their `filteredItems` array before rendering.

### Why this approach

- Single source of truth — the comparator lives in exactly one place.
- Pure function — easy to unit-test without rendering components.
- Minimal blast radius — only two component files change, one utility file is added.
- Frontend-only — no Rust changes needed; sorting is a display concern.

## Sorting rule

```
compare(a, b):
  if a is folder and b is request → a comes first
  if a is request and b is folder → b comes first
  otherwise (same type) → locale-insensitive alphabetical on name
```

The sort is stable (Array.prototype.sort is stable in V8/SpiderMonkey). Equal names keep their relative backend order.

## Files

| File | Change |
|---|---|
| `src/lib/collection-utils.ts` | New — exports `sortItemsFoldersFirst` |
| `src/lib/collection-utils.test.ts` | New — unit tests for the sort function |
| `src/components/collections/CollectionNode.tsx` | Apply sort to `filteredItems` before render |
| `src/components/collections/FolderNode.tsx` | Apply sort to `filteredItems` before render |

## Unit test cases

- Empty array → empty array.
- All folders → alphabetical.
- All requests → alphabetical.
- Mixed → folders first (alpha), then requests (alpha).
- Case-insensitive: `"Zebra"` folder sorts before `"apple"` request but after `"alpha"` folder.
- Nested folders: sorting is applied at each level independently (each component call handles its own level).

## Out of scope

- Drag-to-reorder (the existing `reorderItems` backend API is unused and stays unused).
- Sorting collections themselves in the sidebar (only the tree items within a collection).
- Any backend changes.
