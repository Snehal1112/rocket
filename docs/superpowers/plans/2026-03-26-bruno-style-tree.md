# Bruno-Style Collection Tree Redesign Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Redesign the collection tree panel to match Bruno's aesthetic: plain colored method text, no collection count badge, and a centralised left-bar active state in `tree.tsx`.

**Architecture:** Task 1 extends `TreeItem` in `tree.tsx` with an `active` prop and replaces row hover/selected/active styles — this is the foundation Tasks 2 and 3 build on. Task 2 rewires `RequestNode` to use the new prop and replaces the method Badge with a plain span. Task 3 removes the count badge from `CollectionNode`. Tasks 1 and 2 are sequential (2 depends on 1); Task 3 is independent and runs last.

**Tech Stack:** React, TypeScript, Tailwind CSS (`yarn tsc --noEmit` for verification, `yarn build` for final check)

**Spec:** `docs/superpowers/specs/2026-03-26-bruno-style-tree-design.md`

---

## File Map

| File | Role |
|---|---|
| `src/components/ui/tree.tsx` | Add `active` prop to `TreeItem`; centralise hover/selected/active row styles |
| `src/components/collections/RequestNode.tsx` | Replace Badge with span; wire `active` prop; rename METHOD_BADGE → METHOD_COLOR |
| `src/components/collections/CollectionNode.tsx` | Remove requestCount Badge and its import |

---

### Task 1: Extend `TreeItem` with `active` prop and update row styles

**Files:**
- Modify: `src/components/ui/tree.tsx`

- [ ] **Step 1: Add `active` to the `TreeItem` signature**

Find the current `TreeItem` function signature (~line 51):

```tsx
function TreeItem({
  value,
  className,
  open: openProp,
  onOpenChange,
  children,
  ...props
}: React.ComponentProps<"li"> & {
  value: string;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
}) {
```

Replace with:

```tsx
function TreeItem({
  value,
  className,
  open: openProp,
  onOpenChange,
  active,
  children,
  ...props
}: React.ComponentProps<"li"> & {
  value: string;
  open?: boolean;
  onOpenChange?: (open: boolean) => void;
  active?: boolean;
}) {
```

- [ ] **Step 2: Update the `tree-item-row` div**

Find the `tree-item-row` div (~line 108):

```tsx
<div
  data-slot="tree-item-row"
  data-selected={isSelected || undefined}
  className={cn(
    "flex items-center gap-1 rounded-md px-2 py-1 text-sm cursor-pointer",
    "hover:bg-accent hover:text-accent-foreground",
    "data-[selected]:bg-accent data-[selected]:text-accent-foreground",
  )}
  style={{ paddingLeft: `${(depth + 1) * 12}px` }}
  onClick={handleSelect}
>
```

Replace with:

```tsx
<div
  data-slot="tree-item-row"
  data-selected={isSelected || undefined}
  data-active={active || undefined}
  className={cn(
    "flex items-center gap-1 px-2 py-1 text-sm cursor-pointer",
    "hover:bg-accent/50",
    "data-[selected]:bg-accent/30",
    "data-[active]:border-l-2 data-[active]:border-primary data-[active]:bg-accent/60 data-[active]:text-accent-foreground",
  )}
  style={{ paddingLeft: `${(depth + 1) * 12}px` }}
  onClick={handleSelect}
>
```

Changes: removed `rounded-md`; `hover:bg-accent hover:text-accent-foreground` → `hover:bg-accent/50`; `data-[selected]:bg-accent data-[selected]:text-accent-foreground` → `data-[selected]:bg-accent/30`; added `data-active` attribute and `data-[active]:*` classes.

- [ ] **Step 3: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors.

