# SP2 Plan 1: Tab System + Split Panes (Updated)

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan. Steps use checkbox (`- [ ]`) syntax for tracking.

> **HARD RULE — shadcn/ui ONLY:** Every interactive UI element MUST use a shadcn/ui component. No raw `<button>`, `<input>`, `<select>`, `<dialog>`, `<table>`, or styled `<div>` acting as a component. Only exceptions: Monaco editor wrapper, `<iframe>` for HTML preview, and layout containers (flex/grid wrappers). If in doubt, use shadcn.

**Goal:** Implement VS Code-style tabs with full bidirectional split pane support. Users can open multiple requests in tabs, split the editor area horizontally or vertically, drag tabs between groups, and resize panes.

**Architecture:** Recursive `PaneNode` binary tree in Zustand. React component tree mirrors data tree. Split panes use shadcn `ResizablePanelGroup`. No Rust backend changes — tabs are purely frontend state.

**Tech Stack:** React 18, TypeScript, Zustand, shadcn/ui, @monaco-editor/react, lucide-react

**Prerequisite:** SP2 Plan 0 (bug fixes) must be complete.

---

## File Structure

```
frontend/src/
  stores/
    pane-store.ts               # PaneNode tree + all tab/split actions
  types/
    pane-types.ts               # PaneNode, Tab, RequestState, ResponseState types
  lib/
    pane-utils.ts               # Tree traversal helpers (find, update, collapse)
    url-params.ts               # URL ↔ query params sync utilities
  components/
    editor/
      MonacoWrapper.tsx          # Shared Monaco wrapper with theme sync
      monaco-config.ts           # Editor options, language detection
      useMonacoTheme.ts          # Light/dark theme sync hook
    panes/
      PaneRenderer.tsx           # Recursive: renders Split or EditorGroup
      EditorGroup.tsx            # Tab bar + active tab content (shadcn Tabs + Resizable)
      TabBar.tsx                 # Horizontal tabs (shadcn Tabs + ContextMenu)
      TabItem.tsx                # Single tab (shadcn TabsTrigger + Badge + Button)
    request/
      RequestPanel.tsx           # Modified: receives tab state instead of global state
```

---

## Chunk 1: shadcn setup + types + tree utilities

### Task 1: Initialize shadcn and install all required components

**Files:**
- Modify: `frontend/package.json`
- Create: `frontend/components.json` (shadcn config)

- [ ] **Step 1: Initialize shadcn/ui**

```bash
cd frontend
yarn dlx shadcn@latest init --preset b2CkJ2CsV --template vite --monorepo
```

Follow the prompts. This creates `components.json` and the `src/components/ui/` directory.

- [ ] **Step 2: Install all required shadcn components**

```bash
npx shadcn add tabs badge button input select table checkbox \
  context-menu alert-dialog dialog card \
  tooltip popover calendar scroll-area \
  resizable label separator dropdown-menu
```

This installs every shadcn component needed across SP2 Plans 1-5. Installing them all now avoids mid-plan interruptions.

- [ ] **Step 3: Install Monaco and icons**

```bash
npm install @monaco-editor/react lucide-react
```

- [ ] **Step 4: Install uuid for tab IDs**

```bash
npm install uuid && npm install -D @types/uuid
```

- [ ] **Step 5: Verify shadcn components exist**

```bash
ls frontend/src/components/ui/
```
Expected: `tabs.tsx`, `badge.tsx`, `button.tsx`, `input.tsx`, `select.tsx`, `table.tsx`, `checkbox.tsx`, `context-menu.tsx`, `alert-dialog.tsx`, `dialog.tsx`, `card.tsx`, `tooltip.tsx`, `popover.tsx`, `calendar.tsx`, `scroll-area.tsx`, `resizable.tsx`, `label.tsx`, `separator.tsx`, `dropdown-menu.tsx`

- [ ] **Step 6: Update Tauri CSP for Monaco**

In `src-tauri/tauri.conf.json`, update the security section:
```json
"app": {
  "security": {
    "csp": "default-src 'self'; script-src 'self' 'unsafe-eval' https://cdn.jsdelivr.net; style-src 'self' 'unsafe-inline'; worker-src blob:; font-src 'self' data:;"
  }
}
```

- [ ] **Step 7: Commit**

```bash
git add frontend/ src-tauri/tauri.conf.json
git commit -m "feat: initialize shadcn/ui + install all components + Monaco + CSP"
```

