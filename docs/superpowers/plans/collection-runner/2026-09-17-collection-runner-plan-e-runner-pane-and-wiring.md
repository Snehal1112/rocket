# Collection Runner Plan E: RunnerPane and Wiring — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Compose the Runner tab's picker + view-switching pane, register
it in the tab-rendering chain, and add both entry points (sidebar
context menu, blank-tab picker).

**Architecture:** `RunnerPane` follows the same `{ tab, groupId }` prop
shape `RequestPanel` already uses. It renders a collection picker when
`tab.collectionName` is `null` (the blank-tab entry point), otherwise
`RunnerSummaryHeader` plus `RunnerRequestList` (idle) or
`RunnerResultsList` (running/stopped/done) from Plan D.
`EditorGroup.tsx`'s existing tab-type ternary chain gets one more branch
(`isRunnerTab`), mirroring how `isContractTab`/`isWorkspaceTab` are
already chained there. Sidebar entry points call `openRunnerTab` (Plan C
Task 1) directly from `CollectionNode.tsx`/`FolderNode.tsx`'s existing
`ContextMenuItem` lists — the same pattern `openContractTab` already
uses in `CollectionNode.tsx`.

**Tech Stack:** React, TypeScript, Zustand, Vitest, Testing Library,
shadcn/ui (`Select`, `Button`), `lucide-react`.

**Spec:** `docs/superpowers/specs/2026-09-17-collection-runner-frontend-design.md`

## Global Constraints

- No backend/Rust changes.
- The blank-tab picker (Task 1) only lets the user pick a **collection**,
  not a folder within it — a whole-collection run. Folder-scoped runs
  are only available via the sidebar's "Run folder" action (Task 3).
  This is a deliberate v1 scope trim (YAGNI): building a folder-only
  tree picker inside the blank tab is extra UI for a case the sidebar
  already covers directly.
- `FolderNode.tsx` and `CollectionNode.tsx` already receive both
  `collectionName` (what `openRunnerTab` needs) and `collectionRoot`
  (the absolute path, used only for contract/git IPC) as separate props
  — use `collectionName`, not `collectionRoot`.

---

### Task 1: `RunnerPane`

**Files:**
- Create: `src/components/request/runner/RunnerPane.tsx`
- Test: `src/components/request/runner/RunnerPane.test.tsx`

**Interfaces:**
- Consumes: `RunnerTab` (Plan A), `openRunnerTab`/`closeTab` (Plan C
  Task 1 / existing, via `usePaneStore`), `RunnerSummaryHeader` (Plan D
  Task 3), `RunnerRequestList` (Plan D Task 1), `RunnerResultsList`
  (Plan D Task 2), `listCollections` (`@/lib/tauri-api`, existing).
- Produces: `RunnerPane({ tab, groupId }: { tab: RunnerTab; groupId: string })`
  — consumed by Plan E Task 2 (`EditorGroup.tsx`).

- [ ] **Step 1: Write the failing test**

```tsx
// src/components/request/runner/RunnerPane.test.tsx
import { render, screen, waitFor } from '@testing-library/react';
import { fireEvent } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerTab } from '@/types/pane-types';
import { RunnerPane } from './RunnerPane';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, listCollections: vi.fn(), getCollection: vi.fn() };
});

function pickerTab(): RunnerTab {
  return {
    id: 'blank-1',
    title: 'Runner',
    isDirty: false,
    tabType: 'runner',
    collectionName: null,
    runState: 'idle',
    requests: [],
  };
}

function scopedTab(runState: RunnerTab['runState'] = 'idle'): RunnerTab {
  return {
    id: 'scoped-1',
    title: 'Run: demo',
    isDirty: false,
    tabType: 'runner',
    collectionName: 'demo',
    runState,
    requests: [],
  };
}

describe('RunnerPane', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
  });

  it('shows a collection picker when collectionName is null', async () => {
    const { listCollections } = await import('@/lib/tauri-api');
    vi.mocked(listCollections).mockResolvedValue([
      { uid: 'c1', name: 'demo', path: '/tmp/demo', requestCount: 1 },
    ]);

    render(<RunnerPane tab={pickerTab()} groupId='g1' />);
    await waitFor(() => expect(listCollections).toHaveBeenCalled());
    expect(screen.getByText(/choose a collection/i)).toBeInTheDocument();
  });

  it('shows RunnerSummaryHeader and RunnerRequestList when idle with a collection set', () => {
    const tab = scopedTab('idle');
    usePaneStore.setState((s) => ({
      root: { ...s.root, type: 'leaf', tabs: [tab], activeTabId: tab.id } as typeof s.root,
    }));
    render(<RunnerPane tab={tab} groupId='g1' />);
    expect(screen.getByText(/no requests/i)).toBeInTheDocument(); // RunnerRequestList empty state
  });

  it('shows RunnerResultsList once the run is done', () => {
    const tab = scopedTab('done');
    usePaneStore.setState((s) => ({
      root: { ...s.root, type: 'leaf', tabs: [tab], activeTabId: tab.id } as typeof s.root,
    }));
    render(<RunnerPane tab={tab} groupId='g1' />);
    expect(screen.getByText(/no requests/i)).toBeInTheDocument(); // RunnerResultsList empty state
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/components/request/runner/RunnerPane.test.tsx`
Expected: FAIL — the component file does not exist.