- [ ] **Step 4: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/ui/tree.tsx
git commit -m "feat: add active prop to TreeItem and centralise Bruno-style row hover/active styles"
```

---

### Task 2: Replace method Badge with plain span and wire `active` prop in `RequestNode`

**Files:**
- Modify: `src/components/collections/RequestNode.tsx`

**Depends on:** Task 1 (uses the `active` prop on `TreeItem`)

- [ ] **Step 1: Remove `Badge` import and rename `METHOD_BADGE` → `METHOD_COLOR`**

Find (~line 25):
```tsx
import { Badge } from '@/components/ui/badge';
```
Delete that line entirely.

Find (~line 38):
```tsx
// Badge color classes per HTTP method (matches RequestList.tsx).
const METHOD_BADGE: Record<string, string> = {
  GET:     'text-emerald-500 border-emerald-500/30 bg-emerald-500/10',
  POST:    'text-amber-500   border-amber-500/30   bg-amber-500/10',
  PUT:     'text-blue-500    border-blue-500/30    bg-blue-500/10',
  PATCH:   'text-violet-500  border-violet-500/30  bg-violet-500/10',
  DELETE:  'text-red-500     border-red-500/30     bg-red-500/10',
  OPTIONS: 'text-cyan-500    border-cyan-500/30    bg-cyan-500/10',
  HEAD:    'text-pink-500    border-pink-500/30    bg-pink-500/10',
};
```

Replace with:

```tsx
// Text color per HTTP method.
const METHOD_COLOR: Record<string, string> = {
  GET:     'text-emerald-500',
  POST:    'text-amber-500',
  PUT:     'text-blue-500',
  PATCH:   'text-violet-500',
  DELETE:  'text-red-500',
  OPTIONS: 'text-cyan-500',
  HEAD:    'text-pink-500',
};
```

- [ ] **Step 2: Update `badgeClass` → `methodColor`**

Find (~line 102):
```tsx
const badgeClass = METHOD_BADGE[method.toUpperCase()] ?? 'text-foreground border-border bg-muted';
```

Replace with:
```tsx
const methodColor = METHOD_COLOR[method.toUpperCase()] ?? 'text-foreground';
```

- [ ] **Step 3: Wire `active` prop on `TreeItem` and remove active class from `TreeItemContent`**

Find (~line 118):
```tsx
<TreeItem value={uid} className="flex-1">
  <TreeItemContent
    className={cn(
      'flex items-center gap-1 w-full px-2 py-0.5 text-xs rounded-sm cursor-pointer',
      active && 'bg-accent/50 text-accent-foreground',
    )}
    onClick={handleClick}
    aria-label={`Open ${method} ${name}`}
  >
```

Replace with:
```tsx
<TreeItem value={uid} active={active} className="flex-1">
  <TreeItemContent
    className="flex items-center gap-1 w-full px-2 py-0.5 text-xs rounded-sm cursor-pointer"
    onClick={handleClick}
    aria-label={`Open ${method} ${name}`}
  >
```

- [ ] **Step 4: Replace `<Badge>` with `<span>`**

Find (~line 127):
```tsx
<Badge variant="outline" className={cn('text-[10px] font-semibold w-14 justify-center shrink-0', badgeClass)}>
  {method}
</Badge>
```

Replace with:
```tsx
<span className={cn('w-10 shrink-0 font-mono text-[10px] font-bold', methodColor)}>
  {method}
</span>
```

- [ ] **Step 5: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors. If `Badge` is still referenced anywhere in the file, the compiler will flag it — remove it.

- [ ] **Step 6: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/collections/RequestNode.tsx
git commit -m "feat: replace method Badge with plain span and wire active prop for Bruno-style tree"
```

---

### Task 3: Remove request count badge from `CollectionNode`

**Files:**
- Modify: `src/components/collections/CollectionNode.tsx`

- [ ] **Step 1: Remove the `Badge` import**

Find (~line 13):
```tsx
import { Badge } from '@/components/ui/badge';
```

Delete that line entirely.

- [ ] **Step 2: Remove the `requestCount` Badge element**

Find (~line 183, inside `TreeItemContent`):
```tsx
<Badge variant="outline" className="ml-auto text-[10px] shrink-0">{summary.requestCount}</Badge>
```

Delete that line entirely.

- [ ] **Step 3: Verify types**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit 2>&1 | head -20
```

Expected: no errors. If `Badge` is still referenced elsewhere in the file, do not remove the import.

- [ ] **Step 4: Commit**

```bash
cd /home/numericlabs/data/Rust/Rocket
git add src/components/collections/CollectionNode.tsx
git commit -m "feat: remove request count badge from collection header"
```

---

## Done

The collection tree now matches Bruno's aesthetic:
- Method shown as compact plain colored monospace text — no box, no border.
- Collection header is clean — no count noise.
- Active request has a left primary-colored bar and a stronger background tint, centralised in `tree.tsx`.
- Hover across all tree rows is a consistent subtle `bg-accent/50` with no rounding.
