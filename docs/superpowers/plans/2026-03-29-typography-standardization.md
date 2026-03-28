# Typography Standardization Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Standardize font sizes and weights across all components to match Bruno-style typography — bumping default text from `text-xs` (12px) to `text-sm` (14px) and normalizing font weights.

**Architecture:** Pure Tailwind class transformations across ~30 component files. No behavioral changes, no new code, no structural refactoring. Each task covers one UI area and produces a self-contained commit. The transformation rules are:
- Body text, form labels, form inputs, tree items, tab labels, button text: `text-xs` → `text-sm`
- Error messages: `text-2xs` → `text-xs`
- Status bar console button: `text-2xs` → `text-xs`
- HTTP method badges: normalize to `text-2xs font-semibold` (remove `font-bold` variants)
- `font-bold` on UI text → `font-semibold`
- Section headers that are `text-xs` → `text-sm`; section headers that are `text-sm` → `text-base`
- Keep `text-xs` for: secondary/metadata text, timestamps, dense table cells, console log entries, micro-labels (uppercase tracking-wider section headers)
- Keep `text-2xs` for: micro badges, count indicators, keyboard shortcuts

**Tech Stack:** TypeScript, React, Tailwind CSS v4

**Spec:** `docs/superpowers/specs/2026-03-29-typography-standardization-design.md`

---

## Task 1: Sidebar — CollectionsSidebar, CollectionNode, RequestNode

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`
- Modify: `src/components/collections/CollectionNode.tsx`
- Modify: `src/components/collections/RequestNode.tsx`

- [ ] **Step 1: Update CollectionsSidebar.tsx**

Apply these changes:

- Search input (currently `h-7 text-xs`): change `text-xs` to `text-sm`, change `h-7` to `h-8`
- The "Search requests..." placeholder input class: `text-xs` → `text-sm`
- Collection name input (inline create, currently `h-7 text-xs`): `text-xs` → `text-sm`, `h-7` → `h-8`
- Error message (`text-2xs text-destructive`): `text-2xs` → `text-xs`
- "No collections yet." message (`text-xs`): `text-xs` → `text-sm`
- "Create Collection" button label (`text-xs`): `text-xs` → `text-sm`
- Keep `text-[10px]` and `text-xs` on any uppercase tracking-wider section labels — those are micro-labels and stay small.

- [ ] **Step 2: Update CollectionNode.tsx**

Apply these changes:

- TreeItemContent wrapper class (line ~153, `text-xs`): `text-xs` → `text-sm`
- Collection name span with `font-medium`: keep `font-medium`, it's now paired with `text-sm` from the parent
- Rename input (line ~164, `text-xs`): `text-xs` → `text-sm`
- New request input (line ~259, `text-xs`): `text-xs` → `text-sm`

- [ ] **Step 3: Update RequestNode.tsx**

Apply these changes:

- TreeItemContent wrapper class (line ~97, `text-xs`): `text-xs` → `text-sm`
- HTTP method badge (line ~101, `text-2xs font-bold`): change `font-bold` → `font-semibold`
- Rename input (line ~107, `text-xs`): `text-xs` → `text-sm`

- [ ] **Step 4: Verify**

Run: `npx tsc --noEmit`
Expected: No errors.

- [ ] **Step 5: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx src/components/collections/CollectionNode.tsx src/components/collections/RequestNode.tsx
git commit -m "style: standardize sidebar typography to text-sm default"
```

---

## Task 2: Tab bar and pane chrome — TabItem, EditorGroup

**Files:**
- Modify: `src/components/panes/TabItem.tsx`

- [ ] **Step 1: Update TabItem.tsx**

Apply these changes:

- Tab item container (line ~46, `text-xs`): `text-xs` → `text-sm`
- HTTP method badge in tab (line ~53, `text-2xs font-semibold`): keep as-is (already correct)
- Unsaved dot indicator (line ~63, `text-2xs`): keep as-is (micro indicator)

- [ ] **Step 2: Verify**

