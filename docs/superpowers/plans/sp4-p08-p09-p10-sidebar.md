# SP4-P08: Git Store + Status Badges

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **HARD RULE — shadcn/ui ONLY:** Every interactive UI element MUST use a shadcn/ui component.

**Goal:** Create `useGitStore` (status polling only) and `GitStatusBadge` component. Wire badges into the collection sidebar tree.

**Tech Stack:** React, TypeScript, Zustand, shadcn/ui

**Prerequisite:** SP4-P07 complete.

---

## Task 1: Create useGitStore (status + basic actions)

**Files:**
- Create: `frontend/src/stores/git-store.ts`

- [ ] **Step 1: Implement store with status + stage/unstage/discard/commit**

State: `isRepo`, `collectionPath`, `status`, `loading`, `error`.
Actions: `setCollection`, `refreshStatus`, `stageFiles`, `unstageFiles`, `discardFiles`, `commitChanges`, `stageAll`, `unstageAll`, `reset`.

(Full implementation in the previous plan — see SP4 Plan 3 Task 1.)

- [ ] **Step 2: Commit**

```bash
git add frontend/src/stores/git-store.ts
git commit -m "feat(git): useGitStore — status + staging + commit"
```

---

## Task 2: GitStatusBadge + wire into sidebar

**Files:**
- Create: `frontend/src/components/git/GitStatusBadge.tsx`
- Modify: `frontend/src/components/collections/CollectionsSidebar.tsx`

- [ ] **Step 1: Implement GitStatusBadge using shadcn Badge**

Color-coded badge: M=amber, A=green, D=red, R=blue, ?=gray, C=red-dark. Uses `Badge` from shadcn with custom className per status.

- [ ] **Step 2: Wire into collection tree**

In `CollectionsSidebar.tsx`:
- Import `useGitStore` and `GitStatusBadge`
- Call `setCollection(path)` when active collection changes
- For each tree item, look up file status and show badge if changed

- [ ] **Step 3: Verify badges appear on modified files**

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/git/GitStatusBadge.tsx frontend/src/components/collections/
git commit -m "feat(git): GitStatusBadge + badges on collection tree"
```

---

## Milestone Checklist — P08

- [ ] `useGitStore` with status, stage, unstage, discard, commit
- [ ] `GitStatusBadge` using shadcn `Badge`
- [ ] Badges visible on collection sidebar tree items
- [ ] Badges update after staging/committing

---

---

# SP4-P09: Commit Form + File Lists

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **HARD RULE — shadcn/ui ONLY.**

**Goal:** Create the commit message form, file row component, and staged/unstaged file list sections.

**Tech Stack:** React, TypeScript, shadcn/ui (Textarea, Button, Table, Tooltip)

**Prerequisite:** SP4-P08 complete.

---

## Task 1: GitCommitForm

**Files:**
- Create: `frontend/src/components/git/GitCommitForm.tsx`

- [ ] **Step 1: Implement with shadcn Textarea + Button**

Install textarea if needed: `npx shadcn add textarea`

Commit message `Textarea` (resizable, placeholder), "Commit (N files)" `Button`, disabled when message empty or no staged files. On commit: calls `commitChanges(message)`, clears message.

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/git/GitCommitForm.tsx
git commit -m "feat(git): GitCommitForm — shadcn Textarea + Button"
```

---

## Task 2: GitFileRow + GitChangedFiles + GitStagedFiles

**Files:**
- Create: `frontend/src/components/git/GitFileRow.tsx`
- Create: `frontend/src/components/git/GitChangedFiles.tsx`
- Create: `frontend/src/components/git/GitStagedFiles.tsx`

- [ ] **Step 1: Implement GitFileRow**

Single row: `GitStatusBadge`, filename (mono font), folder path (muted), hover actions using shadcn `Button` (ghost, icon size) + `Tooltip`.

Actions for unstaged: "+" (stage), "↩" (discard).
Actions for staged: "-" (unstage).

