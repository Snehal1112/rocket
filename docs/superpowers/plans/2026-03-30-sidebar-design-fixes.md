# Sidebar Design Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix nine design and accessibility issues in the collection sidebar identified by a UI/UX review: a light-mode chevron visibility bug, missing aria-labels, hidden keyboard targets, a native `window.prompt()` call, a dead button, a missing collection icon, a color-only method badge, faint indent guides, and an unused backdrop-blur layer.

**Architecture:** All changes are isolated to five React/TSX files in `src/components/`. No new files are created. No Rust/Tauri changes. Each task touches one or two files. Changes are purely presentational (className, ARIA attributes, icon additions) plus one logic replacement (`window.prompt` → inline rename state).

**Tech Stack:** React 19, TypeScript 5.8, TailwindCSS v4, Lucide React, shadcn/ui components, existing `METHOD_BADGE_COLOR` map from `src/lib/colors.ts`.

---

## File Map

| File | Tasks |
|---|---|
| `src/components/collections/CollectionNode.tsx` | 1, 2, 4 |
| `src/components/collections/FolderNode.tsx` | 1, 2 |
| `src/components/collections/RequestNode.tsx` | 2, 5 |
| `src/components/layout/CollectionsSidebar.tsx` | 3 |
| `src/components/layout/WorkspaceSection.tsx` | 6 |

---

### Task 1: Fix Chevron Color Bug + Indent Guide Opacity

**Files:**
- Modify: `src/components/collections/CollectionNode.tsx` (lines 229–237, 385)
- Modify: `src/components/collections/FolderNode.tsx` (line 145)

**Problem:** CollectionNode renders expand/collapse chevrons with `text-primary-foreground`. In light mode `--primary-foreground` is white — the chevrons are invisible. Also, indent guide lines use `border-border/30` which is barely visible. Fix is two className changes per file.

- [ ] **Step 1: Fix chevron colors in CollectionNode**

In `src/components/collections/CollectionNode.tsx`, find the two chevron elements (around lines 228–237) and change `text-primary-foreground` to `text-muted-foreground` on both:

```tsx
// Before (line ~230)
<ChevronDown
  className="h-4 w-4 flex-none text-primary-foreground"
  strokeWidth={1.5}
/>
// ...
// Before (line ~234)
<ChevronRight
  className="h-4 w-4 flex-none text-primary-foreground"
  strokeWidth={1.5}
/>
```

```tsx
// After
<ChevronDown
  className="h-4 w-4 flex-none text-muted-foreground"
  strokeWidth={1.5}
/>
// ...
<ChevronRight
  className="h-4 w-4 flex-none text-muted-foreground"
  strokeWidth={1.5}
/>
```

- [ ] **Step 2: Increase indent guide opacity in CollectionNode**

Still in `CollectionNode.tsx`, find the indent guide div (around line 385):

```tsx
// Before
<div className="pl-1.5 border-l border-border/30 ml-2">
```

```tsx
// After
<div className="pl-1.5 border-l border-border/50 ml-2">
```

- [ ] **Step 3: Increase indent guide opacity in FolderNode**

In `src/components/collections/FolderNode.tsx`, find the same pattern (around line 145):

```tsx
// Before
<div className="pl-1.5 border-l border-border/30 ml-2">
```

```tsx
// After
<div className="pl-1.5 border-l border-border/50 ml-2">
```

- [ ] **Step 4: Verify TypeScript compiles**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit
```

Expected: `Done in Xs` with zero errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/collections/CollectionNode.tsx src/components/collections/FolderNode.tsx
git commit -m "fix(sidebar): fix invisible chevrons in light mode, increase indent guide opacity"
```

---

### Task 2: Fix Dropdown Trigger Accessibility (3 Tree Node Files)

**Files:**
- Modify: `src/components/collections/CollectionNode.tsx` (line ~270)
- Modify: `src/components/collections/FolderNode.tsx` (line ~118)
- Modify: `src/components/collections/RequestNode.tsx` (line ~118)

