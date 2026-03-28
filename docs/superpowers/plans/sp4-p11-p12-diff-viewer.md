# SP4-P11: Diff Tab Type + DiffViewer Component

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **HARD RULE — shadcn/ui ONLY.** Exception: Monaco DiffEditor.

**Goal:** Add `diff` tab type to the pane system, create `DiffViewer` with Monaco side-by-side diff, and `DiffHeader` with staged/unstaged diff toggle.

**Tech Stack:** React, TypeScript, @monaco-editor/react (DiffEditor), shadcn/ui

**Prerequisite:** SP4-P10 complete.

---

## Task 1: Add diff tab type + openDiffTab action

**Files:**
- Modify: `frontend/src/types/pane-types.ts`
- Modify: `frontend/src/stores/pane-store.ts`

- [ ] **Step 1: Add DiffState to pane types**

```typescript
export interface Tab {
  // ... existing fields ...
  tabType: 'request' | 'draft' | 'history' | 'diff';
  diffState?: DiffState;
}

export interface DiffState {
  filePath: string;
  collectionPath: string;
  oldContent: string;
  newContent: string;
  status: string;
  isStaged: boolean;
}
```

- [ ] **Step 2: Add openDiffTab to pane store**

Stable tab ID `diff:{collection}/{file}:{staged}` — deduplicates. Creates tab with `tabType: 'diff'`.

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(git): diff tab type + openDiffTab action"
```

---

## Task 2: DiffHeader + DiffViewer

**Files:**
- Create: `frontend/src/components/git/DiffHeader.tsx`
- Create: `frontend/src/components/git/DiffViewer.tsx`

- [ ] **Step 1: Implement DiffHeader**

Shows: file path (mono), `GitStatusBadge`, and a "Staged / Working" toggle using shadcn `Tabs` (small inline tabs to switch between staged diff and working diff).

- [ ] **Step 2: Implement DiffViewer**

```tsx
import { DiffEditor } from '@monaco-editor/react';
```

Full-height Monaco `DiffEditor` with: `renderSideBySide: true`, `readOnly: true`, theme from `useMonacoTheme()`, language detected from file extension.

`DiffHeader` + `DiffEditor` composed vertically.

When user toggles staged/working: calls `gitDiffStaged` or `gitDiff` respectively, updates the `DiffState`.

- [ ] **Step 3: Commit**

```bash
git add frontend/src/components/git/DiffHeader.tsx frontend/src/components/git/DiffViewer.tsx
git commit -m "feat(git): DiffHeader + DiffViewer with Monaco + staged toggle"
```

---

## Milestone Checklist — P11

- [ ] `DiffState` type with `isStaged` flag
- [ ] `openDiffTab` deduplicates tabs
- [ ] `DiffHeader` with file path, badge, staged/working toggle
- [ ] `DiffViewer` with Monaco `DiffEditor` (side-by-side, read-only)
- [ ] Staged diff toggle switches between `gitDiff` and `gitDiffStaged`

---

---

# SP4-P12: Wire Diff into Editor + File Click

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Wire `DiffViewer` into `EditorGroup` so diff tabs render correctly, and make clicking a changed file in the Git panel open the diff.

**Tech Stack:** React, TypeScript

**Prerequisite:** SP4-P11 complete.

---

## Task 1: EditorGroup renders DiffViewer for diff tabs

**Files:**
- Modify: `frontend/src/components/panes/EditorGroup.tsx`

- [ ] **Step 1: Add DiffViewer conditional render**

```tsx
import { DiffViewer } from '@/components/git/DiffViewer';

// In render:
{activeTab?.tabType === 'diff' && activeTab.diffState ? (
  <DiffViewer diffState={activeTab.diffState} />
) : activeTab ? (
  <RequestPanel tab={activeTab} groupId={node.groupId} />
) : null}
```

- [ ] **Step 2: Commit**

```bash
git commit -am "feat(git): EditorGroup renders DiffViewer for diff tabs"
```

---

## Task 2: GitFileRow click opens diff tab

**Files:**
- Modify: `frontend/src/components/git/GitFileRow.tsx`

- [ ] **Step 1: Add onClick handler**

On click: call `gitDiff(collectionPath, filePath)` → `openDiffTab(diffState)`.

For staged files: call `gitDiffStaged` instead and set `isStaged: true`.

- [ ] **Step 2: End-to-end test**

Verify:
- [ ] Click changed file in Git panel → diff tab opens
- [ ] Monaco shows side-by-side (old left, new right)
- [ ] Green/red highlighting for additions/removals
- [ ] Clicking same file again focuses existing tab
- [ ] Staged file click shows staged diff (index vs HEAD)

- [ ] **Step 3: Commit**

```bash
git commit -am "feat(git): click changed file → opens diff tab"
```

---

## Milestone Checklist — P12

- [ ] `EditorGroup` renders `DiffViewer` for `tabType === 'diff'`
- [ ] Clicking unstaged file → opens working tree diff
- [ ] Clicking staged file → opens staged diff
- [ ] No duplicate diff tabs
- [ ] Dark mode works
