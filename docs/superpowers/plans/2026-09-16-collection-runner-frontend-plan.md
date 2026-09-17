# Collection Runner — Frontend Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the frontend surface for the Collection Runner — a "Run Folder"/"Run Collection" context-menu entry point, a new Runner tab that streams per-step results as a run executes, and a stop/cancel control — consuming the IPC contract described in the design spec (§5) and, if by the time work starts the sibling backend plan has landed with a finalized "Frontend contract" section, that section's actual shapes instead.

**Architecture:** A new `RunnerTab` pane-tab type (sibling to `ContractTab`/`GitTab`) opened from the folder/collection context menu. A new Zustand store (`runner-store.ts`), modeled directly on the existing `load-test-store.ts` (the closest real precedent for "a run that streams per-item progress" — same module-level `UnlistenFn` refs, same register-listeners-before-invoking-the-command ordering, same `idle/running/complete/error` status shape), holds the active run's state and owns the `@tauri-apps/api/event` `listen()` subscriptions. `RunnerTab.tsx` renders per-step rows reusing `TestsPanel`'s pass/fail badge and icon style, plus a Stop button and a "stop on first failure" toggle. Two small call sites (`FolderNode.tsx`, `CollectionNode.tsx`) gain a "Run Folder"/"Run Collection" menu entry that opens the tab and starts the run, mirroring how those files already open `CollectionTab`s inline for "Overview".

**Tech Stack:** React 18, TypeScript, Zustand, `@tauri-apps/api/event`/`core`, shadcn/ui (Badge, Button, ScrollArea, Switch, DropdownMenu, ContextMenu), lucide-react icons, Vitest.

**Spec:** `docs/superpowers/specs/2026-09-16-collection-runner-design.md` (see §4 sequencing model, §5 integration points, §7 acceptance criteria, §8 resolved decisions).

## Global Constraints

- All UI components use shadcn/ui primitives only — no raw `<button>`/`<input>`/`<dialog>`/`<select>`/`<form>` (CLAUDE.md Hard Rules).
- Icons: lucide-react only — no inline SVGs (CLAUDE.md Hard Rules).
- Zustand: never fully destructure store state at component top level; prefer narrow selectors (`.claude/rules/frontend-component-guardrails.md`).
- Single-line variable-aware fields use `SingleLineEditor`; this plan introduces none (the runner UI has no free-text/variable-aware inputs).
- `req.getExecutionMode()` / `"runner"` has no frontend-visible surface — it is set purely on the Rust/script side per the spec; nothing in this plan touches it.
- `rok.runRequest(path)` is explicitly out of scope (spec §3) — do not build anything for it.
- Verification for every task: `yarn tsc --noEmit` and `yarn check` (Biome). Run `yarn test <pattern>` for tasks that add store tests.
- Commits: conventional commits format (`feat:`, `fix:`, `chore:`, etc.).

---

## Task 0: Reconcile against the backend plan's frontend contract