---

### Task 2: Define type system

**Files:**
- Create: `frontend/src/types/pane-types.ts`

- [ ] **Step 1: Create pane type definitions**

`frontend/src/types/pane-types.ts`:
```typescript
export type PaneNode = SplitNode | LeafNode;

export interface SplitNode {
  type: 'split';
  id: string;
  direction: 'horizontal' | 'vertical';
  children: [PaneNode, PaneNode];
  sizes: [number, number];
}

export interface LeafNode {
  type: 'leaf';
  id: string;
  groupId: string;
  tabs: Tab[];
  activeTabId: string;
}

export interface Tab {
  id: string;
  title: string;
  tabType: 'request' | 'draft' | 'history';
  request: RequestState;
  response: ResponseState | null;
  isDirty: boolean;
  source?: { collection: string; path: string };
}

export interface RequestState {
  method: HttpMethod;
  url: string;
  queryParams: KeyValueEntry[];
  headers: KeyValueEntry[];
  body: BodyState;
  auth: AuthState;
}

export interface KeyValueEntry {
  id: string;
  key: string;
  value: string;
  enabled: boolean;
}

export interface BodyState {
  mode: 'none' | 'json' | 'xml' | 'text' | 'formdata' | 'binary';
  content: string;
  formData: KeyValueEntry[];
}

export interface AuthState {
  authType: 'none' | 'basic' | 'bearer' | 'api-key' | 'oauth2' | 'aws-sig-v4';
  basic?: { username: string; password: string };
  bearer?: { token: string };
  apiKey?: { key: string; value: string; addTo: 'header' | 'query' };
}

export interface ResponseState {
  status: number;
  statusText: string;
  headers: KeyValueEntry[];
  body: string;
  durationMs: number;
  sizeBytes: number;
  activeView: 'pretty' | 'raw' | 'preview' | 'headers';
}

export type HttpMethod = 'GET' | 'POST' | 'PUT' | 'PATCH' | 'DELETE' | 'OPTIONS' | 'HEAD';
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/types/pane-types.ts
git commit -m "feat(tabs): pane type definitions"
```

---

### Task 3: Tree utility functions

**Files:**
- Create: `frontend/src/lib/pane-utils.ts`
- Test: `frontend/src/lib/__tests__/pane-utils.test.ts`

- [ ] **Step 1: Write failing tests**

```typescript
import { describe, it, expect } from 'vitest';
import {
  findLeaf,
  findTabInTree,
  updateLeaf,
  removeLeaf,
  createDefaultLeaf,
  splitLeaf,
} from '../pane-utils';
import type { PaneNode, LeafNode } from '@/types/pane-types';

describe('pane-utils', () => {
  const leaf1 = createDefaultLeaf('g1');
  const leaf2 = createDefaultLeaf('g2');
  const splitTree: PaneNode = {
    type: 'split', id: 's1', direction: 'horizontal',
    children: [leaf1, leaf2], sizes: [50, 50],
  };

  it('findLeaf returns correct leaf by groupId', () => {
    const found = findLeaf(splitTree, 'g1');
    expect(found).toBeDefined();
    expect(found!.groupId).toBe('g1');
  });

  it('findLeaf returns null for missing groupId', () => {
    expect(findLeaf(splitTree, 'missing')).toBeNull();
  });

  it('updateLeaf replaces a leaf immutably', () => {
    const updated = updateLeaf(splitTree, 'g1', (leaf) => ({
      ...leaf,
      activeTabId: 'new-tab',
    }));
    expect(updated).not.toBe(splitTree);
    const found = findLeaf(updated, 'g1') as LeafNode;
    expect(found.activeTabId).toBe('new-tab');
  });

  it('removeLeaf collapses parent split', () => {
    const result = removeLeaf(splitTree, 'g1');
    expect(result.type).toBe('leaf');
    expect((result as LeafNode).groupId).toBe('g2');
  });

  it('splitLeaf creates a split node', () => {
    const single = createDefaultLeaf('g1');
    const result = splitLeaf(single, 'g1', 'vertical');
    expect(result.type).toBe('split');
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
cd frontend && npx vitest run src/lib/__tests__/pane-utils.test.ts
```
Expected: FAIL — module not found.

- [ ] **Step 3: Implement pane-utils**

