# Variable Source Navigation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the user clicks "Variables in request →" in the `VariableAwareUrlInput` popover, navigate to the correct UI location for that variable's source type instead of always routing to a fixed request panel section.

**Architecture:** Three sequential changes. First, extend `pane-store` with an optional `section` param on `openWorkspaceTabs` and a new `openCollectionTab` action. Second, update `VariableAwareUrlInput` to emit the variable source instead of a panel section name, and compute a context-sensitive link label. Third, update `RequestPanel` to handle the new callback and dispatch to the correct destination (panel section, env dialog, workspace tab, or collection tab).

**Tech Stack:** React, TypeScript, Zustand (`usePaneStore`, `useWorkspaceStore`), lucide-react, sonner (toasts)

---

## File Structure

| File | Action | Responsibility |
|------|--------|---------------|
| `src/stores/pane-store.ts` | Modify | Add `section?` to `openWorkspaceTabs`; add `openCollectionTab` action |
| `src/components/request/VariableAwareUrlInput.tsx` | Modify | Replace `onSwitchToSection` with `onNavigateToSource`; add `navLinkLabel` helper |
| `src/components/request/RequestPanel.tsx` | Modify | Add `handleNavigateToSource`; add env dialog state; render `EnvironmentDialog` |

---

### Task 1: Extend pane-store with section navigation helpers

**Files:**
- Modify: `src/stores/pane-store.ts`

Context: `pane-store` already has `openWorkspaceTabs(workspaceId)` which builds three workspace tabs (overview, environments, git) and always activates the first. We add an optional `section?` param so callers can activate a specific tab. We also add `openCollectionTab(collection, section)` which finds an already-open collection tab by its `collectionName` field, activates it, and sets `activeSection`. If no such tab is open it returns `false`.

- [ ] **Step 1: Update the `PaneState` interface — add `section?` to `openWorkspaceTabs` and declare `openCollectionTab`**

Current lines 107 and 116 in `src/stores/pane-store.ts`:
```typescript
  openWorkspaceTabs: (workspaceId: string) => void;
  ...
  updateCollectionSection: (tabId: string, section: CollectionSection) => void;
```

Replace line 107:
```typescript
  openWorkspaceTabs: (workspaceId: string, section?: WorkspaceTabSection) => void;
```

Add after `updateCollectionSection` declaration (after line 116):
```typescript
  /** Opens or focuses the collection tab for `collection` and navigates to `section`.
   *  Returns false if no collection tab is currently open for that collection. */
  openCollectionTab: (collection: string, section: CollectionSection) => boolean;
```

- [ ] **Step 2: Update the `openWorkspaceTabs` implementation to use `section`**

Current implementation at line 407 starts with `openWorkspaceTabs(workspaceId) {`. Change the signature and the `activeTabId` line inside it.

Find this block (around line 452–458):
```typescript
    // Reset pane tree to a single leaf with workspace tabs.
    const leaf = createDefaultLeaf();
    const newRoot = updateLeaf(leaf, leaf.groupId, (l) => ({
      ...l,
      tabs,
      activeTabId: tabs[0].id,
    }));
```

Replace with:
```typescript
    // Reset pane tree to a single leaf with workspace tabs.
    const leaf = createDefaultLeaf();
    const targetTab = section ? (tabs.find((t) => t.activeSection === section) ?? tabs[0]) : tabs[0];
    const newRoot = updateLeaf(leaf, leaf.groupId, (l) => ({
      ...l,
      tabs,
      activeTabId: targetTab.id,
    }));
```

Also update the function signature from:
```typescript
  openWorkspaceTabs(workspaceId) {
```
To:
```typescript
  openWorkspaceTabs(workspaceId, section) {
```

- [ ] **Step 3: Add the `openCollectionTab` implementation**

Add this block directly before the closing `});` of the store (before the `updateCollectionSection` closing brace, after line 546):

```typescript
  openCollectionTab(collection, section) {
    const { root } = get();

    // Walk the pane tree to find an open tab for this collection.
    const findTarget = (
      node: PaneNode,
    ): { groupId: string; tabId: string } | null => {
      if (node.type === 'leaf') {
        const found = node.tabs.find(
          (t) =>
            t.tabType === 'collection' &&
            (t as CollectionTab).collectionName === collection,
        );
        return found ? { groupId: node.groupId, tabId: found.id } : null;
      }
      return findTarget(node.children[0]) ?? findTarget(node.children[1]);
    };

    const target = findTarget(root);
    if (!target) return false;

    // Activate the tab and navigate to the requested section.
    get().updateCollectionSection(target.tabId, section);
    const newRoot = updateLeaf(root, target.groupId, (l) => ({
      ...l,
      activeTabId: target.tabId,
    }));
    set({ root: newRoot, activeGroupId: target.groupId });
    return true;
  },
```

- [ ] **Step 4: TypeScript check**