**Files:** none (research only — this task's output is the "Working IPC contract" section below, which every later task treats as ground truth).

- [ ] **Step 1: Check whether the backend plan exists yet**

Run: `test -f docs/superpowers/plans/2026-09-16-collection-runner-backend-plan.md && echo EXISTS || echo NOTFOUND`

- [ ] **Step 2: If it exists, read its "Frontend contract" section and reconcile**

If the file exists, open it and find the section titled (or clearly serving as) "Frontend contract" — it is required to have one per the coordinating agent's instructions. Compare every item below against what it actually settled on:
- Exact Tauri command name(s) and parameter names/order for starting a run (spec sketch: `run_collection(collection, folder_path?, environment_name?)`).
- Whether a `stop_on_failure` parameter exists on that command, and its exact name/position.
- Whether folder-level and collection-level runs share one command or two (spec §6 explicitly leaves this open).
- The exact event channel names Tauri emits on (this repo's convention, confirmed by reading `src-tauri/src/tauri_event_bus.rs`, is kebab-case, one channel per `DomainEvent` variant or group — e.g. `request-executed`, `git-changed` — not `snake_case`; the load-test feature is the one exception, using `load_test_progress`/`load_test_complete` because those events bypass the `DomainEvent` enum and are emitted directly by `load_test_service.rs`. Confirm which pattern the runner followed).
- The exact payload field names/casing for `RunnerStepCompleted` and `RunnerFinished` (this repo's `DomainEvent` enum is `#[serde(tag = "type", rename_all = "camelCase")]`, so if the runner event went through that enum, expect `{ type: "runnerStepCompleted", runId, itemName, status, testPassCount, testFailCount, scriptError }` with a camelCased `type` tag — confirm against the actual Rust enum, do not assume).
- Whether a cancel/stop command exists, its name, and its parameters (the spec sketch does not define one explicitly — only prose "a way to stop an in-progress run" — so this is the most likely thing to differ).
- Update every occurrence of the "Working IPC contract" values used in Tasks 2–10 below to match. If no differences are found, note that explicitly instead of silently leaving the assumed contract in place.

- [ ] **Step 3: If it does not exist yet, proceed on the documented working contract below and flag it for later reconciliation**

At the time this plan was written, `docs/superpowers/plans/2026-09-16-collection-runner-backend-plan.md` did **not** exist yet (checked immediately before writing this plan). Every task below is written against this **assumed working IPC contract**, reasoned from spec §5 plus this repo's actual `DomainEvent`/`TauriEventBus` conventions (verified by reading `crates/rocket-shared/src/events.rs` and `src-tauri/src/tauri_event_bus.rs`):

```
Command: run_collection(collection: string, folderPath?: string, environmentName?: string, stopOnFailure: boolean) -> Promise<string /* runId */>
Command: cancel_runner_run(runId: string) -> Promise<void>     // ASSUMED — spec has no concrete shape, see below
Event:   "runner-step-completed" -> { type: "runnerStepCompleted", runId, itemName, status, statusCode?, testPassCount, testFailCount, scriptError? }
Event:   "runner-finished"       -> { type: "runnerFinished", runId, stoppedReason }
```

Two parts of this are explicitly **not** derived from the spec and must be re-verified once the backend plan lands:
1. `cancel_runner_run` — its name, parameters, and even its existence as a dedicated command (vs., e.g., the run-start promise itself supporting an `AbortController`-style cancellation, which is architecturally impossible for a fire-and-return `invoke()` — a real Tauri command cancel needs either a second command or a cancellation flag file/channel). This plan assumes a second command.
2. The `status` field on `RunnerStepCompleted` — the spec sketch lists a bare `status` field without defining its type. This plan assumes a 3-way enum (`"completed" | "skipped" | "error"`) plus a separate optional `statusCode` (the HTTP status), because the acceptance criteria (spec §7) ask the summary to show "status code (or 'skipped')" as one column and pass/fail test counts as another — collapsing "was this step's HTTP call outcome" and "did its tests pass" into a single field would lose information the UI needs to show. **This is this plan's own design decision, not confirmed by any spec or backend-plan text — flag it for reconciliation.**

If, when this task is executed, the backend plan still does not exist, proceed with Tasks 1–11 as written and leave a comment in the modified files (`src/lib/tauri-api.ts`, `src/stores/runner-store.ts`) pointing back to this task so a future pass can reconcile once the backend contract is final.

---

## Task 1: Add the `RunnerTab` pane-tab type

**Files:**
- Modify: `src/types/pane-types.ts`

**Interfaces:**
- Produces: `RunnerTab` interface, `isRunnerTab(tab: Tab): tab is RunnerTab` type guard, `RunnerTab` added to the `Tab` union — consumed by Tasks 3, 6, 7, 8.

- [ ] **Step 1: Add the `RunnerTab` interface next to the other tab types**

In `src/types/pane-types.ts`, add after the `ContractDiffTab` block (after line 102, before `export function isCollectionTab`):

```typescript
export interface RunnerTab extends BaseTab {
  tabType: 'runner';
  collectionName: string;
  collectionRoot: string; // absolute path — required for the run_collection IPC call
  folderPath?: string; // absent = whole-collection run; present = single folder
  targetLabel: string; // display name shown in the tab title (folder or collection name)
}

export function isRunnerTab(tab: Tab): tab is RunnerTab {
  return tab.tabType === 'runner';
}
```

- [ ] **Step 2: Add `RunnerTab` to the `Tab` union**

Change:

```typescript
export type Tab =
  | RequestTab
  | CollectionTab
  | WorkspaceTab
  | DiffTab
  | ConflictTab
  | GitTab
  | ContractTab
  | ContractDiffTab;
```

to:

```typescript
export type Tab =
  | RequestTab
  | CollectionTab
  | WorkspaceTab
  | DiffTab
  | ConflictTab
  | GitTab
  | ContractTab
  | ContractDiffTab
  | RunnerTab;
```

- [ ] **Step 3: Verify the project still type-checks**

Run: `yarn tsc --noEmit`
Expected: FAIL — `BreadcrumbBar.tsx`'s exhaustive `never` check (line ~437) and `EditorGroup.tsx`'s render switch will now be missing a case for `'runner'`. This is expected at this point in the plan; Tasks 7 and 8 fix it. Confirm the *only* new errors are in those two files (and nowhere else) before moving on.

- [ ] **Step 4: Commit**

```bash
git add src/types/pane-types.ts
git commit -m "feat: add RunnerTab pane-tab type for Collection Runner"
```

---

## Task 2: Add IPC wrappers and event types to `tauri-api.ts`

**Files:**
- Modify: `src/lib/tauri-api.ts`

**Interfaces:**
- Consumes: nothing from earlier tasks.
- Produces: `RunnerStepStatus`, `RunnerStoppedReason`, `RunnerStepCompletedEvent`, `RunnerFinishedEvent` types; `runCollection(collection, folderPath, environmentName, stopOnFailure)`, `cancelRunnerRun(runId)`, `onRunnerStepCompleted(handler)`, `onRunnerFinished(handler)` functions — consumed by Task 4 (`runner-store.ts`).

- [ ] **Step 1: Add the runner types and command wrappers**

In `src/lib/tauri-api.ts`, add a new section after the existing `// History` section (after `export const searchHistory = ...` around line 668), before `// Templates`:

```typescript
// ============================================================
// Collection Runner
// ============================================================

export type RunnerStepStatus = 'completed' | 'skipped' | 'error';

export interface RunnerStepCompletedEvent {
  type: 'runnerStepCompleted';
  runId: string;
  itemName: string;
  status: RunnerStepStatus;
  // HTTP status code for a completed step. Absent for skipped/error steps.
  statusCode?: number | null;
  testPassCount: number;
  testFailCount: number;
  scriptError?: string | null;
}

export type RunnerStoppedReason = 'completed' | 'cancelled' | 'error' | 'stoppedByScript';

export interface RunnerFinishedEvent {
  type: 'runnerFinished';
  runId: string;
  stoppedReason: RunnerStoppedReason;
}

export const runCollection = (
  collection: string,
  folderPath: string | undefined,
  environmentName: string | undefined,
  stopOnFailure: boolean,
) =>
  invoke<string>('run_collection', {
    collection,
    folderPath,
    environmentName,
    stopOnFailure,
  });

// ASSUMED command name/shape — the spec (docs/superpowers/specs/2026-09-16-collection-runner-design.md
// §5/§6) does not define a cancel mechanism. Reconcile against the backend plan's
// "Frontend contract" section once it exists (see Task 0 of the frontend plan).
export const cancelRunnerRun = (runId: string) => invoke<void>('cancel_runner_run', { runId });

export const onRunnerStepCompleted = (
  handler: (event: RunnerStepCompletedEvent) => void,
): Promise<UnlistenFn> =>
  listen<RunnerStepCompletedEvent>('runner-step-completed', (e) => handler(e.payload));

export const onRunnerFinished = (
  handler: (event: RunnerFinishedEvent) => void,
): Promise<UnlistenFn> => listen<RunnerFinishedEvent>('runner-finished', (e) => handler(e.payload));
```

- [ ] **Step 2: Verify types compile**

Run: `yarn tsc --noEmit`
Expected: same two pre-existing failures as Task 1 Step 3 (`BreadcrumbBar.tsx`, `EditorGroup.tsx`), no new errors from `tauri-api.ts`.

- [ ] **Step 3: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat: add run_collection IPC wrappers and runner event types"
```

---

## Task 3: Add `openRunnerTab` to the pane store

**Files:**
- Modify: `src/stores/pane-store.ts`

**Interfaces:**
- Consumes: `RunnerTab` from `src/types/pane-types.ts` (Task 1).
- Produces: `openRunnerTab(collectionName: string, collectionRoot: string, folderPath: string | undefined, targetLabel: string): void` on `PaneState` — consumed by Task 9 (`FolderNode.tsx`) and Task 10 (`CollectionNode.tsx`).

- [ ] **Step 1: Import `RunnerTab` and add the action to the `PaneState` interface**

In `src/stores/pane-store.ts`, add `RunnerTab` to the type import block (after `PaneNode,` around line 23):

```typescript
import type {
  CollectionSection,
  CollectionTab,
  ConflictState,
  ConflictTab,
  ContractTab,
  DiffState,
  DiffTab,
  LeafNode,
  PaneNode,
  RequestState,
  RequestTab,
  ResponseState,
  RunnerTab,
  SplitNode,
  Tab,
  WorkspaceTab,
  WorkspaceTabSection,
} from '@/types/pane-types';
```

Then add the action signature to `PaneState`, directly below `openContractTab`:

```typescript
  // Contract tab.
  openContractTab: (collectionName: string, collectionRoot: string) => void;

  // Runner tab.
  openRunnerTab: (
    collectionName: string,
    collectionRoot: string,
    folderPath: string | undefined,
    targetLabel: string,
  ) => void;
```

- [ ] **Step 2: Make `openTab`'s collection-derivation switch recognize runner tabs**

`openTab` currently derives `collectionName` from the tab to update `activeCollection` (so the sidebar stays in sync). Find this block (around line 138):

```typescript
    const collectionName =
      tab.tabType === 'collection'
        ? (tab as CollectionTab).collectionName
        : tab.tabType === 'contract'
          ? (tab as ContractTab).collectionName
          : (tab.source?.collection ?? null);
```

Replace with:

```typescript
    const collectionName =
      tab.tabType === 'collection'
        ? (tab as CollectionTab).collectionName
        : tab.tabType === 'contract'
          ? (tab as ContractTab).collectionName
          : tab.tabType === 'runner'
            ? (tab as RunnerTab).collectionName
            : (tab.source?.collection ?? null);
```

- [ ] **Step 3: Implement `openRunnerTab`**

Add the implementation directly below `openContractTab`'s implementation (after its closing `},` around line 381):

```typescript
  openRunnerTab(collectionName, collectionRoot, folderPath, targetLabel) {
    const id = folderPath ? `runner:${collectionRoot}:${folderPath}` : `runner:${collectionRoot}`;
    const tab: RunnerTab = {
      id,
      title: `Run — ${targetLabel}`,
      tabType: 'runner',
      collectionName,
      collectionRoot,
      folderPath,
      targetLabel,
      isDirty: false,
    };
    get().openTab(tab);
  },
```

- [ ] **Step 4: Write a test for `openRunnerTab`**

Add to `src/stores/__tests__/pane-store.test.ts` (find the existing `describe('usePaneStore'` block and add a new `it` near the `openContractTab`-style tests, or as a new top-level `it` if none exists — search the file for `'opens a contract tab'` or similar to match the existing style):

```typescript
  it('openRunnerTab opens a runner tab scoped to a folder', () => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openRunnerTab('my-api', '/ws/collections/my-api', 'Auth', 'Auth');
    const leaf = usePaneStore.getState().root as import('@/types/pane-types').LeafNode;
    const tab = leaf.tabs.find((t) => t.id === 'runner:/ws/collections/my-api:Auth');
    expect(tab).toBeDefined();
    expect(tab?.tabType).toBe('runner');
    expect((tab as import('@/types/pane-types').RunnerTab).folderPath).toBe('Auth');
    expect(usePaneStore.getState().activeCollection).toBe('my-api');
  });

  it('openRunnerTab opens a whole-collection runner tab without a folderPath', () => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openRunnerTab('my-api', '/ws/collections/my-api', undefined, 'my-api');
    const leaf = usePaneStore.getState().root as import('@/types/pane-types').LeafNode;
    const tab = leaf.tabs.find((t) => t.id === 'runner:/ws/collections/my-api');
    expect(tab).toBeDefined();
    expect((tab as import('@/types/pane-types').RunnerTab).folderPath).toBeUndefined();
  });
```

- [ ] **Step 5: Run the new tests**

Run: `yarn test pane-store`
Expected: PASS (including the two new tests).

- [ ] **Step 6: Commit**

```bash
git add src/stores/pane-store.ts src/stores/__tests__/pane-store.test.ts
git commit -m "feat: add openRunnerTab action to pane store"
```

---

## Task 4: Create the runner Zustand store

**Files:**
- Create: `src/stores/runner-store.ts`
- Test: `src/stores/__tests__/runner-store.test.ts`

**Interfaces:**
- Consumes: `runCollection`, `cancelRunnerRun`, `onRunnerStepCompleted`, `onRunnerFinished`, `RunnerStepCompletedEvent`, `RunnerFinishedEvent`, `RunnerStepStatus`, `RunnerStoppedReason` from `src/lib/tauri-api.ts` (Task 2); `useEnvStore` from `src/stores/env-store.ts`.
- Produces: `useRunnerStore` hook with state `{ status, runId, target, stopOnFailure, steps, stoppedReason, error }` and actions `{ setStopOnFailure, startRun, cancelRun, reset }`, plus `RunnerStepResult` and `RunnerRunTarget` types — consumed by Task 6 (`RunnerTab.tsx`), Task 9 (`FolderNode.tsx`), Task 10 (`CollectionNode.tsx`).

- [ ] **Step 1: Write the store**

Create `src/stores/runner-store.ts`:

```typescript
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { create } from 'zustand';
import {
  cancelRunnerRun,
  onRunnerFinished,
  onRunnerStepCompleted,
  type RunnerStepStatus,
  type RunnerStoppedReason,
  runCollection,
} from '@/lib/tauri-api';
import { useEnvStore } from '@/stores/env-store';

// listen/UnlistenFn re-exported by tauri-api's wrappers already cover the
// runtime import; this import is kept only for the UnlistenFn type used below.
void listen;

export interface RunnerRunTarget {
  collection: string;
  collectionRoot: string;
  folderPath?: string;
  targetLabel: string;
}

export interface RunnerStepResult {
  // Position in the run — used as the React key, since a script can jump
  // back to an earlier item via rok.runner.setNextRequest(), so itemName
  // alone is not unique within a run.
  index: number;
  itemName: string;
  status: RunnerStepStatus;
  statusCode: number | null;
  testPassCount: number;
  testFailCount: number;
  scriptError: string | null;
}

export type RunnerRunStatus = 'idle' | 'running' | 'complete' | 'error' | 'cancelled';

interface RunnerState {
  status: RunnerRunStatus;
  runId: string | null;
  target: RunnerRunTarget | null;
  stopOnFailure: boolean;
  steps: RunnerStepResult[];
  stoppedReason: RunnerStoppedReason | null;
  error: string | null;

  setStopOnFailure: (value: boolean) => void;
  startRun: (target: RunnerRunTarget) => Promise<void>;
  cancelRun: () => Promise<void>;
  reset: () => void;
}

// Module-level refs so a stale run's listeners can always be torn down,
// even if the component that started the run has since unmounted.
// Mirrors the pattern in load-test-store.ts.
let unlistenStep: UnlistenFn | null = null;
let unlistenFinished: UnlistenFn | null = null;

function teardownListeners() {
  unlistenStep?.();
  unlistenFinished?.();
  unlistenStep = null;
  unlistenFinished = null;
}

export const useRunnerStore = create<RunnerState>((set, get) => ({
  status: 'idle',
  runId: null,
  target: null,
  // Default false — matches spec §8.3: a failed step does not stop the
  // run by default (matches Bruno's and Postman's default).
  stopOnFailure: false,
  steps: [],
  stoppedReason: null,
  error: null,

  setStopOnFailure: (value) => set({ stopOnFailure: value }),

  startRun: async (target) => {
    teardownListeners();

    set({
      status: 'running',
      runId: null,
      target,
      steps: [],
      stoppedReason: null,
      error: null,
    });

    // Register listeners before invoking the command — same ordering as
    // load-test-store.ts's startTest, so no step/finish event fired while
    // the run is starting up can be missed.
    unlistenStep = await onRunnerStepCompleted((event) => {
      set((s) => ({
        steps: [
          ...s.steps,
          {
            index: s.steps.length,
            itemName: event.itemName,
            status: event.status,
            statusCode: event.statusCode ?? null,
            testPassCount: event.testPassCount,
            testFailCount: event.testFailCount,
            scriptError: event.scriptError ?? null,
          },
        ],
      }));
    });

    unlistenFinished = await onRunnerFinished((event) => {
      set({
        status: event.stoppedReason === 'cancelled' ? 'cancelled' : 'complete',
        stoppedReason: event.stoppedReason,
      });
      teardownListeners();
    });

    try {
      const runId = await runCollection(
        target.collection,
        target.folderPath,
        useEnvStore.getState().activeEnvId ?? undefined,
        get().stopOnFailure,
      );
      set({ runId });
    } catch (err) {
      set({ status: 'error', error: String(err) });
      teardownListeners();
    }
  },

  cancelRun: async () => {
    const { runId } = get();
    if (!runId) return;
    try {
      await cancelRunnerRun(runId);
    } catch (err) {
      set({ error: String(err) });
    }
  },

  reset: () => {
    teardownListeners();
    set({
      status: 'idle',
      runId: null,
      target: null,
      steps: [],
      stoppedReason: null,
      error: null,
    });
  },
}));
```

- [ ] **Step 2: Remove the placeholder `listen` import once step 1 is reviewed**

The `void listen;` line above only exists to keep the `listen` import from being flagged unused while explaining the `UnlistenFn` type import; simplify by importing only the type instead. Replace the top of the file:

```typescript
import type { UnlistenFn } from '@tauri-apps/api/event';
import { create } from 'zustand';
```

and delete the `void listen;` line entirely. This avoids an unused-import lint failure under `yarn check`.

- [ ] **Step 3: Write the store test**

Create `src/stores/__tests__/runner-store.test.ts`:

```typescript
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useRunnerStore } from '../runner-store';

