# Collection Runner Plan C: Pane-Store Actions — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `usePaneStore` the actions that open a Runner tab and drive
a run: open, toggle inclusion, start, stop, and re-run — plus a pure
aggregate-summary selector.

**Architecture:** Every action follows the exact pattern already used by
`updateRequest`/`setResponse` in `src/stores/pane-store.ts`: look up the
target tab with `updateTabInTree(root, tabId, updater)`, patch it
immutably, `set({ root: newRoot })`. `openRunnerTab` mirrors
`openContractTab` (build the tab object, call `get().openTab(tab)`).
`startRun` is the only action with real async work — it loops over
included entries, calling `executeRunnerEntry` (Plan B) one at a time,
re-reading `get().root` before each iteration so a concurrent `stopRun`
call is observed between requests.

**Tech Stack:** TypeScript, Zustand, Vitest.

**Spec:** `docs/superpowers/specs/2026-09-17-collection-runner-frontend-design.md`

## Global Constraints

- No backend/Rust changes.
- The collection identifier is `collectionName` (see Plan A's Global
  Constraints for why — it's what `getCollection()` and
  `executeRunnerEntry()` actually take).
- New tests are appended to the existing
  `src/stores/__tests__/pane-store.test.ts` (a new `describe` block),
  not a new file — this codebase keeps every pane-store action's tests
  in that one file (`wc -l` confirms it's already 501 lines covering
  every other action). Every test in this plan calls
  `usePaneStore.getState().reset()` in its own `beforeEach` exactly like
  the existing tests there (line 50 of that file).

---

### Task 1: `openRunnerTab`

**Files:**
- Modify: `src/stores/pane-store.ts`
- Test: `src/stores/__tests__/pane-store.test.ts`

**Interfaces:**
- Consumes: `getCollection` (`@/lib/tauri-api`), `flattenRunnerEntries`
  (`@/lib/runner-flatten`, Plan A Task 2), `RunnerTab`/`isRunnerTab`
  (Plan A Task 1).
- Produces: `openRunnerTab(collectionName: string | null, folderPath?: string): Promise<void>`
  on `PaneState` — consumed by Plan E (sidebar context menu + blank-tab
  entry point).

- [ ] **Step 1: Write the failing test**

Add to `src/stores/__tests__/pane-store.test.ts` (new imports at the top
alongside the existing ones, new `vi.mock` alongside the existing
`vi.mock('@/lib/auto-save', ...)`, new `describe` block at the end of
the file):

```ts
// Added near the top, with the other imports:
import { isRunnerTab } from '@/types/pane-types';

// Added near the top, alongside the existing vi.mock('@/lib/auto-save', ...):
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn() };
});

// Added at the end of the file, as a new top-level describe block:
describe('Runner tab actions', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
  });

  it('openRunnerTab opens a runner tab scoped to a collection, populated with its requests', async () => {
    const { getCollection } = await import('@/lib/tauri-api');
    vi.mocked(getCollection).mockResolvedValue({
      name: 'demo',
      settings: { headers: [], variables: [] } as never,
      root: {
        uid: 'root',
        name: 'demo',
        items: [
          {
            type: 'request',
            uid: 'r1',
            name: 'Ping',
            method: 'GET',
            url: 'https://example.com/ping',
            headers: [],
            auth: { authType: 'none' },
            fileName: 'ping.yml',
          },
        ],
      },
    });

    await usePaneStore.getState().openRunnerTab('demo');

    const leaf = getLeaf();
    expect(leaf.tabs).toHaveLength(1);
    const tab = leaf.tabs[0];
    if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
    expect(tab.collectionName).toBe('demo');
    expect(tab.runState).toBe('idle');
    expect(tab.requests).toHaveLength(1);
    expect(tab.requests[0].requestPath).toBe('ping.yml');
  });

  it('openRunnerTab opens an empty picker tab when collectionName is null', async () => {
    await usePaneStore.getState().openRunnerTab(null);

    const leaf = getLeaf();
    const tab = leaf.tabs[0];
    if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
    expect(tab.collectionName).toBeNull();
    expect(tab.requests).toEqual([]);
  });
});
```