```bash
cd /home/numericlabs/data/rocket/rocket
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/stores/pane-store.ts
git commit -m "feat(pane): add section param to openWorkspaceTabs; add openCollectionTab action"
```

---

### Task 2: Update `VariableAwareUrlInput`

**Files:**
- Modify: `src/components/request/VariableAwareUrlInput.tsx`

Context: the component currently has `onSwitchToSection?: (section: 'params' | 'variables') => void` and always renders a fixed link label "Variables in request →". We replace this with `onNavigateToSource?: (source: VariableSource | 'pathParam') => void`. A new `navLinkLabel` helper computes the link label from the source and returns `null` for sources with no navigation target (folder, process, undefined).

- [ ] **Step 1: Replace the prop type and add `navLinkLabel` helper**

In `src/components/request/VariableAwareUrlInput.tsx`, replace:
```tsx
  // Called when "Variables in request →" is clicked; section indicates where to navigate.
  onSwitchToSection?: (section: 'params' | 'variables') => void;
```

With:
```tsx
  // Called when the navigation link is clicked; source indicates the variable origin.
  onNavigateToSource?: (source: VariableSource | 'pathParam') => void;
```

Add this function **before** the `VariableAwareUrlInput` component declaration (after the `tokenMeta` function, around line 58):

```tsx
// Returns the link label for a navigation destination, or null if no nav is available.
function navLinkLabel(source: VariableSource | 'pathParam'): string | null {
  switch (source) {
    case 'pathParam':
      return 'Params \u2192';
    case 'request':
    case 'runtime':
      return 'Request Variables \u2192';
    case 'environment':
      return 'Collection Environments \u2192';
    case 'global':
      return 'Global Environments \u2192';
    case 'collection':
      return 'Collection Variables \u2192';
    default:
      return null; // folder, process — no navigation available
  }
}
```

- [ ] **Step 2: Update the component destructuring and link render inside `renderTokenPopover`**

In the component function signature destructuring, rename the prop:
```tsx
  onNavigateToSource,
```
(replace `onSwitchToSection`)

Inside `renderTokenPopover`, remove the line:
```tsx
    // Target section for the navigation link.
    const navSection: 'params' | 'variables' = token.type === 'pathParam' ? 'params' : 'variables';
```

Replace it with:
```tsx
    // Resolved source for the navigation link (null = no link).
    const navSource: VariableSource | 'pathParam' | null =
      token.type === 'pathParam' ? 'pathParam' : (scopeEntry?.source ?? null);
    const linkLabel = navSource !== null ? navLinkLabel(navSource) : null;
```

Replace the existing link button block:
```tsx
            {onSwitchToSection && (
              <button
                type='button'
                className='text-2xs text-primary hover:underline cursor-pointer'
                // Prevent the Input from losing focus on mousedown, which would fire onBlur
                // and close the popover before the click event fires.
                onMouseDown={(e) => e.preventDefault()}
                onClick={async () => {
                  await handleCommit();
                  onSwitchToSection(navSection);
                }}
              >
                Variables in request &rarr;
              </button>
            )}
```

With:
```tsx
            {onNavigateToSource && navSource !== null && linkLabel !== null && (
              <button
                type='button'
                className='text-2xs text-primary hover:underline cursor-pointer'
                // Prevent blur-before-click from closing the popover.
                onMouseDown={(e) => e.preventDefault()}
                onClick={async () => {
                  await handleCommit();
                  onNavigateToSource(navSource);
                }}
              >
                {linkLabel}
              </button>
            )}
```

- [ ] **Step 3: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors. If there are errors about `onSwitchToSection` still being referenced in `RequestPanel.tsx`, that is expected — it will be fixed in Task 3.

- [ ] **Step 4: Commit**

```bash
git add src/components/request/VariableAwareUrlInput.tsx
git commit -m "feat(ux): replace onSwitchToSection with onNavigateToSource in VariableAwareUrlInput"
```

---

### Task 3: Wire `handleNavigateToSource` into `RequestPanel`

**Files:**
- Modify: `src/components/request/RequestPanel.tsx`