let stepHandler: ((event: unknown) => void) | null = null;
let finishedHandler: ((event: unknown) => void) | null = null;

vi.mock('@/lib/tauri-api', () => ({
  runCollection: vi.fn().mockResolvedValue('run-1'),
  cancelRunnerRun: vi.fn().mockResolvedValue(undefined),
  onRunnerStepCompleted: vi.fn().mockImplementation(async (handler) => {
    stepHandler = handler;
    return () => {
      stepHandler = null;
    };
  }),
  onRunnerFinished: vi.fn().mockImplementation(async (handler) => {
    finishedHandler = handler;
    return () => {
      finishedHandler = null;
    };
  }),
}));

vi.mock('@/stores/env-store', () => ({
  useEnvStore: { getState: () => ({ activeEnvId: null }) },
}));

const TARGET = {
  collection: 'my-api',
  collectionRoot: '/ws/collections/my-api',
  folderPath: 'Auth',
  targetLabel: 'Auth',
};

describe('useRunnerStore', () => {
  beforeEach(() => {
    useRunnerStore.getState().reset();
    vi.clearAllMocks();
    stepHandler = null;
    finishedHandler = null;
  });

  it('starts in idle state', () => {
    const { status, steps, runId } = useRunnerStore.getState();
    expect(status).toBe('idle');
    expect(steps).toHaveLength(0);
    expect(runId).toBeNull();
  });

  it('startRun sets status to running and stores the run id', async () => {
    await useRunnerStore.getState().startRun(TARGET);
    expect(useRunnerStore.getState().status).toBe('running');
    expect(useRunnerStore.getState().runId).toBe('run-1');
    expect(useRunnerStore.getState().target).toEqual(TARGET);
  });

  it('appends a step result when a step-completed event fires', async () => {
    await useRunnerStore.getState().startRun(TARGET);
    expect(stepHandler).not.toBeNull();
    stepHandler?.({
      type: 'runnerStepCompleted',
      runId: 'run-1',
      itemName: 'Login',
      status: 'completed',
      statusCode: 200,
      testPassCount: 2,
      testFailCount: 0,
      scriptError: null,
    });
    const { steps } = useRunnerStore.getState();
    expect(steps).toHaveLength(1);
    expect(steps[0]).toMatchObject({ index: 0, itemName: 'Login', statusCode: 200 });
  });

  it('marks the run complete when a finished event fires', async () => {
    await useRunnerStore.getState().startRun(TARGET);
    expect(finishedHandler).not.toBeNull();
    finishedHandler?.({ type: 'runnerFinished', runId: 'run-1', stoppedReason: 'completed' });
    expect(useRunnerStore.getState().status).toBe('complete');
    expect(useRunnerStore.getState().stoppedReason).toBe('completed');
  });

  it('marks the run cancelled when stoppedReason is cancelled', async () => {
    await useRunnerStore.getState().startRun(TARGET);
    finishedHandler?.({ type: 'runnerFinished', runId: 'run-1', stoppedReason: 'cancelled' });
    expect(useRunnerStore.getState().status).toBe('cancelled');
  });

  it('reset clears steps and returns to idle', async () => {
    await useRunnerStore.getState().startRun(TARGET);
    stepHandler?.({
      type: 'runnerStepCompleted',
      runId: 'run-1',
      itemName: 'Login',
      status: 'completed',
      statusCode: 200,
      testPassCount: 1,
      testFailCount: 0,
      scriptError: null,
    });
    useRunnerStore.getState().reset();
    expect(useRunnerStore.getState().status).toBe('idle');
    expect(useRunnerStore.getState().steps).toHaveLength(0);
  });
});
```

- [ ] **Step 4: Run the new tests**

Run: `yarn test runner-store`
Expected: PASS (all 6 tests).

- [ ] **Step 5: Commit**

```bash
git add src/stores/runner-store.ts src/stores/__tests__/runner-store.test.ts
git commit -m "feat: add runner-store for Collection Runner run state"
```

---

## Task 5: Create `RunnerStepRow` (per-step result rendering)

**Files:**
- Create: `src/components/runner/RunnerStepRow.tsx`

**Interfaces:**
- Consumes: `RunnerStepResult` from `src/stores/runner-store.ts` (Task 4).
- Produces: `RunnerStepRow` component — consumed by Task 6 (`RunnerTab.tsx`).

- [ ] **Step 1: Write the component**

Create `src/components/runner/RunnerStepRow.tsx`, reusing `TestsPanel.tsx`'s icon/badge conventions (green check for pass, red X for fail, same `text-xs font-mono break-all` treatment for error text):

```typescript
import { AlertTriangle, CheckCircle2, SkipForward, XCircle } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import type { RunnerStepResult } from '@/stores/runner-store';

