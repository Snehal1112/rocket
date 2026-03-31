# Plan 7: Git Tab Redesign — Landing Panel & Links Section Components

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Create two new components: `GitLandingPanel` (right panel default state with Fetch/Pull/Push, ahead/behind, branch status) and `GitLinksSection` (collapsible Links list at bottom of left panel).

**Architecture:** Both are presentational components that connect to `useGitStore` for data and actions. `GitLandingPanel` replaces the current `GitRemoteActions` placement by putting Fetch/Pull/Push in a centered layout with status info. `GitLinksSection` is a collapsible list that navigates the right panel view.

**Tech Stack:** React, TypeScript, shadcn/ui, Lucide React icons, Zustand

**Spec:** `docs/superpowers/specs/2026-03-31-sp-git-polish-design.md` — Phase 4

**Depends on:** Plans 1–4 (remote CRUD + GitRemotesDialog must exist)

**Hard rules:**
- ALL UI elements must use shadcn/ui primitives — no raw HTML elements
- Icons from Lucide React only — no inline SVGs
- RocketAPI's existing color tokens — no custom colors

---

## Chunk 1: New Components

### Task 1: Create `GitLandingPanel` component

**Files:**
- Create: `src/components/git/GitLandingPanel.tsx`

- [ ] **Step 1: Read existing components for reference**

Read these files to understand patterns:
- `src/components/git/GitRemoteActions.tsx` — how Fetch/Pull/Push actions work
- `src/stores/git-store.ts` — find `status` (for `ahead`/`behind`), `push`, `pull`, `fetch` actions
- `src/components/ui/button.tsx` — shadcn Button API

- [ ] **Step 2: Create `src/components/git/GitLandingPanel.tsx`**

The component must:

1. **Props:** none (reads from git-store)
2. **Store connection:** `useGitStore` to get `status`, `push`, `pull`, `fetch`, `credentials`, `setShowCredentialsDialog`
3. **Local state:**
   - `pushing`, `pulling`, `fetching` — loading booleans (same pattern as `GitRemoteActions`)
   - `lastFetched: string | null` — updated when fetch completes (store this locally, set to `new Date().toLocaleTimeString()` after fetch)
4. **Layout (vertically centered in parent):**

```
div (flex flex-col items-center justify-center h-full px-6)
  
  // Git icon (large, muted)
  GitBranch icon (Lucide, h-12 w-12, text-muted-foreground/30)
  
  // Helper text
  p (text-sm text-muted-foreground text-center max-w-[280px] mt-4 mb-6)
    "Perform git actions or open files from sidebar to view"
  
  // Fetch / Pull / Push button group
  div (flex gap-2 mb-6)
    Button variant="outline" size="sm" (onClick: handleFetch, disabled: fetching)
      RefreshCw icon (h-3.5 w-3.5) + "Fetch"
    Button variant="outline" size="sm" (onClick: handlePull, disabled: pulling)
      ArrowDown icon (h-3.5 w-3.5) + "Pull"
    Button variant="outline" size="sm" (onClick: handlePush, disabled: pushing)
      ArrowUp icon (h-3.5 w-3.5) + "Push"
    (Show Loader2 spinner replacing icon when loading)
  
  // Last fetched
  p (text-xs text-muted-foreground flex items-center gap-1.5 mb-1.5)
    Clock icon (h-3 w-3)
    "Last fetched: " + span(font-medium text-foreground) "{lastFetched ?? 'Never'}"
  
  // Ahead / Behind
  p (text-xs text-muted-foreground mb-4)
    "↑ {status?.ahead ?? 0} Ahead  |  ↓ {status?.behind ?? 0} Behind"
  
  // Branch status badge
  div (flex items-center gap-1.5 text-xs border rounded-md px-3 py-1.5)
    if status?.isClean && ahead === 0 && behind === 0:
      Check icon (h-3.5 w-3.5 text-emerald-500) + "Your branch is up to date"
    else if behind > 0:
      AlertCircle icon (h-3.5 w-3.5 text-amber-500) + "{behind} commits behind"
    else:
      GitCommit icon (h-3.5 w-3.5 text-muted-foreground) + "{ahead} commits ahead"
```

5. **Handlers:** Same pattern as `GitRemoteActions` — set loading, call store action, unset loading. For push/pull, check credentials first.

