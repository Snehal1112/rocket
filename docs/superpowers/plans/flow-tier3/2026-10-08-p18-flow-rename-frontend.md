# Flow Rename and Delete Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Show flows in the collections sidebar with open, rename and delete actions, and keep open tabs and in-memory Auth tokens correct when a flow is renamed or deleted.

**Architecture:** A `useFlows` query becomes the single source of the flow list, used by the flow picker and by a new "Flows" group inside `CollectionNode`. A `FlowListItem` row (modelled on `ScriptNode`) opens, renames and deletes a flow. Delete reuses the sidebar's shared `AlertDialog` through a new `'flow'` `DeleteTarget`. A new pane-store action `renameFlowTabs` retargets open tabs to the new name (keeping their unsaved edits) and clears the old flow's Auth tokens. Renaming and deleting are blocked while the flow runs.

**Tech Stack:** React, TypeScript, TanStack Query, Zustand (`pane-store`, `flow-auth-store`), shadcn `DropdownMenu`, `AlertDialog`, `Input`, lucide-react, `sonner`, Vitest and Testing Library.

**Spec:** Roadmap item F-46 in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` section P18. Depends on plan P17, which provides `renameFlow(collection, oldName, newName)` in `src/lib/tauri-api.ts` (command `rename_flow`). `deleteFlow(collection, name)` already exists.

## Global Constraints

- shadcn/ui primitives only, `lucide-react` icons only, no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`, no inline SVG.
- Zustand: never fully destructure store state at component top level. Use narrow selectors. Inside event handlers, `usePaneStore.getState()` is fine.
- No Rust changes in this plan.
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format and are created through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check`, and the targeted `yarn test <path>` listed in the task.
- Work in the worktree or branch the human partner names. Only one implementer at a time touches `FlowPane.tsx` (plan P1 also edits it; locate code by the quoted text, not by line number).
- Not in scope: duplicating a flow, drag and drop of flows, folders for flows, rejecting `::` on the backend (until roadmap F-17 lands the check lives in `src/lib/flow-name.ts`), a "Save and rename" flow for dirty tabs (unsaved edits simply stay in the renamed tab).

## Review Focus

Failure modes the obvious tests would miss, most likely first:

1. A dirty open tab must follow the rename. If it keeps the old `flowName`, its next Save silently recreates the old flow next to the renamed one. The tab must keep its id, nodes, edges and `isDirty`, including a tab parked in a collection snapshot. Test pinned in Task 3.
2. Rename and delete must be refused while any tab of that flow has `runState === 'running'`, both at menu time and again at confirm time. Tests pinned in Task 2 (delete) and Task 3 (rename).
3. The flow list must never be stale: the picker, the sidebar group and the cache must refresh after create, rename and delete (the app sets `staleTime` to 30 seconds, so a missed invalidation is visible). Tests pinned in Tasks 1, 2 and 3.
4. In-memory Auth tokens must not outlive or follow a flow: rename clears the old name's tokens (and the new name's, defensively), a failed rename touches neither tabs nor tokens, delete clears tokens even when the only tab is parked in a snapshot. Tests pinned in Task 3. Names containing `::` are rejected at create (Task 1) and rename (Task 3), because `flowAuthKeyMatches` uses `startsWith`.
5. Case-only renames (`Login` to `login`) and renames onto an existing flow (including one that only shares the slug) must reach the backend unmodified and surface its Conflict message in a toast without retargeting tabs. Test pinned in Task 3. Opening a flow that already has a tab must focus that tab, not open a second copy that could diverge. Test pinned in Task 2.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/lib/queries/flow-queries.ts` (new) | `flowKeys` and `useFlows(collection, enabled)`. |
| `src/lib/flow-name.ts` (new) | `validateFlowName(raw)`: empty, `::`, invalid characters, no letter or digit. |
| `src/components/flow/FlowPicker.tsx` (new) | The collection and flow picker, moved out of `FlowPane` and using `useFlows`. |
| `src/components/flow/FlowPane.tsx` (modify) | Renders `FlowPicker` when no flow is chosen. |
| `src/lib/flow-tabs.ts` (new) | `findFlowTabs`, `isFlowRunning`, `hasDirtyFlow`: look in the pane tree and in collection snapshots. |
| `src/stores/pane-store.ts` (modify) | `openFlowTab` focuses an existing tab; `renameFlowTabs`; `dropParkedFlowTabs`. |
| `src/components/collections/FlowListItem.tsx` (new) | One flow row: open, rename, delete. |
| `src/components/collections/CollectionNode.tsx` (modify) | "Flows" group using `useFlows` and `FlowListItem`. |
| `src/components/collections/tree-utils.ts` (modify) | `DeleteTarget` gains `'flow'`; `findAffectedTabs` gains a flow branch. |
| `src/components/layout/CollectionsSidebar.tsx` (modify) | Flow branch in `confirmDelete`, dialog text, dirty warning, running guard, token and parked-tab cleanup. |

Existing tests to know: `src/components/flow/__tests__/FlowPane.test.tsx` (picker tests, `pickerTab`), `src/components/collections/__tests__/ScriptNode.test.tsx` (model for the row tests), `src/components/layout/__tests__/CollectionsSidebar.test.tsx` (model for the delete flow test), `src/components/collections/__tests__/tree-utils.test.ts`, `src/stores/__tests__/pane-store.flowAuth.test.ts` (flow tab fixture and token keys), `src/lib/queries/__tests__/flow-api.test.ts`.

Verified before writing: flows are not in the sidebar today. `CollectionNode.tsx` renders only folders, `scriptFile` items and requests; `CollectionItem` has no flow variant; `ScriptNode`/`RequestNode` are the only leaf rows. Flows appear only in the `FlowPane` picker (`flowName === null`).

---

### Task 1: Shared flow list query, name validation and the extracted picker

**Files:**
- Create: `src/lib/queries/flow-queries.ts`
- Create: `src/lib/queries/__tests__/flow-queries.test.tsx`
- Create: `src/lib/flow-name.ts`
- Create: `src/lib/__tests__/flow-name.test.ts`
- Create: `src/components/flow/FlowPicker.tsx`
- Modify: `src/components/flow/FlowPane.tsx` (imports at lines 2-35, picker state at lines 61-67, picker effects, `openFlow`, `handleCreate` and the `if (tab.flowName === null)` block)
- Modify: `src/components/flow/__tests__/FlowPane.test.tsx` (the `FlowPane picker` describe)

**Interfaces:**
- Produces: `flowKeys.all`, `flowKeys.collection(name)`, and `useFlows(collection: string | null, enabled?: boolean)` returning a TanStack query of `string[]`.
- Produces: `validateFlowName(raw: string): string | null`. Returns an error message, or null when valid. Trims first.
- Produces: `<FlowPicker tab={FlowTab} groupId={string} />`.

- [ ] **Step 1: Write the failing name-validation test**

Create `src/lib/__tests__/flow-name.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { validateFlowName } from '../flow-name';

describe('validateFlowName', () => {
  it('accepts an ordinary name', () => {
    expect(validateFlowName('Login then fetch')).toBeNull();
    expect(validateFlowName('  Sign In  ')).toBeNull();
  });

  it('rejects an empty name', () => {
    expect(validateFlowName('   ')).toBe('Enter a flow name.');
  });

  it('rejects "::" because Auth token keys are joined with it', () => {
    expect(validateFlowName('a::b')).toBe("A flow name cannot contain '::'.");
  });

  it('rejects the characters the sidebar rejects in names', () => {
    for (const bad of ['a/b', 'a\\b', 'a:b', 'a*b', 'a?b', 'a"b', 'a<b', 'a>b', 'a|b']) {
      expect(validateFlowName(bad)).not.toBeNull();
    }
  });

  it('rejects a name with no ASCII letter or digit, which the backend cannot store', () => {
    expect(validateFlowName('---')).toBe('A flow name needs at least one letter or digit.');
    expect(validateFlowName('ünï')).toBe('A flow name needs at least one letter or digit.');
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn test src/lib/__tests__/flow-name.test.ts`
Expected: FAIL, cannot resolve `../flow-name`.

- [ ] **Step 3: Write the validator**

Create `src/lib/flow-name.ts`:

```ts
// The characters the collections sidebar rejects in collection names.
const INVALID_CHARS = /[/\\:*?"<>|]/;

/**
 * Returns why `raw` is not a usable flow name, or null when it is fine.
 * "::" is rejected explicitly because Auth token keys join their parts with
 * it and `flowAuthKeyMatches` matches by prefix. The letter or digit rule
 * mirrors the backend, which derives the file name from ASCII letters and digits.
 */
export function validateFlowName(raw: string): string | null {
  const name = raw.trim();
  if (!name) return 'Enter a flow name.';
  if (name.includes('::')) return "A flow name cannot contain '::'.";
  if (INVALID_CHARS.test(name)) return 'A flow name cannot contain / \\ : * ? " < > |.';
  if (!/[A-Za-z0-9]/.test(name)) return 'A flow name needs at least one letter or digit.';
  return null;
}
```

- [ ] **Step 4: Run it to verify it passes**

Run: `yarn test src/lib/__tests__/flow-name.test.ts`
Expected: PASS (5 tests).

- [ ] **Step 5: Write the failing query test**