- [ ] **Step 3: Implement the component**

```tsx
// src/components/request/runner/RunnerPane.tsx
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from '@/components/ui/select';
import { type CollectionSummary, listCollections } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { RunnerTab } from '@/types/pane-types';
import { RunnerRequestList } from './RunnerRequestList';
import { RunnerResultsList } from './RunnerResultsList';
import { RunnerSummaryHeader } from './RunnerSummaryHeader';

export function RunnerPane({ tab, groupId }: { tab: RunnerTab; groupId: string }) {
  const openRunnerTab = usePaneStore((s) => s.openRunnerTab);
  const closeTab = usePaneStore((s) => s.closeTab);
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [selected, setSelected] = useState('');

  useEffect(() => {
    if (tab.collectionName === null) {
      void listCollections().then(setCollections);
    }
  }, [tab.collectionName]);

  if (tab.collectionName === null) {
    return (
      <div className='flex flex-col items-center justify-center h-full gap-3 p-6'>
        <p className='text-sm text-muted-foreground'>Choose a collection to run</p>
        <Select value={selected} onValueChange={setSelected}>
          <SelectTrigger className='w-64'>
            <SelectValue placeholder='Select collection' />
          </SelectTrigger>
          <SelectContent>
            {collections.map((c) => (
              <SelectItem key={c.name} value={c.name}>
                {c.name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
        <Button
          size='sm'
          disabled={!selected}
          onClick={() => {
            void openRunnerTab(selected);
            closeTab(tab.id, groupId);
          }}
        >
          Load
        </Button>
      </div>
    );
  }

  return (
    <div className='flex flex-col h-full'>
      <RunnerSummaryHeader tab={tab} />
      {tab.runState === 'idle' ? <RunnerRequestList tab={tab} /> : <RunnerResultsList tab={tab} />}
    </div>
  );
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn vitest run src/components/request/runner/RunnerPane.test.tsx`
Expected: PASS (3 tests)

- [ ] **Step 5: Run the full frontend test suite and type check**

Run: `yarn vitest run && yarn tsc --noEmit`
Expected: PASS, no new errors.

- [ ] **Step 6: Commit**

```bash
git add src/components/request/runner/RunnerPane.tsx src/components/request/runner/RunnerPane.test.tsx
git commit -m "feat: add RunnerPane with collection picker and view switching"
```

---

### Task 2: Register `RunnerTab` in `EditorGroup` and the new-tab menu

**Files:**
- Modify: `src/components/panes/EditorGroup.tsx:38,189` (imports and the
  tab-type ternary chain)
- Modify: `src/components/panes/TabBar.tsx:1-20,248-261` (imports and
  the "new tab" context menu)
- Test: `src/components/panes/__tests__/TabBar.test.tsx` (add to
  existing file if present — if no such file exists yet, create it
  following the pattern of the other `__tests__` component tests in
  this plan set, e.g. `RunnerRequestList.test.tsx`)

**Interfaces:**
- Consumes: `RunnerPane` (Task 1), `isRunnerTab` (Plan A Task 1),
  `openRunnerTab` (Plan C Task 1, via `usePaneStore`).
- Produces: a `RunnerTab` now renders as `RunnerPane` when active, and a
  "Runner" option is selectable from the new-tab menu.

- [ ] **Step 1: Write the failing test**

Check first whether `src/components/panes/__tests__/TabBar.test.tsx`
already exists:

```bash
ls src/components/panes/__tests__/TabBar.test.tsx 2>&1
```

If it exists, add this `it` block to its existing top-level `describe`.
If it does not exist, create the file with this content (adjust the
`describe` name only if the existing convention in
`src/components/panes/__tests__/` differs — check that directory first):

```tsx
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import { TabBar } from '../TabBar';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn().mockResolvedValue({ name: 'demo', settings: { headers: [], variables: [] }, root: { uid: 'r', name: 'demo', items: [] } }) };
});

describe('TabBar new-tab menu', () => {
  beforeEach(() => usePaneStore.getState().reset());

  it('opens a blank runner tab from the new-tab context menu', () => {
    const { root } = usePaneStore.getState();
    if (root.type !== 'leaf') throw new Error('Expected leaf root');
    render(<TabBar node={root} onCloseTab={() => {}} />);

    fireEvent.contextMenu(screen.getByLabelText('New request'));
    fireEvent.click(screen.getByText('Runner'));

    const updated = usePaneStore.getState().root;
    if (updated.type !== 'leaf') throw new Error('Expected leaf root');
    expect(updated.tabs.some((t) => t.tabType === 'runner')).toBe(true);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/components/panes/__tests__/TabBar.test.tsx`