`frontend/src/lib/pane-utils.ts`:
```typescript
import { v4 as uuid } from 'uuid';
import type { PaneNode, LeafNode, SplitNode, Tab, RequestState } from '@/types/pane-types';

export function createDefaultTab(): Tab {
  return {
    id: uuid(),
    title: 'New request',
    tabType: 'draft',
    request: createDefaultRequest(),
    response: null,
    isDirty: false,
  };
}

export function createDefaultRequest(): RequestState {
  return {
    method: 'GET',
    url: '',
    queryParams: [],
    headers: [],
    body: { mode: 'none', content: '', formData: [] },
    auth: { authType: 'none' },
  };
}

export function createDefaultLeaf(groupId?: string): LeafNode {
  const tab = createDefaultTab();
  return {
    type: 'leaf',
    id: uuid(),
    groupId: groupId ?? uuid(),
    tabs: [tab],
    activeTabId: tab.id,
  };
}

export function findLeaf(node: PaneNode, groupId: string): LeafNode | null {
  if (node.type === 'leaf') {
    return node.groupId === groupId ? node : null;
  }
  return findLeaf(node.children[0], groupId) ?? findLeaf(node.children[1], groupId);
}

export function findTabInTree(node: PaneNode, tabId: string): { leaf: LeafNode; tab: Tab } | null {
  if (node.type === 'leaf') {
    const tab = node.tabs.find((t) => t.id === tabId);
    return tab ? { leaf: node, tab } : null;
  }
  return findTabInTree(node.children[0], tabId) ?? findTabInTree(node.children[1], tabId);
}

export function findFirstLeaf(node: PaneNode): LeafNode {
  if (node.type === 'leaf') return node;
  return findFirstLeaf(node.children[0]);
}

export function updateLeaf(
  node: PaneNode,
  groupId: string,
  updater: (leaf: LeafNode) => LeafNode,
): PaneNode {
  if (node.type === 'leaf') {
    return node.groupId === groupId ? updater(node) : node;
  }
  return {
    ...node,
    children: [
      updateLeaf(node.children[0], groupId, updater),
      updateLeaf(node.children[1], groupId, updater),
    ],
  };
}

export function removeLeaf(node: PaneNode, groupId: string): PaneNode {
  if (node.type === 'leaf') return node;
  const [left, right] = node.children;
  if (left.type === 'leaf' && left.groupId === groupId) return right;
  if (right.type === 'leaf' && right.groupId === groupId) return left;
  return {
    ...node,
    children: [removeLeaf(left, groupId), removeLeaf(right, groupId)],
  };
}

export function splitLeaf(
  node: PaneNode,
  groupId: string,
  direction: 'horizontal' | 'vertical',
): PaneNode {
  if (node.type === 'leaf' && node.groupId === groupId) {
    const newLeaf = createDefaultLeaf();
    return {
      type: 'split',
      id: uuid(),
      direction,
      children: [node, newLeaf],
      sizes: [50, 50],
    };
  }
  if (node.type === 'split') {
    return {
      ...node,
      children: [
        splitLeaf(node.children[0], groupId, direction),
        splitLeaf(node.children[1], groupId, direction),
      ],
    };
  }
  return node;
}

export function updateTabInTree(node: PaneNode, tabId: string, updater: (tab: Tab) => Tab): PaneNode {
  if (node.type === 'leaf') {
    const idx = node.tabs.findIndex((t) => t.id === tabId);
    if (idx === -1) return node;
    const newTabs = [...node.tabs];
    newTabs[idx] = updater(newTabs[idx]);
    return { ...node, tabs: newTabs };
  }
  return {
    ...node,
    children: [
      updateTabInTree(node.children[0], tabId, updater),
      updateTabInTree(node.children[1], tabId, updater),
    ],
  };
}
```

- [ ] **Step 4: Run tests to verify they pass**

```bash
cd frontend && npx vitest run src/lib/__tests__/pane-utils.test.ts
```
Expected: PASS — 5 tests.

- [ ] **Step 5: Commit**

```bash
git add frontend/src/lib/pane-utils.ts frontend/src/lib/__tests__/pane-utils.test.ts
git commit -m "feat(tabs): pane tree utilities with tests"
```

---

## Chunk 2: Zustand store + Monaco wrapper

### Task 4: Implement pane store

**Files:**
- Create: `frontend/src/stores/pane-store.ts`
- Test: `frontend/src/stores/__tests__/pane-store.test.ts`

