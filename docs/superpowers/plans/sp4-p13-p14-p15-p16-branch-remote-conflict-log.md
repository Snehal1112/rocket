# SP4-P13: Branch Selector + Merge + Bottom Bar

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **HARD RULE — shadcn/ui ONLY.**

**Goal:** Create the branch selector popover with search, create, delete, and merge actions. Create the Git bottom bar and wire into the main layout.

**Tech Stack:** React, TypeScript, shadcn/ui (Popover, Command, Button, Separator, Badge)

**Prerequisite:** SP4-P12 complete.

---

## Task 1: Extend git store with branch actions

**Files:**
- Modify: `frontend/src/stores/git-store.ts`

- [ ] **Step 1: Add branch state + actions**

State: `branches: BranchList | null`.
Actions: `refreshBranches()`, `switchBranch(name)`, `createBranch(name)`, `deleteBranch(name)`, `mergeBranch(name)`.

Each calls the corresponding `git*` API function and refreshes status + branches after.

- [ ] **Step 2: Commit**

```bash
git commit -am "feat(git): extend git store — branch actions + merge"
```

---

## Task 2: BranchSelector + GitBottomBar + wire into layout

**Files:**
- Create: `frontend/src/components/git/BranchSelector.tsx`
- Create: `frontend/src/components/git/GitBottomBar.tsx`
- Modify: `frontend/src/components/layout/MainLayout.tsx`

- [ ] **Step 1: Install shadcn Command if not already**

```bash
npx shadcn add command
```

- [ ] **Step 2: Implement BranchSelector**

shadcn `Popover` + `Command` (searchable list). Shows local branches with checkmark on current. "New branch" inline form at bottom. Delete button per non-current branch. "Merge into current" action per branch via `DropdownMenu` or `ContextMenu`.

- [ ] **Step 3: Implement GitBottomBar**

Bottom bar: `BranchSelector` (left), `Separator`, ahead/behind indicators, `Separator`, flex spacer, existing env/shortcut hints (right). Only shows git section when `isRepo`.

- [ ] **Step 4: Wire into MainLayout**

Replace or extend existing bottom bar with `GitBottomBar`.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/components/git/ frontend/src/components/layout/
git commit -m "feat(git): BranchSelector + GitBottomBar + wire into layout"
```

---

## Milestone Checklist — P13

- [ ] Branch store actions: switch, create, delete, merge
- [ ] `BranchSelector` — searchable, create, delete, merge action
- [ ] `GitBottomBar` — branch name, ahead/behind, in layout
- [ ] All UI is shadcn (Popover, Command, Button)

---

---

# SP4-P14: Push/Pull/Fetch + Credentials + Clone

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **HARD RULE — shadcn/ui ONLY.**

**Goal:** Add push/pull/fetch buttons with credential management, and a clone dialog for cloning remote repos as new collections.

**Tech Stack:** React, TypeScript, shadcn/ui (Button, Dialog, Select, Input, Label)

**Prerequisite:** SP4-P13 complete.

---

## Task 1: GitCredentialsDialog + extend git store

**Files:**
- Create: `frontend/src/components/git/GitCredentialsDialog.tsx`
- Modify: `frontend/src/stores/git-store.ts`

- [ ] **Step 1: Add remote + credential state to git store**

State: `credentials: GitCredentials | null`.
Actions: `setCredentials(creds)`, `push(remote?)`, `pull(remote?)`, `fetch(remote?)`.

Each remote action: if no credentials, dispatch event to open dialog. After dialog, retry with credentials.

- [ ] **Step 2: Implement GitCredentialsDialog**

shadcn `Dialog` with `Select` for auth type (SSH Key, SSH Agent, Username/Password, Token). Conditional fields per type. "Connect" `Button` saves creds to store and closes dialog.

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(git): GitCredentialsDialog + remote store actions"
```

---

## Task 2: GitRemoteActions + GitCloneDialog

**Files:**
- Create: `frontend/src/components/git/GitRemoteActions.tsx`
- Create: `frontend/src/components/git/GitCloneDialog.tsx`

