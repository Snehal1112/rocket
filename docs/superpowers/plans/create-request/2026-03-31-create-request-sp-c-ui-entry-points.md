# Create Request — SP-C: UI Entry Points

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

**Depends on:** SP-A must be merged first (`openEphemeralTab` must exist on `usePaneStore`).

**Goal:** Add the three remaining request-creation entry points — the inline `+` tab button (left-click HTTP, right-click menu for all types), the sidebar `FilePlus` button for workspace-level unsaved requests, and a hardened close-guard that always prompts before losing an ephemeral (sourceless) tab.

**Architecture:** All three entry points call `openEphemeralTab` from SP-A — no new data layer needed. The inline `+` button wraps a plain shadcn/ui `Button` inside a `ContextMenu` for right-click support. The sidebar button is a single icon `Button` in the existing toolbar row. The close-guard fix is a one-condition change in `EditorGroup.tsx` plus a context-aware `AlertDialog` description.

**Tech Stack:** React 18, TypeScript, shadcn/ui (Button, ContextMenu), Lucide React (`Plus`, `FilePlus`), Zustand (`usePaneStore`), existing `EditorGroup.tsx`, `CollectionsSidebar.tsx`, and the tab strip component.

---

## File Structure

| File | Action | Responsibility |
|---|---|---|
| `src/components/panes/TabStrip.tsx` *(or whichever file renders the tab bar)* | **Modify** | Add `+` button: left-click → HTTP ephemeral, right-click → type picker |
| `src/components/layout/CollectionsSidebar.tsx` | **Modify** | Add `FilePlus` toolbar button → `openEphemeralTab('http')` |
| `src/components/panes/EditorGroup.tsx` | **Modify** | Close-guard: always prompt when tab has no `source`; context-aware dialog text |

---

## Chunk 1: Inline `+` tab button

### Task 1: Add `+` button to the tab strip

**Files:**
- Modify: `src/components/panes/TabStrip.tsx` *(adjust path if tab bar lives elsewhere)*

- [ ] **Step 1: Locate the tab bar file**

```bash
find src -name "TabStrip*" -o -name "*tab-strip*" -o -name "BrunoTabBar*" 2>/dev/null
grep -rn "activeTabId\|tabs\.map" src/components/panes/ | head -20
```

Identify the file and component that renders the row of open tabs. All edits in this task go to that file. Update the file path accordingly.

- [ ] **Step 2: Add the `+` button after the tabs list**

Find the section that maps over tabs (something like `{leaf.tabs.map((tab) => ...)}`) and insert the following **after** that block:

```tsx
import { Plus } from 'lucide-react';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { usePaneStore } from '@/stores/pane-store';

// Inside component — destructure from store:
const openEphemeralTab = usePaneStore((s) => s.openEphemeralTab);

// JSX — placed immediately after the closing tag of the tabs map:
<ContextMenu>
  <ContextMenuTrigger asChild>
    <Button
      type="button"
      variant="ghost"
      size="icon"
      aria-label="New request"
      title="New request · Right-click for more types"
      onClick={() => openEphemeralTab('http')}
      className="ml-1 h-7 w-7 shrink-0 text-muted-foreground hover:text-foreground"
    >
      <Plus className="h-3.5 w-3.5" />
    </Button>
  </ContextMenuTrigger>
  <ContextMenuContent>
    <ContextMenuItem onClick={() => openEphemeralTab('http')}>
      HTTP
    </ContextMenuItem>
    <ContextMenuItem onClick={() => openEphemeralTab('graphql')}>
      GraphQL
    </ContextMenuItem>
    <ContextMenuItem onClick={() => openEphemeralTab('grpc')}>
      gRPC
    </ContextMenuItem>
    <ContextMenuItem onClick={() => openEphemeralTab('websocket')}>
      WebSocket
    </ContextMenuItem>
  </ContextMenuContent>
</ContextMenu>
```

> **Note:** Use `Button` from `@/components/ui/button` — no raw `<button>` elements per the shadcn/ui hard rule.

- [ ] **Step 3: Manual smoke test**

```bash
yarn tauri dev
```

1. Left-click `+` → "Untitled" tab opens immediately (no dialog), HTTP type
2. Right-click `+` → context menu shows HTTP / GraphQL / gRPC / WebSocket
3. Select WebSocket → "Untitled" tab opens with `requestType: 'websocket'`
4. Neither action creates a file on disk (tab has no `source`)
5. "Save to Collection" button visible in the new tab's toolbar (from SP-B)

- [ ] **Step 4: Commit**

```bash
git add src/components/panes/TabStrip.tsx   # adjust path if different
git commit -m "feat: add inline + tab button for ephemeral request creation"
```

---

## Chunk 2: Sidebar `FilePlus` button

### Task 2: Add global unsaved-request shortcut to sidebar toolbar

**Files:**
- Modify: `src/components/layout/CollectionsSidebar.tsx`

- [ ] **Step 1: Add import**

In `src/components/layout/CollectionsSidebar.tsx`, add `FilePlus` to the Lucide import:

