# Plan 6: Clone Repository Dialog — Frontend Component

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the `GitCloneDialog` component with a 3-step flow (input → progress → collection picker) and add a trigger button in the Git tab's "not a repo" empty state.

**Architecture:** A shadcn `Dialog` with internal step state. Step 1 collects URL + destination. Step 2 shows a spinner during clone (reuses `GitCredentialsDialog` as popup sub-step for auth). Step 3 shows discovered collections and lets the user pick one to open. Uses `gitClone` from `tauri-api.ts` for cloning and `scanCollectionsInPath` for discovery.

**Tech Stack:** React, TypeScript, shadcn/ui, Lucide React icons, Zustand, Tauri dialog API

**Spec:** `docs/superpowers/specs/2026-03-31-sp-git-polish-design.md` — Phase 3

**Depends on:** Plan 5 (scanCollectionsInPath API), Plan 3 (git-store wiring)

**Hard rules:**
- ALL UI elements must use shadcn/ui primitives — no raw `<button>`, `<input>`, `<div>` for interactive elements
- Icons from Lucide React only — no inline SVGs
- No `.json` files
- Reuse existing `GitCredentialsDialog` for auth — do NOT build inline credentials

---

## Chunk 1: GitCloneDialog Component

### Task 1: Create the `GitCloneDialog` component

**Files:**
- Create: `src/components/git/GitCloneDialog.tsx`

- [ ] **Step 1: Read existing components for reference patterns**

Read these files to understand existing patterns:
- `src/components/git/GitCredentialsDialog.tsx` — dialog pattern, store connection
- `src/lib/tauri-api.ts` — find `gitClone` signature and `openFolderPicker` (the Tauri directory picker)
- `src/stores/git-store.ts` — find `credentials`, `showCredentialsDialog`, `setShowCredentialsDialog`

Note: The Tauri folder picker may be `openFolderPicker` or use `@tauri-apps/plugin-dialog`. Check `tauri-api.ts` for the exact function.

- [ ] **Step 2: Create `src/components/git/GitCloneDialog.tsx`**

The component must:

1. **Props:** `open: boolean`, `onOpenChange: (open: boolean) => void`
2. **State:**
   - `step: 'input' | 'progress' | 'picker'`
   - `repoUrl: string`
   - `destPath: string`
   - `error: string | null`
   - `collections: CollectionScanResult[]`
   - `selectedCollection: string | null` (path of selected collection)
3. **Store connection:** Use `useGitStore` to get `credentials`, `showCredentialsDialog`, `setShowCredentialsDialog`
4. **Step 1 — Input:**

```
DialogContent
  DialogHeader
    DialogTitle: "Clone Repository"
  
  div (space-y-4):
    Label + Input (placeholder: "https://github.com/user/repo.git", value: repoUrl)
    Label + div (flex):
      Input (value: destPath, readOnly or editable)
      Button "Browse" (onClick: open folder picker via Tauri dialog API)
    
    {error && <p className="text-sm text-destructive">{error}</p>}
  
  DialogFooter
    Button "Clone" (disabled if repoUrl or destPath empty, onClick: handleClone)
```

5. **`handleClone` logic:**
   - Set `step = 'progress'`, clear `error`
   - Check if `credentials` exist in git-store; if not, call `setShowCredentialsDialog(true)` and wait
   - Call `gitClone(repoUrl, destPath, credentials)`
   - On success: call `scanCollectionsInPath(destPath)`, set `collections`, set `step = 'picker'`
   - On error: set `error` message, set `step = 'input'`

6. **Step 2 — Progress:**

```
DialogContent
  DialogHeader
    DialogTitle: "Clone Repository"
  
  div (flex items-center justify-center, min-h-[120px]):
    Loader2 (className="h-6 w-6 animate-spin")
    p: "Cloning repository..."
```

7. **Step 3 — Collection Picker:**

