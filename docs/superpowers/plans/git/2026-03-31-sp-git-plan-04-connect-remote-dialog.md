# Plan 4: Connect Remote Dialog — Frontend Component

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `GitRemotesDialog` component and add a trigger button in the Git tab header to open it.

**Architecture:** A shadcn `Dialog` component that displays existing remotes in a list with edit/delete, and an "Add Remote" form at the bottom. Each action is immediate (no global submit). The dialog reads from and writes to `git-store` remote actions. A trigger icon button is added to the Git tab header bar.

**Tech Stack:** React, TypeScript, shadcn/ui, Lucide React icons, Zustand

**Spec:** `docs/superpowers/specs/2026-03-31-sp-git-polish-design.md` — Phase 2

**Depends on:** Plan 3 (remote CRUD wired through Tauri + store)

**Hard rules:**
- ALL UI elements must use shadcn/ui primitives — no raw `<button>`, `<input>`, `<div>` for interactive elements
- Icons from Lucide React only — no inline SVGs
- No `.json` files

---

## Chunk 1: GitRemotesDialog Component & Trigger

### Task 1: Create the `GitRemotesDialog` component

**Files:**
- Create: `src/components/git/GitRemotesDialog.tsx`

- [ ] **Step 1: Read existing dialog components for reference**

Read `src/components/git/GitCredentialsDialog.tsx` to understand the existing pattern for git dialogs: how they use shadcn `Dialog`, how they connect to `git-store`, the import style.

- [ ] **Step 2: Create `src/components/git/GitRemotesDialog.tsx`**

The component must:

1. **Props:** `open: boolean`, `onOpenChange: (open: boolean) => void`
2. **State:**
   - `newName: string` (for add form)
   - `newUrl: string` (for add form)
   - `editingRemote: string | null` (name of remote being edited, null if not editing)
   - `editUrl: string` (URL being edited)
   - `deletingRemote: string | null` (name of remote pending delete confirmation)
   - `loading: boolean`
3. **Store connection:** Use `useGitStore` to get `remotes`, `addRemote`, `removeRemote`, `setRemoteUrl`, `refreshRemotes`
4. **On open:** Call `refreshRemotes()`
5. **Layout:**

```
Dialog
  DialogHeader
    DialogTitle: "Manage Remotes"
  DialogContent
    // Remote list (or empty state)
    ScrollArea (if needed)
      For each remote:
        - Normal mode: row with name (bold text), url (truncated, muted), Pencil button, Trash button
        - Edit mode (editingRemote === remote.name): url Input + Check button + X button
        - Delete mode (deletingRemote === remote.name): "Remove '{name}'?" text + Confirm Button + Cancel Button
    
    Separator
    
    // Add Remote form
    div with flex layout:
      Input (placeholder: "origin", value: newName)
      Input (placeholder: "https://github.com/user/repo.git", value: newUrl)
      Button "Add" (disabled if newName or newUrl empty, or newName has spaces, or newName duplicates existing)
```

6. **Handlers:**
   - `handleAdd`: calls `addRemote(newName, newUrl)`, resets form on success
   - `handleEdit`: sets `editingRemote` and `editUrl`
   - `handleSaveEdit`: calls `setRemoteUrl(editingRemote, editUrl)`, exits edit mode
   - `handleCancelEdit`: resets `editingRemote` to null
   - `handleDeleteClick`: sets `deletingRemote`
   - `handleConfirmDelete`: calls `removeRemote(deletingRemote)`, resets `deletingRemote`
   - `handleCancelDelete`: resets `deletingRemote` to null
7. **Validation:**
   - Remote name: non-empty, no spaces (trim + check for whitespace)
   - Remote URL: non-empty
   - Duplicate name: check against `remotes` array

**shadcn/ui components to use:**
- `Dialog`, `DialogContent`, `DialogHeader`, `DialogTitle` from `@/components/ui/dialog`
- `Button` from `@/components/ui/button`
- `Input` from `@/components/ui/input`
- `ScrollArea` from `@/components/ui/scroll-area`
- `Separator` from `@/components/ui/separator`

**Lucide icons to use:**
- `Pencil` — edit button
- `Trash2` — delete button
- `Check` — save edit confirmation
- `X` — cancel edit
- `Plus` — add button (optional, can use text "Add")

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add src/components/git/GitRemotesDialog.tsx
git commit -m "feat(frontend): create GitRemotesDialog component"
```

### Task 2: Add trigger button in the Git tab header

**Files:**
- Modify: `src/components/workspace/WorkspaceGitTab.tsx`

- [ ] **Step 1: Read `WorkspaceGitTab.tsx` to understand the header layout**

Look at the header area where `BranchSelector` and `GitRemoteActions` are rendered. The trigger button will go in this header bar.

- [ ] **Step 2: Add state and import for the dialog**

Add to the imports at top of `WorkspaceGitTab.tsx`:

```typescript
import { GitRemotesDialog } from '@/components/git/GitRemotesDialog';
import { Settings } from 'lucide-react';
```

Add state for dialog visibility:

```typescript
const [showRemotesDialog, setShowRemotesDialog] = useState(false);
```

- [ ] **Step 3: Add the trigger button in the header bar**

In the header `<div>` that contains `BranchSelector` and `GitRemoteActions`, add a Settings icon button. Place it between `BranchSelector` and `GitRemoteActions` (or in the right-side group near `GitRemoteActions`):

```tsx
<Tooltip>
  <TooltipTrigger asChild>
    <Button
      variant="ghost"
      size="icon"
      className="h-6 w-6"
      onClick={() => setShowRemotesDialog(true)}
    >
      <Settings className="h-3 w-3" />
    </Button>
  </TooltipTrigger>
  <TooltipContent><p>Manage Remotes</p></TooltipContent>
</Tooltip>
```

Make sure `Tooltip`, `TooltipTrigger`, `TooltipContent`, and `TooltipProvider` are imported. Check if `WorkspaceGitTab` already has a `TooltipProvider` wrapping the header — if not, wrap the relevant section.

- [ ] **Step 4: Render the dialog**

Add at the bottom of the component return, next to the existing `{showCredentialsDialog && <GitCredentialsDialog />}`:

```tsx
<GitRemotesDialog open={showRemotesDialog} onOpenChange={setShowRemotesDialog} />
```

- [ ] **Step 5: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 6: Commit**

```bash
git add src/components/workspace/WorkspaceGitTab.tsx
git commit -m "feat(frontend): add Manage Remotes trigger button to Git tab header"
```

### Task 3: Verify shadcn/ui components are installed

**Files:** None created — verification only

- [ ] **Step 1: Check if required shadcn/ui components exist**

Verify these component files exist in `src/components/ui/`:
- `dialog.tsx`
- `button.tsx`
- `input.tsx`
- `scroll-area.tsx`
- `separator.tsx`
- `tooltip.tsx`

Run: `ls src/components/ui/dialog.tsx src/components/ui/button.tsx src/components/ui/input.tsx src/components/ui/scroll-area.tsx src/components/ui/separator.tsx src/components/ui/tooltip.tsx`

- [ ] **Step 2: Install any missing components**

If any are missing, install them. The shadcn/ui init command for this project is:

```bash
npx shadcn@latest add <component-name>
```

For example: `npx shadcn@latest add dialog`

- [ ] **Step 3: Verify the full app builds**

Run: `npm run build` (or `yarn build` — check `package.json` for the correct command)
Expected: builds successfully

- [ ] **Step 4: Commit (if any components were added)**

```bash
git add src/components/ui/
git commit -m "chore: add missing shadcn/ui components for GitRemotesDialog"
```