Expected: FAIL — no "Runner" item in the context menu yet.

- [ ] **Step 3: Wire `EditorGroup.tsx`**

> Note: Plan A Task 1 already had to touch this file — adding `RunnerTab`
> to the `Tab` union broke this file's exhaustive ternary chain, so that
> task's implementer added a minimal placeholder branch to keep
> `tsc --noEmit` clean (see that plan's ledger for the ruling). Check the
> current state of the file first:
> ```bash
> grep -n "isRunnerTab" src/components/panes/EditorGroup.tsx
> ```
> `isRunnerTab` is already imported, and there is already a branch
> `) : isRunnerTab(activeTab) ? (<EmptyState variant={emptyStateVariant} />` —
> positioned after the `isWorkspaceTab` block's closing `: null` and
> right before the final `CollectionOverviewTab` fallback (NOT after
> `isContractDiffTab` as originally planned below). **Replace** that
> existing branch's body in place — do not insert a second
> `isRunnerTab` branch elsewhere in the chain, and do not duplicate the
> `isRunnerTab` import.

Add `import { RunnerPane } from '@/components/request/runner/RunnerPane';`
to the file's import block (this part of the original plan still
applies — only `isRunnerTab` itself was already imported by Task 1's
follow-on edit).

Replace the existing placeholder branch:

```tsx
          ) : isRunnerTab(activeTab) ? (
            <EmptyState variant={emptyStateVariant} />
          ) : (
```

with:

```tsx
          ) : isRunnerTab(activeTab) ? (
            <RunnerPane tab={activeTab} groupId={node.groupId} />
          ) : (
```

- [ ] **Step 4: Wire `TabBar.tsx`**

Add `ListChecks` to the `lucide-react` import at the top of
`src/components/panes/TabBar.tsx` (alongside the existing `Globe`,
`Braces`, `Zap`, `Radio`), and add
`const openRunnerTab = usePaneStore((s) => s.openRunnerTab);` next to
the existing `const openEphemeralTab = usePaneStore((s) => s.openEphemeralTab);`
line.

In the context menu at `src/components/panes/TabBar.tsx:258-260`, add
one more item after the WebSocket entry:

```tsx
          <ContextMenuItem onClick={() => openEphemeralTab('websocket')}>
            <Radio className='h-3.5 w-3.5 mr-2' /> WebSocket
          </ContextMenuItem>
          <ContextMenuItem onClick={() => void openRunnerTab(null)}>
            <ListChecks className='h-3.5 w-3.5 mr-2' /> Runner
          </ContextMenuItem>
```

- [ ] **Step 5: Run test to verify it passes**

Run: `yarn vitest run src/components/panes/__tests__/TabBar.test.tsx`
Expected: PASS

- [ ] **Step 6: Run the full frontend test suite and type check**

Run: `yarn vitest run && yarn tsc --noEmit`
Expected: PASS, no new errors.

- [ ] **Step 7: Commit**

```bash
git add src/components/panes/EditorGroup.tsx src/components/panes/TabBar.tsx src/components/panes/__tests__/TabBar.test.tsx
git commit -m "feat: render RunnerTab via RunnerPane and add Runner to the new-tab menu"
```

---

### Task 3: Sidebar "Run collection" / "Run folder" actions

**Files:**
- Modify: `src/components/collections/CollectionNode.tsx:1-14,464-478`
  (imports and the collection context menu)
- Modify: `src/components/collections/FolderNode.tsx:15-20,267-280`
  (imports and the folder context menu)
- Test: `src/components/collections/CollectionNode.test.tsx` and
  `src/components/collections/FolderNode.test.tsx` if such files exist
  (check first); otherwise add a small dedicated test file for each,
  following the render/fireEvent pattern used throughout this plan set.

**Interfaces:**
- Consumes: `openRunnerTab` (Plan C Task 1, via `usePaneStore`).
- Produces: a "Run collection" item on every collection's context menu,
  a "Run folder" item on every folder's context menu.

- [ ] **Step 1: Write the failing test**

Check first:

```bash
ls src/components/collections/CollectionNode.test.tsx src/components/collections/FolderNode.test.tsx 2>&1
```

If neither exists, this codebase does not currently unit-test these two
components directly (they are large, dialog-heavy tree nodes) — in that
case, verify this task by extending
`src/stores/__tests__/pane-store.test.ts`'s `describe('Runner tab actions', ...)`
block instead is not appropriate (that tests the store, not the menu
wiring), so add a minimal, focused new test file for just the new menu
item in each component:

