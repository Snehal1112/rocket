# Idle-Memory Follow-Up Fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fix two follow-up items surfaced (but explicitly deferred) during the idle-memory investigation earlier in this session: duplicate mount-time IPC calls, and the Monaco TypeScript worker never releasing its ~50-60MB once opened.

**Architecture:** Two independent work streams, each touching a different slice of the app — no task in one group depends on a task in another group:
- **Group A (frontend only):** dedupe three IPC calls that fire twice on mount.
- **Group C (frontend only):** a ref-counted lifecycle helper that tears down Monaco's shared TypeScript/JavaScript worker once no JS/TS editor is mounted anywhere in the app.

> **Scope note:** this plan originally had a third stream, **Group B** — adding a `get_request` Tauri command and switching the collection sidebar tree to `get_collection_summaries` — but that exact work is already implemented and reviewed clean in a sibling session: worktree `.claude/worktrees/sidebar-collection-summaries` (branch `worktree-sidebar-collection-summaries`, plan `docs/superpowers/plans/2026-09-26-sidebar-collection-summaries.md`, commits `fc249c3..0775105`). Only its own Task 5 (final regression verification) remains there. Group B is dropped from this plan entirely to avoid duplicating that work; Group C's tasks are renumbered below as Tasks 4-6.

**Tech Stack:** React 19 + TypeScript, Zustand, TanStack Query, Vitest, `monaco-editor` 0.55.1.

**Spec:** None — this plan is grounded directly in decomposing-investigations passes run against current source (main, commit `6614164`, 2026-09-26) earlier in this session. Each task's "Grounding" note below cites the file:line evidence gathered for it.

## Global Constraints

- Zustand: never fully destructure store state at a component's top level — keep narrow `useStore((s) => s.field)` selectors, matching the existing files.
- Commits: conventional commits format (`feat:`, `fix:`, `refactor:`, `test:`).
- Verification gates for every task: `yarn tsc --noEmit`, `yarn check` (Biome), plus the task's own test command.

## Review Focus

- The `list_contracts` in-flight dedup cache must not permanently poison future loads after one backend error — a rejected promise must clear from the cache so the next call retries instead of replaying the old rejection — covered in Task 1's error-path test.
- `useCollections()` replacing two independently-debounced local fetchers must not regress into un-debounced refetch storms on rapid filesystem-watcher events — Task 3 gives both `CollectionsSidebar` and `WorkspaceOverviewTab` their own 300ms debounce around the shared `invalidateQueries` call (Step 6), a test proving the shared cache itself works (Step 7), and a manual burst-event check (Step 10).
- React 18 `StrictMode`'s intentional dev-mode double-invoke of mount effects (mount → cleanup → mount) must not desynchronize the Monaco worker ref-count into tearing the worker down while an editor is still visibly mounted — covered in Task 4's balanced-double-invoke test.

---

## Group A — Dedupe mount-time IPC calls

### Task 1: Dedupe `list_contracts` via an in-flight request cache