```tsx
import { Plus, Upload, Layers, FilePlus /* ...existing */ } from 'lucide-react';
```

- [ ] **Step 2: Destructure `openEphemeralTab` from store**

Find where other store values are destructured in this component (e.g. `const openWorkspaceTabs = usePaneStore((s) => s.openWorkspaceTabs)`). Add:

```tsx
const openEphemeralTab = usePaneStore((s) => s.openEphemeralTab);
```

- [ ] **Step 3: Add the button to the toolbar**

Find the `view === "collections"` toolbar row — it currently has the "New Collection" (`Plus`) and "Import Collection" (`Upload`) icon buttons. Add the `FilePlus` button after them:

```tsx
<Button
  variant="ghost"
  size="icon"
  className="h-6 w-6 shrink-0 text-muted-foreground hover:text-foreground"
  onClick={() => openEphemeralTab('http')}
  aria-label="New unsaved request"
  title="New unsaved request"
>
  <FilePlus className="h-3.5 w-3.5" />
</Button>
```

- [ ] **Step 4: Manual smoke test**

```bash
yarn tauri dev
```

1. Click the `FilePlus` icon in the sidebar toolbar
2. "Untitled" tab opens with no collection binding
3. "Save to Collection" button visible in the tab's toolbar (SP-B)
4. No file created on disk

- [ ] **Step 5: Commit**

```bash
git add src/components/layout/CollectionsSidebar.tsx
git commit -m "feat: add FilePlus sidebar button for workspace-level unsaved requests"
```

---

## Chunk 3: Close-guard for ephemeral tabs

### Task 3: Always prompt before closing a sourceless tab

**Files:**
- Modify: `src/components/panes/EditorGroup.tsx`

Currently the close-guard only fires when `tab.isDirty`. An ephemeral tab with no `source` should always prompt — even before the user has typed anything — because closing it means permanent data loss with no undo.

- [ ] **Step 1: Locate the close-tab handler in `EditorGroup`**

```bash
grep -n "pendingClose\|closeTab\|isDirty" src/components/panes/EditorGroup.tsx | head -20
```

Find the function (likely `handleCloseTab` or inline handler on the tab's close `×` button) that decides whether to show a confirmation dialog before calling `closeTab`.

- [ ] **Step 2: Update the guard condition**

Replace the existing guard condition (which only checks `isDirty`) with one that also guards sourceless tabs:

```tsx
// Before (example — match the actual code):
if (tab.isDirty) {
  setPendingCloseTabId(tabId);
  return;
}

// After:
const needsGuard =
  (isRequestTab(tab) && !tab.source) ||   // ephemeral — always guard
  tab.isDirty;                             // has unsaved edits

if (needsGuard) {
  setPendingCloseTabId(tabId);
  return;
}
closeTab(tabId, node.groupId);
```

- [ ] **Step 3: Make the `AlertDialog` description context-aware**

Find the `<AlertDialogDescription>` inside the close-guard `AlertDialog`. Replace its static text with context-sensitive copy:

```tsx
<AlertDialogDescription>
  {(() => {
    if (!pendingCloseTabId) return null;
    const found = findTabInTree(root, pendingCloseTabId);
    if (found && isRequestTab(found.tab) && !found.tab.source) {
      return 'This request has never been saved to a collection. Closing it will discard all changes. Close anyway?';
    }
    return 'This request has unsaved changes. Close anyway?';
  })()}
</AlertDialogDescription>
```

Make sure `root` is available in scope (it should already be from the store subscription). Make sure `findTabInTree` is imported from `@/lib/pane-utils` and `isRequestTab` from `@/types/pane-types`.

- [ ] **Step 4: Manual smoke test**

```bash
yarn tauri dev
```

1. Open ephemeral tab via `+` button, type a URL
2. Click `×` to close → dialog: "never been saved… Close anyway?"
3. Click Cancel → tab stays open
4. Click Close → tab is gone, no file created

5. Open ephemeral tab, do NOT modify anything, click `×`
6. Dialog still appears ("never been saved…") — guard fires regardless of dirty state

7. Open a normal saved request, edit it, click `×`
8. Dialog: "unsaved changes…" — different copy, correct behaviour unchanged

- [ ] **Step 5: Commit**

```bash
git add src/components/panes/EditorGroup.tsx
git commit -m "fix: always prompt before closing ephemeral (sourceless) tabs"
```

---

## Definition of Done

- [ ] Left-click `+` in tab strip → instant HTTP ephemeral tab, no dialog
- [ ] Right-click `+` in tab strip → context menu with HTTP / GraphQL / gRPC / WebSocket
- [ ] `FilePlus` button in sidebar toolbar → instant HTTP ephemeral tab
- [ ] Ephemeral tab always shows close-guard dialog (even before any edits)
- [ ] Close-guard copy is context-aware: "never been saved" vs "unsaved changes"
- [ ] Pre-existing close-guard for dirty sourced tabs is unchanged
- [ ] `yarn vitest run` → no regressions