Run: `npx tsc --noEmit`
Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add src/components/panes/TabItem.tsx
git commit -m "style: standardize tab bar typography to text-sm"
```

---

## Task 3: Request panel — RequestPanel, AuthEditor

**Files:**
- Modify: `src/components/request/RequestPanel.tsx`
- Modify: `src/components/request/AuthEditor.tsx`

- [ ] **Step 1: Update RequestPanel.tsx**

Read the file and apply these rules:

- Tab triggers (Params, Headers, Body, Auth, Pre-Request, Tests — currently `text-xs`): `text-xs` → `text-sm`
- Form labels (`text-xs font-medium`): `text-xs` → `text-sm`
- Error messages (`text-2xs text-destructive`): `text-2xs` → `text-xs`
- Keep `text-2xs` on param count badges and micro indicators
- Any `font-bold` on UI text → `font-semibold`

- [ ] **Step 2: Update AuthEditor.tsx**

This file has 68 instances of `text-xs`. Read the entire file and apply:

- All `<label>` elements with `text-xs font-medium`: `text-xs` → `text-sm`
- All `<Input>` wrapper classes with `text-xs`: `text-xs` → `text-sm`
- All `<Select>` and `<SelectTrigger>` with `text-xs`: `text-xs` → `text-sm`
- All `<Button>` text with `text-xs`: `text-xs` → `text-sm`
- Keep `text-xs` on: helper/description text below inputs (muted-foreground), token expiry info, secondary metadata
- Keep `text-2xs` on micro badges

- [ ] **Step 3: Verify**

Run: `npx tsc --noEmit`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/request/RequestPanel.tsx src/components/request/AuthEditor.tsx
git commit -m "style: standardize request panel typography to text-sm default"
```

---

## Task 4: Response panel — ResponseBodyViewer, ResponseHeadersTable

**Files:**
- Modify: `src/components/response/ResponseBodyViewer.tsx`
- Modify: `src/components/response/ResponseHeadersTable.tsx`

- [ ] **Step 1: Update ResponseBodyViewer.tsx**

Read the file and apply:

- Tab triggers (Body, Headers, Cookies — currently `text-xs`): `text-xs` → `text-sm`
- Status code badge (`text-xs font-semibold`): `text-xs` → `text-sm`
- Duration/size metadata: keep `text-xs` (secondary info)
- Any section headers: `text-xs` → `text-sm`

- [ ] **Step 2: Update ResponseHeadersTable.tsx**

Read the file and apply:

- Filter input (`text-xs`): `text-xs` → `text-sm`
- Table headers (`text-xs font-semibold`): keep as-is (dense table context)
- Table cell values (`text-xs`): keep as-is (dense table context)

- [ ] **Step 3: Verify**