**Problem:** The `...` dropdown trigger button in each tree node has no `aria-label` (screen readers announce it as an unlabelled button) and uses `opacity-0 group-hover:opacity-100` with no focus variant — making it invisible to keyboard-only users. Add `aria-label` and `group-focus-within:opacity-100` to all three.

- [ ] **Step 1: Fix CollectionNode dropdown trigger**

In `src/components/collections/CollectionNode.tsx`, find the DropdownMenuTrigger button (around line 268–274):

```tsx
// Before
<button
  type="button"
  className="absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 hover:bg-muted text-muted-foreground"
  onClick={(e) => e.stopPropagation()}
>
  <MoreHorizontal className="h-3 w-3" />
</button>
```

```tsx
// After
<button
  type="button"
  aria-label={`Actions for ${summary.name}`}
  className="absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 hover:bg-muted text-muted-foreground"
  onClick={(e) => e.stopPropagation()}
>
  <MoreHorizontal className="h-3 w-3" />
</button>
```

- [ ] **Step 2: Fix FolderNode dropdown trigger**

In `src/components/collections/FolderNode.tsx`, find the same button (around line 116–120):

```tsx
// Before
<button type="button" className="absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 hover:bg-muted text-muted-foreground" onClick={(e) => e.stopPropagation()}>
  <MoreHorizontal className="h-3 w-3" />
</button>
```

```tsx
// After
<button
  type="button"
  aria-label={`Actions for ${name}`}
  className="absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 hover:bg-muted text-muted-foreground"
  onClick={(e) => e.stopPropagation()}
>
  <MoreHorizontal className="h-3 w-3" />
</button>
```

- [ ] **Step 3: Fix RequestNode dropdown trigger**

In `src/components/collections/RequestNode.tsx`, find the same button (around line 116–120):

```tsx
// Before
<button
  type="button"
  className="absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 hover:bg-muted text-muted-foreground"
  onClick={(e) => e.stopPropagation()}
>
  <MoreHorizontal className="h-3 w-3" />
</button>
```

```tsx
// After
<button
  type="button"
  aria-label={`Actions for ${name}`}
  className="absolute right-1 h-5 w-5 flex items-center justify-center rounded-sm opacity-0 group-hover:opacity-100 group-focus-within:opacity-100 hover:bg-muted text-muted-foreground"
  onClick={(e) => e.stopPropagation()}
>
  <MoreHorizontal className="h-3 w-3" />
</button>
```

- [ ] **Step 4: Verify TypeScript compiles**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit
```

Expected: `Done in Xs` with zero errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/collections/CollectionNode.tsx src/components/collections/FolderNode.tsx src/components/collections/RequestNode.tsx
git commit -m "fix(a11y): add aria-label and keyboard visibility to tree node action buttons"
```

---

### Task 3: Fix CollectionsSidebar — ARIA Semantics + Toolbar aria-labels + Remove Backdrop Blur

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx` (lines 288, 291–317, 323–349)

**Problem:** Three separate issues in one file:
1. Sidebar container uses `bg-card/50 backdrop-blur-sm` — the blur composites against nothing (sidebar is flush with window edge). Change to `bg-sidebar` using the dedicated sidebar token.
2. Collections/History view switcher buttons have no ARIA tab semantics — screen readers can't identify these as tabs.
3. Plus, Upload, and Layers toolbar buttons use only `title` (tooltip) but no `aria-label` (screen reader attribute).

- [ ] **Step 1: Fix sidebar container styling**

In `src/components/layout/CollectionsSidebar.tsx`, find the outer container div (line ~288):

```tsx
// Before
<div className="h-full flex flex-col bg-card/50 backdrop-blur-sm border-r border-border/50">
```

```tsx
// After
<div className="h-full flex flex-col bg-sidebar border-r border-sidebar-border">
```

- [ ] **Step 2: Add ARIA tab semantics to view switcher**

Find the view switcher section (lines ~291–317). Wrap the buttons in a `role="tablist"` div and add `role="tab"` plus `aria-selected` to each button:

```tsx
// Before
<div className="flex items-center gap-0.5">
  <button
    type="button"
    onClick={() => setView("collections")}
    className={cn(
      "px-2 py-1 text-xs font-medium rounded-md transition-colors",
      view === "collections"
        ? "bg-accent text-accent-foreground"
        : "text-muted-foreground hover:text-foreground hover:bg-accent/50",
    )}
  >
    {multiWorkspaceMode ? "Workspaces" : "Collections"}
  </button>
  <button
    type="button"
    onClick={() => setView("history")}
    className={cn(
      "px-2 py-1 text-xs font-medium rounded-md transition-colors",
      view === "history"
        ? "bg-accent text-accent-foreground"
        : "text-muted-foreground hover:text-foreground hover:bg-accent/50",
    )}
  >
    History
  </button>