interface RunnerStepRowProps {
  step: RunnerStepResult;
}

export function RunnerStepRow({ step }: RunnerStepRowProps) {
  const hasFailedTests = step.testFailCount > 0;
  const isHttpFailure = step.statusCode != null && step.statusCode >= 400;
  const isFailure = step.status === 'completed' && (hasFailedTests || isHttpFailure);

  return (
    <div className='flex items-start gap-2 px-3 py-2 border-b last:border-b-0 text-sm'>
      {step.status === 'skipped' ? (
        <SkipForward className='h-4 w-4 text-muted-foreground mt-0.5 shrink-0' />
      ) : step.status === 'error' ? (
        <AlertTriangle className='h-4 w-4 text-red-500 mt-0.5 shrink-0' />
      ) : isFailure ? (
        <XCircle className='h-4 w-4 text-red-500 mt-0.5 shrink-0' />
      ) : (
        <CheckCircle2 className='h-4 w-4 text-green-500 mt-0.5 shrink-0' />
      )}

      <div className='flex flex-col gap-0.5 min-w-0 flex-1'>
        <div className='flex items-center gap-2'>
          <span className={isFailure ? 'text-foreground' : 'text-muted-foreground'}>
            {step.itemName}
          </span>
          {step.status === 'skipped' ? (
            <Badge variant='outline' className='text-2xs text-muted-foreground'>
              Skipped
            </Badge>
          ) : step.statusCode != null ? (
            <Badge
              variant='outline'
              className={
                isHttpFailure
                  ? 'text-2xs text-red-600 border-red-200 bg-red-50 dark:bg-red-950 dark:border-red-800 dark:text-red-400'
                  : 'text-2xs text-green-600 border-green-200 bg-green-50 dark:bg-green-950 dark:border-green-800 dark:text-green-400'
              }
            >
              {step.statusCode}
            </Badge>
          ) : null}
          {(step.testPassCount > 0 || step.testFailCount > 0) && (
            <span className='text-2xs text-muted-foreground'>
              {step.testPassCount} passed
              {step.testFailCount > 0 ? `, ${step.testFailCount} failed` : ''}
            </span>
          )}
        </div>
        {step.scriptError && (
          <span className='text-xs text-red-500 font-mono break-all'>{step.scriptError}</span>
        )}
      </div>
    </div>
  );
}
```

- [ ] **Step 2: Verify types compile**

Run: `yarn tsc --noEmit`
Expected: same pre-existing failures as before (`BreadcrumbBar.tsx`, `EditorGroup.tsx` still missing the `'runner'` case), nothing new from this file.

- [ ] **Step 3: Commit**

```bash
git add src/components/runner/RunnerStepRow.tsx
git commit -m "feat: add RunnerStepRow for per-step run results"
```

---

## Task 6: Create the `RunnerTab` component

**Files:**
- Create: `src/components/runner/RunnerTab.tsx`

**Interfaces:**
- Consumes: `RunnerTab` type from `src/types/pane-types.ts` (Task 1); `useRunnerStore` from `src/stores/runner-store.ts` (Task 4); `RunnerStepRow` from `src/components/runner/RunnerStepRow.tsx` (Task 5).
- Produces: `RunnerTab` component with props `{ tab: RunnerTabType }` — consumed by Task 7 (`EditorGroup.tsx`).

- [ ] **Step 1: Write the component**

Create `src/components/runner/RunnerTab.tsx`. Layout follows `TestsPanel.tsx`'s header-plus-scrollable-list shape (a summary bar of Badges, then a scrollable list), with the Stop/Re-run controls and "stop on first failure" toggle from `LoadTestTab.tsx`'s aside-button pattern:

```typescript
import { CheckCircle2, Play, Square, XCircle } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Label } from '@/components/ui/label';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Switch } from '@/components/ui/switch';
import { useRunnerStore } from '@/stores/runner-store';
import type { RunnerTab as RunnerTabType } from '@/types/pane-types';
import { RunnerStepRow } from './RunnerStepRow';