- [ ] **Step 1: Write failing tests**

Test: initial state (one leaf, one draft tab), newDraftTab, closeTab, splitGroup, updateRequest sets isDirty.

- [ ] **Step 2: Implement the store**

Full Zustand store with all actions: `newDraftTab`, `openTab`, `closeTab`, `setActiveTab`, `moveTab`, `splitGroup`, `resizePane`, `updateRequest`, `setResponse`, `markDirty`, `markClean`, `reset`.

Uses the pane-utils functions for all tree mutations. All updates are immutable.

- [ ] **Step 3: Run tests**

```bash
cd frontend && npx vitest run src/stores/__tests__/pane-store.test.ts
```
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add frontend/src/stores/pane-store.ts frontend/src/stores/__tests__/
git commit -m "feat(tabs): Zustand pane store with full action set"
```

---

### Task 5: Monaco editor wrapper

**Files:**
- Create: `frontend/src/components/editor/MonacoWrapper.tsx`
- Create: `frontend/src/components/editor/monaco-config.ts`
- Create: `frontend/src/components/editor/useMonacoTheme.ts`

- [ ] **Step 1: Install Monaco**

```bash
cd frontend && npm install @monaco-editor/react
```

- [ ] **Step 2: Create Monaco configuration**

`frontend/src/components/editor/monaco-config.ts`:

Shared editor options (fontSize 13, JetBrains Mono font family, tab size 2, word wrap on, bracket pair colorization). Read-only variant adds minimap + disables editing. Language detection function maps body mode / content-type to Monaco language ID.

- [ ] **Step 3: Create theme sync hook**

`frontend/src/components/editor/useMonacoTheme.ts`:

Defines `rocket-light` and `rocket-dark` themes using the app's color ramps (teal for strings, amber for numbers, purple for keywords). Listens to `prefers-color-scheme` media query and switches themes automatically.

- [ ] **Step 4: Create MonacoWrapper component**

`frontend/src/components/editor/MonacoWrapper.tsx`:

Props: `value`, `onChange`, `language`, `bodyMode`, `contentType`, `readOnly`, `height`, `onMount`.

Resolves language from props, applies correct theme and options, renders `<Editor>` from `@monaco-editor/react` with loading placeholder.

- [ ] **Step 5: Test Monaco loads**

Create a temporary test page or render MonacoWrapper in any existing component:
```tsx
<MonacoWrapper value='{"test": true}' bodyMode="json" height="200px" />
```
Verify: editor renders, JSON is syntax-highlighted, theme matches app.

- [ ] **Step 6: Commit**

```bash
git add frontend/src/components/editor/
git commit -m "feat(editor): Monaco wrapper with theme sync + language detection"
```

---

## Chunk 3: React components (all shadcn)

### Task 6: TabItem component

**Files:**
- Create: `frontend/src/components/panes/TabItem.tsx`

- [ ] **Step 1: Implement TabItem using shadcn primitives**

```tsx
import { Button } from '@/components/ui/button';
import { Badge } from '@/components/ui/badge';
import { X } from 'lucide-react';
import type { Tab, HttpMethod } from '@/types/pane-types';

const METHOD_COLORS: Record<HttpMethod, string> = {
  GET: 'bg-green-100 text-green-800 dark:bg-green-900/30 dark:text-green-400',
  POST: 'bg-amber-100 text-amber-800 dark:bg-amber-900/30 dark:text-amber-400',
  PUT: 'bg-blue-100 text-blue-800 dark:bg-blue-900/30 dark:text-blue-400',
  PATCH: 'bg-purple-100 text-purple-800 dark:bg-purple-900/30 dark:text-purple-400',
  DELETE: 'bg-red-100 text-red-800 dark:bg-red-900/30 dark:text-red-400',
  OPTIONS: 'bg-gray-100 text-gray-800 dark:bg-gray-900/30 dark:text-gray-400',
  HEAD: 'bg-gray-100 text-gray-800 dark:bg-gray-900/30 dark:text-gray-400',
};

interface TabItemProps {
  tab: Tab;
  isActive: boolean;
  onSelect: () => void;
  onClose: () => void;
}