</div>
```

```tsx
// After
<div role="tablist" aria-label="Sidebar views" className="flex items-center gap-0.5">
  <button
    type="button"
    role="tab"
    aria-selected={view === "collections"}
    onClick={() => setView("collections")}
    className={cn(
      "px-2 py-1 text-xs font-medium rounded-md transition-colors",
      view === "collections"
        ? "bg-accent text-accent-foreground"
        : "text-muted-foreground hover:text-foreground hover:bg-accent/50",
    )}
  >
    {multiWorkspaceMode ? "Workspaces" : "Collections"}
  </button>
  <button
    type="button"
    role="tab"
    aria-selected={view === "history"}
    onClick={() => setView("history")}
    className={cn(
      "px-2 py-1 text-xs font-medium rounded-md transition-colors",
      view === "history"
        ? "bg-accent text-accent-foreground"
        : "text-muted-foreground hover:text-foreground hover:bg-accent/50",
    )}
  >
    History
  </button>
</div>
```

- [ ] **Step 3: Add aria-label to toolbar action buttons**

Find the three toolbar buttons (lines ~323–349). Add `aria-label` to each. They already have `title` — keep those:

```tsx
// Before
<Button
  variant="ghost"
  size="icon"
  className="h-6 w-6 shrink-0 text-muted-foreground hover:text-foreground"
  onClick={() => setIsCreating(true)}
  title="New Collection"
>
  <Plus className="h-3.5 w-3.5" />
</Button>
<Button
  variant="ghost"
  size="icon"
  className="h-6 w-6 shrink-0 text-muted-foreground hover:text-foreground"
  onClick={() => void handleImport()}
  title="Import Collection"
>
  <Upload className="h-3.5 w-3.5" />
</Button>
<Button
  variant="ghost"
  size="icon"
  className="h-6 w-6 shrink-0 text-muted-foreground hover:text-foreground"
  title={multiWorkspaceMode ? "Switch to single workspace mode" : "Switch to multi-workspace mode"}
  onClick={() => {
    const store = useWorkspaceStore.getState()
    void store.setMultiWorkspaceMode(!store.multiWorkspaceMode)
  }}
>
  <Layers className="h-3.5 w-3.5" />
</Button>
```

```tsx
// After
<Button
  variant="ghost"
  size="icon"
  className="h-6 w-6 shrink-0 text-muted-foreground hover:text-foreground"
  onClick={() => setIsCreating(true)}
  aria-label="New Collection"
  title="New Collection"
>
  <Plus className="h-3.5 w-3.5" />
</Button>
<Button
  variant="ghost"
  size="icon"
  className="h-6 w-6 shrink-0 text-muted-foreground hover:text-foreground"
  onClick={() => void handleImport()}
  aria-label="Import Collection"
  title="Import Collection"
>
  <Upload className="h-3.5 w-3.5" />