interface RunnerTabProps {
  tab: RunnerTabType;
}

export function RunnerTab({ tab }: RunnerTabProps) {
  const status = useRunnerStore((s) => s.status);
  const steps = useRunnerStore((s) => s.steps);
  const stopOnFailure = useRunnerStore((s) => s.stopOnFailure);
  const stoppedReason = useRunnerStore((s) => s.stoppedReason);
  const error = useRunnerStore((s) => s.error);
  const setStopOnFailure = useRunnerStore((s) => s.setStopOnFailure);
  const startRun = useRunnerStore((s) => s.startRun);
  const cancelRun = useRunnerStore((s) => s.cancelRun);

  const isRunning = status === 'running';

  // Spec §4/§7: a run can end because it reached the end of the sequence,
  // because a script called rok.runner.setNextRequest(null), because the
  // user hit Stop, or because of an error — each is a distinct, user-visible
  // outcome, not just "done".
  const stoppedReasonLabel: Record<NonNullable<typeof stoppedReason>, string> = {
    completed: 'Run complete.',
    cancelled: 'Run cancelled.',
    stoppedByScript: 'Run stopped by script (setNextRequest(null)).',
    error: 'Run ended with an error.',
  };

  const passedSteps = steps.filter(
    (s) => s.status === 'completed' && s.testFailCount === 0 && (s.statusCode ?? 0) < 400,
  ).length;
  const failedSteps = steps.filter(
    (s) => s.status === 'error' || (s.status === 'completed' && (s.testFailCount > 0 || (s.statusCode ?? 0) >= 400)),
  ).length;
  const skippedSteps = steps.filter((s) => s.status === 'skipped').length;

  const handleRerun = () => {
    void startRun({
      collection: tab.collectionName,
      collectionRoot: tab.collectionRoot,
      folderPath: tab.folderPath,
      targetLabel: tab.targetLabel,
    });
  };

  return (
    <div className='flex flex-col h-full min-h-0'>
      <div className='flex items-center justify-between gap-2 px-3 py-2 border-b shrink-0'>
        <div className='flex items-center gap-2 min-w-0'>
          <span className='truncate font-medium text-sm'>{tab.targetLabel}</span>
          {passedSteps > 0 && (
            <Badge
              variant='outline'
              className='gap-1 text-green-600 border-green-200 bg-green-50 dark:bg-green-950 dark:border-green-800 dark:text-green-400'
            >
              <CheckCircle2 className='h-3 w-3' />
              {passedSteps} passed
            </Badge>
          )}
          {failedSteps > 0 && (
            <Badge
              variant='outline'
              className='gap-1 text-red-600 border-red-200 bg-red-50 dark:bg-red-950 dark:border-red-800 dark:text-red-400'
            >
              <XCircle className='h-3 w-3' />
              {failedSteps} failed
            </Badge>
          )}
          {skippedSteps > 0 && (
            <Badge variant='outline' className='text-muted-foreground'>
              {skippedSteps} skipped
            </Badge>
          )}
        </div>

        <div className='flex items-center gap-3 shrink-0'>
          <div className='flex items-center gap-1.5'>
            <Switch
              id='stop-on-failure'
              checked={stopOnFailure}
              onCheckedChange={setStopOnFailure}
              disabled={isRunning}
              className='scale-75'
            />
            <Label htmlFor='stop-on-failure' className='text-xs text-muted-foreground'>
              Stop on first failure
            </Label>
          </div>
          {isRunning ? (
            <Button variant='outline' size='sm' onClick={() => void cancelRun()}>
              <Square className='mr-2 h-3.5 w-3.5' />
              Stop
            </Button>
          ) : (
            <Button size='sm' onClick={handleRerun}>
              <Play className='mr-2 h-3.5 w-3.5' />
              {steps.length > 0 ? 'Re-run' : 'Run'}
            </Button>
          )}
        </div>
      </div>

      {error && <p className='px-3 py-1 text-xs text-destructive shrink-0'>{error}</p>}
      {!isRunning && stoppedReason && (
        <p className='px-3 py-1 text-xs text-muted-foreground shrink-0'>
          {stoppedReasonLabel[stoppedReason]}
        </p>
      )}

      <ScrollArea className='flex-1'>
        {steps.length === 0 ? (
          <div className='flex items-center justify-center h-32 text-sm text-muted-foreground'>
            {isRunning ? 'Running…' : 'No steps yet.'}
          </div>
        ) : (
          steps.map((step) => <RunnerStepRow key={step.index} step={step} />)
        )}
        {isRunning && steps.length > 0 && (
          <div className='px-3 py-2 text-xs text-muted-foreground'>Running…</div>
        )}
      </ScrollArea>
    </div>
  );
}
```

- [ ] **Step 2: Verify types compile**

Run: `yarn tsc --noEmit`
Expected: same pre-existing failures as before (`BreadcrumbBar.tsx`, `EditorGroup.tsx`), nothing new from this file.

- [ ] **Step 3: Commit**

```bash
git add src/components/runner/RunnerTab.tsx
git commit -m "feat: add RunnerTab component for streaming per-step run results"
```

---

## Task 7: Wire `RunnerTab` into `EditorGroup.tsx`

**Files:**
- Modify: `src/components/panes/EditorGroup.tsx`

**Interfaces:**
- Consumes: `RunnerTab` component from `src/components/runner/RunnerTab.tsx` (Task 6); `isRunnerTab` from `src/types/pane-types.ts` (Task 1).

- [ ] **Step 1: Import the component and type guard**

Add `RunnerTab as RunnerTabComponent` import (aliased to avoid colliding with the `RunnerTab` type name already imported elsewhere in this file) and `isRunnerTab`:

```typescript
import { RunnerTab as RunnerTabComponent } from '@/components/runner/RunnerTab';
```

Add `isRunnerTab` to the existing `import { ... } from '@/types/pane-types'` block (alongside `isContractTab`, `isGitTab`, etc.).

- [ ] **Step 2: Add the render branch**

In the render switch (around line 189, right after the `isContractTab` branch and before `isContractDiffTab`), add:

```typescript
          ) : isContractTab(activeTab) ? (
            <ContractsTab
              collectionId={activeTab.collectionRoot}
              collectionName={activeTab.collectionName}
            />
          ) : isRunnerTab(activeTab) ? (
            <RunnerTabComponent tab={activeTab} />
          ) : isContractDiffTab(activeTab) ? (
```

- [ ] **Step 3: Verify types compile**

Run: `yarn tsc --noEmit`
Expected: the `EditorGroup.tsx` error from Task 1 Step 3 is now gone; only the `BreadcrumbBar.tsx` exhaustive-check error remains (fixed in Task 8).

- [ ] **Step 4: Commit**

```bash
git add src/components/panes/EditorGroup.tsx
git commit -m "feat: render RunnerTab in the editor group"
```

---

## Task 8: Wire the runner tab into `TabItem.tsx` and `BreadcrumbBar.tsx`

**Files:**
- Modify: `src/components/panes/TabItem.tsx`
- Modify: `src/components/panes/BreadcrumbBar.tsx`

**Interfaces:**
- Consumes: `isRunnerTab` from `src/types/pane-types.ts` (Task 1).

- [ ] **Step 1: Add a tab-bar icon in `TabItem.tsx`**

Import `PlayCircle` from `lucide-react` (add to the existing `lucide-react` import line) and `isRunnerTab` (add to the existing `@/types/pane-types` import line). Add a branch in the icon `if/else if` chain (around line 93, right after the `isContractTab` branch):

```typescript
      ) : isContractTab(tab) ? (
        <FileLock aria-hidden='true' className='h-4 w-4 shrink-0' />
      ) : isRunnerTab(tab) ? (
        <PlayCircle aria-hidden='true' className='h-4 w-4 shrink-0 text-muted-foreground' />
      ) : (
        <BoxIcon aria-hidden='true' className='h-4 w-4 shrink-0' />
      )}