export function TabItem({ tab, isActive, onSelect, onClose }: TabItemProps) {
  return (
    <button
      role="tab"
      aria-selected={isActive}
      onClick={onSelect}
      className={cn(
        'group flex items-center gap-1.5 px-3 py-1.5 text-sm border-b-2 transition-colors',
        isActive
          ? 'border-primary text-foreground'
          : 'border-transparent text-muted-foreground hover:text-foreground',
      )}
    >
      <Badge variant="outline" className={cn('text-[10px] px-1 py-0 font-mono', METHOD_COLORS[tab.request.method])}>
        {tab.request.method}
      </Badge>
      <span className="max-w-[120px] truncate">{tab.title}</span>
      {tab.isDirty && (
        <span className="w-1.5 h-1.5 rounded-full bg-primary" />
      )}
      <Button
        variant="ghost"
        size="icon"
        className="h-4 w-4 opacity-0 group-hover:opacity-100"
        onClick={(e) => { e.stopPropagation(); onClose(); }}
      >
        <X className="h-3 w-3" />
      </Button>
    </button>
  );
}
```

> **Note:** The outer `<button>` here is the tab trigger itself — this is acceptable as it's a semantic `role="tab"` element. All interactive sub-elements (close button) use shadcn `Button`.

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/panes/TabItem.tsx
git commit -m "feat(tabs): TabItem with method badge, dirty dot, close button"
```

---

### Task 7: TabBar with context menu

**Files:**
- Create: `frontend/src/components/panes/TabBar.tsx`

- [ ] **Step 1: Implement TabBar using shadcn ContextMenu**

```tsx
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { Button } from '@/components/ui/button';
import { ScrollArea, ScrollBar } from '@/components/ui/scroll-area';
import { Plus } from 'lucide-react';
import { TabItem } from './TabItem';
import { usePaneStore } from '@/stores/pane-store';
import type { LeafNode } from '@/types/pane-types';

interface TabBarProps {
  node: LeafNode;
}

export function TabBar({ node }: TabBarProps) {
  const { setActiveTab, closeTab, newDraftTab, splitGroup } = usePaneStore();

  return (
    <div className="flex items-center border-b">
      <ScrollArea className="flex-1">
        <div className="flex" role="tablist">
          {node.tabs.map((tab) => (
            <ContextMenu key={tab.id}>
              <ContextMenuTrigger>
                <TabItem
                  tab={tab}
                  isActive={tab.id === node.activeTabId}
                  onSelect={() => setActiveTab(tab.id, node.groupId)}
                  onClose={() => closeTab(tab.id, node.groupId)}
                />
              </ContextMenuTrigger>
              <ContextMenuContent>
                <ContextMenuItem onClick={() => closeTab(tab.id, node.groupId)}>
                  Close
                </ContextMenuItem>
                <ContextMenuItem onClick={() => {
                  node.tabs.filter(t => t.id !== tab.id).forEach(t => closeTab(t.id, node.groupId));
                }}>
                  Close others
                </ContextMenuItem>
                <ContextMenuSeparator />
                <ContextMenuItem onClick={() => splitGroup(node.groupId, 'vertical')}>
                  Split right
                </ContextMenuItem>
                <ContextMenuItem onClick={() => splitGroup(node.groupId, 'horizontal')}>
                  Split down
                </ContextMenuItem>
              </ContextMenuContent>
            </ContextMenu>
          ))}
        </div>
        <ScrollBar orientation="horizontal" />
      </ScrollArea>
      <Button
        variant="ghost"
        size="icon"
        className="h-8 w-8 shrink-0"
        onClick={() => newDraftTab(node.groupId)}
      >
        <Plus className="h-4 w-4" />
      </Button>
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/panes/TabBar.tsx
git commit -m "feat(tabs): TabBar with shadcn ContextMenu + ScrollArea"
```

---

### Task 8: EditorGroup (tab bar + request panel with Monaco)

**Files:**
- Create: `frontend/src/components/panes/EditorGroup.tsx`

- [ ] **Step 1: Implement EditorGroup**

Renders `TabBar` at top, then the active tab's `RequestPanel` below. The request body section uses `MonacoWrapper` for JSON/XML/text modes. FormData and binary modes render their own specialized UI (shadcn Table for formdata, shadcn Button + Card for binary).