</Button>
<Button
  variant="ghost"
  size="icon"
  className="h-6 w-6 shrink-0 text-muted-foreground hover:text-foreground"
  aria-label={multiWorkspaceMode ? "Switch to single workspace mode" : "Switch to multi-workspace mode"}
  title={multiWorkspaceMode ? "Switch to single workspace mode" : "Switch to multi-workspace mode"}
  onClick={() => {
    const store = useWorkspaceStore.getState()
    void store.setMultiWorkspaceMode(!store.multiWorkspaceMode)
  }}
>
  <Layers className="h-3.5 w-3.5" />
</Button>
```

- [ ] **Step 4: Verify TypeScript compiles**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit
```

Expected: `Done in Xs` with zero errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx
git commit -m "fix(a11y): add ARIA tab semantics, aria-labels to toolbar buttons, use sidebar token"
```

---

### Task 4: Add Collection Icon to CollectionNode

**Files:**
- Modify: `src/components/collections/CollectionNode.tsx` (lines 1–9 imports, ~252–255 render)

**Problem:** Collection-level tree rows show only a chevron + name. Folder rows show `Folder`/`FolderOpen` icons. Request rows show a method badge. Collections have no visual anchor icon, making the hierarchy levels hard to distinguish at a glance.

Add a `Layers` icon (already imported for the sidebar toolbar) between the chevron and the collection name. Use `text-sidebar-primary` color to give collection nodes a distinctive accent distinct from folder nodes (`text-muted-foreground`).

- [ ] **Step 1: Add Layers to the import line**

In `src/components/collections/CollectionNode.tsx`, find the lucide import at the top:

```tsx
// Before (line ~1–9)
import {
  FolderPlus,
  Plus,
  Trash2,
  MoreHorizontal,
  ChevronDown,
  ChevronRight,
} from "lucide-react";
```

```tsx
// After
import {
  FolderPlus,
  Layers,
  Plus,
  Trash2,
  MoreHorizontal,
  ChevronDown,
  ChevronRight,
} from "lucide-react";
```

- [ ] **Step 2: Insert icon between chevron and name in TreeItemContent**

Find the `TreeItemContent` render (around lines 221–263). The content area currently renders: `{chevron} {isRenaming ? <Input> : <span name> <ext badge>}`. Add the `Layers` icon between the chevron and the name/input:

```tsx
// Before — the span/input section inside TreeItemContent
{isRenaming ? (
  <Input
    autoFocus
    className="h-6 text-sm flex-1"
    value={renameValue}
    onChange={(e) => setRenameValue(e.target.value)}
    onKeyDown={(e) => {
      if (e.key === "Enter") void handleRename();
      if (e.key === "Escape") setIsRenaming(false);
    }}
    onBlur={() => void handleRename()}
    onClick={(e) => e.stopPropagation()}
  />
) : (
  <>
    <span className="truncate font-medium text-foreground">
      {summary.name}
    </span>
    {summary.refType === 'external' && (
      <span className="ml-auto shrink-0 text-2xs text-muted-foreground bg-muted px-1.5 py-0.5 rounded">
        ext
      </span>
    )}
  </>
)}
```

```tsx
// After — Layers icon added before the name/input
<Layers className="h-3.5 w-3.5 shrink-0 text-primary" />
{isRenaming ? (
  <Input
    autoFocus
    className="h-6 text-sm flex-1"
    value={renameValue}
    onChange={(e) => setRenameValue(e.target.value)}
    onKeyDown={(e) => {
      if (e.key === "Enter") void handleRename();
      if (e.key === "Escape") setIsRenaming(false);
    }}
    onBlur={() => void handleRename()}
    onClick={(e) => e.stopPropagation()}
  />
) : (
  <>
    <span className="truncate font-medium text-foreground">
      {summary.name}
    </span>
    {summary.refType === 'external' && (
      <span className="ml-auto shrink-0 text-2xs text-muted-foreground bg-muted px-1.5 py-0.5 rounded">
        ext
      </span>
    )}
  </>
)}
```

Note: `TreeItemContent` already has `flex gap-3` so spacing is automatic.

- [ ] **Step 3: Verify TypeScript compiles**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit
```