Context: `RequestPanel` currently passes `onSwitchToSection={(section) => setActiveSection(section)}` to `VariableAwareUrlInput`. We replace this with `onNavigateToSource={handleNavigateToSource}` and add the handler. The handler opens `EnvironmentDialog` for collection env vars, calls `openWorkspaceTabs(wsId, 'environments')` for global vars, calls `openCollectionTab(collection, 'variables')` for collection vars (showing a toast if the tab isn't open), and switches the request panel section for request/runtime/pathParam vars.

- [ ] **Step 1: Add missing imports**

Current line 1 imports start with `import { Loader2, Send, Zap } from 'lucide-react';`. Existing relevant imports include:
- Line 31: `import { buildScopedContext } from '@/lib/url-variables';`
- Line 34: `import { usePaneStore } from '@/stores/pane-store';`

Add these three imports (insert after line 34, the `usePaneStore` import):
```tsx
import type { VariableSource } from '@/lib/url-variables';
import { useWorkspaceStore } from '@/stores/workspace-store';
```

Add `EnvironmentDialog` to the existing component imports block. Current line 3–15 (approximately):
```tsx
import { EnvironmentDialog } from '@/components/environments/EnvironmentDialog';
```
(Add this after the existing `import { LoadTestDialog } ...` line.)

- [ ] **Step 2: Add `envDialogOpen` state**

Current state declarations start around line 93–99:
```tsx
  const [activeSection, setActiveSection] = useState<SectionTab>('params');
  const [docMode, setDocMode] = useState<'edit' | 'preview'>('preview');
  const [unsavedDialogOpen, setUnsavedDialogOpen] = useState(false);
  const [showLoadTest, setShowLoadTest] = useState(false);
  const [saveToCollectionOpen, setSaveToCollectionOpen] = useState(false);
  const [urlError, setUrlError] = useState('');
  const [collectionVars, setCollectionVars] = useState<Record<string, string>>({});
```

Add after the `saveToCollectionOpen` line:
```tsx
  const [envDialogOpen, setEnvDialogOpen] = useState(false);
```

- [ ] **Step 3: Add `handleNavigateToSource` callback**

Find the block of `useCallback` handlers inside the component (there are several — `handleUrlChange`, `handleParamsChange`, etc.). Add this new callback after the last existing `useCallback` (before the `tabDefs` useMemo, around line 400):

```tsx
  const handleNavigateToSource = useCallback(
    (source: VariableSource | 'pathParam') => {
      switch (source) {
        case 'pathParam':
          setActiveSection('params');
          break;
        case 'request':
        case 'runtime':
          setActiveSection('variables');
          break;
        case 'environment':
          setEnvDialogOpen(true);
          break;
        case 'global': {
          const wsId = useWorkspaceStore.getState().activeWorkspaceId;
          if (wsId) usePaneStore.getState().openWorkspaceTabs(wsId, 'environments');
          break;
        }
        case 'collection': {
          const collection = tab.source?.collection;
          if (collection) {
            const found = usePaneStore.getState().openCollectionTab(collection, 'variables');
            if (!found) {
              toast.info('Open the collection tab to edit collection variables.');
            }
          }
          break;
        }
        default:
          break;
      }
    },
    [tab.source?.collection],
  );
```

Note: `toast` is already imported in `RequestPanel` (check the existing imports — if it isn't, add `import { toast } from 'sonner';` to the imports).

- [ ] **Step 4: Update the `VariableAwareUrlInput` prop**

Find line 617:
```tsx
            onSwitchToSection={(section) => setActiveSection(section)}
```

Replace with:
```tsx
            onNavigateToSource={handleNavigateToSource}
```

- [ ] **Step 5: Render `EnvironmentDialog`**

Find line 774:
```tsx
      <LoadTestDialog open={showLoadTest} onOpenChange={setShowLoadTest} request={request} />
```

Add the env dialog directly after it:
```tsx
      <EnvironmentDialog open={envDialogOpen} onOpenChange={setEnvDialogOpen} />
```

- [ ] **Step 6: Add `toast` import**

`RequestPanel` does not currently import `sonner`. Add this import after the existing `import { useEnvStore } from '@/stores/env-store';` line:
```tsx
import { toast } from 'sonner';
```

- [ ] **Step 7: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 8: Lint check**

```bash
yarn check
```

Expected: no errors or warnings.

- [ ] **Step 9: Commit**

```bash
git add src/components/request/RequestPanel.tsx
git commit -m "feat(ux): navigate to correct variable source location from URL input popup"
```

---

## Manual Verification Checklist

Run the app:
```bash
yarn tauri dev
```

Open a request that belongs to a collection. Add a URL with variables of different types, e.g.:
```
https://api.example.com/{{envVar}}/{{collVar}}/:userId
```

| Scenario | Expected |
|----------|----------|
| Click `{{envVar}}` (from active collection env) | Link reads "Collection Environments →"; clicking opens EnvironmentDialog |
| Click `{{globalVar}}` (from active global env) | Link reads "Global Environments →"; clicking switches to workspace Environments tab |
| Click `{{collVar}}` (from collection variables) | Link reads "Collection Variables →"; clicking activates collection tab on Variables section |
| Click `{{collVar}}` when collection tab not open | Link reads "Collection Variables →"; clicking shows toast "Open the collection tab to edit collection variables." |
| Click `{{reqVar}}` (from request variables) | Link reads "Request Variables →"; clicking switches request panel to Variables section |
| Click `:userId` (path param) | Link reads "Params →"; clicking switches request panel to Params section |
| Click `{{folderVar}}` (from folder variables) | No navigation link rendered |
| Click `{{processVar}}` (from process env) | No navigation link rendered |
| Click `{{unknown}}` (unresolved variable) | No navigation link rendered |
