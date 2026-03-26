# Collection Tree Spacing — Approach C Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Push the collection tree to full VS Code-style compactness by tightening row padding to `py-0.5` and reducing folder icon sizes to `h-3 w-3` across all three node levels.

**Architecture:** Pure Tailwind class edits on top of the Approach A baseline (already committed). No logic changes, no new files. Approach A already unified `py-1 gap-1 px-2` and aligned guide containers — this plan only applies the remaining delta.

**Tech Stack:** React, TypeScript, Tailwind CSS, Vite (`yarn dev` for visual check, `yarn build` for type verification)

**Spec:** `docs/superpowers/specs/2026-03-26-collection-tree-spacing-c-design.md`

---

## File Map

| File | Changes |
|---|---|
| `src/components/collections/CollectionNode.tsx` | `py-1` → `py-0.5` on TreeItemContent; `h-4 w-4` → `h-3 w-3` on FolderOpen/Folder icons |
| `src/components/collections/FolderNode.tsx` | `py-1` → `py-0.5` on TreeItemContent; `h-3.5 w-3.5` → `h-3 w-3` on FolderOpen/Folder inline icons and DragOverlay Folder icon |
| `src/components/collections/RequestNode.tsx` | `py-1` → `py-0.5` on TreeItemContent only |

---

### Task 1: Tighten CollectionNode row and shrink folder icons

**Files:**
- Modify: `src/components/collections/CollectionNode.tsx` (~lines 162, 168–169)

- [ ] **Step 1: Edit TreeItemContent — `py-1` → `py-0.5`**

Find (~line 162):
```tsx
className="flex items-center gap-1 w-full px-2 py-1 text-xs rounded-sm cursor-pointer"
```

Change to:
```tsx
className="flex items-center gap-1 w-full px-2 py-0.5 text-xs rounded-sm cursor-pointer"
```

One token changes: `py-1` → `py-0.5`.

- [ ] **Step 2: Shrink FolderOpen icon — `h-4 w-4` → `h-3 w-3`**

Find (~line 168):
```tsx
? <FolderOpen className="h-4 w-4 shrink-0 text-primary" />
```

Change to:
```tsx
? <FolderOpen className="h-3 w-3 shrink-0 text-primary" />
```

- [ ] **Step 3: Shrink Folder icon — `h-4 w-4` → `h-3 w-3`**

Find (~line 169):
```tsx
: <Folder className="h-4 w-4 shrink-0 text-primary" />
```

Change to:
```tsx
: <Folder className="h-3 w-3 shrink-0 text-primary" />
```

- [ ] **Step 4: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/collections/CollectionNode.tsx
git commit -m "fix: py-0.5 and h-3 w-3 folder icons on CollectionNode"
```

---

### Task 2: Tighten FolderNode row and shrink all folder icons

**Files:**
- Modify: `src/components/collections/FolderNode.tsx` (~lines 118, 119, 206)

- [ ] **Step 1: Edit TreeItemContent — `py-1` → `py-0.5`**

Find (~line 118):
```tsx
<TreeItemContent className="flex items-center gap-1 w-full px-2 py-1 text-xs rounded-sm cursor-pointer">
```

Change to:
```tsx
<TreeItemContent className="flex items-center gap-1 w-full px-2 py-0.5 text-xs rounded-sm cursor-pointer">
```

- [ ] **Step 2: Shrink inline FolderOpen/Folder icons — `h-3.5 w-3.5` → `h-3 w-3`**

Find (~line 119):
```tsx
{open ? <FolderOpen className="h-3.5 w-3.5 shrink-0 text-muted-foreground" /> : <Folder className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />}
```

Change to:
```tsx
{open ? <FolderOpen className="h-3 w-3 shrink-0 text-muted-foreground" /> : <Folder className="h-3 w-3 shrink-0 text-muted-foreground" />}
```

- [ ] **Step 3: Shrink DragOverlay Folder icon — `h-3.5 w-3.5` → `h-3 w-3`**

Find (~line 206, inside the DragOverlay block):
```tsx
<Folder className="h-3.5 w-3.5 text-muted-foreground" />
```

Change to:
```tsx
<Folder className="h-3 w-3 text-muted-foreground" />
```

- [ ] **Step 4: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/collections/FolderNode.tsx
git commit -m "fix: py-0.5 and h-3 w-3 folder icons on FolderNode"
```

---

### Task 3: Tighten RequestNode row

**Files:**
- Modify: `src/components/collections/RequestNode.tsx` (~line 121)

- [ ] **Step 1: Edit TreeItemContent — `py-1` → `py-0.5`**

Find (~line 121, inside the `cn(...)` call):
```tsx
className={cn(
  'flex items-center gap-1 w-full px-2 py-1 text-xs rounded-sm cursor-pointer',
  active && 'bg-accent/50 text-accent-foreground',
)}
```

Change to:
```tsx
className={cn(
  'flex items-center gap-1 w-full px-2 py-0.5 text-xs rounded-sm cursor-pointer',
  active && 'bg-accent/50 text-accent-foreground',
)}
```

One token changes: `py-1` → `py-0.5`. The `active &&` line is unchanged.

- [ ] **Step 2: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 3: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/collections/RequestNode.tsx
git commit -m "fix: py-0.5 on RequestNode tree row"
```

---

## Done

All three node levels now use `py-0.5 gap-1 px-2` for compact VS Code-style rows. Folder icons are uniformly `h-3 w-3` across inline and drag-overlay appearances.