```tsx
import { TabBar } from './TabBar';
import { RequestPanel } from '@/components/request/RequestPanel';
import { usePaneStore } from '@/stores/pane-store';
import type { LeafNode } from '@/types/pane-types';

interface EditorGroupProps {
  node: LeafNode;
}

export function EditorGroup({ node }: EditorGroupProps) {
  const { setActiveTab } = usePaneStore();
  const activeTab = node.tabs.find((t) => t.id === node.activeTabId);

  return (
    <div className="flex flex-col h-full">
      <TabBar node={node} />
      <div className="flex-1 overflow-hidden">
        {activeTab ? (
          <RequestPanel tab={activeTab} groupId={node.groupId} />
        ) : (
          <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
            No open tabs
          </div>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/panes/EditorGroup.tsx
git commit -m "feat(tabs): EditorGroup — tab bar + request panel"
```

---

### Task 9: PaneRenderer with shadcn Resizable

**Files:**
- Create: `frontend/src/components/panes/PaneRenderer.tsx`

- [ ] **Step 1: Implement PaneRenderer using shadcn ResizablePanelGroup**

```tsx
import {
  ResizablePanelGroup,
  ResizablePanel,
  ResizableHandle,
} from '@/components/ui/resizable';
import { EditorGroup } from './EditorGroup';
import { usePaneStore } from '@/stores/pane-store';
import type { PaneNode } from '@/types/pane-types';

interface PaneRendererProps {
  node: PaneNode;
}

export function PaneRenderer({ node }: PaneRendererProps) {
  const { resizePane } = usePaneStore();

  if (node.type === 'leaf') {
    return <EditorGroup node={node} />;
  }

  // Split node — render two children with a resize handle
  return (
    <ResizablePanelGroup
      direction={node.direction}
      onLayout={(sizes) => {
        if (sizes.length === 2) {
          resizePane(node.id, [sizes[0], sizes[1]]);
        }
      }}
    >
      <ResizablePanel defaultSize={node.sizes[0]} minSize={15}>
        <PaneRenderer node={node.children[0]} />
      </ResizablePanel>
      <ResizableHandle withHandle />
      <ResizablePanel defaultSize={node.sizes[1]} minSize={15}>
        <PaneRenderer node={node.children[1]} />
      </ResizablePanel>
    </ResizablePanelGroup>
  );
}
```

- [ ] **Step 2: Commit**

```bash
git add frontend/src/components/panes/PaneRenderer.tsx
git commit -m "feat(tabs): PaneRenderer with shadcn ResizablePanelGroup"
```

---

### Task 10: Update RequestPanel to use Monaco + tab state

**Files:**
- Modify: `frontend/src/components/request/RequestPanel.tsx`

- [ ] **Step 1: Refactor RequestPanel to receive tab props**

The existing `RequestPanel` likely uses global state or a single "active request" pattern. Refactor it to:
- Accept `tab: Tab` and `groupId: string` as props
- Use `usePaneStore().updateRequest(tab.id, ...)` for mutations
- Use `usePaneStore().setResponse(tab.id, ...)` after execution

- [ ] **Step 2: Replace body textarea with MonacoWrapper**

For body modes `json`, `xml`, `text`:
```tsx
import { MonacoWrapper } from '@/components/editor/MonacoWrapper';

// Inside the Body tab content:
{tab.request.body.mode !== 'none' &&
 tab.request.body.mode !== 'formdata' &&
 tab.request.body.mode !== 'binary' && (
  <MonacoWrapper
    value={tab.request.body.content}
    onChange={(val) => updateRequest(tab.id, {
      body: { ...tab.request.body, content: val },
    })}
    bodyMode={tab.request.body.mode}
    height="250px"
  />
)}
```

For formdata mode: use shadcn `Table` with `Input` cells + `Checkbox` for enabled + `Button` for add/delete.

- [ ] **Step 3: Replace body mode selector with shadcn Select**

```tsx
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';

<Select
  value={tab.request.body.mode}
  onValueChange={(mode) => updateRequest(tab.id, {
    body: { ...tab.request.body, mode: mode as BodyState['mode'] },
  })}
>
  <SelectTrigger className="w-[120px]">
    <SelectValue />
  </SelectTrigger>
  <SelectContent>
    <SelectItem value="none">None</SelectItem>
    <SelectItem value="json">JSON</SelectItem>
    <SelectItem value="xml">XML</SelectItem>
    <SelectItem value="text">Text</SelectItem>
    <SelectItem value="formdata">Form Data</SelectItem>
    <SelectItem value="binary">Binary</SelectItem>
  </SelectContent>
</Select>
```