Run: `npx tsc --noEmit`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/response/ResponseBodyViewer.tsx src/components/response/ResponseHeadersTable.tsx
git commit -m "style: standardize response panel typography"
```

---

## Task 5: Git components — GitTab and sub-components

**Files:**
- Modify: `src/components/git/GitTab.tsx`
- Modify: `src/components/git/BranchSelector.tsx`
- Modify: `src/components/git/GitCommitForm.tsx`
- Modify: `src/components/git/GitChangedFiles.tsx`
- Modify: `src/components/git/GitStagedFiles.tsx`
- Modify: `src/components/git/GitCommitLog.tsx`
- Modify: `src/components/git/GitStashSection.tsx`
- Modify: `src/components/git/ConflictResolver.tsx`
- Modify: `src/components/git/DiffViewer.tsx`
- Modify: `src/components/git/GitCloneDialog.tsx`
- Modify: `src/components/git/GitCredentialsDialog.tsx`

- [ ] **Step 1: Update GitTab.tsx**

- Sub-tab triggers (Changes, Log, Stash — currently `text-xs`): `text-xs` → `text-sm`
- Changed file count text (`text-xs`): keep as-is (secondary metadata)
- "Loading git status..." and "This collection is not a git repository." (`text-sm`): keep as-is
- Changed count inside sub-tab trigger (`text-2xs`): keep as-is (micro badge)

- [ ] **Step 2: Update all other git component files**

Read each file and apply these rules consistently:

- Form labels (`text-xs font-medium`): `text-xs` → `text-sm`
- Form inputs/textareas (`text-xs`): `text-xs` → `text-sm`
- Button text (`text-xs`): `text-xs` → `text-sm`
- Dialog form labels and inputs (`text-xs`): `text-xs` → `text-sm`
- File list item names (`text-xs`): `text-xs` → `text-sm`
- Section headers (`text-xs font-semibold` or `text-xs font-medium`): `text-xs` → `text-sm`
- Keep `text-xs` on: file status badges, commit hashes, timestamps, secondary metadata, muted helper text
- Keep `text-2xs` on: micro badges, counts
- Any `font-bold` on UI text → `font-semibold`

- [ ] **Step 3: Verify**

Run: `npx tsc --noEmit`
Expected: No errors.

- [ ] **Step 4: Commit**

```bash
git add src/components/git/
git commit -m "style: standardize git component typography to text-sm default"
```

---

## Task 6: Toolbar, status bar, history, and remaining components

**Files:**
- Modify: `src/components/layout/WorkspaceToolbar.tsx`
- Modify: `src/components/layout/CollectionDropdown.tsx`
- Modify: `src/components/layout/SandboxPopover.tsx`
- Modify: `src/components/layout/StatusBar.tsx`
- Modify: `src/components/layout/ConsolePanel.tsx`
- Modify: `src/components/layout/EnvironmentSwitcher.tsx`
- Modify: `src/components/history/HistoryPanel.tsx`
- Modify: `src/components/environments/EnvironmentDialog.tsx`
- Modify: `src/components/collections/CollectionOverviewTab.tsx`
- Modify: `src/components/collections/CollectionVariablesEditor.tsx`
- Modify: `src/components/workspace/CreateWorkspaceDialog.tsx`
- Modify: `src/components/workspace/RenameWorkspaceDialog.tsx`
- Modify: `src/components/workspace/WorkspaceSwitcher.tsx`

- [ ] **Step 1: Update toolbar components**

**WorkspaceToolbar.tsx**: no text classes to change (just layout).

**CollectionDropdown.tsx**:
- Trigger button text (`text-xs font-medium`): `text-xs` → `text-sm`
- Keep uppercase micro-labels (`text-[10px]` uppercase tracking-wider): as-is
- Collection list item text (`text-sm`): already correct
- Keep `text-xs` on collection count and muted secondary text

**SandboxPopover.tsx**:
- Mode label text (`text-sm font-medium`): already correct
- Description text (`text-xs`): keep as-is (secondary)
- Keep badge text as-is

- [ ] **Step 2: Update StatusBar.tsx**

- Console button text (currently `text-2xs`): `text-2xs` → `text-xs`
- Entry count badge (`text-2xs`): keep as-is (micro badge)

- [ ] **Step 3: Update ConsolePanel.tsx**

- Section labels (`font-medium`): ensure paired with `text-sm`
- "No requests sent yet" message (`text-xs`): `text-xs` → `text-sm`
- Entry count badge (`text-2xs`): keep as-is
- Console log entry text (`text-2xs font-mono`): keep as-is (dense log)
- HTTP method in entry row (`font-semibold`): keep as-is
- Status code in entry row (`font-semibold`): keep as-is

- [ ] **Step 4: Update HistoryPanel.tsx**

- Search input (`text-xs`): `text-xs` → `text-sm`
- Filter select dropdowns (`text-xs`): `text-xs` → `text-sm`
- "No history yet" message (`text-xs`): `text-xs` → `text-sm`
- URL display in history entry (`text-xs`): `text-xs` → `text-sm`
- Keep `text-xs` on: timestamps, duration (secondary metadata)

- [ ] **Step 5: Update remaining component files**

Read each file and apply the standard rules:

**EnvironmentDialog.tsx**: Form labels and inputs `text-xs` → `text-sm`

**CollectionOverviewTab.tsx**: Section headers `text-xs` → `text-sm`; keep `text-2xs` on micro text

**CollectionVariablesEditor.tsx**: Table headers keep `text-xs font-semibold` (dense table); form inputs `text-xs` → `text-sm`

**Workspace dialogs (CreateWorkspaceDialog.tsx, RenameWorkspaceDialog.tsx, WorkspaceSwitcher.tsx)**: Form labels and inputs `text-xs` → `text-sm`; list items `text-xs` → `text-sm`

**EnvironmentSwitcher.tsx**: Button text (`font-normal`): keep as-is; any `text-xs` on labels → `text-sm`

- [ ] **Step 6: Verify**

Run: `npx tsc --noEmit`
Expected: No errors.

Run: `npx vitest run`
Expected: All 143 tests pass.

- [ ] **Step 7: Commit**

```bash
git add src/components/layout/ src/components/history/ src/components/environments/ src/components/collections/CollectionOverviewTab.tsx src/components/collections/CollectionVariablesEditor.tsx src/components/workspace/
git commit -m "style: standardize toolbar, status bar, history, and dialog typography"
```