```
DialogContent
  DialogHeader
    DialogTitle: "Clone Repository"
    DialogDescription: "Repository cloned successfully"
  
  if collections.length === 0:
    p: "No collections found in this repository."
    DialogFooter: Button "Close"
  
  else if collections.length === 1:
    p: "Found collection: {collections[0].name}"
    DialogFooter: Button "Open" (onClick: handleOpen(collections[0].path))
  
  else:
    RadioGroup (value: selectedCollection, onValueChange: setSelectedCollection):
      for each collection:
        RadioGroupItem + Label showing collection.name and collection.path
    DialogFooter: Button "Open" (disabled if !selectedCollection, onClick: handleOpen)
```

8. **`handleOpen` logic:**
   - This should open the collection in RocketAPI. Find the existing pattern for opening a collection by path — likely involves the workspace store or collection store.
   - Read existing code to find the correct function (e.g., `openWorkspaceFromDisk` or similar)
   - Close the dialog after opening

9. **Credentials sub-step:**
   - The existing `GitCredentialsDialog` is rendered by `WorkspaceGitTab` when `showCredentialsDialog` is true
   - In `handleClone`, if no credentials: set `showCredentialsDialog(true)`, then retry clone after credentials are set
   - Consider using a `useEffect` that watches `credentials` — when it transitions from null to a value while `step === 'progress'`, proceed with the clone

**shadcn/ui components to use:**
- `Dialog`, `DialogContent`, `DialogHeader`, `DialogTitle`, `DialogDescription`, `DialogFooter` from `@/components/ui/dialog`
- `Button` from `@/components/ui/button`
- `Input` from `@/components/ui/input`
- `Label` from `@/components/ui/label`
- `RadioGroup`, `RadioGroupItem` from `@/components/ui/radio-group` (for collection picker)

**Lucide icons to use:**
- `Loader2` — progress spinner
- `FolderOpen` — browse button icon (optional)

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add src/components/git/GitCloneDialog.tsx
git commit -m "feat(frontend): create GitCloneDialog component with 3-step flow"
```

### Task 2: Add "Clone Repository" button to Git tab empty state

**Files:**
- Modify: `src/components/workspace/WorkspaceGitTab.tsx`

- [ ] **Step 1: Read the "not a repo" empty state in `WorkspaceGitTab.tsx`**

Find the section that renders when `isRepo === false`. Currently shows "This workspace is not a Git repository" with an "Initialize" button.

- [ ] **Step 2: Add state and import for the clone dialog**

Add to imports:

```typescript
import { GitCloneDialog } from '@/components/git/GitCloneDialog';
```

Add state:

```typescript
const [showCloneDialog, setShowCloneDialog] = useState(false);
```

- [ ] **Step 3: Add "Clone Repository" button next to "Initialize"**

In the `isRepo === false` block, add a second button:

```tsx
<div className="flex flex-col items-center justify-center gap-3 h-full px-4 text-center">
  <p className="text-sm text-muted-foreground">
    This workspace is not a Git repository.
  </p>
  <div className="flex gap-2">
    <Button onClick={handleInit} size="sm">Initialize</Button>
    <Button onClick={() => setShowCloneDialog(true)} size="sm" variant="outline">
      Clone Repository
    </Button>
  </div>
</div>
```

- [ ] **Step 4: Render the clone dialog**

Add next to the existing dialog renders at the bottom of the component:

```tsx
<GitCloneDialog open={showCloneDialog} onOpenChange={setShowCloneDialog} />
```

- [ ] **Step 5: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 6: Commit**

```bash
git add src/components/workspace/WorkspaceGitTab.tsx
git commit -m "feat(frontend): add Clone Repository button to Git tab empty state"
```

### Task 3: Verify shadcn/ui components are installed and full build passes

**Files:** None created — verification only

- [ ] **Step 1: Check if required shadcn/ui components exist**

Verify these component files exist in `src/components/ui/`:
- `radio-group.tsx`
- `label.tsx`

Run: `ls src/components/ui/radio-group.tsx src/components/ui/label.tsx`

- [ ] **Step 2: Install any missing components**

If any are missing:

```bash
npx shadcn@latest add radio-group
npx shadcn@latest add label
```

- [ ] **Step 3: Verify the full app builds**

Run: `npm run build` (or `yarn build` — check `package.json`)
Expected: builds successfully with no errors

- [ ] **Step 4: Commit (if any components were added)**

```bash
git add src/components/ui/
git commit -m "chore: add missing shadcn/ui components for GitCloneDialog"
```