```tsx
// src/components/collections/CollectionNode.test.tsx (only if the file
// does not already exist — if it does, add this it() block to it)
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import { CollectionNode } from './CollectionNode';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn().mockResolvedValue({ name: 'demo', settings: { headers: [], variables: [] }, root: { uid: 'r', name: 'demo', items: [] } }) };
});

describe('CollectionNode "Run collection"', () => {
  beforeEach(() => usePaneStore.getState().reset());

  it('opens a runner tab scoped to the collection', () => {
    const summary = { uid: 'c1', name: 'demo', path: '/tmp/demo', requestCount: 0 };
    render(<CollectionNode summary={summary} />);

    fireEvent.contextMenu(screen.getByText('demo'));
    fireEvent.click(screen.getByText('Run collection'));

    const { root } = usePaneStore.getState();
    if (root.type !== 'leaf') throw new Error('Expected leaf root');
    expect(root.tabs.some((t) => t.tabType === 'runner')).toBe(true);
  });
});
```

```tsx
// src/components/collections/FolderNode.test.tsx (only if the file
// does not already exist — if it does, add this it() block to it)
import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import { FolderNode } from './FolderNode';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn().mockResolvedValue({ name: 'demo', settings: { headers: [], variables: [] }, root: { uid: 'r', name: 'demo', items: [] } }) };
});

describe('FolderNode "Run folder"', () => {
  beforeEach(() => usePaneStore.getState().reset());

  it('opens a runner tab scoped to the folder path', () => {
    render(
      <FolderNode
        folder={{ uid: 'f1', name: 'Auth', dirName: 'auth', items: [] }}
        basePath='auth'
        collectionName='demo'
        collectionRoot='/tmp/demo'
      />,
    );

    fireEvent.contextMenu(screen.getByText('Auth'));
    fireEvent.click(screen.getByText('Run folder'));

    const { root } = usePaneStore.getState();
    if (root.type !== 'leaf') throw new Error('Expected leaf root');
    const tab = root.tabs.find((t) => t.tabType === 'runner');
    expect(tab && 'folderPath' in tab ? tab.folderPath : undefined).toBe('auth');
  });
});
```

If `CollectionNode`/`FolderNode` require additional props beyond what's
shown above to render (check each component's full props interface
before writing the test — both take several props for git/contract
state that may need minimal stub values), add those stubs; the shape
above covers only what this task's new menu item needs to verify.

- [ ] **Step 2: Run test to verify it fails**

Run:
```bash
yarn vitest run src/components/collections/CollectionNode.test.tsx src/components/collections/FolderNode.test.tsx
```
Expected: FAIL — no "Run collection"/"Run folder" menu item yet.

- [ ] **Step 3: Wire `CollectionNode.tsx`**

Add `ListChecks` to the `lucide-react` import block at the top of
`src/components/collections/CollectionNode.tsx`, and
`const openRunnerTab = usePaneStore((s) => s.openRunnerTab);` next to
the existing `const openContractTab = usePaneStore((s) => s.openContractTab);`
line.

In `ContextMenuContent` (around line 478, right after the "Overview"
item and its `ContextMenuSeparator`), add:

```tsx
        <ContextMenuItem onClick={() => void openRunnerTab(summary.name)}>
          <ListChecks className='h-3.5 w-3.5 mr-2' /> Run collection
        </ContextMenuItem>
```

- [ ] **Step 4: Wire `FolderNode.tsx`**

Add `ListChecks` to the `lucide-react` import block at the top of
`src/components/collections/FolderNode.tsx`, and
`const openRunnerTab = usePaneStore((s) => s.openRunnerTab);` inside the
component, alongside its other `usePaneStore` selector calls.

In `ContextMenuContent` (around line 280, right after the "New Folder"
item), add:

```tsx
        <ContextMenuItem onClick={() => void openRunnerTab(collectionName, basePath)}>
          <ListChecks className='h-3.5 w-3.5 mr-2' /> Run folder
        </ContextMenuItem>
```

- [ ] **Step 5: Run test to verify it passes**

Run:
```bash
yarn vitest run src/components/collections/CollectionNode.test.tsx src/components/collections/FolderNode.test.tsx
```
Expected: PASS

- [ ] **Step 6: Run the full frontend test suite and type check**

Run: `yarn vitest run && yarn tsc --noEmit`
Expected: PASS, no new errors.

- [ ] **Step 7: Commit**

```bash
git add src/components/collections/CollectionNode.tsx src/components/collections/FolderNode.tsx src/components/collections/CollectionNode.test.tsx src/components/collections/FolderNode.test.tsx
git commit -m "feat: add Run collection / Run folder sidebar context menu actions"
```
