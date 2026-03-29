# Close All Tabs on Workspace Switch Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the user switches workspaces, auto-save any dirty tabs then close all open tabs so the editor is clean for the new workspace.

**Architecture:** Add `closeAll()` to `pane-store` — it walks the pane tree, calls `scheduleAutoSave` for each dirty request tab that has a source, then resets the pane tree to a single empty leaf. Wire the call into the `workspace-switched` event listener in `workspace-store`.

**Tech Stack:** TypeScript, Zustand, Vitest

---

### Task 1: Add `closeAll()` to pane-store

**Files:**
- Modify: `src/stores/pane-store.ts`
- Modify: `src/stores/__tests__/pane-store.test.ts`

- [ ] **Step 1: Add vi.mock and failing tests**

Open `src/stores/__tests__/pane-store.test.ts`.

Change the vitest import to include `vi`:
```ts
import { vi, describe, it, expect, beforeEach } from 'vitest';
```

Add this mock declaration immediately after all imports, before the helper functions:
```ts
vi.mock('@/lib/auto-save', () => ({
  scheduleAutoSave: vi.fn(),
}))
```

Add a top-level import for `scheduleAutoSave` after the other imports:
```ts
import { scheduleAutoSave } from '@/lib/auto-save';
```

At the bottom of the file, inside the `describe('pane-store', ...)` block, add:
```ts
  // ── closeAll ──────────────────────────────────────────────────────────────

  it('closeAll resets the pane tree to a single empty leaf', () => {
    usePaneStore.getState().openTab(makeTab());
    usePaneStore.getState().openTab(makeTab());
    usePaneStore.getState().closeAll();
    const leaf = getLeaf();
    expect(leaf.tabs).toHaveLength(0);
  });

  it('closeAll auto-saves only dirty request tabs that have a source', () => {
    const mockSave = vi.mocked(scheduleAutoSave);
    mockSave.mockClear();

    const dirtyWithSource: RequestTab = {
      ...makeTab(),
      isDirty: true,
      source: { collection: 'my-col', path: 'req1' },
    };
    const dirtyNoSource: RequestTab = {
      ...makeTab(),
      isDirty: true,
    };
    const cleanWithSource: RequestTab = {
      ...makeTab(),
      isDirty: false,
      source: { collection: 'my-col', path: 'req2' },
    };

    usePaneStore.getState().openTab(dirtyWithSource);
    usePaneStore.getState().openTab(dirtyNoSource);
    usePaneStore.getState().openTab(cleanWithSource);
    usePaneStore.getState().closeAll();

    expect(mockSave).toHaveBeenCalledTimes(1);
    expect(mockSave).toHaveBeenCalledWith(
      dirtyWithSource.id,
      'my-col',
      'req1',
      dirtyWithSource.title,
      dirtyWithSource.request,
    );
  });
```

- [ ] **Step 2: Run tests to verify they fail**

```bash
yarn test src/stores/__tests__/pane-store.test.ts
```

Expected: the two new `closeAll` tests FAIL with "usePaneStore.getState().closeAll is not a function". All existing tests still pass.

- [ ] **Step 3: Add `closeAll` to PaneState interface**

In `src/stores/pane-store.ts`, add `closeAll` to the `PaneState` interface directly after `reset`:

```ts
  // Utility.
  reset: () => void;
  closeAll: () => void;
```

- [ ] **Step 4: Implement `closeAll` in the store**

In the store body in `src/stores/pane-store.ts`, add the `closeAll` implementation directly after the `reset` action:

```ts
  reset() {
    set(buildInitialState());
  },

  closeAll() {
    const { root } = get();
    const flush = (node: PaneNode): void => {
      if (node.type === 'leaf') {
        for (const tab of node.tabs) {
          if (tab.isDirty && tab.source && isRequestTab(tab)) {
            scheduleAutoSave(
              tab.id,
              tab.source.collection,
              tab.source.path,
              tab.title,
              tab.request,
            );
          }
        }
      } else {
        flush(node.children[0]);
        flush(node.children[1]);
      }
    };
    flush(root);
    get().reset();
  },
```

- [ ] **Step 5: Run tests to verify they pass**

```bash
yarn test src/stores/__tests__/pane-store.test.ts
```

Expected: All tests PASS including the two new `closeAll` tests.

- [ ] **Step 6: Commit**

```bash
git add src/stores/pane-store.ts src/stores/__tests__/pane-store.test.ts
git commit -m "feat(pane-store): add closeAll to auto-save dirty tabs and reset pane tree"
```

---

### Task 2: Wire closeAll into workspace-switched listener

**Files:**
- Modify: `src/stores/workspace-store.ts`

- [ ] **Step 1: Update the workspace-switched listener**

In `src/stores/workspace-store.ts`, inside `subscribeToEvents()`, locate the `workspace-switched` listener:

```ts
  listen<Workspace>('workspace-switched', ({ payload }) => {
    useWorkspaceStore.setState({ activeWorkspaceId: payload.id })
  })
```

Replace it with:

```ts
  listen<Workspace>('workspace-switched', ({ payload }) => {
    usePaneStore.getState().closeAll()
    useWorkspaceStore.setState({ activeWorkspaceId: payload.id })
  })
```

`usePaneStore` is already imported at line 13 — no import change needed.

- [ ] **Step 2: TypeScript check**

```bash
yarn tsc --noEmit
```

Expected: No errors.

- [ ] **Step 3: Commit**

```bash
git add src/stores/workspace-store.ts
git commit -m "feat(workspace-store): close all tabs when switching workspace"
```