- [ ] **Step 4: Replace method selector with shadcn Select**

```tsx
<Select
  value={tab.request.method}
  onValueChange={(method) => updateRequest(tab.id, { method: method as HttpMethod })}
>
  <SelectTrigger className="w-[100px]">
    <SelectValue />
  </SelectTrigger>
  <SelectContent>
    <SelectItem value="GET">GET</SelectItem>
    <SelectItem value="POST">POST</SelectItem>
    <SelectItem value="PUT">PUT</SelectItem>
    <SelectItem value="PATCH">PATCH</SelectItem>
    <SelectItem value="DELETE">DELETE</SelectItem>
    <SelectItem value="OPTIONS">OPTIONS</SelectItem>
    <SelectItem value="HEAD">HEAD</SelectItem>
  </SelectContent>
</Select>
```

- [ ] **Step 5: Replace URL input with shadcn Input**

```tsx
import { Input } from '@/components/ui/input';

<Input
  value={tab.request.url}
  onChange={(e) => updateRequest(tab.id, { url: e.target.value })}
  placeholder="Enter request URL"
  className="flex-1 font-mono text-sm"
/>
```

- [ ] **Step 6: Replace Send button with shadcn Button**

```tsx
import { Button } from '@/components/ui/button';
import { Send } from 'lucide-react';

<Button onClick={handleSend} disabled={isLoading}>
  <Send className="h-4 w-4 mr-2" />
  Send
</Button>
```

- [ ] **Step 7: Replace request panel tabs with shadcn Tabs**

The sub-tabs inside the request panel (Params, Headers, Body, Auth) use shadcn Tabs:
```tsx
import { Tabs, TabsList, TabsTrigger, TabsContent } from '@/components/ui/tabs';

<Tabs defaultValue="params">
  <TabsList>
    <TabsTrigger value="params">Params</TabsTrigger>
    <TabsTrigger value="headers">Headers</TabsTrigger>
    <TabsTrigger value="body">Body</TabsTrigger>
    <TabsTrigger value="auth">Auth</TabsTrigger>
  </TabsList>
  <TabsContent value="params">{/* QueryParamsEditor */}</TabsContent>
  <TabsContent value="headers">{/* HeadersEditor */}</TabsContent>
  <TabsContent value="body">{/* Body editor with Monaco */}</TabsContent>
  <TabsContent value="auth">{/* AuthEditor */}</TabsContent>
</Tabs>
```

- [ ] **Step 8: Replace headers editor table with shadcn Table**

```tsx
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from '@/components/ui/table';
import { Checkbox } from '@/components/ui/checkbox';
import { Input } from '@/components/ui/input';
import { Button } from '@/components/ui/button';
import { Trash2, Plus } from 'lucide-react';

// Each header row:
<TableRow>
  <TableCell><Checkbox checked={header.enabled} onCheckedChange={...} /></TableCell>
  <TableCell><Input value={header.key} onChange={...} placeholder="Key" /></TableCell>
  <TableCell><Input value={header.value} onChange={...} placeholder="Value" /></TableCell>
  <TableCell>
    <Button variant="ghost" size="icon" onClick={...}>
      <Trash2 className="h-4 w-4" />
    </Button>
  </TableCell>
</TableRow>
```

- [ ] **Step 9: Add close-unsaved dialog using shadcn AlertDialog**

```tsx
import {
  AlertDialog, AlertDialogAction, AlertDialogCancel,
  AlertDialogContent, AlertDialogDescription,
  AlertDialogFooter, AlertDialogHeader, AlertDialogTitle,
} from '@/components/ui/alert-dialog';

<AlertDialog open={showCloseConfirm}>
  <AlertDialogContent>
    <AlertDialogHeader>
      <AlertDialogTitle>Unsaved changes</AlertDialogTitle>
      <AlertDialogDescription>
        This request has unsaved changes. Do you want to save before closing?
      </AlertDialogDescription>
    </AlertDialogHeader>
    <AlertDialogFooter>
      <AlertDialogCancel onClick={discardAndClose}>Discard</AlertDialogCancel>
      <AlertDialogAction onClick={saveAndClose}>Save</AlertDialogAction>
    </AlertDialogFooter>
  </AlertDialogContent>
</AlertDialog>
```

- [ ] **Step 10: Verify all interactive elements are shadcn**