(`getLeaf()` is the existing helper already defined near the top of
`pane-store.test.ts` — reuse it, do not redefine it.)

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/stores/__tests__/pane-store.test.ts -t "Runner tab actions"`
Expected: FAIL — `openRunnerTab` is not a function on the store.

- [ ] **Step 3: Implement the action**

In `src/stores/pane-store.ts`, add to the imports:

```ts
import { flattenRunnerEntries } from '@/lib/runner-flatten';
import { getCollection } from '@/lib/tauri-api';
```

and add `RunnerRequestEntry`, `RunnerTab` to the existing `import type { ... } from '@/types/pane-types'` block. Do NOT import `isRunnerTab` in this task — `openRunnerTab`'s own code never calls it (it only constructs a `RunnerTab` object and calls `get().openTab(tab)`), so importing it here would be an unused import and fail both `tsc --noEmit` (`noUnusedLocals`) and `yarn check` (`noUnusedImports`). `isRunnerTab` is added by Task 2, which is the first task that actually uses it.

Add to the `PaneState` interface (near `openCollectionTab`):

```ts
openRunnerTab: (collectionName: string | null, folderPath?: string) => Promise<void>;
```

Add to the store implementation (near `openContractTab`):

```ts
async openRunnerTab(collectionName, folderPath) {
  let requests: RunnerRequestEntry[] = [];
  if (collectionName) {
    try {
      const collection = await getCollection(collectionName);
      requests = flattenRunnerEntries(collection, folderPath);
    } catch (err) {
      console.error('[pane-store] openRunnerTab: failed to load collection', err);
    }
  }
  const label = folderPath ? folderPath.split('/').pop() : collectionName;
  const tab: RunnerTab = {
    id: crypto.randomUUID(),
    title: label ? `Run: ${label}` : 'Runner',
    isDirty: false,
    tabType: 'runner',
    collectionName,
    folderPath,
    runState: 'idle',
    requests,
  };
  get().openTab(tab);
},
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn vitest run src/stores/__tests__/pane-store.test.ts -t "Runner tab actions"`
Expected: PASS (2 tests)

- [ ] **Step 5: Run the full frontend test suite and type check**

Run: `yarn vitest run && yarn tsc --noEmit`
Expected: PASS — the `vi.mock('@/lib/tauri-api', ...)` partial mock
(spreading `actual` and overriding only `getCollection`) must not break
any other test in this file that calls other `tauri-api` functions for
real; if it does, check that `vi.importActual` is spelled and awaited
exactly as above.

- [ ] **Step 6: Commit**

```bash
git add src/stores/pane-store.ts src/stores/__tests__/pane-store.test.ts
git commit -m "feat: add openRunnerTab pane-store action"
```

---

### Task 2: `toggleRunnerEntry`, `startRun`, `stopRun`, `rerunAll`

**Files:**
- Modify: `src/stores/pane-store.ts`
- Test: `src/stores/__tests__/pane-store.test.ts`

**Interfaces:**
- Consumes: `executeRunnerEntry` (`@/lib/runner-execute`, Plan B Task 2),
  `useEnvStore` (already imported in this file).
- Produces: `toggleRunnerEntry(tabId, requestPath): void`,
  `startRun(tabId): Promise<void>`, `stopRun(tabId): void`,
  `rerunAll(tabId): Promise<void>` on `PaneState` — consumed by Plan D's
  components (`RunnerRequestList` calls `toggleRunnerEntry`,
  `RunnerSummaryHeader` calls `startRun`/`stopRun`/`rerunAll`).

- [ ] **Step 1: Write the failing test**

Add to the same `describe('Runner tab actions', ...)` block from Task 1:

```ts
vi.mock('@/lib/runner-execute', () => ({
  executeRunnerEntry: vi.fn(),
}));

// ... inside describe('Runner tab actions', ...):