```

- [ ] **Step 2: Add a breadcrumb segment in `BreadcrumbBar.tsx`**

Import `PlayCircle` (add to the existing `lucide-react` import) and `isRunnerTab` (add to the existing `@/types/pane-types` import). Add a branch in `deriveSegments` (around line 431, right after the `isContractTab` branch and before `isContractDiffTab`):

```typescript
  if (isContractTab(tab)) {
    return [
      {
        label: tab.collectionName,
        picker: {
          loadItems: async () => {
            const summaries = await listCollections();
            return summaries.map((s) => ({
              id: s.name,
              label: s.name,
              isActive: s.name === tab.collectionName,
            }));
          },
          onSelect: (item) => nav.switchCollection(item.id),
        },
      },
      { label: 'Contracts', icon: <FileLock className='h-3 w-3' /> },
    ];
  }

  if (isRunnerTab(tab)) {
    return [
      {
        label: tab.collectionName,
        picker: {
          loadItems: async () => {
            const summaries = await listCollections();
            return summaries.map((s) => ({
              id: s.name,
              label: s.name,
              isActive: s.name === tab.collectionName,
            }));
          },
          onSelect: (item) => nav.switchCollection(item.id),
        },
      },
      { label: 'Runner', icon: <PlayCircle className='h-3 w-3' /> },
      ...(tab.folderPath ? [{ label: tab.targetLabel }] : []),
    ];
  }