```bash
# Check for any raw HTML interactive elements in new/modified files
grep -rn "<button\|<input\|<select\|<textarea\|<dialog" \
  frontend/src/components/panes/ \
  frontend/src/components/request/ \
  --include="*.tsx" | grep -v "shadcn\|ui/" | grep -v "role="
```
Expected: no results (except `role="tab"` on TabItem which is semantic).

- [ ] **Step 11: Commit**

```bash
git add frontend/src/components/request/
git commit -m "feat(tabs): RequestPanel refactored — shadcn components + Monaco body editor"
```

---

## Chunk 4: Wire into layout + end-to-end test

### Task 11: Replace main layout with PaneRenderer

**Files:**
- Modify: `frontend/src/components/layout/MainLayout.tsx`
- Modify: `frontend/src/components/collections/CollectionsSidebar.tsx`

- [ ] **Step 1: Replace single RequestPanel with PaneRenderer**

In `MainLayout.tsx`:
```tsx
import { PaneRenderer } from '@/components/panes/PaneRenderer';
import { usePaneStore } from '@/stores/pane-store';

export function MainLayout() {
  const { root } = usePaneStore();

  return (
    <ResizablePanelGroup direction="horizontal">
      <ResizablePanel defaultSize={20} minSize={15} maxSize={35}>
        <CollectionsSidebar />
      </ResizablePanel>
      <ResizableHandle withHandle />
      <ResizablePanel defaultSize={80}>
        <PaneRenderer node={root} />
      </ResizablePanel>
    </ResizablePanelGroup>
  );
}
```

Note: the sidebar itself is a resizable panel — this uses shadcn `ResizablePanelGroup` for the top-level layout too.

- [ ] **Step 2: Update sidebar to open tabs**

When clicking a request in the sidebar:
```tsx
import { usePaneStore } from '@/stores/pane-store';
import { v4 as uuid } from 'uuid';

function handleRequestClick(collection: string, path: string, request: Request) {
  const { openTab } = usePaneStore.getState();
  openTab({
    id: `${collection}/${path}`,  // Stable ID so re-clicking focuses existing tab
    title: request.name,
    tabType: 'request',
    request: { /* map from collection Request to RequestState */ },
    response: null,
    isDirty: false,
    source: { collection, path },
  });
}
```

- [ ] **Step 3: End-to-end smoke test**

```bash
cargo tauri dev
```

Verify:
- [ ] App opens with one empty draft tab
- [ ] Click request in sidebar → opens in tab with correct method/URL
- [ ] Click same request again → focuses existing tab (no duplicate)
- [ ] Click "+" → new draft tab
- [ ] Edit URL → dirty dot appears on tab
- [ ] Right-click tab → context menu with Close / Split Right / Split Down
- [ ] Split Right → pane splits vertically with resize handle
- [ ] Split Down → pane splits horizontally
- [ ] Drag resize handle → panes resize
- [ ] Close tab → tab closes
- [ ] Close last tab in a group → group collapses, tree simplifies
- [ ] Body editor shows Monaco with syntax highlighting for JSON
- [ ] All buttons/inputs/selects are shadcn components (no unstyled raw HTML)
- [ ] Light/dark mode → Monaco theme switches correctly

- [ ] **Step 4: Commit**

```bash
git add frontend/src/components/layout/ frontend/src/components/collections/
git commit -m "feat(tabs): wire PaneRenderer into MainLayout + sidebar opens tabs"
```

---

## Milestone Checklist — Plan 1

- [ ] shadcn/ui initialized with all required components installed
- [ ] Monaco editor wrapper with theme sync working
- [ ] `PaneNode` types + tree utilities — 5 tests passing
- [ ] Zustand pane store — 4+ tests passing
- [ ] PaneRenderer recursively renders split/leaf using shadcn ResizablePanelGroup
- [ ] TabBar with shadcn ContextMenu (Close, Close Others, Split Right, Split Down)
- [ ] TabItem with method Badge, dirty dot, close Button
- [ ] RequestPanel refactored: all inputs/buttons/selects/tables are shadcn
- [ ] Body editor uses MonacoWrapper for JSON/XML/text
- [ ] Close unsaved dialog uses shadcn AlertDialog
- [ ] Sidebar opens requests as tabs (no duplicates)
- [ ] **Zero raw HTML interactive elements** — verified with grep
- [ ] No regressions in existing functionality