async function openTwoRequestRunnerTab(): Promise<string> {
  const { getCollection } = await import('@/lib/tauri-api');
  vi.mocked(getCollection).mockResolvedValue({
    name: 'demo',
    settings: { headers: [], variables: [] } as never,
    root: {
      uid: 'root',
      name: 'demo',
      items: [
        {
          type: 'request',
          uid: 'r1',
          name: 'First',
          method: 'GET',
          url: 'https://example.com/1',
          headers: [],
          auth: { authType: 'none' },
          fileName: 'first.yml',
        },
        {
          type: 'request',
          uid: 'r2',
          name: 'Second',
          method: 'GET',
          url: 'https://example.com/2',
          headers: [],
          auth: { authType: 'none' },
          fileName: 'second.yml',
        },
      ],
    },
  });
  await usePaneStore.getState().openRunnerTab('demo');
  const tab = getLeaf().tabs[0];
  if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
  return tab.id;
}

it('toggleRunnerEntry flips included for one entry', async () => {
  const tabId = await openTwoRequestRunnerTab();
  usePaneStore.getState().toggleRunnerEntry(tabId, 'first.yml');

  const tab = getLeaf().tabs[0];
  if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
  expect(tab.requests.find((e) => e.requestPath === 'first.yml')?.included).toBe(false);
  expect(tab.requests.find((e) => e.requestPath === 'second.yml')?.included).toBe(true);
});

it('startRun executes every included entry in order and marks the tab done', async () => {
  const { executeRunnerEntry } = await import('@/lib/runner-execute');
  vi.mocked(executeRunnerEntry).mockResolvedValue({ status: 'passed', result: undefined });

  const tabId = await openTwoRequestRunnerTab();
  await usePaneStore.getState().startRun(tabId);

  expect(executeRunnerEntry).toHaveBeenCalledTimes(2);
  const tab = getLeaf().tabs[0];
  if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
  expect(tab.runState).toBe('done');
  expect(tab.requests.every((e) => e.status === 'passed')).toBe(true);
});

it('startRun skips excluded entries', async () => {
  const { executeRunnerEntry } = await import('@/lib/runner-execute');
  vi.mocked(executeRunnerEntry).mockResolvedValue({ status: 'passed', result: undefined });

  const tabId = await openTwoRequestRunnerTab();
  usePaneStore.getState().toggleRunnerEntry(tabId, 'first.yml');
  await usePaneStore.getState().startRun(tabId);

  expect(executeRunnerEntry).toHaveBeenCalledTimes(1);
  expect(executeRunnerEntry).toHaveBeenCalledWith('demo', 'second.yml', expect.anything(), undefined);
});

it('stopRun halts the run and marks remaining entries skipped', async () => {
  const { executeRunnerEntry } = await import('@/lib/runner-execute');
  let resolveFirst: (() => void) | undefined;
  vi.mocked(executeRunnerEntry).mockImplementation(
    () =>
      new Promise((resolve) => {
        resolveFirst = () => resolve({ status: 'passed', result: undefined });
      }),
  );

  const tabId = await openTwoRequestRunnerTab();
  const runPromise = usePaneStore.getState().startRun(tabId);

  usePaneStore.getState().stopRun(tabId);
  resolveFirst?.();
  await runPromise;

  const tab = getLeaf().tabs[0];
  if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
  expect(tab.runState).toBe('stopped');
  expect(tab.requests.find((e) => e.requestPath === 'first.yml')?.status).toBe('passed');
  expect(tab.requests.find((e) => e.requestPath === 'second.yml')?.status).toBe('skipped');
  expect(executeRunnerEntry).toHaveBeenCalledTimes(1);
});