```

- [ ] **Step 3: Verify types compile**

Run: `yarn tsc --noEmit`
Expected: PASS — no errors. The exhaustive `never` check in `BreadcrumbBar.tsx` (`deriveSegments`'s final `const _exhaustive: never = tab;`) now accepts `RunnerTab` because every branch handles it.

- [ ] **Step 4: Run lint**

Run: `yarn check`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/components/panes/TabItem.tsx src/components/panes/BreadcrumbBar.tsx
git commit -m "feat: add tab icon and breadcrumb segment for the runner tab"
```

---

## Task 9: Add "Run Folder" to the folder context menu

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` — this task's `basePath`/folder-path values are read from the on-disk `_order.yml`-backed folder tree, and getting that path wrong means `run_collection`'s `folderPath` argument points at the wrong folder.

**Files:**
- Modify: `src/components/collections/FolderNode.tsx`

**Interfaces:**
- Consumes: `useRunnerStore` from `src/stores/runner-store.ts` (Task 4); `usePaneStore().openRunnerTab` from `src/stores/pane-store.ts` (Task 3).

- [ ] **Step 1: Import `Play` icon, `useRunnerStore`**

Add `Play` to the existing `lucide-react` import (alphabetically, between `Pencil` and `Plus`):

```typescript
import {
  Folder,
  FolderOpen,
  FolderPlus,
  MoreHorizontal,
  Pencil,
  Play,
  Plus,
  Trash2,
  Variable,
} from 'lucide-react';
```

Add the store import alongside the existing `usePaneStore` import:

```typescript
import { useRunnerStore } from '@/stores/runner-store';
```

- [ ] **Step 2: Add a `handleRunFolder` helper inside the component**

Add near the top of the `FolderNode` component body, after `handleRename` (around line 120):

```typescript
  const handleRunFolder = () => {
    usePaneStore.getState().openRunnerTab(collectionName, collectionRoot, basePath, name);
    void useRunnerStore.getState().startRun({
      collection: collectionName,
      collectionRoot,
      folderPath: basePath,
      targetLabel: name,
    });
  };