- [ ] **Step 1: Implement GitRemoteActions**

Three shadcn `Button` (ghost, sm): Pull (ArrowDown icon), Push (ArrowUp icon), Fetch (RefreshCw icon). Each shows loading spinner during operation. Wire into `GitBottomBar`.

- [ ] **Step 2: Implement GitCloneDialog**

shadcn `Dialog`: URL `Input`, destination path `Input` (defaults to `~/.rocket-api/collections/{repo-name}`), credentials section (reuse `GitCredentialsDialog` pattern or inline). "Clone" `Button` calls `gitClone(url, dest, creds)`.

Accessible from: sidebar "+" button → "Clone from Git" option.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/git/
git commit -am "feat(git): GitRemoteActions + GitCloneDialog"
```

---

## Milestone Checklist — P14

- [ ] `GitCredentialsDialog` — SSH/HTTPS auth input (shadcn)
- [ ] Push/Pull/Fetch buttons with loading states
- [ ] Buttons wired into `GitBottomBar`
- [ ] `GitCloneDialog` — URL + path + creds → clone
- [ ] Clone accessible from sidebar

---

---

# SP4-P15: Conflict Resolver

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **HARD RULE — shadcn/ui ONLY.** Exception: Monaco editors.

**Goal:** Create the conflict resolution UI — shows ours/theirs versions with accept buttons, plus a manual edit option. Opens as a special tab type.

**Tech Stack:** React, TypeScript, shadcn/ui, @monaco-editor/react

**Prerequisite:** SP4-P14 complete.

---

## Task 1: Conflict tab type + ConflictResolver component

**Files:**
- Modify: `frontend/src/types/pane-types.ts` (add 'conflict' tab type)
- Modify: `frontend/src/stores/pane-store.ts` (add openConflictTab)
- Create: `frontend/src/components/git/ConflictResolver.tsx`

- [ ] **Step 1: Add conflict tab type**

```typescript
tabType: 'request' | 'draft' | 'history' | 'diff' | 'conflict';
conflictState?: ConflictState;

interface ConflictState {
  filePath: string;
  collectionPath: string;
  ours: string;
  theirs: string;
  ancestor: string | null;
}
```

- [ ] **Step 2: Add openConflictTab to pane store**

Stable ID: `conflict:{collection}/{file}`. Deduplicates.

- [ ] **Step 3: Implement ConflictResolver**

Layout (top to bottom):
- Header: file path + "Conflict" `Badge` (red)
- Two-column Monaco editors: "Ours" (left, read-only) + "Theirs" (right, read-only)
- Action bar: "Accept ours" `Button`, "Accept theirs" `Button`, "Edit manually" `Button`

"Edit manually" replaces the two-column view with a single writable Monaco editor showing the full file with conflict markers. User edits, then clicks "Save resolution".

On resolution: calls `gitResolveConflict(path, file, resolution)` → refreshes status → closes tab.

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(git): ConflictResolver — ours/theirs/manual with Monaco"
```

---

## Task 2: Wire conflicts into Git panel + EditorGroup

**Files:**
- Modify: `frontend/src/components/git/GitFileRow.tsx`
- Modify: `frontend/src/components/panes/EditorGroup.tsx`
- Modify: `frontend/src/stores/git-store.ts`

- [ ] **Step 1: Add refreshConflicts + resolveConflict to git store**

- [ ] **Step 2: Conflicted files in GitFileRow open ConflictResolver instead of DiffViewer**

If `file.status === 'conflicted'`: click calls `gitConflicts(path)` → finds the conflict → `openConflictTab(conflictState)`.

- [ ] **Step 3: EditorGroup renders ConflictResolver for conflict tabs**

```tsx
{activeTab?.tabType === 'conflict' && activeTab.conflictState ? (
  <ConflictResolver conflictState={activeTab.conflictState} />
) : activeTab?.tabType === 'diff' ? ...}
```

- [ ] **Step 4: Commit**

```bash
git commit -am "feat(git): wire conflicts into file click + EditorGroup"
```

---

## Milestone Checklist — P15