it('rerunAll resets every entry to pending and runs again', async () => {
  const { executeRunnerEntry } = await import('@/lib/runner-execute');
  vi.mocked(executeRunnerEntry).mockResolvedValue({ status: 'failed', error: 'boom' });

  const tabId = await openTwoRequestRunnerTab();
  await usePaneStore.getState().startRun(tabId);
  vi.mocked(executeRunnerEntry).mockClear();
  vi.mocked(executeRunnerEntry).mockResolvedValue({ status: 'passed', result: undefined });

  await usePaneStore.getState().rerunAll(tabId);

  expect(executeRunnerEntry).toHaveBeenCalledTimes(2);
  const tab = getLeaf().tabs[0];
  if (!isRunnerTab(tab)) throw new Error('Expected a runner tab');
  expect(tab.requests.every((e) => e.status === 'passed')).toBe(true);
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/stores/__tests__/pane-store.test.ts -t "Runner tab actions"`
Expected: FAIL — `toggleRunnerEntry`/`startRun`/`stopRun`/`rerunAll` are
not functions on the store.

- [ ] **Step 3: Implement the actions**

Add to the imports in `src/stores/pane-store.ts`:

```ts
import { executeRunnerEntry } from '@/lib/runner-execute';
```

Also add `isRunnerTab` to the existing `import { isRequestTab } from '@/types/pane-types'` line (making it `import { isRequestTab, isRunnerTab } from '@/types/pane-types';`) — this task is the first to actually call `isRunnerTab` in `pane-store.ts` (Task 1's `openRunnerTab` doesn't need it).

Add to the `PaneState` interface:

```ts
toggleRunnerEntry: (tabId: string, requestPath: string) => void;
startRun: (tabId: string) => Promise<void>;
stopRun: (tabId: string) => void;
rerunAll: (tabId: string) => Promise<void>;
```

Add to the store implementation, after `openRunnerTab`:

```ts
toggleRunnerEntry(tabId, requestPath) {
  set({
    root: updateTabInTree(get().root, tabId, (tab) => {
      if (!isRunnerTab(tab) || tab.runState === 'running') return tab;
      return {
        ...tab,
        requests: tab.requests.map((e) =>
          e.requestPath === requestPath ? { ...e, included: !e.included } : e,
        ),
      };
    }),
  });
},

async startRun(tabId) {
  const found = findTabInTree(get().root, tabId);
  if (!found || !isRunnerTab(found.tab) || !found.tab.collectionName) return;
  const collectionName = found.tab.collectionName;
  const entries = found.tab.requests;

  set({
    root: updateTabInTree(get().root, tabId, (tab) =>
      isRunnerTab(tab) ? { ...tab, runState: 'running' } : tab,
    ),
  });

  const environmentName = useEnvStore.getState().activeEnvId ?? undefined;

  for (const entry of entries) {
    const live = findTabInTree(get().root, tabId);
    if (!live || !isRunnerTab(live.tab) || live.tab.runState !== 'running') break;
    if (!entry.included) continue;

    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isRunnerTab(tab)) return tab;
        return {
          ...tab,
          requests: tab.requests.map((e) =>
            e.requestPath === entry.requestPath ? { ...e, status: 'running' } : e,
          ),
        };
      }),
    });

    const outcome = await executeRunnerEntry(
      collectionName,
      entry.requestPath,
      entry.request,
      environmentName,
    );

    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isRunnerTab(tab)) return tab;
        return {
          ...tab,
          requests: tab.requests.map((e) =>
            e.requestPath === entry.requestPath
              ? { ...e, status: outcome.status, result: outcome.result, error: outcome.error }
              : e,
          ),
        };
      }),
    });
  }

  set({
    root: updateTabInTree(get().root, tabId, (tab) => {
      if (!isRunnerTab(tab)) return tab;
      const requests = tab.requests.map((e) =>
        e.status === 'pending' ? { ...e, status: 'skipped' as const } : e,
      );
      return { ...tab, runState: tab.runState === 'stopped' ? 'stopped' : 'done', requests };
    }),
  });
},

stopRun(tabId) {
  set({
    root: updateTabInTree(get().root, tabId, (tab) =>
      isRunnerTab(tab) && tab.runState === 'running' ? { ...tab, runState: 'stopped' } : tab,
    ),
  });
},