Expected: `Done in Xs` with zero errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/collections/CollectionNode.tsx
git commit -m "feat(sidebar): add Layers icon to collection nodes for visual hierarchy"
```

---

### Task 5: Upgrade RequestNode Method Badge to Use Background Color

**Files:**
- Modify: `src/components/collections/RequestNode.tsx` (line ~26 imports, ~79, ~91)

**Problem:** The HTTP method label in each request row uses only `METHOD_TEXT_COLOR` — color is the only differentiator. `src/lib/colors.ts` already defines `METHOD_BADGE_COLOR` which includes text color + background + border. Use it to give each method a distinct chip with a faint background.

`METHOD_BADGE_COLOR` values look like: `text-emerald-500 dark:text-emerald-400 border-emerald-500/30 bg-emerald-500/10 dark:bg-emerald-500/20`

- [ ] **Step 1: Add METHOD_BADGE_COLOR to the import**

In `src/components/collections/RequestNode.tsx`, find the colors import (around line 26):

```tsx
// Before
import { METHOD_TEXT_COLOR } from '@/lib/colors';
```

```tsx
// After
import { METHOD_BADGE_COLOR } from '@/lib/colors';
```

- [ ] **Step 2: Update the methodColor variable and badge element**

Find the `methodColor` line (around line 79) and the badge `<span>` (around line 91):

```tsx
// Before
const methodColor = METHOD_TEXT_COLOR[method.toUpperCase()] ?? 'text-foreground';

// ...inside TreeItemContent:
<span className={cn('w-10 shrink-0 font-mono text-2xs font-semibold', methodColor)}>
  {method}
</span>
```

```tsx
// After
const methodBadgeColor = METHOD_BADGE_COLOR[method.toUpperCase()] ?? 'text-foreground border-border bg-muted';

// ...inside TreeItemContent:
<span className={cn(
  'shrink-0 font-mono text-2xs font-semibold px-1 py-0.5 rounded border',
  methodBadgeColor,
)}>
  {method}
</span>
```

The fixed `w-10` width is removed — the badge is now naturally sized by its content with padding, which handles long methods like `DELETE` and `OPTIONS` correctly.

- [ ] **Step 3: Verify TypeScript compiles**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit
```

Expected: `Done in Xs` with zero errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/collections/RequestNode.tsx
git commit -m "feat(sidebar): upgrade method badge to use background color chip"
```

---

### Task 6: Fix WorkspaceSection — Inline Rename + Remove Dead Button

**Files:**
- Modify: `src/components/layout/WorkspaceSection.tsx`

**Problem:** Two issues:
1. `handleRename` (line 28) uses `window.prompt()` — a native OS dialog that bypasses the app's design system. Replace with inline rename state using the same pattern as `CollectionNode` and `FolderNode`.
2. The `Plus` button (lines 77–84) calls `onClick={(e) => e.stopPropagation()}` only — it has no actual action. Remove it since it does nothing and confuses users.

- [ ] **Step 1: Add inline rename state**

At the top of `WorkspaceSection`, add `isRenaming` and `renameValue` state. Currently the component imports `useState`. Add:

```tsx
// Add these two lines after the existing useState call
const [isRenaming, setIsRenaming] = useState(false)
const [renameValue, setRenameValue] = useState(workspace.name)
```

Also add `Input` to the imports — it's available from `@/components/ui/input`:

```tsx
// Add to imports at the top of the file
import { Input } from '@/components/ui/input'
```

- [ ] **Step 2: Replace handleRename with inline version**

Find the `handleRename` function (around line 28):

```tsx
// Before
const handleRename = () => {
  const newName = window.prompt('Rename workspace', workspace.name)
  if (newName && newName.trim() && newName.trim() !== workspace.name) {
    void useWorkspaceStore.getState().renameWorkspace(workspace.id, newName.trim())
  }
}
```

```tsx
// After
const handleRename = async () => {
  const trimmed = renameValue.trim()
  if (!trimmed || trimmed === workspace.name) { setIsRenaming(false); return }
  await useWorkspaceStore.getState().renameWorkspace(workspace.id, trimmed)
  setIsRenaming(false)
}
```

- [ ] **Step 3: Replace workspace name span with conditional rename input**

Find the workspace name `<span>` (around line 66–71):

```tsx
// Before
<span
  className="flex-1 truncate text-sm font-medium"
  onClick={handleOpenWorkspace}