**Lucide icons:** `GitBranch`, `RefreshCw`, `ArrowDown`, `ArrowUp`, `Loader2`, `Clock`, `Check`, `AlertCircle`, `GitCommit`

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add src/components/git/GitLandingPanel.tsx
git commit -m "feat(frontend): create GitLandingPanel component for right panel default state"
```

### Task 2: Create `GitLinksSection` component

**Files:**
- Create: `src/components/git/GitLinksSection.tsx`

- [ ] **Step 1: Check if shadcn Collapsible component exists**

Run: `ls src/components/ui/collapsible.tsx`

If missing, install it:
```bash
npx shadcn@latest add collapsible
```

- [ ] **Step 2: Create `src/components/git/GitLinksSection.tsx`**

The component must:

1. **Props:**
   - `onNavigate: (view: 'commits' | 'stashes') => void` — callback when a link is clicked to switch right panel view
   - `onOpenRemotes: () => void` — callback to open `GitRemotesDialog`
2. **State:** `open: boolean` (collapsible state, default `true`)
3. **Layout:**

```
Collapsible (open, onOpenChange)
  CollapsibleTrigger (asChild)
    div (flex items-center gap-1 px-3 py-2 cursor-pointer text-sm font-medium text-primary)
      ChevronDown icon (h-3 w-3, rotated when closed)
      "Links"
  
  CollapsibleContent
    div (px-3 pb-2 space-y-0.5)
      // Commits link
      Button variant="ghost" size="sm" className="w-full justify-start gap-2 h-8 text-sm font-normal"
        onClick: () => onNavigate('commits')
        History icon (h-3.5 w-3.5 text-muted-foreground) + "Commits"
      
      // Stashes link
      Button variant="ghost" size="sm" className="w-full justify-start gap-2 h-8 text-sm font-normal"
        onClick: () => onNavigate('stashes')
        Archive icon (h-3.5 w-3.5 text-muted-foreground) + "Stashes"
      
      // Remotes link
      Button variant="ghost" size="sm" className="w-full justify-start gap-2 h-8 text-sm font-normal"
        onClick: onOpenRemotes
        Link icon (h-3.5 w-3.5 text-muted-foreground) + "Remotes"
```

**shadcn/ui components:** `Collapsible`, `CollapsibleTrigger`, `CollapsibleContent`, `Button`

**Lucide icons:** `ChevronDown`, `History`, `Archive`, `Link`

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add src/components/git/GitLinksSection.tsx
git commit -m "feat(frontend): create GitLinksSection collapsible component"
```

### Task 3: Create `GitFileList` component with status badges

**Files:**
- Create: `src/components/git/GitFileList.tsx`

- [ ] **Step 1: Read existing file list components**

Read `src/components/git/GitStagedFiles.tsx`, `src/components/git/GitChangedFiles.tsx`, and `src/components/git/GitFileRow.tsx` to understand:
- How files are rendered
- How stage/unstage/discard actions work
- How file click opens a diff tab

- [ ] **Step 2: Create `src/components/git/GitFileList.tsx`**

This component combines staged and unstaged file lists into a single component with Bruno-style layout (commit form at top, then unstaged, then staged).

1. **Props:**
   - `onFileClick: (file: FileStatus) => void` — callback when a file is clicked (to show diff in right panel)
2. **Store connection:** `useGitStore` for `status`, `stageFiles`, `stageAll`, `unstageFiles`, `unstageAll`, `discardFiles`, `collectionPath`
3. **Derived data:**
   - `staged = status?.files.filter(f => f.staged) ?? []`
   - `unstaged = status?.files.filter(f => !f.staged) ?? []`
4. **Status badge helper:**

```typescript
function statusBadge(status: GitStatusKind): { letter: string; className: string } {
  switch (status) {
    case 'modified': return { letter: 'M', className: 'text-amber-600 dark:text-amber-400' };
    case 'added': return { letter: 'A', className: 'text-emerald-600 dark:text-emerald-400' };
    case 'deleted': return { letter: 'D', className: 'text-red-600 dark:text-red-400' };
    case 'untracked': return { letter: 'U', className: 'text-amber-600 dark:text-amber-400' };
    case 'renamed': return { letter: 'R', className: 'text-blue-600 dark:text-blue-400' };
    case 'conflicted': return { letter: 'C', className: 'text-red-600 dark:text-red-400' };
    default: return { letter: '?', className: 'text-muted-foreground' };
  }
}
```

5. **Layout:**

```
ScrollArea (className="flex-1")
  div (p-3 space-y-1)
    
    // Unstaged section header
    div (flex items-center justify-between px-2 py-1)
      span (text-xs font-medium text-muted-foreground) "Unstaged Changes"
      div (flex items-center gap-1.5)
        Tooltip: Button ghost icon-only (h-5 w-5) → RotateCcw icon → discard all
        Tooltip: Button ghost icon-only (h-5 w-5) → Plus icon → stage all
        span (text-xs text-muted-foreground) "{unstaged.length}"
    
    // Unstaged file rows
    for each file in unstaged:
      div (flex items-center justify-between px-2 py-1 rounded-md hover:bg-muted/50 cursor-pointer)
        onClick: () => onFileClick(file)
        span (text-sm truncate flex-1) "{file.path}"
        span (text-xs font-medium {statusBadge(file.status).className}) "{statusBadge(file.status).letter}"
    
    // Staged section (only if staged.length > 0)
    if staged.length > 0:
      Separator (my-2)
      div (flex items-center justify-between px-2 py-1)
        span (text-xs font-medium text-muted-foreground) "Staged Changes"
        div (flex items-center gap-1.5)
          Tooltip: Button ghost icon-only (h-5 w-5) → Minus icon → unstage all
          span (text-xs text-muted-foreground) "{staged.length}"
      
      for each file in staged:
        div (same row format as unstaged)
```

**shadcn/ui components:** `ScrollArea`, `Button`, `Separator`, `Tooltip`, `TooltipTrigger`, `TooltipContent`, `TooltipProvider`

**Lucide icons:** `Plus`, `Minus`, `RotateCcw`

- [ ] **Step 3: Verify TypeScript compiles**

Run: `npx tsc --noEmit`
Expected: no type errors

- [ ] **Step 4: Commit**

```bash
git add src/components/git/GitFileList.tsx
git commit -m "feat(frontend): create GitFileList component with status badges"
```