- [ ] Conflict tab type in pane system
- [ ] `ConflictResolver` — ours/theirs Monaco (read-only) + action buttons
- [ ] "Edit manually" mode with writable Monaco
- [ ] Resolution calls `gitResolveConflict` and closes tab
- [ ] Conflicted files in Git panel open resolver (not diff)
- [ ] `EditorGroup` renders `ConflictResolver`

---

---

# SP4-P16: Commit Log Panel

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **HARD RULE — shadcn/ui ONLY.**

**Goal:** Create a scrollable commit history panel showing SHA, message, author, and timestamp. Accessible from the Git sidebar panel.

**Tech Stack:** React, TypeScript, shadcn/ui (ScrollArea, Badge, Button, Separator, Tooltip)

**Prerequisite:** SP4-P15 complete.

---

## Task 1: GitCommitLog component

**Files:**
- Create: `frontend/src/components/git/GitCommitLog.tsx`
- Modify: `frontend/src/stores/git-store.ts` (add log action)

- [ ] **Step 1: Add commit log to git store**

State: `commitLog: CommitInfo[]`.
Action: `refreshLog(limit?: number)` — calls `gitLog(collectionPath, limit ?? 50)`.

- [ ] **Step 2: Implement GitCommitLog**

Scrollable list using shadcn `ScrollArea`. Each commit row:
- Short SHA in `Badge` (mono font, outline variant)
- Commit message (truncated, bold)
- Author name + relative timestamp (e.g. "2 hours ago") in muted text
- `Tooltip` on SHA showing full SHA with "click to copy"

Load more `Button` at bottom (increments limit by 50).

```tsx
import { ScrollArea } from '@/components/ui/scroll-area';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipTrigger } from '@/components/ui/tooltip';
import { Separator } from '@/components/ui/separator';
```

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/git/GitCommitLog.tsx frontend/src/stores/git-store.ts
git commit -m "feat(git): GitCommitLog — scrollable commit history"
```

---

## Task 2: Wire into GitSidebarPanel

**Files:**
- Modify: `frontend/src/components/git/GitSidebarPanel.tsx`

- [ ] **Step 1: Add "Log" section or tab within the Git sidebar**

Option A: Add as a collapsible section below stash.
Option B: Add a third sub-tab in the Git sidebar: "Changes" | "Log" | "Stash".

Recommended: **Option B** — use shadcn `Tabs` within the Git sidebar panel:
```tsx
<Tabs defaultValue="changes">
  <TabsList className="w-full">
    <TabsTrigger value="changes" className="flex-1">Changes</TabsTrigger>
    <TabsTrigger value="log" className="flex-1">Log</TabsTrigger>
    <TabsTrigger value="stash" className="flex-1">Stash</TabsTrigger>
  </TabsList>
  <TabsContent value="changes">
    <GitCommitForm /> + <GitStagedFiles /> + <GitChangedFiles />
  </TabsContent>
  <TabsContent value="log">
    <GitCommitLog />
  </TabsContent>
  <TabsContent value="stash">
    <GitStashSection />
  </TabsContent>
</Tabs>
```

- [ ] **Step 2: Load log when "Log" tab is selected**

```tsx
useEffect(() => {
  if (activeSubTab === 'log') refreshLog();
}, [activeSubTab]);
```

- [ ] **Step 3: End-to-end test**

Verify:
- [ ] Git sidebar → "Log" tab shows commit history
- [ ] Each entry: SHA badge, message, author, time
- [ ] SHA tooltip shows full hash, click copies
- [ ] "Load more" button fetches additional commits
- [ ] All UI is shadcn

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/git/
git commit -m "feat(git): wire GitCommitLog into sidebar + sub-tabs"
```

---

## Milestone Checklist — P16

- [ ] `commitLog` state + `refreshLog` action in git store
- [ ] `GitCommitLog` — scrollable list with SHA badge, message, author, timestamp
- [ ] "Load more" pagination
- [ ] SHA copy via tooltip click
- [ ] Integrated into GitSidebarPanel as "Log" sub-tab
- [ ] All UI is shadcn