>
  {workspace.name}
</span>
```

```tsx
// After
{isRenaming ? (
  <Input
    autoFocus
    className="h-6 text-sm flex-1"
    value={renameValue}
    onChange={(e) => setRenameValue(e.target.value)}
    onKeyDown={(e) => {
      if (e.key === 'Enter') void handleRename()
      if (e.key === 'Escape') { setIsRenaming(false); setRenameValue(workspace.name) }
    }}
    onBlur={() => void handleRename()}
    onClick={(e) => e.stopPropagation()}
  />
) : (
  <span
    className="flex-1 truncate text-sm font-medium"
    onClick={handleOpenWorkspace}
  >
    {workspace.name}
  </span>
)}
```

- [ ] **Step 4: Update context menu Rename action to trigger inline rename**

Find the context menu `Rename workspace` item (around line 92):

```tsx
// Before
<ContextMenuItem onSelect={handleRename}>
  <Pencil className="mr-2 h-4 w-4" /> Rename workspace
</ContextMenuItem>
```

```tsx
// After
<ContextMenuItem onSelect={() => { setRenameValue(workspace.name); setIsRenaming(true) }}>
  <Pencil className="mr-2 h-4 w-4" /> Rename workspace
</ContextMenuItem>
```

- [ ] **Step 5: Remove the dead Plus button**

Find and remove the dead Plus button entirely (lines ~77–84):

```tsx
// Remove this entire block
<Button
  variant="ghost"
  size="icon"
  className="h-5 w-5 opacity-0 group-hover:opacity-100"
  onClick={(e) => e.stopPropagation()}
>
  <Plus className="h-3 w-3" />
</Button>
```

Also remove `Plus` from the lucide import at the top of the file since it is no longer used:

```tsx
// Before
import { ChevronDown, ChevronRight, Plus, LayoutDashboard, Pencil, X } from 'lucide-react'
```

```tsx
// After
import { ChevronDown, ChevronRight, LayoutDashboard, Pencil, X } from 'lucide-react'
```

- [ ] **Step 6: Verify TypeScript compiles**

```bash
cd /home/numericlabs/data/Rust/Rocket && yarn tsc --noEmit
```

Expected: `Done in Xs` with zero errors.

- [ ] **Step 7: Commit**

```bash
git add src/components/layout/WorkspaceSection.tsx
git commit -m "fix(sidebar): replace window.prompt with inline rename, remove dead Plus button"
```

---

## Self-Review

**Spec coverage check:**

| Issue | Task |
|---|---|
| Bug: chevrons invisible in light mode | Task 1, Step 1 |
| window.prompt breaks design system | Task 6, Steps 1–4 |
| Dead Plus button | Task 6, Step 5 |
| Dropdown triggers missing aria-label + keyboard visibility | Task 2 |
| Missing collection icon | Task 4 |
| View tabs lack ARIA semantics | Task 3, Step 2 |
| Toolbar buttons missing aria-label | Task 3, Step 3 |
| Method badge color-only | Task 5 |
| Indent guides too faint | Task 1, Steps 2–3 |
| Unnecessary backdrop-blur | Task 3, Step 1 |

All 9 issues + the bug are covered. No gaps.

**Placeholder scan:** All steps contain exact code. No TBDs.

**Type consistency:** `METHOD_BADGE_COLOR` is imported from `@/lib/colors` in Task 5 — confirmed it exists in `src/lib/colors.ts` as `export const METHOD_BADGE_COLOR: Record<string, string>`. `renameValue` / `isRenaming` state pattern in Task 6 matches exact pattern used in `CollectionNode.tsx` and `FolderNode.tsx`.