Create `src/lib/queries/__tests__/flow-queries.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listFlows } from '@/lib/tauri-api';
import { flowKeys, useFlows } from '../flow-queries';

vi.mock('@/lib/tauri-api', () => ({ listFlows: vi.fn() }));

function wrapperFor(client: QueryClient) {
  return ({ children }: { children: ReactNode }) => (
    <QueryClientProvider client={client}>{children}</QueryClientProvider>
  );
}

describe('useFlows', () => {
  beforeEach(() => {
    vi.mocked(listFlows).mockReset();
  });

  it('lists the flows of a collection under its own key', async () => {
    vi.mocked(listFlows).mockResolvedValue(['Login', 'Sync']);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { result } = renderHook(() => useFlows('demo'), { wrapper: wrapperFor(client) });
    await waitFor(() => expect(result.current.data).toEqual(['Login', 'Sync']));
    expect(listFlows).toHaveBeenCalledWith('demo');
    expect(client.getQueryData(flowKeys.collection('demo'))).toEqual(['Login', 'Sync']);
  });

  it('does not fetch without a collection or while disabled', async () => {
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    renderHook(() => useFlows(null), { wrapper: wrapperFor(client) });
    renderHook(() => useFlows('demo', false), { wrapper: wrapperFor(client) });
    await Promise.resolve();
    expect(listFlows).not.toHaveBeenCalled();
  });

  it('refetches after the collection key is invalidated', async () => {
    vi.mocked(listFlows).mockResolvedValueOnce(['Login']).mockResolvedValue(['Login', 'New']);
    const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
    const { result } = renderHook(() => useFlows('demo'), { wrapper: wrapperFor(client) });
    await waitFor(() => expect(result.current.data).toEqual(['Login']));
    await client.invalidateQueries({ queryKey: flowKeys.collection('demo') });
    await waitFor(() => expect(result.current.data).toEqual(['Login', 'New']));
  });
});
```

- [ ] **Step 6: Run it to verify it fails**

Run: `yarn test src/lib/queries/__tests__/flow-queries.test.tsx`
Expected: FAIL, cannot resolve `../flow-queries`.

- [ ] **Step 7: Write the query module**

Create `src/lib/queries/flow-queries.ts`:

```ts
import { useQuery } from '@tanstack/react-query';
import { listFlows } from '@/lib/tauri-api';

export const flowKeys = {
  all: ['flows'] as const,
  collection: (collectionName: string) => ['flows', collectionName] as const,
};

/**
 * The flow names of one collection. Invalidate `flowKeys.collection(name)` after
 * create, rename and delete, because the app caches queries for 30 seconds.
 * Pass `enabled = false` to hold the fetch until the list is on screen.
 */
export function useFlows(collectionName: string | null, enabled = true) {
  return useQuery({
    queryKey: flowKeys.collection(collectionName ?? ''),
    queryFn: () => listFlows(collectionName ?? ''),
    enabled: !!collectionName && enabled,
  });
}
```

- [ ] **Step 8: Run it to verify it passes**

Run: `yarn test src/lib/queries/__tests__/flow-queries.test.tsx`
Expected: PASS (3 tests).

- [ ] **Step 9: Write the failing picker tests**

In `src/components/flow/__tests__/FlowPane.test.tsx`:

1. Add these imports at the top (merge with the existing import lines, keep them sorted for Biome):

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { flowKeys } from '@/lib/queries/flow-queries';
```

2. Add this helper directly above `describe('FlowPane picker', ...)`:

```tsx
// The picker reads its flow list through TanStack Query, so it needs a provider.
function renderPicker() {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const view = render(
    <QueryClientProvider client={client}>
      <FlowPane tab={pickerTab('demo')} groupId='g1' />
    </QueryClientProvider>,
  );
  return { client, ...view };
}
```

3. Replace the three existing picker renders. Run this command from the repo root:

```bash
sed -i "s|render(<FlowPane tab={pickerTab('demo')} groupId='g1' />);|renderPicker();|" src/components/flow/__tests__/FlowPane.test.tsx
```

4. Add these tests at the end of `describe('FlowPane picker', ...)`, before its closing `});`:

```tsx

  it('shows a flow that appears after the flow list is invalidated', async () => {
    vi.mocked(listFlows).mockResolvedValueOnce([]).mockResolvedValue(['Alpha']);
    const { client } = renderPicker();
    expect(await screen.findByText('No flows yet')).toBeInTheDocument();

    await act(async () => {
      await client.invalidateQueries({ queryKey: flowKeys.collection('demo') });
    });

    expect(await screen.findByText('Select flow')).toBeInTheDocument();
  });

  it('invalidates the flow list after creating a flow', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    usePaneStore.setState({
      openFlowTab: vi.fn().mockResolvedValue(undefined),
      closeTab: vi.fn(),
    });
    const { client } = renderPicker();
    const spy = vi.spyOn(client, 'invalidateQueries');

    await userEvent.type(screen.getByLabelText('New flow name'), 'Login flow');
    await userEvent.click(screen.getByRole('button', { name: 'Create flow' }));

    await waitFor(() =>
      expect(spy).toHaveBeenCalledWith({ queryKey: flowKeys.collection('demo') }),
    );
  });

  it('rejects a name containing "::" without calling the backend', async () => {
    renderPicker();
    await userEvent.type(screen.getByLabelText('New flow name'), 'a::b');
    await userEvent.click(screen.getByRole('button', { name: 'Create flow' }));

    expect(await screen.findByRole('alert')).toHaveTextContent("cannot contain '::'");
    expect(saveFlow).not.toHaveBeenCalled();
  });
```

5. Add `act` to the Testing Library import at the top of the file: `import { act, render, screen, waitFor } from '@testing-library/react';`.

- [ ] **Step 10: Run them to verify they fail**

Run: `yarn test src/components/flow/__tests__/FlowPane.test.tsx -t "picker"`
Expected: FAIL. The new tests fail because the picker does not use the query yet (no `No flows yet` refresh, no `alert`, no invalidation).

- [ ] **Step 11: Create `FlowPicker`**

Create `src/components/flow/FlowPicker.tsx`:

```tsx
import { useQueryClient } from '@tanstack/react-query';
import { Plus } from 'lucide-react';
import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { validateFlowName } from '@/lib/flow-name';
import { flowKeys, useFlows } from '@/lib/queries/flow-queries';
import { type CollectionSummary, listCollections, saveFlow } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';

// Stable empty list, so the query default does not change identity on each render.
const NO_FLOWS: string[] = [];