**Grounding:** `CollectionNode.tsx:135-139` calls `loadContracts(collectionRoot)` (old `useContractStore`) and `newLoadContracts(collectionRoot)` (new `useContractsStore`) back-to-back in one effect — both genuinely live, both feed real UI (`ContractBadge`/`RequestNode`/`FolderNode` read the old store; `ContractsTab`/`NewContractModal`/`ContractDiffPane`/`ContractsStatusItem`/`CollectionNode`'s own drift badge read the new store), so neither store can simply be deleted. Both route through `listContracts(collectionRoot)` at `src/lib/tauri-api.ts:1473-1474`. Fix: dedupe at the IPC-call level so both stores populate from one network round-trip.

**Files:**
- Modify: `src/lib/tauri-api.ts:1473-1474`
- Test: `src/lib/__tests__/tauri-api.test.ts`

**Interfaces:**
- Consumes: nothing new — `invoke` from `@tauri-apps/api/core` (already imported in `tauri-api.ts`).
- Produces: `listContracts(collectionRoot: string): Promise<Contract[]>` — same signature and behavior as today, callers (`contract-store.ts`, `contractsActions.ts`) need no changes.

- [ ] **Step 1: Write the failing test**

```typescript
// src/lib/__tests__/tauri-api.test.ts — add to the file (new describe block)
import { invoke } from '@tauri-apps/api/core';
vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));

describe('listContracts in-flight dedup', () => {
  beforeEach(() => {
    vi.mocked(invoke).mockReset();
  });

  it('coalesces two concurrent calls for the same collectionRoot into one invoke', async () => {
    vi.mocked(invoke).mockResolvedValue([{ id: 'c1' }]);
    const { listContracts } = await import('../tauri-api');

    const [a, b] = await Promise.all([
      listContracts('/ws/collections/my-api'),
      listContracts('/ws/collections/my-api'),
    ]);

    expect(invoke).toHaveBeenCalledTimes(1);
    expect(a).toEqual(b);
  });

  it('issues a fresh invoke for a different collectionRoot', async () => {
    vi.mocked(invoke).mockResolvedValue([]);
    const { listContracts } = await import('../tauri-api');

    await Promise.all([listContracts('/ws/collections/a'), listContracts('/ws/collections/b')]);

    expect(invoke).toHaveBeenCalledTimes(2);
  });

  it('clears the cached promise after a rejection, so the next call retries', async () => {
    vi.mocked(invoke).mockRejectedValueOnce(new Error('boom'));
    const { listContracts } = await import('../tauri-api');

    await expect(listContracts('/ws/collections/my-api')).rejects.toThrow('boom');

    vi.mocked(invoke).mockResolvedValueOnce([{ id: 'c1' }]);
    const result = await listContracts('/ws/collections/my-api');

    expect(invoke).toHaveBeenCalledTimes(2);
    expect(result).toEqual([{ id: 'c1' }]);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn test src/lib/__tests__/tauri-api.test.ts -t "listContracts in-flight dedup"`
Expected: FAIL — `invoke` called twice in the first test (no dedup yet).

- [ ] **Step 3: Write minimal implementation**

```typescript
// src/lib/tauri-api.ts — replace the existing listContracts export (currently lines 1473-1474)
const inFlightContractsRequests = new Map<string, Promise<Contract[]>>();

export const listContracts = (collectionRoot: string): Promise<Contract[]> => {
  const cached = inFlightContractsRequests.get(collectionRoot);
  if (cached) return cached;

  const request = invoke<Contract[]>('list_contracts', { collectionRoot }).finally(() => {
    inFlightContractsRequests.delete(collectionRoot);
  });
  inFlightContractsRequests.set(collectionRoot, request);
  return request;
};
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn test src/lib/__tests__/tauri-api.test.ts -t "listContracts in-flight dedup"`
Expected: PASS (all 3 cases)

- [ ] **Step 5: Run full tauri-api test file to check for regressions**

Run: `yarn test src/lib/__tests__/tauri-api.test.ts`
Expected: PASS — no other `listContracts` callers assert call counts, so this is additive.

- [ ] **Step 6: Commit**

```bash
git add src/lib/tauri-api.ts src/lib/__tests__/tauri-api.test.ts
git commit -m "fix(contracts): dedupe concurrent list_contracts calls for the same collection"
```

---

### Task 2: Remove the redundant `list_history` mount-time effect

**Grounding:** `HistoryPanel.tsx:79-81` (`useEffect(() => { void fetchEntries('', 'All', 'All'); }, [fetchEntries])`) and `HistoryPanel.tsx:85-88` (`useEffect(() => { void fetchEntries(urlQuery, method, statusLabel); }, [fetchEntries, method, statusLabel])`) both fire on mount with identical effective arguments (`urlQuery=''`, `method='All'`, `statusLabel='All'` at initial state) — not different trigger conditions, a straightforward duplicate. The second effect already covers mount, method-change, and status-change.

**Files:**
- Modify: `src/components/history/HistoryPanel.tsx:78-88`
- Test: `src/components/history/__tests__/HistoryPanel.test.tsx` (create — none exists today)

**Interfaces:**
- Consumes: `listHistory`, `searchHistory` from `@/lib/tauri-api` (unchanged).
- Produces: no exported interface change — `HistoryPanel` remains a default-export-free named component with no props.

- [ ] **Step 1: Write the failing test**

```typescript
// src/components/history/__tests__/HistoryPanel.test.tsx (new file)
import { render, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { HistoryPanel } from '../HistoryPanel';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, listHistory: vi.fn(), searchHistory: vi.fn() };
});

describe('HistoryPanel mount', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listHistory).mockResolvedValue([]);
  });

  it('calls listHistory exactly once on mount, not twice', async () => {
    render(<HistoryPanel />);

    await waitFor(() => {
      expect(tauriApi.listHistory).toHaveBeenCalledTimes(1);
    });
    expect(tauriApi.listHistory).toHaveBeenCalledWith(200);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn test src/components/history/__tests__/HistoryPanel.test.tsx`
Expected: FAIL — `listHistory` called twice.

- [ ] **Step 3: Write minimal implementation**

```typescript
// src/components/history/HistoryPanel.tsx
// Remove the effect at (current) lines 78-81:
//   // Load recent history on mount.
//   useEffect(() => {
//     void fetchEntries('', 'All', 'All');
//   }, [fetchEntries]);
//
// Keep the remaining effect (current lines 83-88) and fold the removed
// comment's intent into its own comment:

  // Load on mount, and re-fetch immediately when method or status changes.
  // (urlQuery is excluded — debounced separately in handleUrlChange below.)
  // biome-ignore lint/correctness/useExhaustiveDependencies: urlQuery excluded intentionally, debounced in handleUrlChange
  useEffect(() => {
    void fetchEntries(urlQuery, method, statusLabel);
  }, [fetchEntries, method, statusLabel]);
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn test src/components/history/__tests__/HistoryPanel.test.tsx`
Expected: PASS

- [ ] **Step 5: Commit**

```bash
git add src/components/history/HistoryPanel.tsx src/components/history/__tests__/HistoryPanel.test.tsx
git commit -m "fix(history): remove duplicate list_history call on mount"
```

---

### Task 3: Share `list_collections` between `CollectionsSidebar` and `WorkspaceOverviewTab` via TanStack Query

**Grounding:** `CollectionsSidebar.tsx:75-82` (`fetchCollections`) and `WorkspaceOverviewTab.tsx:49-53` (`refresh`) each keep an independent local `useState<CollectionSummary[]>` populated by their own `listCollections()` call, fired on mount (`CollectionsSidebar.tsx:251-252`, `WorkspaceOverviewTab.tsx:58-59`) and confirmed to co-occur in the default app-start path (`App.tsx:38-95`, `WorkspaceOverviewTab` renders via `EditorGroup.tsx` for the default workspace tab; `CollectionsSidebar` is always mounted at `App.tsx:175`). The project already has this exact pattern solved for workspaces (`src/lib/queries/workspace-queries.ts`) and environments (`environment-queries.ts`) — collections just never got migrated onto it.

**Files:**
- Create: `src/lib/queries/collection-queries.ts`
- Modify: `src/components/layout/CollectionsSidebar.tsx` (replace `fetchCollections`/local `summaries` state)
- Modify: `src/components/workspace/WorkspaceOverviewTab.tsx` (replace `refresh`/local `summaries` state)
- Test: `src/lib/queries/__tests__/collection-queries.test.ts` (create)

**Interfaces:**
- Consumes: `listCollections` from `@/lib/tauri-api` (unchanged), `useQuery`/`useQueryClient` from `@tanstack/react-query`, `getQueryClient` from `@/lib/query-client` (same pattern as `workspace-queries.ts`).
- Produces:
  - `collectionKeys.all: readonly ['collections']`
  - `useCollections(): UseQueryResult<CollectionSummary[]>` — `data` defaults to `undefined` until loaded; callers must fall back to `[]` the same way `useWorkspaces()` callers already do (`const { data: workspaces = [] } = useWorkspaces()`).

- [ ] **Step 1: Write the failing test**

```typescript
// src/lib/queries/__tests__/collection-queries.test.ts (new file)
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook, waitFor } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return { ...actual, listCollections: vi.fn() };
});

function wrapper({ children }: { children: ReactNode }) {
  const queryClient = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  return <QueryClientProvider client={queryClient}>{children}</QueryClientProvider>;
}

describe('useCollections', () => {
  beforeEach(() => {
    vi.mocked(tauriApi.listCollections).mockResolvedValue([
      { uid: 'c1', repositoryId: 'r1', name: 'my-api', path: '/ws/collections/my-api', requestCount: 3 },
    ]);
  });

  it('fetches collections once and serves a second mount from cache', async () => {
    const { useCollections } = await import('../collection-queries');
    const { result: first } = renderHook(() => useCollections(), { wrapper });
    await waitFor(() => expect(first.current.data).toHaveLength(1));

    const { result: second } = renderHook(() => useCollections(), { wrapper });
    await waitFor(() => expect(second.current.data).toHaveLength(1));

    expect(tauriApi.listCollections).toHaveBeenCalledTimes(1);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn test src/lib/queries/__tests__/collection-queries.test.ts`
Expected: FAIL — `../collection-queries` doesn't exist yet.

- [ ] **Step 3: Write minimal implementation**

```typescript
// src/lib/queries/collection-queries.ts (new file)
import { useQuery } from '@tanstack/react-query';
import { listCollections } from '@/lib/tauri-api';

export const collectionKeys = {
  all: ['collections'] as const,
};

export function useCollections() {
  return useQuery({
    queryKey: collectionKeys.all,
    queryFn: listCollections,
  });
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn test src/lib/queries/__tests__/collection-queries.test.ts`
Expected: PASS

- [ ] **Step 5: Wire `CollectionsSidebar` onto `useCollections()`**

```typescript
// src/components/layout/CollectionsSidebar.tsx
// Add import:
import { collectionKeys, useCollections } from '@/lib/queries/collection-queries';

// Inside CollectionsSidebar(), replace:
//   const [summaries, setSummaries] = useState<CollectionSummary[]>([]);
//   ...
//   const fetchCollections = useCallback(async () => {
//     try {
//       const results = await listCollections();
//       setSummaries(results);
//     } catch (err) {
//       console.error('[CollectionsSidebar] list error', err);
//     }
//   }, []);
// with:
  const { data: summaries = [] } = useCollections();

// Inside the mount effect (current lines 251-252), replace
//   void fetchCollections();
// with:
  void getQueryClient().invalidateQueries({ queryKey: collectionKeys.all });

// Inside the onCollectionChanged debounce handler (current line 261), replace
//   listDebounce.current = setTimeout(() => void fetchCollections(), 300);
// with:
  listDebounce.current = setTimeout(
    () => void getQueryClient().invalidateQueries({ queryKey: collectionKeys.all }),
    300,
  );
```

Remove the now-unused `listCollections` import and `fetchCollections` callback; `getQueryClient` and `CollectionSummary` type imports already exist in this file (`getQueryClient` is already imported at line 25 for the environment-keys invalidation a few lines below).

- [ ] **Step 6: Wire `WorkspaceOverviewTab` onto `useCollections()`**

```typescript
// src/components/workspace/WorkspaceOverviewTab.tsx
// Add import:
import { collectionKeys, useCollections } from '@/lib/queries/collection-queries';
import { getQueryClient } from '@/lib/query-client';

// Replace:
//   const [summaries, setSummaries] = useState<CollectionSummary[]>([]);
//   ...
//   const refresh = useCallback(async () => {
//     const cols = await listCollections();
//     setSummaries(cols);
//   }, []);
// with:
  const { data: summaries = [] } = useCollections();

// Add a debounce ref alongside the component's other refs (mirroring
// CollectionsSidebar's own listDebounce pattern — without this, this
// listener would invalidate un-debounced while CollectionsSidebar's sibling
// listener is still debounced, defeating the point of debouncing at all):
  const listDebounce = useRef<ReturnType<typeof setTimeout> | null>(null);

// Replace the mount effect's body (current lines 58-64):
//   refresh().catch(console.error);
//   let cancelled = false;
//   const unlistenPromise = onCollectionChanged(() => {
//     if (!cancelled) refresh().catch(console.error);
//   });
// with:
  let cancelled = false;
  const unlistenPromise = onCollectionChanged(() => {
    if (cancelled) return;
    if (listDebounce.current) clearTimeout(listDebounce.current);
    listDebounce.current = setTimeout(() => {
      void getQueryClient().invalidateQueries({ queryKey: collectionKeys.all });
    }, 300);
  });
```

Also clear `listDebounce.current` in this effect's cleanup function (alongside the existing `cancelled = true` line), the same way `CollectionsSidebar` clears its own debounce ref on cleanup. Remove the now-unused `listCollections` import and `refresh` callback.

- [ ] **Step 7: Write a test proving rapid events collapse into one invalidate**

```typescript
// src/lib/queries/__tests__/collection-queries.test.ts — add to the existing describe block
import { act } from '@testing-library/react';

it('useCollections is a shared cache — invalidating once refetches for every mounted consumer', async () => {
  const { useCollections, collectionKeys } = await import('../collection-queries');
  const { getQueryClient } = await import('@/lib/query-client');
  const { result } = renderHook(() => useCollections(), { wrapper });
  await waitFor(() => expect(result.current.data).toHaveLength(1));

  vi.mocked(tauriApi.listCollections).mockResolvedValue([
    { uid: 'c1', repositoryId: 'r1', name: 'my-api', path: '/ws/collections/my-api', requestCount: 4 },
  ]);
  await act(async () => {
    await getQueryClient().invalidateQueries({ queryKey: collectionKeys.all });
  });

  expect(result.current.data?.[0].requestCount).toBe(4);
});
```

This confirms the shared-cache half of the fix (one invalidate reaches both `CollectionsSidebar` and `WorkspaceOverviewTab` since they now read the same query key). The 300ms debounce timers added above in `CollectionsSidebar` and `WorkspaceOverviewTab` are plain `setTimeout` wrapping — covered by manual verification in Step 9, since faking timers around two separate components' effects in one integration test would need a full `App`-level render this task doesn't otherwise require.

- [ ] **Step 8: Run TypeScript check**

Run: `yarn tsc --noEmit`
Expected: no errors — `summaries` in both files is now `CollectionSummary[]` sourced from the query instead of local state; no other code in either file reads the removed `fetchCollections`/`refresh` functions (both call sites were the mount effect and the debounce handler, both updated above).

- [ ] **Step 9: Run the full frontend test suite for regressions**

Run: `yarn test src/components/layout src/components/workspace src/lib/queries`
Expected: PASS

- [ ] **Step 10: Manually verify the debounce still holds**

Run: `yarn tauri dev`, open a workspace with at least one collection, and rapidly create/delete a couple of throwaway requests via the filesystem (or the app's own "new request" button several times in quick succession) to fire several `collection-changed` events within the same second. Confirm in the Network/console that `list_collections` fires once shortly after the burst settles, not once per event, for both the sidebar and the workspace overview tab.

- [ ] **Step 11: Commit**

```bash
git add src/lib/queries/collection-queries.ts src/lib/queries/__tests__/collection-queries.test.ts src/components/layout/CollectionsSidebar.tsx src/components/workspace/WorkspaceOverviewTab.tsx
git commit -m "refactor(collections): share list_collections between sidebar and overview via TanStack Query"
```

---

## Group C — Release the Monaco JS/TS worker when no script editor is mounted

**Grounding:** `WorkerManager._stopWorker()` (`node_modules/monaco-editor/esm/vs/language/typescript/workerManager.js:93-99`) is the only teardown path and is private — unreachable from application code directly. It's wired to fire on `javascriptDefaults.onDidChange` (`workerManager.js:82`), and all four public setters (`setCompilerOptions`, `setDiagnosticsOptions`, `setWorkerOptions`, `setInlayHintsOptions`) fire that event (`monaco.contribution.js:224-242`) — confirmed public, documented API (`monaco.d.ts:9613-9633`, including a `readonly workerOptions`/`getCompilerOptions()` getter pair). `MonacoWrapper.tsx:111-116`'s unmount cleanup disposes only content-change/hover listeners today, never the shared worker. Three sites can mount a JS/TS-language Monaco model: `ScriptsTab.tsx` (3 `MonacoWrapper` instances) and `ResponseBodyViewer.tsx` (1 `MonacoWrapper` instance) both go through `MonacoWrapper.tsx`'s `resolvedLanguage` (line 74); `DiffViewer.tsx` uses a raw `<DiffEditor>` with its own `language` variable (line 104) and needs its own wiring.

### Task 4: Create the ref-counted worker-lifecycle helper

**Files:**
- Create: `src/components/editor/monaco-js-worker-lifecycle.ts`
- Test: `src/components/editor/__tests__/monaco-js-worker-lifecycle.test.ts`

**Interfaces:**
- Consumes: `monaco-editor`'s `typescript.javascriptDefaults` namespace (mocked in the test; real in production via the existing `import * as monacoNs from 'monaco-editor'` pattern already used in `MonacoWrapper.tsx`).
- Produces: `acquireJsWorker(): void`; `releaseJsWorker(): void` — both side-effecting, no return value, safe to call from a `useEffect`/cleanup pair.

- [ ] **Step 1: Write the failing test**

```typescript
// src/components/editor/__tests__/monaco-js-worker-lifecycle.test.ts (new file)
import { beforeEach, describe, expect, it, vi } from 'vitest';

const setCompilerOptions = vi.fn();
const getCompilerOptions = vi.fn(() => ({ target: 99 }));

vi.mock('monaco-editor', () => ({
  typescript: {
    javascriptDefaults: { setCompilerOptions, getCompilerOptions },
  },
}));

describe('monaco-js-worker-lifecycle', () => {
  beforeEach(() => {
    setCompilerOptions.mockClear();
  });

  it('does not tear down the worker while another reference is still held', async () => {
    const { acquireJsWorker, releaseJsWorker } = await import('../monaco-js-worker-lifecycle');
    acquireJsWorker();
    acquireJsWorker();
    releaseJsWorker();

    expect(setCompilerOptions).not.toHaveBeenCalled();

    releaseJsWorker(); // balance back to 0
  });

  it('tears down the worker once the last reference releases', async () => {
    const { acquireJsWorker, releaseJsWorker } = await import('../monaco-js-worker-lifecycle');
    acquireJsWorker();
    releaseJsWorker();

    expect(setCompilerOptions).toHaveBeenCalledTimes(1);
    expect(setCompilerOptions).toHaveBeenCalledWith({ target: 99 });
  });

  it('is a no-op to release when the count is already at zero', async () => {
    const { releaseJsWorker } = await import('../monaco-js-worker-lifecycle');
    releaseJsWorker();

    expect(setCompilerOptions).not.toHaveBeenCalled();
  });

  it('survives a balanced double-acquire/double-release (React StrictMode dev double-invoke)', async () => {
    const { acquireJsWorker, releaseJsWorker } = await import('../monaco-js-worker-lifecycle');
    acquireJsWorker();
    releaseJsWorker();
    acquireJsWorker();
    releaseJsWorker();

    expect(setCompilerOptions).toHaveBeenCalledTimes(2);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn test src/components/editor/__tests__/monaco-js-worker-lifecycle.test.ts`
Expected: FAIL — module doesn't exist.

- [ ] **Step 3: Write minimal implementation**

```typescript
// src/components/editor/monaco-js-worker-lifecycle.ts (new file)
import * as monacoNs from 'monaco-editor';

// Monaco's TypeScript/JavaScript language-service worker (a full TS compiler
// running in a Web Worker, ~50-60MB once loaded) has no idle-stop timer and
// is shared process-wide across every JS/TS-language editor in the app
// (ScriptsTab, ResponseBodyViewer, DiffViewer). It only tears down via a
// private WorkerManager method with no public entry point — the only public
// way to trigger that teardown is to re-apply the current compiler options,
// which fires `onDidChange` and forces the worker to restart lazily on next
// use. This module ref-counts every mounted JS/TS editor across the app so
// the worker is released only once none of them are visible.
let jsWorkerRefCount = 0;

export function acquireJsWorker(): void {
  jsWorkerRefCount += 1;
}

export function releaseJsWorker(): void {
  if (jsWorkerRefCount === 0) return;
  jsWorkerRefCount -= 1;
  if (jsWorkerRefCount === 0) {
    const defaults = monacoNs.typescript.javascriptDefaults;
    defaults.setCompilerOptions(defaults.getCompilerOptions());
  }
}
```

- [ ] **Step 4: Run test to verify it passes**

Run: `yarn test src/components/editor/__tests__/monaco-js-worker-lifecycle.test.ts`
Expected: PASS (all 4 cases)

- [ ] **Step 5: Commit**

```bash
git add src/components/editor/monaco-js-worker-lifecycle.ts src/components/editor/__tests__/monaco-js-worker-lifecycle.test.ts
git commit -m "feat(editor): add ref-counted lifecycle helper to release the Monaco TS worker"
```

---

### Task 5: Wire the lifecycle helper into `MonacoWrapper.tsx`

**Files:**
- Modify: `src/components/editor/MonacoWrapper.tsx`
- Test: `src/components/editor/__tests__/MonacoWrapper.test.tsx` (create if it doesn't already exist — check first; extend if it does)

**Interfaces:**
- Consumes: `acquireJsWorker`, `releaseJsWorker` from `./monaco-js-worker-lifecycle` (Task 4).
- Produces: no change to `MonacoWrapper`'s public props/behavior — this is an internal-only lifecycle effect.

- [ ] **Step 1: Check for an existing MonacoWrapper test file**

Run: `find src/components/editor -iname "MonacoWrapper.test*"`
If one exists, read it fully before writing Step 2's test so the new test matches its existing render/mocking setup (e.g. how it mocks `@monaco-editor/react`'s `<Editor>`). If none exists, use the test below as-is.

- [ ] **Step 2: Write the failing test**

```typescript
// src/components/editor/__tests__/MonacoWrapper.test.tsx
// (Add to the existing file's describe block if one exists; otherwise this is the whole new file.)
import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { MonacoWrapper } from '../MonacoWrapper';

const acquireJsWorker = vi.fn();
const releaseJsWorker = vi.fn();
vi.mock('../monaco-js-worker-lifecycle', () => ({ acquireJsWorker, releaseJsWorker }));

// @monaco-editor/react's <Editor> needs a DOM measurement environment it
// doesn't have in jsdom by default — stub it to a plain div so this test
// only exercises MonacoWrapper's own lifecycle effect, not Monaco itself.
vi.mock('@monaco-editor/react', () => ({
  default: () => <div data-testid='editor-stub' />,
}));

describe('MonacoWrapper JS worker lifecycle', () => {
  it('acquires the JS worker on mount for language="javascript" and releases on unmount', () => {
    const { unmount } = render(<MonacoWrapper value='' language='javascript' />);
    expect(acquireJsWorker).toHaveBeenCalledTimes(1);
    expect(releaseJsWorker).not.toHaveBeenCalled();

    unmount();
    expect(releaseJsWorker).toHaveBeenCalledTimes(1);
  });

  it('does not acquire the JS worker for a non-JS language', () => {
    render(<MonacoWrapper value='' language='json' />);
    expect(acquireJsWorker).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 3: Run test to verify it fails**

Run: `yarn test src/components/editor/__tests__/MonacoWrapper.test.tsx`
Expected: FAIL — `acquireJsWorker` not called yet.

- [ ] **Step 4: Write minimal implementation**

```typescript
// src/components/editor/MonacoWrapper.tsx
// Add import:
import { acquireJsWorker, releaseJsWorker } from './monaco-js-worker-lifecycle';

// Add a new effect right after the "Inject decoration styles once on mount"
// effect (current lines 91-94), keyed on resolvedLanguage so a language
// change (e.g. ResponseBodyViewer switching content-type) re-evaluates it:
  useEffect(() => {
    const isJsLike = resolvedLanguage === 'javascript' || resolvedLanguage === 'typescript';
    if (!isJsLike) return;
    acquireJsWorker();
    return () => releaseJsWorker();
  }, [resolvedLanguage]);
```

- [ ] **Step 5: Run test to verify it passes**

Run: `yarn test src/components/editor/__tests__/MonacoWrapper.test.tsx`
Expected: PASS

- [ ] **Step 6: Run the full editor test directory for regressions**

Run: `yarn test src/components/editor`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/components/editor/MonacoWrapper.tsx src/components/editor/__tests__/MonacoWrapper.test.tsx
git commit -m "fix(editor): release Monaco TS worker when MonacoWrapper unmounts a JS/TS editor"
```

---

### Task 6: Wire the lifecycle helper into `DiffViewer.tsx`

**Files:**
- Modify: `src/components/git/DiffViewer.tsx`
- Test: `src/components/git/__tests__/DiffViewer.test.tsx` (check for an existing file first, same as Task 5 Step 1)

**Interfaces:**
- Consumes: `acquireJsWorker`, `releaseJsWorker` from `@/components/editor/monaco-js-worker-lifecycle` (Task 4).
- Produces: no change to `DiffViewer`'s public props/behavior.

- [ ] **Step 1: Check for an existing DiffViewer test file**

Run: `find src/components/git -iname "DiffViewer.test*"`
Read it fully if it exists, to match its mocking of `@monaco-editor/react`'s `<DiffEditor>` before writing Step 2.

- [ ] **Step 2: Write the failing test**

```typescript
// src/components/git/__tests__/DiffViewer.test.tsx
// (Add to the existing file if one exists; otherwise this is the whole new file — adapt the
// diffState/gitDiff mocking below to match whatever setup an existing file already has.)
import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { DiffViewer } from '../DiffViewer';
import type { DiffState } from '@/types/pane-types';

const acquireJsWorker = vi.fn();
const releaseJsWorker = vi.fn();
vi.mock('@/components/editor/monaco-js-worker-lifecycle', () => ({ acquireJsWorker, releaseJsWorker }));

vi.mock('@monaco-editor/react', () => ({
  DiffEditor: () => <div data-testid='diff-editor-stub' />,
}));

const jsDiffState: DiffState = {
  filePath: 'src/index.js',
  oldContent: '',
  newContent: '',
} as DiffState;

const jsonDiffState: DiffState = {
  filePath: 'package.json',
  oldContent: '',
  newContent: '',
} as DiffState;

describe('DiffViewer JS worker lifecycle', () => {
  it('acquires the JS worker for a .js file and releases on unmount', () => {
    const { unmount } = render(<DiffViewer diffState={jsDiffState} />);
    expect(acquireJsWorker).toHaveBeenCalledTimes(1);

    unmount();
    expect(releaseJsWorker).toHaveBeenCalledTimes(1);
  });

  it('does not acquire the JS worker for a non-JS file', () => {
    render(<DiffViewer diffState={jsonDiffState} />);
    expect(acquireJsWorker).not.toHaveBeenCalled();
  });
});
```

- [ ] **Step 3: Run test to verify it fails**

Run: `yarn test src/components/git/__tests__/DiffViewer.test.tsx`
Expected: FAIL — `acquireJsWorker` not called yet. (If this fails instead on unrelated setup — e.g. `gitDiff`/`gitDiffStaged` IPC calls this component also makes on mount — mock `@/lib/tauri-api`'s `gitDiff`/`gitDiffStaged` the same way any pre-existing `DiffViewer` test already does.)

- [ ] **Step 4: Write minimal implementation**

```typescript
// src/components/git/DiffViewer.tsx
// Add import:
import { acquireJsWorker, releaseJsWorker } from '@/components/editor/monaco-js-worker-lifecycle';

// The `language` variable is computed at (current) line 104: const language = getLanguage(diffState.filePath);
// Add a new effect right after that line, keyed on language so switching
// which file this same mounted DiffViewer shows (diffState can change while
// mounted) re-evaluates it:
  useEffect(() => {
    const isJsLike = language === 'javascript' || language === 'typescript';
    if (!isJsLike) return;
    acquireJsWorker();
    return () => releaseJsWorker();
  }, [language]);
```

- [ ] **Step 5: Run test to verify it passes**

Run: `yarn test src/components/git/__tests__/DiffViewer.test.tsx`
Expected: PASS

- [ ] **Step 6: Run the full git component test directory for regressions**

Run: `yarn test src/components/git`
Expected: PASS

- [ ] **Step 7: Commit**

```bash
git add src/components/git/DiffViewer.tsx src/components/git/__tests__/DiffViewer.test.tsx
git commit -m "fix(git): release Monaco TS worker when DiffViewer unmounts a JS/TS diff"
```

---

## Final Verification (run once, after all 6 tasks)

- [ ] `yarn tsc --noEmit` — no TypeScript errors across the frontend
- [ ] `yarn check` — Biome lint/format clean
- [ ] `yarn test` — full Vitest suite green