Click on row: will open diff (wired in P12).

- [ ] **Step 2: Implement GitChangedFiles**

Header "Changes" with "Stage all" `Button`. Maps unstaged files to `GitFileRow` components.

- [ ] **Step 3: Implement GitStagedFiles**

Header "Staged changes" with "Unstage all" `Button`. Maps staged files to `GitFileRow` components.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/git/
git commit -m "feat(git): GitFileRow + GitChangedFiles + GitStagedFiles"
```

---

## Milestone Checklist — P09

- [ ] `GitCommitForm` with textarea + commit button
- [ ] `GitFileRow` with status badge + hover action buttons
- [ ] `GitChangedFiles` section with "Stage all"
- [ ] `GitStagedFiles` section with "Unstage all"
- [ ] All UI is shadcn

---

---

# SP4-P10: Git Sidebar Panel + Tab Toggle + Stash

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **HARD RULE — shadcn/ui ONLY.**

**Goal:** Create the stash section, assemble the full `GitSidebarPanel`, and add the Collections/Git tab toggle to the sidebar.

**Tech Stack:** React, TypeScript, shadcn/ui (Tabs, Button, Input, DropdownMenu, ScrollArea)

**Prerequisite:** SP4-P09 complete.

---

## Task 1: GitStashSection + extend git store

**Files:**
- Create: `frontend/src/components/git/GitStashSection.tsx`
- Modify: `frontend/src/stores/git-store.ts` (add stash actions)

- [ ] **Step 1: Add stash state + actions to git store**

Add: `stashes: StashEntry[]`, `refreshStashes()`, `saveStash(msg)`, `popStash(idx)`, `applyStash(idx)`, `dropStash(idx)`.

- [ ] **Step 2: Implement GitStashSection**

Collapsible section. "Save" button with inline `Input` for message. Each stash entry shows message + branch. `DropdownMenu` per entry: Pop, Apply, Drop.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/git/GitStashSection.tsx frontend/src/stores/git-store.ts
git commit -m "feat(git): GitStashSection + stash store actions (incl apply)"
```

---

## Task 2: GitSidebarPanel + sidebar tab toggle

**Files:**
- Create: `frontend/src/components/git/GitSidebarPanel.tsx`
- Modify: sidebar wrapper (MainLayout or CollectionsSidebar parent)

- [ ] **Step 1: Implement GitSidebarPanel**

Assembles: `GitCommitForm` → `GitStagedFiles` → `GitChangedFiles` → `GitStashSection`.

When `isRepo === false`: shows "Not a git repository" message + "Initialize Git" `Button`.

Wraps everything in `ScrollArea`.

- [ ] **Step 2: Add sidebar tab toggle**

Use shadcn `Tabs` at the top of the sidebar:
```tsx
<Tabs defaultValue="collections">
  <TabsList className="w-full">
    <TabsTrigger value="collections" className="flex-1">Collections</TabsTrigger>
    <TabsTrigger value="git" className="flex-1">
      Git
      {changedCount > 0 && <Badge variant="secondary" className="ml-1 text-[9px] px-1">{changedCount}</Badge>}
    </TabsTrigger>
  </TabsList>
  <TabsContent value="collections"><CollectionsSidebar /></TabsContent>
  <TabsContent value="git"><GitSidebarPanel /></TabsContent>
</Tabs>
```

- [ ] **Step 3: End-to-end: verify sidebar toggle works**

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/git/GitSidebarPanel.tsx frontend/src/components/layout/
git commit -m "feat(git): GitSidebarPanel + sidebar Collections/Git toggle"
```

---

## Milestone Checklist — P10

- [ ] `GitStashSection` — save (with message), pop, apply, drop
- [ ] `GitSidebarPanel` — assembles all git components
- [ ] "Not a git repo" state with "Initialize Git" button
- [ ] Sidebar toggle: Collections / Git with change count badge
- [ ] All UI is shadcn
