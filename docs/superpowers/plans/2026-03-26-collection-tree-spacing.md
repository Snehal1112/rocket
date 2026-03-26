# Collection Tree Spacing Enhancement Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the collection tree compact and consistent — uniform `py-1 gap-1 px-2` across all three node levels, with aligned indentation guide offsets.

**Architecture:** Pure Tailwind class edits across three files. No logic changes, no new files. The indentation structure (border-l guide lines) is preserved; only the offset values are unified.

**Tech Stack:** React, TypeScript, Tailwind CSS, Vite (`yarn dev`)

**Spec:** `docs/superpowers/specs/2026-03-26-collection-tree-spacing-design.md`

---

## File Map

| File | Change |
|---|---|
| `src/components/collections/CollectionNode.tsx` | Line ~162: `py-1.5 gap-1.5` → `py-1 gap-1` |
| `src/components/collections/FolderNode.tsx` | Line ~118: remove `pl-4`; Line ~164: `pl-3 ml-4` → `pl-2 ml-3` |
| `src/components/collections/RequestNode.tsx` | Line ~121: remove `pl-4`, `gap-1.5` → `gap-1` |

---

### Task 1: Fix CollectionNode row spacing

**Files:**
- Modify: `src/components/collections/CollectionNode.tsx` (~line 162)

- [ ] **Step 1: Edit the TreeItemContent className**

Find this line in `CollectionNode.tsx`:

```tsx
className="flex items-center gap-1.5 w-full px-2 py-1.5 text-xs rounded-sm cursor-pointer"
```

Change it to:

```tsx
className="flex items-center gap-1 w-full px-2 py-1 text-xs rounded-sm cursor-pointer"
```

Two tokens change: `gap-1.5` → `gap-1` and `py-1.5` → `py-1`.

- [ ] **Step 2: Start the frontend dev server and verify visually**

```bash
yarn dev
```

Open the app in a browser (Vite default: `http://localhost:1420` or `http://localhost:5173`). Expand a collection — the collection header row should now be the same height as folder and request rows below it.

- [ ] **Step 3: Commit**

```bash
git add src/components/collections/CollectionNode.tsx
git commit -m "fix: uniform py-1 gap-1 on CollectionNode tree row"
```

---

### Task 2: Fix FolderNode row and children wrapper

**Files:**
- Modify: `src/components/collections/FolderNode.tsx` (~lines 118, 164)

- [ ] **Step 1: Fix the TreeItemContent className (remove pl-4)**

Find this line in `FolderNode.tsx`:

```tsx
<TreeItemContent className="flex items-center gap-1 w-full px-2 pl-4 py-1 text-xs rounded-sm cursor-pointer">
```

Change it to:

```tsx
<TreeItemContent className="flex items-center gap-1 w-full px-2 py-1 text-xs rounded-sm cursor-pointer">
```

One token removed: `pl-4`. The left padding now comes from `px-2` (8 px), consistent with the other nodes. The container wrapper handles depth-based indentation.

- [ ] **Step 2: Fix the children wrapper offsets**

Find this line (the indentation guide container, inside the `{open && (...)}` block):

```tsx
<div className="pl-3 border-l border-border/30 ml-4">
```

Change it to:

```tsx
<div className="pl-2 border-l border-border/30 ml-3">
```

This matches the `ml-3 pl-2` offset that `CollectionNode` already uses, so every depth level now increments identically.

- [ ] **Step 3: Verify visually**

With `yarn dev` still running, expand a collection that contains folders. The folder rows should align flush with request rows, and nested items inside folders should indent by the same amount as items inside a collection.

- [ ] **Step 4: Commit**

```bash
git add src/components/collections/FolderNode.tsx
git commit -m "fix: remove pl-4 override and align children wrapper to ml-3 pl-2 in FolderNode"
```

---

### Task 3: Fix RequestNode row spacing

**Files:**
- Modify: `src/components/collections/RequestNode.tsx` (~line 121)

- [ ] **Step 1: Edit the TreeItemContent className**

Find this block in `RequestNode.tsx`:

```tsx
className={cn(
  'flex items-center gap-1.5 w-full px-2 pl-4 py-1 text-xs rounded-sm cursor-pointer',
  active && 'bg-accent/50 text-accent-foreground',
)}
```

Change it to:

```tsx
className={cn(
  'flex items-center gap-1 w-full px-2 py-1 text-xs rounded-sm cursor-pointer',
  active && 'bg-accent/50 text-accent-foreground',
)}
```

Two tokens change: `pl-4` removed, `gap-1.5` → `gap-1`.

- [ ] **Step 2: Verify visually**

With `yarn dev` running, open a collection with requests. Check that:
- Request rows are the same height as folder rows.
- The method badge (GET/POST/etc.) aligns with the folder icon at the same horizontal position.
- Requests inside a folder are indented consistently relative to the folder header.

- [ ] **Step 3: Commit**

```bash
git add src/components/collections/RequestNode.tsx
git commit -m "fix: remove pl-4 override and unify gap-1 on RequestNode tree row"
```

---

## Done

All three node levels now share `py-1 gap-1 px-2` row classes and the guide line containers use the same `ml-3 pl-2` offset at every depth.