/** Shown while a flow tab has no flow chosen yet: pick a collection and a flow, or create one. */
export function FlowPicker({ tab, groupId }: { tab: FlowTab; groupId: string }) {
  const openFlowTab = usePaneStore((s) => s.openFlowTab);
  const closeTab = usePaneStore((s) => s.closeTab);
  const queryClient = useQueryClient();
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  // Start from the tab's own collection, so a tab opened for a collection
  // (or one that fell back after a failed load) keeps that choice.
  const [selectedCollection, setSelectedCollection] = useState(tab.collectionName ?? '');
  const { data: flowNames = NO_FLOWS } = useFlows(selectedCollection || null);
  const [newFlowName, setNewFlowName] = useState('');
  const [createError, setCreateError] = useState('');
  const [isCreating, setIsCreating] = useState(false);

  useEffect(() => {
    void listCollections()
      .then(setCollections)
      .catch((err) => console.error('[FlowPicker] failed to list collections', err));
  }, []);

  const openFlow = (name: string) => {
    closeTab(tab.id, groupId);
    void openFlowTab(selectedCollection, name);
  };

  // Saves an empty flow first, so the new tab loads it like any existing one.
  const handleCreate = async () => {
    const name = newFlowName.trim();
    if (!selectedCollection || !name) return;
    if (flowNames.includes(name)) {
      openFlow(name);
      return;
    }
    const problem = validateFlowName(name);
    if (problem) {
      setCreateError(problem);
      return;
    }
    setIsCreating(true);
    try {
      await saveFlow(selectedCollection, { name, nodes: [], edges: [] });
      void queryClient.invalidateQueries({ queryKey: flowKeys.collection(selectedCollection) });
      openFlow(name);
    } catch (err) {
      toast.error(`Could not create flow: ${String(err)}`);
    } finally {
      setIsCreating(false);
    }
  };

  return (
    <div className='flex flex-col items-center justify-center h-full gap-3 p-6'>
      <p className='text-sm text-muted-foreground'>Choose a collection and a flow</p>
      <Select value={selectedCollection} onValueChange={setSelectedCollection}>
        <SelectTrigger className='w-64' aria-label='Collection'>
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
      <Select disabled={!selectedCollection || flowNames.length === 0} onValueChange={openFlow}>
        <SelectTrigger className='w-64' aria-label='Flow'>
          <SelectValue placeholder={flowNames.length === 0 ? 'No flows yet' : 'Select flow'} />
        </SelectTrigger>
        <SelectContent>
          {flowNames.map((name) => (
            <SelectItem key={name} value={name}>
              {name}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
      <div className='flex w-64 items-center gap-2'>
        <Input
          aria-label='New flow name'
          placeholder='New flow name'
          value={newFlowName}
          disabled={!selectedCollection || isCreating}
          onChange={(e) => {
            setNewFlowName(e.target.value);
            setCreateError('');
          }}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void handleCreate();
          }}
        />
        <Button
          size='sm'
          variant='outline'
          aria-label='Create flow'
          disabled={!selectedCollection || !newFlowName.trim() || isCreating}
          onClick={() => void handleCreate()}
        >
          <Plus className='h-3.5 w-3.5' />
        </Button>
      </div>
      {createError && (
        <p role='alert' className='w-64 text-xs text-destructive'>
          {createError}
        </p>
      )}
    </div>
  );
}
```

- [ ] **Step 12: Slim down `FlowPane`**

In `src/components/flow/FlowPane.tsx`:

1. Add the import (keep Biome's import order): `import { FlowPicker } from './FlowPicker';` next to the other `./` imports.
2. Delete these state declarations: `collections`/`setCollections`, `selectedCollection`/`setSelectedCollection` (with its two-line comment above), `flowNames`/`setFlowNames`, `newFlowName`/`setNewFlowName`, `isCreating`/`setIsCreating`.
3. Delete the two effects that call `listCollections()` (the one keyed on `[tab.flowName]`) and `listFlows(selectedCollection)` (the one keyed on `[selectedCollection]`, including its `cancelled` guard).
4. Delete `const openFlow = ...` and `const handleCreate = ...`.
5. Replace the whole `if (tab.flowName === null) { return ( ... ); }` block with:

```tsx
  if (tab.flowName === null) {
    return <FlowPicker tab={tab} groupId={groupId} />;
  }
```

6. Delete the selectors `const openFlowTab = ...` and `const closeTab = ...` at the top of the component.
7. Remove the imports that are now unused: `Plus`, the `Select`, `SelectContent`, `SelectItem`, `SelectTrigger`, `SelectValue` import block, `Input`, `type CollectionSummary`, `listCollections`, `listFlows`. Keep `Button`, `toast`, `saveFlow`, `useState`, `useEffect`.

Run `yarn tsc --noEmit` and `yarn check`; remove exactly the imports they flag as unused and nothing else.

- [ ] **Step 13: Run the tests to verify they pass**

Run: `yarn test src/components/flow/__tests__/FlowPane.test.tsx src/lib/queries/__tests__/flow-queries.test.tsx src/lib/__tests__/flow-name.test.ts`
Expected: PASS. The other `FlowPane.*.test.tsx` files render open flows (never the picker), so they need no provider; confirm with the next step.

- [ ] **Step 14: Gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/lib/queries src/lib/__tests__/flow-name.test.ts`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/queries/flow-queries.ts src/lib/queries/__tests__/flow-queries.test.tsx src/lib/flow-name.ts src/lib/__tests__/flow-name.test.ts src/components/flow/FlowPicker.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowPane.test.tsx`
Suggested subject: `feat(flow): share the flow list query and validate new flow names`.

---

### Task 2: Flows group in the sidebar, and delete

**Files:**
- Create: `src/lib/flow-tabs.ts`
- Create: `src/lib/__tests__/flow-tabs.test.ts`
- Create: `src/components/collections/FlowListItem.tsx`
- Create: `src/components/collections/__tests__/FlowListItem.test.tsx`
- Create: `src/components/collections/__tests__/CollectionNode.flows.test.tsx`
- Create: `src/stores/__tests__/pane-store-flow-open.test.ts`
- Modify: `src/stores/pane-store.ts` (`openFlowTab`, near line 875; imports at the top)
- Modify: `src/components/collections/CollectionNode.tsx` (imports, constants near line 50, hooks near line 108, render loop near line 640)
- Modify: `src/components/collections/tree-utils.ts` (`DeleteTarget` at line 12, `findAffectedTabs` at line 27)
- Modify: `src/components/collections/__tests__/tree-utils.test.ts`
- Modify: `src/components/layout/CollectionsSidebar.tsx` (imports, selectors near line 84, `confirmDelete` at line 87, dialog text near line 615)
- Modify: `src/components/layout/__tests__/CollectionsSidebar.test.tsx`

**Interfaces:**
- Consumes: `useFlows`, `flowKeys` (Task 1), `deleteFlow` (existing).
- Produces: `findFlowTabs(root, snapshots, collection, flowName): FlowTab[]`, `isFlowRunning(root, snapshots, collection, flowName): boolean`, `hasDirtyFlow(root, snapshots, collection, flowName): boolean` in `src/lib/flow-tabs.ts`. `snapshots` is the pane store's `collectionTabState`.
- Produces: `DeleteTarget` variant `{ type: 'flow'; collection: string; name: string }` (no `path`).
- Produces: `<FlowListItem name collectionName onDelete />`.

- [ ] **Step 1: Write the failing `flow-tabs` tests**

Create `src/lib/__tests__/flow-tabs.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { FlowTab, LeafNode } from '@/types/pane-types';
import { findFlowTabs, hasDirtyFlow, isFlowRunning } from '../flow-tabs';

const flowTab = (id: string, flowName: string, patch: Partial<FlowTab> = {}): FlowTab => ({
  id,
  title: `Flow: ${flowName}`,
  isDirty: false,
  tabType: 'flow',
  collectionName: 'col',
  flowName,
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
  ...patch,
});

const leaf = (tabs: FlowTab[]): LeafNode =>
  ({ type: 'leaf', groupId: 'g1', tabs, activeTabId: tabs[0]?.id ?? null }) as LeafNode;

describe('flow-tabs', () => {
  it('finds tabs of one flow in the tree and in snapshots, once each', () => {
    const shared = flowTab('t1', 'Login');
    const root = leaf([shared, flowTab('t2', 'Other')]);
    const snapshots = {
      other: { tabs: [shared, flowTab('t3', 'Login', { collectionName: 'col' })] },
    };
    expect(findFlowTabs(root, snapshots, 'col', 'Login').map((t) => t.id)).toEqual(['t1', 't3']);
  });

  it('matches the collection and the exact flow name', () => {
    const root = leaf([flowTab('t1', 'Login'), flowTab('t2', 'login'), flowTab('t3', 'Login', { collectionName: 'x' })]);
    expect(findFlowTabs(root, {}, 'col', 'Login').map((t) => t.id)).toEqual(['t1']);
  });

  it('reports a running tab, also one parked in a snapshot', () => {
    const root = leaf([flowTab('t1', 'Login')]);
    expect(isFlowRunning(root, {}, 'col', 'Login')).toBe(false);
    const parked = { c: { tabs: [flowTab('t2', 'Login', { runState: 'running' })] } };
    expect(isFlowRunning(root, parked, 'col', 'Login')).toBe(true);
  });

  it('reports a dirty tab', () => {
    const root = leaf([flowTab('t1', 'Login', { isDirty: true })]);
    expect(hasDirtyFlow(root, {}, 'col', 'Login')).toBe(true);
    expect(hasDirtyFlow(root, {}, 'col', 'Other')).toBe(false);
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn test src/lib/__tests__/flow-tabs.test.ts`
Expected: FAIL, cannot resolve `../flow-tabs`.

- [ ] **Step 3: Write the helper module**

Create `src/lib/flow-tabs.ts`:

```ts
import { collectAllTabs } from '@/lib/pane-utils';
import type { FlowTab, PaneNode, Tab } from '@/types/pane-types';
import { isFlowTab } from '@/types/pane-types';

// The shape of the pane store's `collectionTabState`: tabs parked when the user switched collection.
export type TabSnapshots = Record<string, { tabs: Tab[] }>;

/**
 * Every open tab of one flow, in the pane tree and in the collection snapshots.
 * A tab that appears in both is returned once.
 */
export function findFlowTabs(
  root: PaneNode,
  snapshots: TabSnapshots,
  collection: string,
  flowName: string,
): FlowTab[] {
  const seen = new Set<string>();
  const found: FlowTab[] = [];
  const candidates = [...collectAllTabs(root), ...Object.values(snapshots).flatMap((e) => e.tabs)];
  for (const tab of candidates) {
    if (!isFlowTab(tab) || seen.has(tab.id)) continue;
    if (tab.collectionName !== collection || tab.flowName !== flowName) continue;
    seen.add(tab.id);
    found.push(tab);
  }
  return found;
}

// True when any tab of the flow is running.
export function isFlowRunning(
  root: PaneNode,
  snapshots: TabSnapshots,
  collection: string,
  flowName: string,
): boolean {
  return findFlowTabs(root, snapshots, collection, flowName).some((t) => t.runState === 'running');
}

// True when any tab of the flow has unsaved edits.
export function hasDirtyFlow(
  root: PaneNode,
  snapshots: TabSnapshots,
  collection: string,
  flowName: string,
): boolean {
  return findFlowTabs(root, snapshots, collection, flowName).some((t) => t.isDirty);
}
```

- [ ] **Step 4: Run it to verify it passes**

Run: `yarn test src/lib/__tests__/flow-tabs.test.ts`
Expected: PASS (4 tests). If Biome later reformats the long `root` line in the second test, run `yarn format` on that file.

- [ ] **Step 5: Write the failing `openFlowTab` test**

Create `src/stores/__tests__/pane-store-flow-open.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { collectAllTabs } from '@/lib/pane-utils';
import { getFlow } from '@/lib/tauri-api';
import { isFlowTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getFlow: vi.fn(), endAgentSession: vi.fn() };
});

const flowTabs = () => collectAllTabs(usePaneStore.getState().root).filter(isFlowTab);

describe('openFlowTab', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(getFlow).mockReset();
    vi.mocked(getFlow).mockResolvedValue({ name: 'Login', nodes: [], edges: [] });
  });

  it('focuses the tab already open for the flow instead of opening a copy', async () => {
    await usePaneStore.getState().openFlowTab('col', 'Login');
    const firstId = flowTabs()[0]?.id;
    await usePaneStore.getState().openFlowTab('col', 'Login');
    expect(flowTabs()).toHaveLength(1);
    expect(flowTabs()[0]?.id).toBe(firstId);
    expect(getFlow).toHaveBeenCalledTimes(1);
  });

  it('opens separate tabs for different flows and for the same name in another collection', async () => {
    await usePaneStore.getState().openFlowTab('col', 'Login');
    await usePaneStore.getState().openFlowTab('col', 'Sync');
    await usePaneStore.getState().openFlowTab('other', 'Login');
    expect(flowTabs()).toHaveLength(3);
  });
});
```

- [ ] **Step 6: Run it to verify it fails**

Run: `yarn test src/stores/__tests__/pane-store-flow-open.test.ts`
Expected: FAIL on the first test (2 flow tabs, `getFlow` called twice).

- [ ] **Step 7: Make `openFlowTab` focus an existing tab**

In `src/stores/pane-store.ts`, add the import next to the other `@/lib/` imports:

```ts
import { findFlowTabs } from '@/lib/flow-tabs';
```

At the very start of `async openFlowTab(collectionName, flowName) {`, before `let nodes: FlowNode[] = [];`, insert:

```ts
    // An open tab only needs focusing. A second copy could diverge and later overwrite the first.
    if (collectionName && flowName) {
      const existing = findFlowTabs(get().root, {}, collectionName, flowName)[0];
      if (existing) {
        get().openTab(existing);
        return;
      }
    }
```

- [ ] **Step 8: Run it to verify it passes**

Run: `yarn test src/stores/__tests__/pane-store-flow-open.test.ts src/components/flow/__tests__/FlowPane.test.tsx`
Expected: PASS. The `FlowPane` save/reload test uses the real action with a fresh store, so it still opens a new tab.

- [ ] **Step 9: Write the failing `tree-utils` tests**

In `src/components/collections/__tests__/tree-utils.test.ts`, add `FlowTab` to the type import and append:

```ts
const flowTab = (id: string, flowName: string, collectionName = 'col'): FlowTab => ({
  id,
  title: `Flow: ${flowName}`,
  isDirty: false,
  tabType: 'flow',
  collectionName,
  flowName,
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
});

describe('tree-utils flow delete matching', () => {
  const target = { type: 'flow' as const, collection: 'col', name: 'Login' };

  it('matches the open tab of exactly that flow', () => {
    const root = {
      type: 'leaf',
      groupId: 'g1',
      tabs: [
        flowTab('f1', 'Login'),
        flowTab('f2', 'login'),
        flowTab('f3', 'Login', 'other'),
        scriptTab('lib/a.js'),
      ],
      activeTabId: 'f1',
    } as LeafNode;
    expect(findAffectedTabs(root, target).map((h) => h.tab.id)).toEqual(['f1']);
  });

  it('does not treat a flow target as a script or request target', () => {
    const root = {
      type: 'leaf',
      groupId: 'g1',
      tabs: [scriptTab('Login')],
      activeTabId: 's:Login',
    } as LeafNode;
    expect(findAffectedTabs(root, target)).toEqual([]);
  });
});
```

Change the first import line to `import type { FlowTab, LeafNode, ScriptTab } from '@/types/pane-types';`.

- [ ] **Step 10: Run them to verify they fail**

Run: `yarn test src/components/collections/__tests__/tree-utils.test.ts`
Expected: FAIL (TypeScript error on `type: 'flow'`, and the first test finds no tab).

- [ ] **Step 11: Extend `tree-utils.ts`**

In `src/components/collections/tree-utils.ts`:

1. Change the import `import { isFolderTab, isScriptTab } from '@/types/pane-types';` to `import { isFlowTab, isFolderTab, isScriptTab } from '@/types/pane-types';`.
2. Change `DeleteTarget`'s union to include `'flow'`:

```ts
export type DeleteTarget = {
  type: 'collection' | 'folder' | 'request' | 'script' | 'flow';
  collection: string;
  path?: string;
  name: string;
};
```

3. In `findAffectedTabs`, inside the `for (const tab of node.tabs) {` loop, directly before the `// Folder tabs have no source...` comment, insert:

```ts
      // Flow tabs have no source. They match by collection and flow name.
      if (isFlowTab(tab)) {
        if (
          target.type === 'flow' &&
          tab.collectionName === target.collection &&
          tab.flowName === target.name
        ) {
          found.push({ tab, groupId: node.groupId });
        }
        continue;
      }
```

- [ ] **Step 12: Run them to verify they pass**

Run: `yarn test src/components/collections/__tests__/tree-utils.test.ts`
Expected: PASS.

- [ ] **Step 13: Write the failing `FlowListItem` tests (open and delete)**

Create `src/components/collections/__tests__/FlowListItem.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { FlowListItem } from '@/components/collections/FlowListItem';
import { Tree } from '@/components/ui/tree';
import { collectAllTabs } from '@/lib/pane-utils';
import { getFlow } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getFlow: vi.fn(), endAgentSession: vi.fn() };
});
vi.mock('sonner', () => ({ toast: { error: vi.fn(), info: vi.fn(), success: vi.fn() } }));

const runningTab: FlowTab = {
  id: 'run-1',
  title: 'Flow: Login',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'col',
  flowName: 'Login',
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'running',
};

function renderItem(onDelete = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <Tree aria-label='tree'>
        <FlowListItem name='Login' collectionName='col' onDelete={onDelete} />
      </Tree>
    </QueryClientProvider>,
  );
  return onDelete;
}

describe('FlowListItem', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.mocked(getFlow).mockReset().mockResolvedValue({ name: 'Login', nodes: [], edges: [] });
    vi.mocked(toast.error).mockReset();
  });

  it('opens a flow tab on click', async () => {
    renderItem();
    fireEvent.click(screen.getByText('Login'));
    await waitFor(() => {
      const tabs = collectAllTabs(usePaneStore.getState().root).filter(isFlowTab);
      expect(tabs.map((t) => t.flowName)).toEqual(['Login']);
    });
  });

  it('asks the sidebar to delete with a flow target', async () => {
    const onDelete = renderItem();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));
    expect(onDelete).toHaveBeenCalledWith({ type: 'flow', collection: 'col', name: 'Login' });
  });

  it('refuses to delete a flow that is running', async () => {
    usePaneStore.getState().openTab(runningTab);
    const onDelete = renderItem();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));
    expect(onDelete).not.toHaveBeenCalled();
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Stop the run'));
  });
});
```

- [ ] **Step 14: Run it to verify it fails**

Run: `yarn test src/components/collections/__tests__/FlowListItem.test.tsx`
Expected: FAIL, cannot resolve `FlowListItem`.

- [ ] **Step 15: Write `FlowListItem` (open and delete)**

Create `src/components/collections/FlowListItem.tsx`:

```tsx
import { MoreHorizontal, Trash2, Workflow } from 'lucide-react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { TreeItem, TreeItemContent } from '@/components/ui/tree';
import { isFlowRunning } from '@/lib/flow-tabs';
import { usePaneStore } from '@/stores/pane-store';
import type { DeleteTarget } from './tree-utils';

interface FlowListItemProps {
  name: string;
  collectionName: string;
  onDelete: (target: DeleteTarget) => void;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

// True while any tab of this flow is running, also one parked after a collection switch.
function flowIsRunning(collectionName: string, name: string): boolean {
  const state = usePaneStore.getState();
  return isFlowRunning(state.root, state.collectionTabState, collectionName, name);
}

export function FlowListItem({ name, collectionName, onDelete }: FlowListItemProps) {
  const open = async () => {
    try {
      await usePaneStore.getState().openFlowTab(collectionName, name);
    } catch (err) {
      toast.error(`Could not open "${name}": ${errorMessage(err)}`);
    }
  };

  const requestDelete = () => {
    if (flowIsRunning(collectionName, name)) {
      toast.error(`Stop the run of "${name}" before deleting it.`);
      return;
    }
    onDelete({ type: 'flow', collection: collectionName, name });
  };

  return (
    <div className='group relative flex items-center'>
      <TreeItem value={`flow-${collectionName}-${name}`} className='w-full'>
        <TreeItemContent
          className='flex items-center gap-1 w-full px-2 py-1 text-sm rounded-sm cursor-pointer'
          onClick={() => void open()}
          aria-label={`Open flow ${name}`}
        >
          <Workflow aria-hidden='true' className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
          <span className='truncate text-foreground'>{name}</span>
        </TreeItemContent>
      </TreeItem>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            type='button'
            variant='ghost'
            size='icon'
            aria-label={`Actions for ${name}`}
            className='absolute right-1 h-5 w-5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100'
            onClick={(e) => e.stopPropagation()}
          >
            <MoreHorizontal aria-hidden='true' className='h-3 w-3' />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent className='w-48' onClick={(e) => e.stopPropagation()}>
          <DropdownMenuItem className='text-destructive' onClick={requestDelete}>
            <Trash2 aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
```

- [ ] **Step 16: Run it to verify it passes**

Run: `yarn test src/components/collections/__tests__/FlowListItem.test.tsx`
Expected: PASS (3 tests). If the click test cannot find `Login` twice, the tree root rendered the label once; that is expected.

- [ ] **Step 17: Write the failing sidebar group test**

Create `src/components/collections/__tests__/CollectionNode.flows.test.tsx`:

```tsx
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { CollectionNode } from '@/components/collections/CollectionNode';
import type { CollectionSummary } from '@/lib/tauri-api';
import * as tauriApi from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    getCollectionSummaries: vi.fn(),
    listFlows: vi.fn(),
    // biome-ignore lint/suspicious/noEmptyBlockStatements: unlisten stub.
    onCollectionChanged: vi.fn().mockResolvedValue(() => {}),
  };
});

const summary: CollectionSummary = {
  uid: 'col-1',
  repositoryId: 'repo-1',
  name: 'my-collection',
  path: '/workspace/collections/my-collection',
  requestCount: 0,
};

function renderNode(filter = '') {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return render(
    <QueryClientProvider client={queryClient}>
      <CollectionNode
        summary={summary}
        filter={filter}
        summaries={[summary]}
        onNewFolder={vi.fn()}
        onMove={vi.fn()}
        onDelete={vi.fn()}
        onDuplicate={vi.fn()}
      />
    </QueryClientProvider>,
  );
}

describe('CollectionNode flows group', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue({
      name: 'my-collection',
      root: { uid: 'root', name: 'my-collection', items: [] },
      settings: { headers: [], variables: [], sandboxMode: 'safe' },
    });
    vi.mocked(tauriApi.listFlows).mockReset().mockResolvedValue(['Login', 'Sync']);
    // Opens the node through the pane store's active collection, as the sibling test does.
    usePaneStore.setState({ activeCollection: summary.name });
  });

  it('lists the collection flows under a Flows heading once the node is open', async () => {
    renderNode();
    expect(await screen.findByLabelText('Open flow Login')).toBeInTheDocument();
    expect(screen.getByLabelText('Open flow Sync')).toBeInTheDocument();
    expect(screen.getByText('Flows')).toBeInTheDocument();
    expect(tauriApi.listFlows).toHaveBeenCalledWith('my-collection');
  });

  it('applies the sidebar filter to flow names', async () => {
    renderNode('syn');
    expect(await screen.findByLabelText('Open flow Sync')).toBeInTheDocument();
    expect(screen.queryByLabelText('Open flow Login')).not.toBeInTheDocument();
  });

  it('shows no Flows heading when the collection has no flows', async () => {
    vi.mocked(tauriApi.listFlows).mockResolvedValue([]);
    renderNode();
    await vi.waitFor(() => expect(tauriApi.listFlows).toHaveBeenCalled());
    expect(screen.queryByText('Flows')).not.toBeInTheDocument();
  });
});
```

- [ ] **Step 18: Run it to verify it fails**

Run: `yarn test src/components/collections/__tests__/CollectionNode.flows.test.tsx`
Expected: FAIL, `Unable to find a label 'Open flow Login'`.

- [ ] **Step 19: Add the Flows group to `CollectionNode`**

In `src/components/collections/CollectionNode.tsx`:

1. Add imports: `import { FlowListItem } from '@/components/collections/FlowListItem';` beside the `ScriptNode` import, and `import { useFlows } from '@/lib/queries/flow-queries';` beside the `useWorkspaces` import.
2. Add next to `EMPTY_IDS`:

```tsx
const EMPTY_NAMES: string[] = [];
```

3. Directly after `const [newScriptOpen, setNewScriptOpen] = useState(false);` add:

```tsx
  // Fetch the flow list only while the node is expanded.
  const { data: flowNames = EMPTY_NAMES } = useFlows(summary.name, open);
```

4. Directly after the `const filteredItems = sortItemsFoldersFirst(...)` statement add:

```tsx
  const filteredFlows = filter
    ? flowNames.filter((n) => n.toLowerCase().includes(filter.toLowerCase()))
    : flowNames;
```

5. In the JSX, directly after the closing `})}` of `{filteredItems.map((item) => { ... })}` and before `{creatingRequest && (`, insert:

```tsx
          {filteredFlows.length > 0 && (
            <div role='group' aria-label='Flows'>
              <div className='px-2 pt-2 pb-1 text-xs font-medium text-muted-foreground'>Flows</div>
              {filteredFlows.map((flowName) => (
                <FlowListItem
                  key={`flow-${flowName}`}
                  name={flowName}
                  collectionName={summary.name}
                  onDelete={onDelete}
                />
              ))}
            </div>
          )}
```

- [ ] **Step 20: Run it to verify it passes**

Run: `yarn test src/components/collections/__tests__/CollectionNode.flows.test.tsx src/components/collections/__tests__/CollectionNode.test.tsx`
Expected: PASS. The existing `CollectionNode.test.tsx` does not mock `listFlows`, so the real binding rejects inside the query; the query swallows it and the test is unaffected. If it logs noisy errors, add `listFlows: vi.fn().mockResolvedValue([])` to that file's `tauri-api` mock.

- [ ] **Step 21: Write the failing sidebar delete tests**

In `src/components/layout/__tests__/CollectionsSidebar.test.tsx`:

1. In the `vi.mock('@/lib/tauri-api', ...)` factory add `listFlows: vi.fn().mockResolvedValue([]),` and `deleteFlow: vi.fn(),`.
2. Add `vi.mock('sonner', () => ({ toast: { error: vi.fn(), info: vi.fn(), success: vi.fn() } }));` below the `HistoryPanel` mock, and `import { toast } from 'sonner';` to the imports.
3. Add `import { collectAllTabs } from '@/lib/pane-utils';` (extend the existing `pane-utils` import) and `import { type FlowTab, isFlowTab } from '@/types/pane-types';`.
4. Append this describe at the end of the file:

```tsx
describe('CollectionsSidebar flow delete', () => {
  const summary: tauriApi.CollectionSummary = {
    uid: 'c1',
    repositoryId: 'r1',
    name: 'col',
    path: '/w/col',
    requestCount: 0,
  };
  const loginTab = (patch: Partial<FlowTab> = {}): FlowTab => ({
    id: 'flow-login',
    title: 'Flow: Login',
    isDirty: false,
    tabType: 'flow',
    collectionName: 'col',
    flowName: 'Login',
    nodes: [],
    edges: [],
    nodeStatus: {},
    runState: 'idle',
    ...patch,
  });
  const flowTabs = () => collectAllTabs(usePaneStore.getState().root).filter(isFlowTab);

  beforeEach(() => {
    usePaneStore.getState().closeAll();
    vi.mocked(toast.error).mockReset();
    vi.mocked(tauriApi.listCollections).mockResolvedValue([summary]);
    vi.mocked(tauriApi.listWorkspaces).mockResolvedValue([
      { id: 'ws1', repositoryId: 'r1', name: 'WS', path: '/w', pinned: false },
    ]);
    vi.mocked(tauriApi.getCollectionSummaries).mockResolvedValue({
      name: 'col',
      root: { uid: 'root', name: 'col', items: [] },
      settings: { headers: [], variables: [], sandboxMode: 'safe' },
    });
    vi.mocked(tauriApi.listFlows).mockReset().mockResolvedValue(['Login']);
    vi.mocked(tauriApi.deleteFlow).mockReset().mockResolvedValue(undefined);
    useWorkspaceStore.setState({ activeWorkspaceId: 'ws1' });
    usePaneStore.setState({ activeCollection: 'col' });
  });

  it('warns about unsaved edits, deletes the flow, closes its tab and refreshes the list', async () => {
    usePaneStore.getState().openTab(loginTab({ isDirty: true }));
    render(<CollectionsSidebar />, { wrapper });

    await userEvent.click(await screen.findByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));
    expect(await screen.findByText(/Delete flow 'Login'/)).toBeInTheDocument();
    expect(screen.getByText(/unsaved changes that will be lost/)).toBeInTheDocument();

    vi.mocked(tauriApi.listFlows).mockResolvedValue([]);
    await userEvent.click(screen.getByRole('button', { name: 'Delete' }));

    await waitFor(() => expect(tauriApi.deleteFlow).toHaveBeenCalledWith('col', 'Login'));
    await waitFor(() => expect(flowTabs()).toHaveLength(0));
    await waitFor(() =>
      expect(screen.queryByRole('button', { name: 'Actions for Login' })).not.toBeInTheDocument(),
    );
  });

  it('blocks delete while the flow runs and leaves the dialog closed', async () => {
    usePaneStore.getState().openTab(loginTab({ runState: 'running' }));
    render(<CollectionsSidebar />, { wrapper });

    await userEvent.click(await screen.findByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));

    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Stop the run'));
    expect(screen.queryByText('Confirm Delete')).not.toBeInTheDocument();
    expect(tauriApi.deleteFlow).not.toHaveBeenCalled();
  });

  it('re-checks the run at confirm time, in case the run started while the dialog was open', async () => {
    usePaneStore.getState().openTab(loginTab());
    render(<CollectionsSidebar />, { wrapper });

    await userEvent.click(await screen.findByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));
    await screen.findByText('Confirm Delete');

    usePaneStore.getState().setFlowRunState('flow-login', 'running', 'r1');
    await userEvent.click(screen.getByRole('button', { name: 'Delete' }));

    await waitFor(() => expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Stop the run')));
    expect(tauriApi.deleteFlow).not.toHaveBeenCalled();
    expect(flowTabs()).toHaveLength(1);
  });
});
```

- [ ] **Step 22: Run them to verify they fail**

Run: `yarn test src/components/layout/__tests__/CollectionsSidebar.test.tsx -t "flow delete"`
Expected: FAIL (the dialog text is `Delete request 'Login'?`, and `deleteFlow` is never called).

- [ ] **Step 23: Add the flow branch to `CollectionsSidebar`**

In `src/components/layout/CollectionsSidebar.tsx`:

1. Imports: add `import { toast } from 'sonner';` after the `react` import; add `deleteFlow,` to the `@/lib/tauri-api` import list (alphabetical, after `deleteCollection`); add `import { hasDirtyFlow, isFlowRunning } from '@/lib/flow-tabs';` and `import { flowKeys } from '@/lib/queries/flow-queries';` beside the other `@/lib/` imports.
2. Directly after the `deleteHasDirtyScripts` selector add:

```tsx
  // The same warning for an open flow tab with unsaved edits.
  const deleteHasDirtyFlows = usePaneStore((s) =>
    deleteTarget?.type === 'flow'
      ? hasDirtyFlow(s.root, s.collectionTabState, deleteTarget.collection, deleteTarget.name)
      : false,
  );
```

3. In `confirmDelete`, change the `try {` block's first branch. Replace

```tsx
      if (deleteTarget.type === 'collection') {
        await deleteCollection(deleteTarget.collection);
      } else if (deleteTarget.type === 'folder') {
```

with

```tsx
      if (deleteTarget.type === 'flow') {
        // A run can start while the dialog is open, so check again here.
        const before = usePaneStore.getState();
        if (
          isFlowRunning(
            before.root,
            before.collectionTabState,
            deleteTarget.collection,
            deleteTarget.name,
          )
        ) {
          toast.error(`Stop the run of "${deleteTarget.name}" before deleting it.`);
          setDeleteTarget(null);
          return;
        }
        await deleteFlow(deleteTarget.collection, deleteTarget.name);
        void getQueryClient().invalidateQueries({
          queryKey: flowKeys.collection(deleteTarget.collection),
        });
      } else if (deleteTarget.type === 'collection') {
        await deleteCollection(deleteTarget.collection);
      } else if (deleteTarget.type === 'folder') {
```

4. In the `catch (err)` of `confirmDelete`, after `console.error('Delete failed:', err);` add:

```tsx
      if (deleteTarget.type === 'flow') toast.error(`Could not delete "${deleteTarget.name}": ${String(err)}`);
```

5. In the `AlertDialogDescription`, add a flow case. Replace

```tsx
                  : deleteTarget?.type === 'script'
                    ? `Delete script '${deleteTarget.name}'?`
                    : `Delete request '${deleteTarget?.name}'?`}
              {deleteHasDirtyScripts && ' An open script has unsaved changes that will be lost.'}
```

with

```tsx
                  : deleteTarget?.type === 'script'
                    ? `Delete script '${deleteTarget.name}'?`
                    : deleteTarget?.type === 'flow'
                      ? `Delete flow '${deleteTarget.name}'?`
                      : `Delete request '${deleteTarget?.name}'?`}
              {deleteHasDirtyScripts && ' An open script has unsaved changes that will be lost.'}
              {deleteHasDirtyFlows && ' An open flow has unsaved changes that will be lost.'}
```

The existing loop after the delete (`for (const { tab, groupId } of findAffectedTabs(...)) store.closeTab(...)`) already closes the live flow tabs through the new `findAffectedTabs` branch. `closeTab` also clears the flow's Auth tokens when its last tab closes.

- [ ] **Step 24: Run the tests to verify they pass**

Run: `yarn test src/components/layout/__tests__/CollectionsSidebar.test.tsx src/components/collections src/stores/__tests__/pane-store-flow-open.test.ts src/lib/__tests__/flow-tabs.test.ts`
Expected: PASS.

- [ ] **Step 25: Gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/collections src/components/layout src/components/flow src/stores src/lib/__tests__/flow-tabs.test.ts`
Expected: all pass. If Biome reports formatting on the long lines added in Steps 21 and 23, run `yarn format` and re-run `yarn check`.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/lib/flow-tabs.ts src/lib/__tests__/flow-tabs.test.ts src/stores/pane-store.ts src/stores/__tests__/pane-store-flow-open.test.ts src/components/collections/FlowListItem.tsx src/components/collections/CollectionNode.tsx src/components/collections/tree-utils.ts src/components/collections/__tests__/FlowListItem.test.tsx src/components/collections/__tests__/CollectionNode.flows.test.tsx src/components/collections/__tests__/tree-utils.test.ts src/components/layout/CollectionsSidebar.tsx src/components/layout/__tests__/CollectionsSidebar.test.tsx`
Suggested subject: `feat(flow): list flows in the sidebar and delete them from there`.

---

### Task 3: Rename, tab retargeting and Auth token lifetime

**Model:** review in the main loop (auth-token lifetime invariant).

**Files:**
- Modify: `src/stores/pane-store.ts` (`PaneState` type near line 249, actions near line 708)
- Create: `src/stores/__tests__/pane-store-flow-rename.test.ts`
- Modify: `src/components/collections/FlowListItem.tsx` (final version replaces Task 2's)
- Modify: `src/components/collections/__tests__/FlowListItem.test.tsx`
- Modify: `src/components/layout/CollectionsSidebar.tsx` (`confirmDelete`)
- Modify: `src/components/layout/__tests__/CollectionsSidebar.test.tsx`

**Interfaces:**
- Consumes: `renameFlow` from P17, `findFlowTabs`, `isFlowRunning`, `validateFlowName`, `flowKeys`, `useFlows`.
- Produces: `renameFlowTabs(collection: string, oldName: string, newName: string): void` and `dropParkedFlowTabs(collection: string, flowName: string): void` on the pane store.
- Invariant: after `renameFlowTabs`, no tab shows `oldName`, every former tab of `oldName` keeps its `id`, `nodes`, `edges`, `callbackHost`, `isDirty` and `nodeStatus`, and the Auth store holds no key of `oldName` or `newName`.

- [ ] **Step 1: Write the failing store tests**

Create `src/stores/__tests__/pane-store-flow-rename.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { flowAuthKey } from '@/lib/flow-auth';
import { collectAllTabs } from '@/lib/pane-utils';
import { type AuthState, type FlowTab, isFlowTab } from '@/types/pane-types';
import { useFlowAuthStore } from '../flow-auth-store';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, endAgentSession: vi.fn() };
});

const node = { id: 'n1', kind: { kind: 'Output' as const, label: 'Out' }, position: { x: 1, y: 2 } };

const flowTab = (id: string, flowName: string, patch: Partial<FlowTab> = {}): FlowTab => ({
  id,
  title: `Flow: ${flowName}`,
  isDirty: false,
  tabType: 'flow',
  collectionName: 'c',
  flowName,
  nodes: [],
  edges: [],
  callbackHost: null,
  nodeStatus: {},
  runState: 'idle',
  ...patch,
});

const auth = { authType: 'bearer' } as AuthState;
const key = (flow: string) => flowAuthKey('c', flow, 'a1', null, null);
const liveTab = (id: string) =>
  collectAllTabs(usePaneStore.getState().root).filter(isFlowTab).find((t) => t.id === id);

describe('renameFlowTabs', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    useFlowAuthStore.setState({ auths: {} });
  });

  it('retargets a dirty tab and keeps its id and unsaved edits', () => {
    usePaneStore
      .getState()
      .openTab(flowTab('t1', 'Login', { isDirty: true, nodes: [node], nodeStatus: { n1: 'success' } }));

    usePaneStore.getState().renameFlowTabs('c', 'Login', 'Sign In');

    const tab = liveTab('t1');
    expect(tab?.flowName).toBe('Sign In');
    expect(tab?.title).toBe('Flow: Sign In');
    expect(tab?.isDirty).toBe(true);
    expect(tab?.nodes).toEqual([node]);
    expect(tab?.nodeStatus).toEqual({ n1: 'success' });
  });

  it('leaves other flows and other collections alone', () => {
    usePaneStore.getState().openTab(flowTab('t1', 'Login'));
    usePaneStore.getState().openTab(flowTab('t2', 'login'));
    usePaneStore.getState().openTab(flowTab('t3', 'Login', { collectionName: 'other' }));

    usePaneStore.getState().renameFlowTabs('c', 'Login', 'Sign In');

    expect(liveTab('t1')?.flowName).toBe('Sign In');
    expect(liveTab('t2')?.flowName).toBe('login');
    expect(liveTab('t3')?.flowName).toBe('Login');
  });

  it('retargets a tab parked in a collection snapshot', () => {
    usePaneStore.setState({
      collectionTabState: {
        other: { tabs: [flowTab('p1', 'Login', { isDirty: true })], activeTabId: 'p1' },
      },
    });

    usePaneStore.getState().renameFlowTabs('c', 'Login', 'Sign In');

    const parked = usePaneStore.getState().collectionTabState.other?.tabs[0];
    expect(parked && isFlowTab(parked) ? parked.flowName : null).toBe('Sign In');
    expect(parked?.isDirty).toBe(true);
  });

  it('clears the old name tokens and the new name tokens, and keeps other flows', () => {
    useFlowAuthStore.setState({
      auths: {
        [key('Login')]: { auth },
        [key('Sign In')]: { auth },
        [key('Other')]: { auth },
      },
    });
    usePaneStore.getState().openTab(flowTab('t1', 'Login'));

    usePaneStore.getState().renameFlowTabs('c', 'Login', 'Sign In');

    expect(Object.keys(useFlowAuthStore.getState().auths)).toEqual([key('Other')]);
  });

  it('clears the tokens even when no tab of the flow is open', () => {
    useFlowAuthStore.setState({ auths: { [key('Login')]: { auth } } });
    usePaneStore.getState().renameFlowTabs('c', 'Login', 'Sign In');
    expect(useFlowAuthStore.getState().auths).toEqual({});
  });
});

describe('dropParkedFlowTabs', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
  });

  it('removes only the matching flow tabs from snapshots and fixes the active id', () => {
    usePaneStore.setState({
      collectionTabState: {
        other: {
          tabs: [flowTab('p1', 'Login'), flowTab('p2', 'Sync')],
          activeTabId: 'p1',
        },
      },
    });

    usePaneStore.getState().dropParkedFlowTabs('c', 'Login');

    const entry = usePaneStore.getState().collectionTabState.other;
    expect(entry?.tabs.map((t) => t.id)).toEqual(['p2']);
    expect(entry?.activeTabId).toBe('p2');
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/stores/__tests__/pane-store-flow-rename.test.ts`
Expected: FAIL, `renameFlowTabs is not a function`.

- [ ] **Step 3: Add the store actions**

In `src/stores/pane-store.ts`:

1. In the `PaneState` interface, directly after `renameScriptTabs: (...) => void;` add:

```ts
  /** Retargets open (and parked) tabs of a renamed flow and clears the old flow's Auth tokens. */
  renameFlowTabs: (collection: string, oldName: string, newName: string) => void;
  /** Removes tabs of a deleted flow that are parked in collection snapshots. */
  dropParkedFlowTabs: (collection: string, flowName: string) => void;
```

2. Directly after the `renameScriptTabs(...) { ... },` implementation add:

```ts
  renameFlowTabs(collection, oldName, newName) {
    // Matching by tab id keeps ids stable, so panes keep their active tab. The tab keeps its
    // nodes and dirty flag. Without the new name, its next Save would recreate the old flow.
    const tabs = findFlowTabs(get().root, get().collectionTabState, collection, oldName);
    let next = get();
    for (const found of tabs) {
      next = {
        ...next,
        ...updateTabEverywhere(next, found.id, (tab) =>
          isFlowTab(tab) ? { ...tab, flowName: newName, title: `Flow: ${newName}` } : tab,
        ),
      };
    }
    if (tabs.length > 0) set({ root: next.root, collectionTabState: next.collectionTabState });
    // Tokens are keyed by flow name and are not migrated. The user signs in again.
    useFlowAuthStore.getState().clearFlow(collection, oldName);
    useFlowAuthStore.getState().clearFlow(collection, newName);
  },

  dropParkedFlowTabs(collection, flowName) {
    const parked: CollectionTabState = {};
    for (const [key, entry] of Object.entries(get().collectionTabState)) {
      const tabs = entry.tabs.filter(
        (t) => !(isFlowTab(t) && t.collectionName === collection && t.flowName === flowName),
      );
      if (tabs.length === entry.tabs.length) {
        parked[key] = entry;
        continue;
      }
      const activeTabId = tabs.some((t) => t.id === entry.activeTabId)
        ? entry.activeTabId
        : (tabs[0]?.id ?? '');
      parked[key] = { ...entry, tabs, activeTabId };
    }
    set({ collectionTabState: parked });
  },
```

`useFlowAuthStore`, `isFlowTab` and `CollectionTabState` are already in scope in this file.

- [ ] **Step 4: Run them to verify they pass**

Run: `yarn test src/stores/__tests__/pane-store-flow-rename.test.ts src/stores/__tests__/pane-store.flowAuth.test.ts`
Expected: PASS.

- [ ] **Step 5: Write the failing `FlowListItem` rename tests**

In `src/components/collections/__tests__/FlowListItem.test.tsx`:

1. Change the `@/lib/tauri-api` mock to also stub `renameFlow`: `return { ...actual, getFlow: vi.fn(), renameFlow: vi.fn(), endAgentSession: vi.fn() };`, and extend the import to `import { getFlow, renameFlow } from '@/lib/tauri-api';`.
2. Add imports: `import { flowAuthKey } from '@/lib/flow-auth';`, `import { flowKeys } from '@/lib/queries/flow-queries';`, `import { useFlowAuthStore } from '@/stores/flow-auth-store';`, and `import type { AuthState } from '@/types/pane-types';` (merge into the existing `pane-types` import).
3. Replace `renderItem` with a version that returns the client, and reset more state in `beforeEach`:

```tsx
function renderItem(onDelete = vi.fn()) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  render(
    <QueryClientProvider client={client}>
      <Tree aria-label='tree'>
        <FlowListItem name='Login' collectionName='col' onDelete={onDelete} />
      </Tree>
    </QueryClientProvider>,
  );
  return { onDelete, client };
}
```

Update the two Task 2 tests that used `const onDelete = renderItem();` to `const { onDelete } = renderItem();`. In the `beforeEach` add `vi.mocked(renameFlow).mockReset(); vi.mocked(toast.info).mockReset(); useFlowAuthStore.setState({ auths: {} });`.

4. Append these tests inside `describe('FlowListItem', ...)`:

```tsx
  const idleTab = (patch: Partial<FlowTab> = {}): FlowTab => ({ ...runningTab, runState: 'idle', ...patch });
  const authKey = flowAuthKey('col', 'Login', 'a1', null, null);

  async function renameTo(value: string) {
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Rename'));
    const input = await screen.findByDisplayValue('Login');
    fireEvent.change(input, { target: { value } });
    fireEvent.keyDown(input, { key: 'Enter' });
  }

  it('renames the flow, retargets a dirty tab, clears tokens, tells the user and refreshes the list', async () => {
    vi.mocked(renameFlow).mockResolvedValue(undefined);
    usePaneStore.getState().openTab(idleTab({ isDirty: true }));
    useFlowAuthStore.setState({ auths: { [authKey]: { auth: { authType: 'bearer' } as AuthState } } });
    const { client } = renderItem();
    const spy = vi.spyOn(client, 'invalidateQueries');

    await renameTo('  Sign In  ');

    await waitFor(() => expect(renameFlow).toHaveBeenCalledWith('col', 'Login', 'Sign In'));
    await waitFor(() => {
      const tab = collectAllTabs(usePaneStore.getState().root).filter(isFlowTab)[0];
      expect(tab?.flowName).toBe('Sign In');
      expect(tab?.isDirty).toBe(true);
    });
    expect(useFlowAuthStore.getState().auths).toEqual({});
    expect(toast.info).toHaveBeenCalledWith(expect.stringContaining('Authenticate again'));
    expect(spy).toHaveBeenCalledWith({ queryKey: flowKeys.collection('col') });
  });

  it('does not mention tokens when the flow held none', async () => {
    vi.mocked(renameFlow).mockResolvedValue(undefined);
    renderItem();
    await renameTo('Sign In');
    await waitFor(() => expect(renameFlow).toHaveBeenCalled());
    expect(toast.info).not.toHaveBeenCalled();
  });

  it('passes a case-only rename to the backend unchanged', async () => {
    vi.mocked(renameFlow).mockResolvedValue(undefined);
    usePaneStore.getState().openTab(idleTab());
    renderItem();

    await renameTo('login');

    await waitFor(() => expect(renameFlow).toHaveBeenCalledWith('col', 'Login', 'login'));
    await waitFor(() => {
      const tab = collectAllTabs(usePaneStore.getState().root).filter(isFlowTab)[0];
      expect(tab?.flowName).toBe('login');
    });
  });

  it('rejects a name containing "::" without calling the backend', async () => {
    renderItem();
    await renameTo('a::b');
    await waitFor(() => expect(toast.error).toHaveBeenCalledWith("A flow name cannot contain '::'."));
    expect(renameFlow).not.toHaveBeenCalled();
  });

  it('does not call the backend for an unchanged name', async () => {
    renderItem();
    await renameTo('  Login ');
    expect(renameFlow).not.toHaveBeenCalled();
  });

  it('refuses to rename a running flow', async () => {
    usePaneStore.getState().openTab(runningTab);
    renderItem();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Rename'));
    expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Stop the run'));
    expect(screen.queryByDisplayValue('Login')).not.toBeInTheDocument();
    expect(renameFlow).not.toHaveBeenCalled();
  });

  it('refuses when a run starts while the name is being typed', async () => {
    usePaneStore.getState().openTab(idleTab());
    renderItem();
    await userEvent.click(screen.getByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Rename'));
    const input = await screen.findByDisplayValue('Login');

    usePaneStore.getState().setFlowRunState('run-1', 'running', 'r1');
    fireEvent.change(input, { target: { value: 'Sign In' } });
    fireEvent.keyDown(input, { key: 'Enter' });

    await waitFor(() => expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Stop the run')));
    expect(renameFlow).not.toHaveBeenCalled();
  });

  it('keeps the tab and the tokens, and shows the backend message, when the rename fails', async () => {
    vi.mocked(renameFlow).mockRejectedValue('Conflict: Flow name collides with an existing flow');
    usePaneStore.getState().openTab(idleTab());
    useFlowAuthStore.setState({ auths: { [authKey]: { auth: { authType: 'bearer' } as AuthState } } });
    renderItem();

    await renameTo('Taken');

    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Conflict: Flow name collides')),
    );
    const tab = collectAllTabs(usePaneStore.getState().root).filter(isFlowTab)[0];
    expect(tab?.flowName).toBe('Login');
    expect(Object.keys(useFlowAuthStore.getState().auths)).toEqual([authKey]);
  });
```

- [ ] **Step 6: Run them to verify they fail**

Run: `yarn test src/components/collections/__tests__/FlowListItem.test.tsx`
Expected: FAIL, there is no `Rename` menu item.

- [ ] **Step 7: Replace `FlowListItem` with the final version**

Replace the whole content of `src/components/collections/FlowListItem.tsx` with:

```tsx
import { useQueryClient } from '@tanstack/react-query';
import { MoreHorizontal, Pencil, Trash2, Workflow } from 'lucide-react';
import { useRef, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { TreeItem, TreeItemContent } from '@/components/ui/tree';
import { flowAuthKeyMatches } from '@/lib/flow-auth';
import { validateFlowName } from '@/lib/flow-name';
import { isFlowRunning } from '@/lib/flow-tabs';
import { flowKeys } from '@/lib/queries/flow-queries';
import { renameFlow } from '@/lib/tauri-api';
import { useFlowAuthStore } from '@/stores/flow-auth-store';
import { usePaneStore } from '@/stores/pane-store';
import type { DeleteTarget } from './tree-utils';

interface FlowListItemProps {
  name: string;
  collectionName: string;
  onDelete: (target: DeleteTarget) => void;
}

function errorMessage(err: unknown): string {
  return err instanceof Error ? err.message : String(err);
}

// True while any tab of this flow is running, also one parked after a collection switch.
function flowIsRunning(collectionName: string, name: string): boolean {
  const state = usePaneStore.getState();
  return isFlowRunning(state.root, state.collectionTabState, collectionName, name);
}

// True when the flow still holds in-memory Auth tokens.
function holdsAuthTokens(collectionName: string, name: string): boolean {
  return Object.keys(useFlowAuthStore.getState().auths).some((key) =>
    flowAuthKeyMatches(key, collectionName, name),
  );
}

export function FlowListItem({ name, collectionName, onDelete }: FlowListItemProps) {
  const queryClient = useQueryClient();
  const [isRenaming, setIsRenaming] = useState(false);
  const [renameValue, setRenameValue] = useState(name);
  const renameInFlight = useRef(false);
  // Set on Escape to block the blur that fires when the Input unmounts.
  const renameCancelled = useRef(false);

  const open = async () => {
    if (isRenaming) return;
    try {
      await usePaneStore.getState().openFlowTab(collectionName, name);
    } catch (err) {
      toast.error(`Could not open "${name}": ${errorMessage(err)}`);
    }
  };

  const stopRunMessage = (verb: string) => `Stop the run of "${name}" before ${verb} it.`;

  const requestDelete = () => {
    if (flowIsRunning(collectionName, name)) {
      toast.error(stopRunMessage('deleting'));
      return;
    }
    onDelete({ type: 'flow', collection: collectionName, name });
  };

  const startRename = () => {
    if (flowIsRunning(collectionName, name)) {
      toast.error(stopRunMessage('renaming'));
      return;
    }
    setRenameValue(name);
    // Wait for the menu to release focus, or the input blurs at once.
    setTimeout(() => setIsRenaming(true), 0);
  };

  const handleRename = async () => {
    if (renameInFlight.current) return;
    if (renameCancelled.current) {
      renameCancelled.current = false;
      return;
    }
    const trimmed = renameValue.trim();
    if (!trimmed || trimmed === name) {
      setIsRenaming(false);
      return;
    }
    const problem = validateFlowName(trimmed);
    if (problem) {
      toast.error(problem);
      setIsRenaming(false);
      return;
    }
    // A run can start while the name is typed, so check again before touching anything.
    if (flowIsRunning(collectionName, name)) {
      toast.error(stopRunMessage('renaming'));
      setIsRenaming(false);
      return;
    }
    renameInFlight.current = true;
    const hadTokens = holdsAuthTokens(collectionName, name);
    try {
      await renameFlow(collectionName, name, trimmed);
      // Only after the backend succeeded: open tabs follow the new name and the old tokens go.
      usePaneStore.getState().renameFlowTabs(collectionName, name, trimmed);
      void queryClient.invalidateQueries({ queryKey: flowKeys.collection(collectionName) });
      if (hadTokens) {
        toast.info(`Sign-in tokens for "${name}" were cleared. Authenticate again before the next run.`);
      }
    } catch (err) {
      toast.error(`Could not rename "${name}": ${errorMessage(err)}`);
    } finally {
      renameInFlight.current = false;
      setIsRenaming(false);
    }
  };

  return (
    <div className='group relative flex items-center'>
      <TreeItem value={`flow-${collectionName}-${name}`} className='w-full'>
        <TreeItemContent
          className='flex items-center gap-1 w-full px-2 py-1 text-sm rounded-sm cursor-pointer'
          onClick={() => void open()}
          aria-label={`Open flow ${name}`}
        >
          <Workflow aria-hidden='true' className='h-3.5 w-3.5 shrink-0 text-muted-foreground' />
          {isRenaming ? (
            <Input
              autoFocus
              className='h-6 text-sm flex-1'
              value={renameValue}
              onChange={(e) => setRenameValue(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void handleRename();
                if (e.key === 'Escape') {
                  renameCancelled.current = true;
                  setIsRenaming(false);
                }
              }}
              onBlur={() => void handleRename()}
              onClick={(e) => e.stopPropagation()}
            />
          ) : (
            <span className='truncate text-foreground'>{name}</span>
          )}
        </TreeItemContent>
      </TreeItem>
      <DropdownMenu>
        <DropdownMenuTrigger asChild>
          <Button
            type='button'
            variant='ghost'
            size='icon'
            aria-label={`Actions for ${name}`}
            className='absolute right-1 h-5 w-5 opacity-0 group-hover:opacity-100 group-focus-within:opacity-100'
            onClick={(e) => e.stopPropagation()}
          >
            <MoreHorizontal aria-hidden='true' className='h-3 w-3' />
          </Button>
        </DropdownMenuTrigger>
        <DropdownMenuContent
          className='w-48'
          onClick={(e) => e.stopPropagation()}
          // Keeps the rename input focused instead of returning focus to the trigger.
          onCloseAutoFocus={(e) => e.preventDefault()}
        >
          <DropdownMenuItem onSelect={startRename}>
            <Pencil aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Rename
          </DropdownMenuItem>
          <DropdownMenuItem className='text-destructive' onClick={requestDelete}>
            <Trash2 aria-hidden='true' className='h-3.5 w-3.5 mr-2' /> Delete
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>
    </div>
  );
}
```

- [ ] **Step 8: Run them to verify they pass**

Run: `yarn test src/components/collections/__tests__/FlowListItem.test.tsx`
Expected: PASS (11 tests). If the "unchanged name" test fails because the blur handler runs twice, check that `renameInFlight` and `renameCancelled` are refs, as in `ScriptNode`.

- [ ] **Step 9: Write the failing delete-cleanup tests**

In `src/components/layout/__tests__/CollectionsSidebar.test.tsx`, add these imports: `import { flowAuthKey } from '@/lib/flow-auth';`, `import { useFlowAuthStore } from '@/stores/flow-auth-store';`, and add `type AuthState` to the `pane-types` import. Append inside `describe('CollectionsSidebar flow delete', ...)`:

```tsx
  it('clears the flow tokens and the parked tabs of a deleted flow', async () => {
    const key = flowAuthKey('col', 'Login', 'a1', null, null);
    const otherKey = flowAuthKey('col', 'Sync', 'a1', null, null);
    const auth = { authType: 'bearer' } as AuthState;
    useFlowAuthStore.setState({ auths: { [key]: { auth }, [otherKey]: { auth } } });
    // The only tab of the flow is parked in a snapshot, so closeTab never sees it.
    usePaneStore.setState({
      collectionTabState: { other: { tabs: [loginTab({ id: 'parked' })], activeTabId: 'parked' } },
    });
    render(<CollectionsSidebar />, { wrapper });

    await userEvent.click(await screen.findByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));
    await userEvent.click(await screen.findByRole('button', { name: 'Delete' }));

    await waitFor(() => expect(tauriApi.deleteFlow).toHaveBeenCalledWith('col', 'Login'));
    await waitFor(() => expect(Object.keys(useFlowAuthStore.getState().auths)).toEqual([otherKey]));
    expect(usePaneStore.getState().collectionTabState.other?.tabs).toEqual([]);
  });

  it('keeps tabs and tokens when the delete fails', async () => {
    vi.mocked(tauriApi.deleteFlow).mockRejectedValue('Io error: disk');
    const key = flowAuthKey('col', 'Login', 'a1', null, null);
    useFlowAuthStore.setState({ auths: { [key]: { auth: { authType: 'bearer' } as AuthState } } });
    usePaneStore.getState().openTab(loginTab());
    render(<CollectionsSidebar />, { wrapper });

    await userEvent.click(await screen.findByRole('button', { name: 'Actions for Login' }));
    await userEvent.click(await screen.findByText('Delete'));
    await userEvent.click(await screen.findByRole('button', { name: 'Delete' }));

    await waitFor(() => expect(toast.error).toHaveBeenCalledWith(expect.stringContaining('Could not delete')));
    expect(flowTabs()).toHaveLength(1);
    expect(Object.keys(useFlowAuthStore.getState().auths)).toEqual([key]);
  });
```

Also add `useFlowAuthStore.setState({ auths: {} });` to that describe's `beforeEach`.

- [ ] **Step 10: Run them to verify the first fails**

Run: `yarn test src/components/layout/__tests__/CollectionsSidebar.test.tsx -t "flow delete"`
Expected: the "clears the flow tokens and the parked tabs" test FAILS (tokens and parked tab remain). The failure test already passes, which is the point: it pins that cleanup happens only after a successful delete.

- [ ] **Step 11: Clean up after a flow delete**

In `src/components/layout/CollectionsSidebar.tsx`:

1. Add `import { useFlowAuthStore } from '@/stores/flow-auth-store';` beside the other store imports.
2. In `confirmDelete`, directly after the existing close-tabs loop (`for (const { tab, groupId } of findAffectedTabs(store.root, deleteTarget)) { store.closeTab(tab.id, groupId); }`) add:

```tsx
      if (deleteTarget.type === 'flow') {
        // Tabs parked in a collection snapshot were not closed above, and no open tab
        // may be left to clear the tokens, so drop both explicitly.
        usePaneStore.getState().dropParkedFlowTabs(deleteTarget.collection, deleteTarget.name);
        useFlowAuthStore.getState().clearFlow(deleteTarget.collection, deleteTarget.name);
      }
```

This runs only after `deleteFlow` succeeded, because a failed delete throws before this line.

- [ ] **Step 12: Run the tests to verify they pass**

Run: `yarn test src/components/layout/__tests__/CollectionsSidebar.test.tsx src/components/collections src/stores src/components/flow`
Expected: PASS.

- [ ] **Step 13: Gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/collections src/components/layout src/components/flow src/stores src/lib`
Expected: all pass. Run `yarn format` if Biome flags the long lines in the new tests, then re-run `yarn check`.

Manual check in the real app (`yarn tauri dev`), since jsdom cannot show the sidebar layout: create a flow in the picker and see it appear in the sidebar without a reload; rename it with a dirty tab open and press Save (only one flow file must exist); rename `Login` to `login`; try `a::b`; start a run and confirm Rename and Delete refuse; delete a flow with a dirty tab and confirm the warning.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/stores/pane-store.ts src/stores/__tests__/pane-store-flow-rename.test.ts src/components/collections/FlowListItem.tsx src/components/collections/__tests__/FlowListItem.test.tsx src/components/layout/CollectionsSidebar.tsx src/components/layout/__tests__/CollectionsSidebar.test.tsx`
Suggested subject: `feat(flow): rename flows from the sidebar and clear tokens on rename and delete`.

---

## Self-Review

- **Spec coverage:** F-46 frontend. Flows group and open (Task 2), delete with dirty warning and run block (Task 2), shared list query and picker refresh (Task 1), `::` rejection at create (Task 1) and rename (Task 3), rename with run block, tab retargeting and token clearing (Task 3), delete cleanup of tokens and parked tabs (Task 3). Backend `rename_flow` is plan P17.
- **Placeholders:** none. Every code step shows code; edits to existing files quote the exact anchors.
- **Type consistency:** `renameFlow(collection, oldName, newName)` matches P17. `DeleteTarget` flow variant is `{ type: 'flow'; collection; name }` in `tree-utils`, `FlowListItem` and `CollectionsSidebar`. `findFlowTabs`, `isFlowRunning`, `hasDirtyFlow` take `(root, snapshots, collection, flowName)` everywhere, with `usePaneStore.getState().collectionTabState` as `snapshots`. `flowKeys.collection(name)` is the only invalidation key. `renameFlowTabs(collection, oldName, newName)` and `dropParkedFlowTabs(collection, flowName)` match between the interface, the store and the callers.
- **Review Focus coverage:** item 1 is the dirty and parked tab tests in `pane-store-flow-rename.test.ts` and the dirty rename test in `FlowListItem.test.tsx`; item 2 is the delete menu and confirm tests (Task 2) and the two rename run tests (Task 3); item 3 is the picker invalidation tests (Task 1), the delete refresh (Task 2) and the rename invalidation spy (Task 3); item 4 is the token tests in the store file, `FlowListItem.test.tsx` and the sidebar cleanup tests, plus the `::` tests in Tasks 1 and 3; item 5 is the case-only test, the failure test, and the `openFlowTab` focus tests.

Known follow-ups outside this plan: a run started in the instant between the run check and `renameFlow` resolving would run under the old name and fail with not found (the window is one IPC call); deleting a collection does not close its flow tabs (existing behavior, flow tabs have no `source`); closing a flow tab does not warn when other collections are switched (shared limitation noted in plan P1).
