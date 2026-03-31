# Plan 8: Git Tab Redesign — Restructure WorkspaceGitTab Layout

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Restructure `WorkspaceGitTab` from its current vertical scrolling panel with sub-tabs into Bruno's two-panel layout: left panel (changes + commit form + links) and right panel (landing state / diff / commits / stashes).

**Architecture:** The component manages a `rightPanelView` state that determines what the right panel shows. Left panel is a fixed-width column with `GitFileList` + `GitLinksSection`. Right panel switches between `GitLandingPanel`, `DiffViewer`, `GitCommitLog`, and `GitStashSection` based on user actions.

**Tech Stack:** React, TypeScript, shadcn/ui, Lucide React icons, Zustand

**Spec:** `docs/superpowers/specs/2026-03-31-sp-git-polish-design.md` — Phase 4

**Depends on:** Plan 7 (GitLandingPanel, GitLinksSection, GitFileList components)

**Hard rules:**
- ALL UI elements must use shadcn/ui primitives
- Icons from Lucide React only
- No raw HTML interactive elements

---

## Chunk 1: Restructure WorkspaceGitTab

### Task 1: Rewrite `WorkspaceGitTab` with two-panel layout

**Files:**
- Modify: `src/components/workspace/WorkspaceGitTab.tsx`

- [ ] **Step 1: Read the current `WorkspaceGitTab.tsx` fully**

Understand:
- All imports
- All state variables
- The `checkAndLoad` logic
- The `isRepo === null` (loading), `isRepo === false` (not a repo), and `isRepo === true` (repo) branches
- How `activeSubTab` currently drives the Tabs layout
- Where `GitCredentialsDialog` and `GitRemotesDialog` are rendered

- [ ] **Step 2: Define the new right panel view type**

Add a type for what the right panel shows:

```typescript
type RightPanelView =
  | { kind: 'landing' }
  | { kind: 'diff'; file: FileStatus }
  | { kind: 'commits' }
  | { kind: 'stashes' };
```

Replace the `activeSubTab` state with:

```typescript
const [rightPanel, setRightPanel] = useState<RightPanelView>({ kind: 'landing' });
const [showRemotesDialog, setShowRemotesDialog] = useState(false);
```

- [ ] **Step 3: Rewrite the repo-active layout (when `isRepo === true`)**

Replace the entire Tabs-based layout with:

```tsx
<div className="flex flex-col h-full">
  {/* Two-panel layout */}
  <div className="flex-1 flex overflow-hidden">
    
    {/* LEFT PANEL — fixed width */}
    <div className="w-80 border-r border-border/70 flex flex-col overflow-hidden">
      
      {/* Collection name header */}
      <div className="flex items-center gap-2 px-3 py-2.5 border-b border-border/70 shrink-0">
        <Package className="h-3.5 w-3.5 text-muted-foreground" />
        <span className="text-sm font-medium truncate">
          {workspace?.name ?? 'Collection'}
        </span>
      </div>
      
      {/* Changes section with commit form */}
      <div className="shrink-0 px-3 pt-3 pb-2 space-y-2 border-b border-border/70">
        <Collapsible defaultOpen>
          <CollapsibleTrigger className="flex items-center gap-1 text-sm font-medium text-primary">
            <ChevronDown className="h-3 w-3" />
            Changes
          </CollapsibleTrigger>
          <CollapsibleContent className="pt-2 space-y-2">
            <GitCommitForm />
          </CollapsibleContent>
        </Collapsible>
      </div>
      
      {/* File list — takes remaining space */}
      <GitFileList
        onFileClick={(file) => setRightPanel({ kind: 'diff', file })}
      />
      
      {/* Links section — pinned to bottom */}
      <div className="shrink-0 border-t border-border/70">
        <GitLinksSection
          onNavigate={(view) => setRightPanel({ kind: view })}
          onOpenRemotes={() => setShowRemotesDialog(true)}
        />
      </div>
    </div>
    
    {/* RIGHT PANEL — flexible */}
    <div className="flex-1 overflow-hidden">
      {rightPanel.kind === 'landing' && <GitLandingPanel />}
      {rightPanel.kind === 'diff' && (
        <DiffViewForFile file={rightPanel.file} collectionPath={workspacePath!} />
      )}
      {rightPanel.kind === 'commits' && <GitCommitLog />}
      {rightPanel.kind === 'stashes' && (
        <ScrollArea className="h-full">
          <div className="p-4">
            <GitStashSection />
          </div>
        </ScrollArea>
      )}
    </div>
    
  </div>
  
  {/* Dialogs */}
  {showCredentialsDialog && <GitCredentialsDialog />}
  <GitRemotesDialog open={showRemotesDialog} onOpenChange={setShowRemotesDialog} />
</div>
```

Note: `DiffViewForFile` is a small wrapper that fetches the diff for a file and renders the existing `DiffViewer`. See Task 2.

- [ ] **Step 4: Update imports**

Remove unused imports (Tabs, TabsList, TabsTrigger, TabsContent) and add new ones:

```typescript
import { GitLandingPanel } from '@/components/git/GitLandingPanel';
import { GitLinksSection } from '@/components/git/GitLinksSection';
import { GitFileList } from '@/components/git/GitFileList';
import { GitRemotesDialog } from '@/components/git/GitRemotesDialog';
import { Collapsible, CollapsibleTrigger, CollapsibleContent } from '@/components/ui/collapsible';
import { Package, ChevronDown } from 'lucide-react';
```

Remove imports for `Tabs`, `TabsList`, `TabsTrigger`, `TabsContent` if no longer used.
Remove `activeSubTab` state and the `useEffect` that refreshed log on tab switch.

- [ ] **Step 5: Keep the "not a repo" and "loading" states unchanged**

The `isRepo === null` (loading spinner) and `isRepo === false` (Initialize + Clone buttons) blocks stay exactly as they are. Only the `isRepo === true` block is rewritten.

- [ ] **Step 6: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 7: Commit**

```bash
git add src/components/workspace/WorkspaceGitTab.tsx
git commit -m "feat(frontend): restructure WorkspaceGitTab to Bruno-style two-panel layout"
```

### Task 2: Create `DiffViewForFile` wrapper component

**Files:**
- Create: `src/components/git/DiffViewForFile.tsx`

- [ ] **Step 1: Read existing diff viewing pattern**

Read `src/components/git/GitStagedFiles.tsx` and `src/components/git/GitChangedFiles.tsx` to see how they call `gitDiff` / `gitDiffStaged` and pass data to the diff viewer. Also read the existing `DiffViewer` component to understand its props.

- [ ] **Step 2: Create `src/components/git/DiffViewForFile.tsx`**

A wrapper that takes a `FileStatus` and `collectionPath`, fetches the diff, and renders it inline (not as a separate tab).

```typescript
interface DiffViewForFileProps {
  file: FileStatus;
  collectionPath: string;
}
```

1. **State:** `diff: FileDiff | null`, `loading: boolean`, `error: string | null`
2. **Effect:** When `file` or `collectionPath` changes, fetch the diff:
   - If `file.staged` → call `gitDiffStaged(collectionPath, file.path)`
   - Else → call `gitDiff(collectionPath, file.path)`
   - Set `diff` on success, `error` on failure
3. **Render:**
   - Loading: centered `Loader2` spinner
   - Error: error message
   - Diff loaded: render a header with file path + additions/deletions count, then the `DiffViewer` component (or Monaco diff editor) with `diff.oldContent` and `diff.newContent`

Look at how the existing `DiffViewer` component is used (check its props) and render it the same way. If the existing diff viewing opens in a pane tab (not inline), create a simpler inline Monaco diff display:

```tsx
<div className="flex flex-col h-full">
  {/* Diff header */}
  <div className="flex items-center justify-between px-4 py-2 border-b border-border/70 shrink-0">
    <span className="text-sm font-medium">{file.path}</span>
    <span className="text-xs text-muted-foreground">
      +{diff.additions()} −{diff.deletions()}
    </span>
  </div>
  {/* Diff content — reuse existing DiffViewer */}
  <div className="flex-1 overflow-hidden">
    <DiffViewer
      oldContent={diff.oldContent ?? ''}
      newContent={diff.newContent ?? ''}
      filePath={file.path}
    />
  </div>
</div>
```

Adapt the prop names to match the actual `DiffViewer` component interface.

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add src/components/git/DiffViewForFile.tsx
git commit -m "feat(frontend): create DiffViewForFile inline wrapper component"
```

### Task 3: Update `GitCommitForm` to fit the new layout

**Files:**
- Modify: `src/components/git/GitCommitForm.tsx`

- [ ] **Step 1: Read current `GitCommitForm.tsx`**

Understand:
- Current layout and styling
- How the commit message and button work
- Any extra padding/margin that was designed for the old scrolling layout

- [ ] **Step 2: Adjust styling for the left panel context**

The commit form now lives inside a narrower left panel (~320px) instead of a full-width scrolling area. Adjustments may include:
- Ensure the input and button are `w-full`
- Use compact spacing (`space-y-2` not `space-y-4`)
- Make the commit button a prominent primary style (like Bruno's amber "Commit Changes" button — but using RocketAPI's primary color)
- Add a check icon to the commit button: `<Check className="h-3.5 w-3.5" />` + "Commit Changes"
- Ensure the textarea/input doesn't have excessive height — a single-line `Input` is fine for the narrow panel (Bruno uses a single-line input)

If the current form uses a `Textarea`, consider changing to an `Input` for compactness, or keep `Textarea` with `rows={1}` and auto-expand.

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 4: Verify the full app builds**

Run: `npm run build` (or `yarn build`)
Expected: builds successfully

- [ ] **Step 5: Commit**

```bash
git add src/components/git/GitCommitForm.tsx
git commit -m "refactor(frontend): adjust GitCommitForm styling for left panel layout"
```