```

- [ ] **Step 3: Add the menu item to the `DropdownMenu`**

In the `DropdownMenuContent` block, add "Run Folder" right after "Variables" and before the `Rename` separator (around line 238, after the Variables `DropdownMenuItem` and its following `DropdownMenuSeparator`):

```typescript
                <DropdownMenuItem onClick={() => setVarsOpen(true)}>
                  <Variable className='h-3.5 w-3.5 mr-2' /> Variables
                </DropdownMenuItem>
                <DropdownMenuSeparator />
                <DropdownMenuItem onClick={handleRunFolder}>
                  <Play className='h-3.5 w-3.5 mr-2' /> Run Folder
                </DropdownMenuItem>
                <DropdownMenuSeparator />
```

- [ ] **Step 4: Add the same menu item to the `ContextMenu`**

In the `ContextMenuContent` block, mirror the same insertion (around line 283, after the Variables `ContextMenuItem`):

```typescript
        <ContextMenuItem onClick={() => setVarsOpen(true)}>
          <Variable className='h-3.5 w-3.5 mr-2' /> Variables
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem onClick={handleRunFolder}>
          <Play className='h-3.5 w-3.5 mr-2' /> Run Folder
        </ContextMenuItem>
        <ContextMenuSeparator />
```

- [ ] **Step 5: Verify types compile and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/components/collections/FolderNode.tsx
git commit -m "feat: add Run Folder entry to the folder context menu"
```

---

## Task 10: Add "Run Collection" to the collection context menu

📖 Before starting, read `docs/superpowers/specs/opencollection-spec-reference.md` — `collectionRoot` here is derived from the active workspace path plus the collection name, the same construction the existing "Manage contracts" entry already uses; getting it wrong means `run_collection` is invoked against the wrong on-disk collection.

**Files:**
- Modify: `src/components/collections/CollectionNode.tsx`

**Interfaces:**
- Consumes: `useRunnerStore` from `src/stores/runner-store.ts` (Task 4); `usePaneStore().openRunnerTab` from `src/stores/pane-store.ts` (Task 3).

- [ ] **Step 1: Import `Play` icon and `useRunnerStore`**

Add `Play` to the existing `lucide-react` import (alphabetically, between `Pencil` and `Plus`):

```typescript
import {
  BoxIcon,
  ChevronDown,
  ChevronRight,
  CircleAlert,
  FolderPlus,
  LayoutGrid,
  Lock,
  MoreHorizontal,
  Pencil,
  Play,
  Plus,
  Trash2,
  TriangleAlert,
} from 'lucide-react';
```

Add the store import alongside the existing `usePaneStore` import:

```typescript
import { useRunnerStore } from '@/stores/runner-store';
```

- [ ] **Step 2: Add a `handleRunCollection` helper inside the component**

Add near the top of the `CollectionNode` component body, after `handleRename` (around line 217), guarded the same way "Manage contracts" is guarded (`disabled={!collectionRoot}` — `collectionRoot` is empty until `activeWorkspace` resolves):

```typescript
  const handleRunCollection = () => {
    if (!collectionRoot) return;
    usePaneStore.getState().openRunnerTab(summary.name, collectionRoot, undefined, summary.name);
    void useRunnerStore.getState().startRun({
      collection: summary.name,
      collectionRoot,
      targetLabel: summary.name,
    });
  };
```

- [ ] **Step 3: Add the menu item to the `DropdownMenu`**

In the `DropdownMenuContent` block, add "Run Collection" right after "New Folder" and before the Rename separator (around line 429, after the New Folder `DropdownMenuItem`):

```typescript
              <DropdownMenuItem
                onClick={async () => {
                  await onNewFolder(summary.name, '');
                  setOpen(true);
                }}
              >
                <FolderPlus className='h-3.5 w-3.5 mr-2' /> New Folder
              </DropdownMenuItem>
              <DropdownMenuSeparator />
              <DropdownMenuItem disabled={!collectionRoot} onClick={handleRunCollection}>
                <Play className='h-3.5 w-3.5 mr-2' /> Run Collection
              </DropdownMenuItem>
              <DropdownMenuSeparator />
```

- [ ] **Step 4: Add the same menu item to the `ContextMenu`**

In the `ContextMenuContent` block, mirror the same insertion (around line 485, after the New Folder `ContextMenuItem`):

```typescript
        <ContextMenuItem onClick={() => void onNewFolder(summary.name, '')}>
          <FolderPlus className='h-3.5 w-3.5 mr-2' /> New Folder
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem disabled={!collectionRoot} onClick={handleRunCollection}>
          <Play className='h-3.5 w-3.5 mr-2' /> Run Collection
        </ContextMenuItem>
        <ContextMenuSeparator />
```

- [ ] **Step 5: Verify types compile and lint**

Run: `yarn tsc --noEmit && yarn check`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/components/collections/CollectionNode.tsx
git commit -m "feat: add Run Collection entry to the collection context menu"
```

---

## Task 11: Full verification pass

**Files:** none (verification only).

- [ ] **Step 1: Type-check the whole project**

Run: `yarn tsc --noEmit`
Expected: PASS, zero errors.

- [ ] **Step 2: Lint and format-check**

Run: `yarn check`
Expected: PASS, zero issues.

- [ ] **Step 3: Run the full frontend test suite**

Run: `yarn test`
Expected: PASS, including the new `pane-store` and `runner-store` tests from Tasks 3 and 4, and no regressions in existing suites (`src/stores/__tests__/pane-store.test.ts` in particular, since Task 3 modified `openTab`'s collection-derivation switch).

- [ ] **Step 4: Manual smoke check (if `yarn tauri dev` is available in this environment)**

Run: `yarn tauri dev`, open a collection with at least one folder containing requests, right-click the folder, confirm "Run Folder" appears in both the dropdown (`...` button) and native right-click context menu, and that clicking it opens a "Run — <folder>" tab. Since the backend command `run_collection` may not exist yet (depends on the sibling backend plan's completion), expect the IPC call to reject with a "command not found" style error — confirm the tab still opens and the error is surfaced via the `error` banner in `RunnerTab.tsx` rather than crashing the app. This is a smoke check, not a blocking gate — record the outcome but do not treat a missing backend command as a frontend plan failure.

- [ ] **Step 5: Commit any fixes found during verification**

If Steps 1–3 turned up issues, fix them and commit:

```bash
git add -A
git commit -m "fix: address verification findings for Collection Runner frontend"
```

If no issues were found, skip this step — nothing to commit.