async rerunAll(tabId) {
  set({
    root: updateTabInTree(get().root, tabId, (tab) => {
      if (!isRunnerTab(tab)) return tab;
      return {
        ...tab,
        runState: 'idle',
        requests: tab.requests.map((e) => ({
          ...e,
          status: 'pending' as const,
          result: undefined,
          error: undefined,
        })),
      };
    }),
  });
  await get().startRun(tabId);
},
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn vitest run src/stores/__tests__/pane-store.test.ts -t "Runner tab actions"`
Expected: PASS (7 tests total across Task 1 and Task 2)

- [ ] **Step 5: Run the full frontend test suite and type check**

Run: `yarn vitest run && yarn tsc --noEmit`
Expected: PASS, no new errors.

- [ ] **Step 6: Commit**

```bash
git add src/stores/pane-store.ts src/stores/__tests__/pane-store.test.ts
git commit -m "feat: add startRun/stopRun/rerunAll/toggleRunnerEntry pane-store actions"
```

---

### Task 3: `getRunnerSummary` selector

**Files:**
- Create: `src/lib/runner-summary.ts`
- Test: `src/lib/__tests__/runner-summary.test.ts`

**Interfaces:**
- Consumes: `RunnerTab` (Plan A Task 1).
- Produces: `getRunnerSummary(tab: RunnerTab): { included: number; total: number; passed: number; failed: number; skipped: number }`
  — consumed by Plan D Task 3 (`RunnerSummaryHeader`).

- [ ] **Step 1: Write the failing test**

```ts
// src/lib/__tests__/runner-summary.test.ts
import { describe, expect, it } from 'vitest';
import { getRunnerSummary } from '@/lib/runner-summary';
import type { RunnerRequestEntry, RunnerTab } from '@/types/pane-types';

function makeTab(requests: RunnerRequestEntry[]): RunnerTab {
  return {
    id: 't1',
    title: 'Runner',
    isDirty: false,
    tabType: 'runner',
    collectionName: 'demo',
    runState: 'done',
    requests,
  };
}

function entry(overrides: Partial<RunnerRequestEntry>): RunnerRequestEntry {
  return {
    requestPath: 'x.yml',
    request: { uid: 'x', name: 'X', method: 'GET', url: '', headers: [], auth: { authType: 'none' } },
    included: true,
    status: 'pending',
    ...overrides,
  };
}

describe('getRunnerSummary', () => {
  it('returns all zeros for an empty run', () => {
    expect(getRunnerSummary(makeTab([]))).toEqual({
      included: 0,
      total: 0,
      passed: 0,
      failed: 0,
      skipped: 0,
    });
  });

  it('counts total regardless of inclusion, but included only counts included entries', () => {
    const tab = makeTab([
      entry({ included: true, status: 'passed' }),
      entry({ included: false, status: 'pending' }),
    ]);
    expect(getRunnerSummary(tab)).toMatchObject({ total: 2, included: 1 });
  });

  it('counts passed, failed, and skipped independently', () => {
    const tab = makeTab([
      entry({ status: 'passed' }),
      entry({ status: 'passed' }),
      entry({ status: 'failed' }),
      entry({ status: 'skipped' }),
      entry({ status: 'running' }),
    ]);
    expect(getRunnerSummary(tab)).toMatchObject({ passed: 2, failed: 1, skipped: 1 });
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/lib/__tests__/runner-summary.test.ts`
Expected: FAIL — `src/lib/runner-summary.ts` does not exist.

- [ ] **Step 3: Implement the selector**

```ts
// src/lib/runner-summary.ts
import type { RunnerTab } from '@/types/pane-types';

export interface RunnerSummary {
  included: number;
  total: number;
  passed: number;
  failed: number;
  skipped: number;
}

// Pure aggregate derived from a RunnerTab's current requests — no
// separate summary state to keep in sync with the run.
export function getRunnerSummary(tab: RunnerTab): RunnerSummary {
  let included = 0;
  let passed = 0;
  let failed = 0;
  let skipped = 0;
  for (const e of tab.requests) {
    if (e.included) included += 1;
    if (e.status === 'passed') passed += 1;
    if (e.status === 'failed') failed += 1;
    if (e.status === 'skipped') skipped += 1;
  }
  return { included, total: tab.requests.length, passed, failed, skipped };
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn vitest run src/lib/__tests__/runner-summary.test.ts`
Expected: PASS (3 tests)

- [ ] **Step 5: Run the full frontend test suite and type check**

Run: `yarn vitest run && yarn tsc --noEmit`
Expected: PASS, no new errors.

- [ ] **Step 6: Commit**

```bash
git add src/lib/runner-summary.ts src/lib/__tests__/runner-summary.test.ts
git commit -m "feat: add getRunnerSummary aggregate selector"
```
