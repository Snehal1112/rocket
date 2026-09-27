# Flow Plan 08: Frontend Types, Bindings, FlowTab — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add the TypeScript domain types, Tauri IPC bindings, and the
`FlowTab` pane-store plumbing that every later Flow frontend plan builds on.
No canvas UI yet — this plan produces a tab that opens and can load/save a
flow's raw data, nothing visual beyond a picker stub.

**Architecture:** Mirrors the existing `RunnerTab`/`openRunnerTab` pattern
exactly (`src/types/pane-types.ts`, `src/stores/pane-store.ts`). Domain types
live in `src/lib/tauri-api.ts` alongside the other mirrored-Rust types
(`Header`, `Body`, `Auth`) — **not** a separate `src/types/flow-types.ts`
file; `pane-types.ts` references them the same way `RunnerRequestEntry`
references `import('@/lib/tauri-api').Request` today. This is a deliberate
deviation from the plan index's file-path sketch, made to match this repo's
actual, verified convention rather than an untested suggestion — see
"Deviations from plan index" below.

**Tech Stack:** React, TypeScript, Zustand, Vitest.

**Spec:** `docs/superpowers/specs/2026-09-27-flow-visual-workflow-builder-design.md`
(§9 Frontend). Plan index:
`docs/superpowers/plans/flow-visual-workflow-builder/00-plan-index.md`
(has the full locked interface contract; see deviations below for where this
plan's actual field names differ from the index's sketch and why).

## Deviations from plan index

The index's frontend sketch used `type: 'flow'` and `collectionRoot` for
`FlowTab`. Reading the actual `src/types/pane-types.ts` and
`src/stores/pane-store.ts` shows every existing tab discriminates on
**`tabType`**, not `type` (`RunnerTab.tabType === 'runner'`, etc.), and the
Runner's collection-scoping field is **`collectionName: string | null`**
(`ContractTab` separately has a `collectionRoot: string` for an absolute
path, but that is not what `RunnerTab`/`FlowTab` need — a flow is scoped by
collection name, exactly like a runner run). This plan follows the verified
real convention: `FlowTab.tabType = 'flow'`, `FlowTab.collectionName: string
| null`. Update the index's frontend section to match once this plan lands.

The index also said `FlowPane` registers in `PaneRenderer.tsx`. The actual
registration point for tab-content components is `src/components/panes/EditorGroup.tsx`
(`PaneRenderer.tsx` only handles the split/leaf resize tree) — `RunnerPane`
is lazy-loaded and switched on there (`isRunnerTab(activeTab) ? <RunnerPane .../> : ...`).
`FlowPane` follows the identical lazy-load + `isFlowTab` pattern in that same
file.

### Corrections from the Plan 07 post-implementation review

This plan was first written before Plan 07 was built. The review checked it
against the real `src-tauri/src/commands/flow.rs` and `DomainEvent`
serialization, and corrected Task 1's code below in place:

- `run_flow` takes one nested argument, `input: RunFlowInputDto`. The
  binding must invoke `'run_flow'` with
  `{ input: { collection, flowName, environmentName } }`, not flat
  `{ collection, name, environmentName }` (that would reject with a
  missing-`input` error).
- `run_flow` resolves only when the run ends, with a camelCase
  `FlowRunSummary`, not a `run_id` string.
- Streamed event payloads are `DomainEvent`s with **snake_case** fields
  (`run_id`, `node_id`, `status_code`, `duration_ms`, `stopped_reason`,
  `node_count`, ...) plus a `type` tag, like every other `DomainEvent`.
  The event interfaces below mirror that exactly.
- An `onFlowRunStarted` binding is added, since `flow-run-started` is the
  only place the `run_id` is available while the run is still going.
- Optional Rust fields arrive as `null`, not absent, so they are typed
  `T | null`.

See the "Wire contract" list in the plan index's `src-tauri` section.

## Global Constraints

- Domain types (`FlowNodeKind`, `RequestSource`, `InlineRequestData`,
  `InlineHeader`, `NodePosition`, `FlowNode`, `FlowEdge`, `Flow`,
  `FlowNodeStatus`) live in `src/lib/tauri-api.ts`, in a new section comment
  block `// Flow (visual workflow builder)`, placed near the end of the file
  alongside the other feature sections (matching where `OAuth2`/collection
  events sections live).
- `invoke()` calls pass camelCase param keys directly (Tauri maps them to the
  Rust command's snake_case parameter names automatically) — confirmed
  against the existing `oauth2AuthCodeFlow` wrapper, which passes
  `{ authorizationUrl, tokenUrl, ... }` verbatim. Do not manually
  snake_case any param object key.
- Event listener helpers follow the exact `onRequestExecuted`/`onCollectionChanged`
  shape already in `tauri-api.ts`: `export const onX = (handler: (event: T) => void): Promise<UnlistenFn> => listen<T>('event-name', (e) => handler(e.payload));`.
  Do not hand-roll a second `listen()`-wrapping convention.
- Zustand: `pane-store.ts` actions read/write `get().root` via
  `updateTabInTree`/`findTabInTree`, exactly like `openRunnerTab`/`toggleRunnerEntry`/`startRun`
  — new Flow actions follow the same helpers, not a parallel state shape.
  Components must never fully destructure store state at the top level
  (per this repo's hard rule) — this plan adds no components yet, but the
  actions themselves must still follow the store's own conventions.
- Conventional-commit format for every Commit step (`feat(frontend): ...`).

## Review Focus

- Opening a `FlowTab` with `flowName` already provided loads that flow's
  nodes/edges immediately via `getFlow` — the tab must not show a picker in
  that case (mirrors `RunnerPane`'s `tab.collectionName === null` picker
  gate, but for Flow the gate should also require `flowName` to be set, not
  just the collection).
- `patchFlowNodeStatus` for a `nodeId` not present in the tab's `nodes` array
  must not throw and must not corrupt other nodes' statuses — a safe no-op.
- Two open `FlowTab`s for different flows (or the same flow opened twice)
  must not share or cross-contaminate `nodeStatus`/`nodes`/`edges` — each
  tab's state is independent, exactly like two open `RunnerTab`s don't share
  `requests`.
- `openFlowTab` called with `flowName` for a flow that fails to load (IPC
  rejects — e.g. deleted file) must not crash the app; it should log the
  error and open the tab in its picker state, mirroring
  `openRunnerTab`'s `try/catch` around `getCollection`.
- `getFlow` rejecting after `openFlowTab` already returned must not leave the
  tab stuck showing a stale loading state forever — the picker-state fallback
  above resolves this by construction, but confirm with a test that the tab
  ends up in a usable (picker) state, not an infinite-loading one.

---

## Task 1: Domain types + Tauri API bindings

**Files:**
- Modify: `src/lib/tauri-api.ts`
- Create: `src/lib/queries/__tests__/flow-api.test.ts`

**Interfaces:**
- Produces: `FlowNodeKind`, `RequestSource`, `InlineRequestData`,
  `InlineHeader`, `NodePosition`, `FlowNode`, `FlowEdge`, `Flow`,
  `FlowNodeStatus`, `FlowRunNodeStatus`, `FlowStepResult`, `FlowRunSummary`,
  `FlowRunStartedEvent`, `FlowStepCompletedEvent`, `FlowRunFinishedEvent` types;
  `listFlows`, `getFlow`, `saveFlow`, `deleteFlow`, `runFlow`,
  `cancelFlowRun`, `onFlowRunStarted`, `onFlowStepCompleted`,
  `onFlowRunFinished` functions —
  consumed by Task 2 of this plan and every later Flow frontend plan.

- [ ] **Step 1: Write the failing test**

```typescript
// src/lib/queries/__tests__/flow-api.test.ts
import { describe, expect, it, vi } from 'vitest';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

describe('flow tauri-api bindings', () => {
  it('listFlows invokes list_flows with the collection name', async () => {
    vi.mocked(invoke).mockResolvedValue(['My Flow']);
    const { listFlows } = await import('@/lib/tauri-api');
    const result = await listFlows('my-collection');
    expect(invoke).toHaveBeenCalledWith('list_flows', { collection: 'my-collection' });
    expect(result).toEqual(['My Flow']);
  });

  it('getFlow invokes get_flow with collection and name', async () => {
    const sample = { name: 'My Flow', nodes: [], edges: [] };
    vi.mocked(invoke).mockResolvedValue(sample);
    const { getFlow } = await import('@/lib/tauri-api');
    const result = await getFlow('my-collection', 'My Flow');
    expect(invoke).toHaveBeenCalledWith('get_flow', {
      collection: 'my-collection',
      name: 'My Flow',
    });
    expect(result).toEqual(sample);
  });

  it('saveFlow invokes save_flow with collection and flow', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { saveFlow } = await import('@/lib/tauri-api');
    const flow = { name: 'My Flow', nodes: [], edges: [] };
    await saveFlow('my-collection', flow);
    expect(invoke).toHaveBeenCalledWith('save_flow', { collection: 'my-collection', flow });
  });

  it('deleteFlow invokes delete_flow with collection and name', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { deleteFlow } = await import('@/lib/tauri-api');
    await deleteFlow('my-collection', 'My Flow');
    expect(invoke).toHaveBeenCalledWith('delete_flow', {
      collection: 'my-collection',
      name: 'My Flow',
    });
  });

  it('runFlow invokes run_flow with a nested input object and resolves with the summary', async () => {
    const summary = { runId: 'run-1', steps: [], stoppedReason: 'completed' };
    vi.mocked(invoke).mockResolvedValue(summary);
    const { runFlow } = await import('@/lib/tauri-api');
    const result = await runFlow('my-collection', 'My Flow', 'staging');
    expect(invoke).toHaveBeenCalledWith('run_flow', {
      input: {
        collection: 'my-collection',
        flowName: 'My Flow',
        environmentName: 'staging',
      },
    });
    expect(result).toEqual(summary);
  });

  it('runFlow sends a null environmentName when none is given', async () => {
    vi.mocked(invoke).mockResolvedValue({ runId: 'run-1', steps: [], stoppedReason: 'completed' });
    const { runFlow } = await import('@/lib/tauri-api');
    await runFlow('my-collection', 'My Flow');
    expect(invoke).toHaveBeenCalledWith('run_flow', {
      input: { collection: 'my-collection', flowName: 'My Flow', environmentName: null },
    });
  });

  it('cancelFlowRun invokes cancel_flow_run with the run id', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { cancelFlowRun } = await import('@/lib/tauri-api');
    await cancelFlowRun('run-1');
    expect(invoke).toHaveBeenCalledWith('cancel_flow_run', { runId: 'run-1' });
  });

  it('onFlowRunStarted subscribes to the flow-run-started event and unwraps the payload', async () => {
    const payload = {
      type: 'flowRunStarted',
      run_id: 'run-1',
      flow_name: 'My Flow',
      collection: 'my-collection',
      total_nodes: 2,
    };
    vi.mocked(listen).mockImplementation(((_event: string, cb: (e: { payload: unknown }) => void) => {
      cb({ payload });
      return Promise.resolve(() => {});
    }) as typeof listen);
    const { onFlowRunStarted } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onFlowRunStarted(handler);
    expect(listen).toHaveBeenCalledWith('flow-run-started', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });

  it('onFlowStepCompleted subscribes to the flow-step-completed event and unwraps the payload', async () => {
    const payload = {
      type: 'flowStepCompleted',
      run_id: 'run-1',
      node_id: 'n1',
      status: 'success' as const,
      status_code: 200,
      duration_ms: 12,
      error: null,
    };
    vi.mocked(listen).mockImplementation(((_event: string, cb: (e: { payload: unknown }) => void) => {
      cb({ payload });
      return Promise.resolve(() => {});
    }) as typeof listen);
    const { onFlowStepCompleted } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onFlowStepCompleted(handler);
    expect(listen).toHaveBeenCalledWith('flow-step-completed', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });

  it('onFlowRunFinished subscribes to the flow-run-finished event and unwraps the payload', async () => {
    const payload = {
      type: 'flowRunFinished',
      run_id: 'run-1',
      stopped_reason: 'completed',
      node_count: 3,
      failed_count: 0,
      skipped_count: 0,
    };
    vi.mocked(listen).mockImplementation(((_event: string, cb: (e: { payload: unknown }) => void) => {
      cb({ payload });
      return Promise.resolve(() => {});
    }) as typeof listen);
    const { onFlowRunFinished } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onFlowRunFinished(handler);
    expect(listen).toHaveBeenCalledWith('flow-run-finished', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });
});
```

- [ ] **Step 2: Run test to verify it fails**

Run: `yarn vitest run src/lib/queries/__tests__/flow-api.test.ts`
Expected: FAIL — `listFlows`, `getFlow`, etc. do not exist in
`@/lib/tauri-api` yet.

- [ ] **Step 3: Add the domain types and bindings**

In `src/lib/tauri-api.ts`, add a new section near the end of the file
(alongside the existing `OAuth2`/collection-event sections):

```typescript
// ============================================================
// Flow (visual workflow builder)
// ============================================================

export interface NodePosition {
  x: number;
  y: number;
}

export interface InlineHeader {
  name: string;
  value: string;
}

export interface InlineRequestData {
  method: string;
  url: string;
  headers: InlineHeader[];
  // The backend sends null for "no body"; it accepts null or absent.
  body?: string | null;
}

export type RequestSource =
  | { type: 'Saved'; requestPath: string }
  | { type: 'Inline'; request: InlineRequestData };

export type FlowNodeKind =
  | { kind: 'Request'; label: string; source: RequestSource }
  | { kind: 'Input'; label: string; value: unknown }
  | { kind: 'Output'; label: string };

export interface FlowNode {
  id: string;
  kind: FlowNodeKind;
  position: NodePosition;
}

export interface FlowEdge {
  id: string;
  sourceNodeId: string;
  targetNodeId: string;
  targetField: string;
  expression: string;
}

export interface Flow {
  name: string;
  nodes: FlowNode[];
  edges: FlowEdge[];
}

export type FlowNodeStatus = 'idle' | 'running' | 'success' | 'failed' | 'skipped';

export const listFlows = (collection: string) =>
  invoke<string[]>('list_flows', { collection });

export const getFlow = (collection: string, name: string) =>
  invoke<Flow>('get_flow', { collection, name });

export const saveFlow = (collection: string, flow: Flow) =>
  invoke<void>('save_flow', { collection, flow });

export const deleteFlow = (collection: string, name: string) =>
  invoke<void>('delete_flow', { collection, name });

/** Backend-reported node status. `'idle'` is frontend-only. */
export type FlowRunNodeStatus = Exclude<FlowNodeStatus, 'idle'>;

/** `run_flow`'s return value. Camel-cased by the Rust IPC DTO. */
export interface FlowStepResult {
  nodeId: string;
  status: FlowRunNodeStatus;
  statusCode: number | null;
  durationMs: number | null;
  error: string | null;
}

export interface FlowRunSummary {
  runId: string;
  steps: FlowStepResult[];
  stoppedReason: 'completed' | 'cancelled' | string;
}

/**
 * Runs a flow. The promise resolves only when the run ENDS. Subscribe to
 * the flow-run-* events before calling this; the run id arrives first on
 * `flow-run-started`.
 */
export const runFlow = (collection: string, flowName: string, environmentName?: string | null) =>
  invoke<FlowRunSummary>('run_flow', {
    input: { collection, flowName, environmentName: environmentName ?? null },
  });

export const cancelFlowRun = (runId: string) => invoke<void>('cancel_flow_run', { runId });

// Event payloads are DomainEvent JSON. Their fields are snake_case, like
// every other DomainEvent. Do not camelCase them here.
export interface FlowRunStartedEvent {
  type: 'flowRunStarted';
  run_id: string;
  flow_name: string;
  collection: string;
  total_nodes: number;
}

export const onFlowRunStarted = (
  handler: (event: FlowRunStartedEvent) => void,
): Promise<UnlistenFn> =>
  listen<FlowRunStartedEvent>('flow-run-started', (e) => handler(e.payload));

export interface FlowStepCompletedEvent {
  type: 'flowStepCompleted';
  run_id: string;
  node_id: string;
  status: FlowRunNodeStatus;
  status_code: number | null;
  duration_ms: number | null;
  error: string | null;
}

export const onFlowStepCompleted = (
  handler: (event: FlowStepCompletedEvent) => void,
): Promise<UnlistenFn> =>
  listen<FlowStepCompletedEvent>('flow-step-completed', (e) => handler(e.payload));

export interface FlowRunFinishedEvent {
  type: 'flowRunFinished';
  run_id: string;
  stopped_reason: string;
  node_count: number;
  failed_count: number;
  skipped_count: number;
}

export const onFlowRunFinished = (
  handler: (event: FlowRunFinishedEvent) => void,
): Promise<UnlistenFn> =>
  listen<FlowRunFinishedEvent>('flow-run-finished', (e) => handler(e.payload));
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `yarn vitest run src/lib/queries/__tests__/flow-api.test.ts`
Expected: PASS — 10 tests. Also run `yarn tsc --noEmit` to confirm no type
errors elsewhere in the codebase.

- [ ] **Step 5: Commit**

```bash
git add src/lib/tauri-api.ts src/lib/queries/__tests__/flow-api.test.ts
git commit -m "feat(frontend): add Flow types and Tauri API bindings"
```

---

## Task 2: `FlowTab`, pane-store actions, `FlowPane` stub

**Files:**
- Modify: `src/types/pane-types.ts`
- Modify: `src/stores/pane-store.ts`
- Modify: `src/components/panes/EditorGroup.tsx`
- Modify: `src/components/panes/BreadcrumbBar.tsx` (confirmed necessary during
  implementation — this file has an exhaustive `const _exhaustive: never =
  tab` switch over the `Tab` union; adding `FlowTab` to that union without a
  matching branch here is a `tsc` compile error, not an optional cleanup. Add
  an `isFlowTab(tab)` branch labeling the breadcrumb with
  `tab.collectionName || 'Flow'`, mirroring the existing `RunnerTab` branch.)
- Create: `src/components/flow/FlowPane.tsx`
- Modify: `src/stores/__tests__/pane-store.test.ts`

**Interfaces:**
- Consumes: `Flow`, `FlowNode`, `FlowEdge`, `FlowNodeStatus`, `getFlow` from
  Task 1 of this plan.
- Produces: `FlowTab`, `isFlowTab`; pane-store actions `openFlowTab`,
  `updateFlowNodes`, `updateFlowEdges`, `patchFlowNodeStatus` — consumed by
  Plans 09 and 10.

- [ ] **Step 1: Write the failing tests**

```typescript
// src/stores/__tests__/pane-store.test.ts (add to the existing test file;
// mirror the existing top-of-file vi.mock for '@/lib/tauri-api' — add getFlow
// to whatever mock object already exists there rather than creating a second
// mock block)
describe('Flow tab actions', () => {
  it('openFlowTab with no flowName opens a picker-state tab', () => {
    usePaneStore.getState().openFlowTab('my-collection');
    const tab = findFirstFlowTab();
    expect(tab?.tabType).toBe('flow');
    expect(tab?.flowName).toBeNull();
    expect(tab?.nodes).toEqual([]);
  });

  it('openFlowTab with a flowName loads nodes/edges immediately', async () => {
    vi.mocked(getFlow).mockResolvedValue({
      name: 'My Flow',
      nodes: [{ id: 'n1', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } }],
      edges: [],
    });
    await usePaneStore.getState().openFlowTab('my-collection', 'My Flow');
    const tab = findFirstFlowTab();
    expect(tab?.flowName).toBe('My Flow');
    expect(tab?.nodes).toHaveLength(1);
  });

  it('openFlowTab falls back to picker state if getFlow rejects', async () => {
    vi.mocked(getFlow).mockRejectedValue(new Error('not found'));
    await usePaneStore.getState().openFlowTab('my-collection', 'Missing Flow');
    const tab = findFirstFlowTab();
    expect(tab?.nodes).toEqual([]);
  });

  it('patchFlowNodeStatus updates only the targeted node', () => {
    usePaneStore.getState().openFlowTab('my-collection');
    const tabId = findFirstFlowTab()!.id;
    usePaneStore.setState({
      root: updateTabInTreeForTest(usePaneStore.getState().root, tabId, (tab) =>
        tab.tabType === 'flow'
          ? {
              ...tab,
              nodes: [
                { id: 'n1', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } },
              ],
            }
          : tab,
      ),
    });
    usePaneStore.getState().patchFlowNodeStatus(tabId, 'n1', 'running');
    const tab = findFirstFlowTab();
    expect(tab?.nodeStatus.n1).toBe('running');
  });

  it('patchFlowNodeStatus for an unknown node id is a safe no-op', () => {
    usePaneStore.getState().openFlowTab('my-collection');
    const tabId = findFirstFlowTab()!.id;
    expect(() =>
      usePaneStore.getState().patchFlowNodeStatus(tabId, 'does-not-exist', 'running'),
    ).not.toThrow();
    expect(findFirstFlowTab()?.nodeStatus['does-not-exist']).toBeUndefined();
  });
});
```

Add a `findFirstFlowTab()` test helper next to this file's existing
`findFirstRunnerTab`-style helper if one exists (grep the file first — reuse
it if present, add an analogous one for `isFlowTab` if not). Add `getFlow` to
this test file's existing `vi.mock('@/lib/tauri-api', ...)` block rather than
introducing a second mock of the module.

- [ ] **Step 2: Run tests to verify they fail**

Run: `yarn vitest run src/stores/__tests__/pane-store.test.ts -t "Flow tab actions"`
Expected: FAIL — `openFlowTab`/`patchFlowNodeStatus` do not exist yet.

- [ ] **Step 3: Add `FlowTab` to `pane-types.ts`**

In `src/types/pane-types.ts`, add alongside `RunnerTab`:

```typescript
export interface FlowTab extends BaseTab {
  tabType: 'flow';
  collectionName: string | null;
  flowName: string | null;
  nodes: import('@/lib/tauri-api').FlowNode[];
  edges: import('@/lib/tauri-api').FlowEdge[];
  nodeStatus: Record<string, import('@/lib/tauri-api').FlowNodeStatus>;
  runState: 'idle' | 'running' | 'done';
}

export function isFlowTab(tab: Tab): tab is FlowTab {
  return tab.tabType === 'flow';
}
```

Add `| FlowTab` to the `Tab` union (alongside `| RunnerTab`).

- [ ] **Step 4: Add pane-store actions**

In `src/stores/pane-store.ts`, add the action type declarations alongside
the existing `openRunnerTab`/`toggleRunnerEntry`/`startRun` declarations:

```typescript
openFlowTab: (collectionName: string | null, flowName?: string) => Promise<void>;
updateFlowNodes: (tabId: string, nodes: FlowNode[]) => void;
updateFlowEdges: (tabId: string, edges: FlowEdge[]) => void;
patchFlowNodeStatus: (tabId: string, nodeId: string, status: FlowNodeStatus) => void;
```

Import `FlowTab`, `isFlowTab` from `@/types/pane-types` and `getFlow`,
`type Flow`, `type FlowNode`, `type FlowEdge`, `type FlowNodeStatus` from
`@/lib/tauri-api` at the top of the file, alongside the existing imports.
Implement the actions alongside `openRunnerTab`:

```typescript
async openFlowTab(collectionName, flowName) {
  let nodes: FlowNode[] = [];
  let edges: FlowEdge[] = [];
  let resolvedFlowName: string | null = flowName ?? null;
  if (collectionName && flowName) {
    try {
      const flow: Flow = await getFlow(collectionName, flowName);
      nodes = flow.nodes;
      edges = flow.edges;
    } catch (err) {
      console.error('[pane-store] openFlowTab: failed to load flow', err);
      resolvedFlowName = null;
    }
  }
  const tab: FlowTab = {
    id: crypto.randomUUID(),
    title: resolvedFlowName ? `Flow: ${resolvedFlowName}` : 'Flow',
    isDirty: false,
    tabType: 'flow',
    collectionName,
    flowName: resolvedFlowName,
    nodes,
    edges,
    nodeStatus: {},
    runState: 'idle',
  };
  get().openTab(tab);
},

updateFlowNodes(tabId, nodes) {
  set({
    root: updateTabInTree(get().root, tabId, (tab) =>
      isFlowTab(tab) ? { ...tab, nodes, isDirty: true } : tab,
    ),
  });
},

updateFlowEdges(tabId, edges) {
  set({
    root: updateTabInTree(get().root, tabId, (tab) =>
      isFlowTab(tab) ? { ...tab, edges, isDirty: true } : tab,
    ),
  });
},

patchFlowNodeStatus(tabId, nodeId, status) {
  set({
    root: updateTabInTree(get().root, tabId, (tab) => {
      if (!isFlowTab(tab)) return tab;
      if (!tab.nodes.some((n) => n.id === nodeId)) return tab;
      return { ...tab, nodeStatus: { ...tab.nodeStatus, [nodeId]: status } };
    }),
  });
},
```

- [ ] **Step 5: Create the `FlowPane` stub and register it**

Create `src/components/flow/FlowPane.tsx` — a picker-only stub (the real
canvas arrives in Plan 09; this deliberately renders only the "choose a
collection and flow" state, following `RunnerPane`'s picker branch
structurally):

```tsx
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { type CollectionSummary, listCollections, listFlows } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';

export function FlowPane({ tab }: { tab: FlowTab; groupId: string }) {
  const openFlowTab = usePaneStore((s) => s.openFlowTab);
  const closeTab = usePaneStore((s) => s.closeTab);
  const [collections, setCollections] = useState<CollectionSummary[]>([]);
  const [selectedCollection, setSelectedCollection] = useState('');
  const [flowNames, setFlowNames] = useState<string[]>([]);

  useEffect(() => {
    if (tab.flowName === null) {
      void listCollections().then(setCollections);
    }
  }, [tab.flowName]);

  useEffect(() => {
    if (!selectedCollection) {
      setFlowNames([]);
      return;
    }
    void listFlows(selectedCollection)
      .then(setFlowNames)
      .catch((err) => console.error('[FlowPane] failed to list flows', err));
  }, [selectedCollection]);

  if (tab.flowName === null) {
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
        <Select
          disabled={!selectedCollection}
          onValueChange={(name) => {
            closeTab(tab.id);
            void openFlowTab(selectedCollection, name);
          }}
        >
          <SelectTrigger className='w-64' aria-label='Flow'>
            <SelectValue placeholder='Select flow' />
          </SelectTrigger>
          <SelectContent>
            {flowNames.map((name) => (
              <SelectItem key={name} value={name}>
                {name}
              </SelectItem>
            ))}
          </SelectContent>
        </Select>
      </div>
    );
  }

  // Real canvas rendering (React Flow) arrives in Plan 09 — this stub
  // confirms the tab/data plumbing works end to end first.
  return (
    <div className='flex h-full items-center justify-center text-sm text-muted-foreground'>
      Loaded flow &quot;{tab.flowName}&quot; with {tab.nodes.length} node(s) — canvas rendering
      lands in Plan 09.
    </div>
  );
}
```

In `src/components/panes/EditorGroup.tsx`, add the lazy import alongside the
existing `RunnerPane` one:

```typescript
const FlowPane = lazy(() =>
  import('@/components/flow/FlowPane').then((m) => ({ default: m.FlowPane })),
);
```

Import `isFlowTab` from `@/types/pane-types` alongside the existing
`isRunnerTab` import, and add a branch alongside the existing
`isRunnerTab(activeTab)` conditional:

```tsx
) : isFlowTab(activeTab) ? (
  <FlowPane tab={activeTab} groupId={node.groupId} />
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `yarn vitest run src/stores/__tests__/pane-store.test.ts -t "Flow tab actions"`
Expected: PASS — 5 tests.

Run: `yarn tsc --noEmit`
Expected: succeeds.

- [ ] **Step 7: Commit**

```bash
git add src/types/pane-types.ts src/stores/pane-store.ts src/stores/__tests__/pane-store.test.ts src/components/panes/EditorGroup.tsx src/components/panes/BreadcrumbBar.tsx src/components/flow/FlowPane.tsx
git commit -m "feat(frontend): add FlowTab and pane-store actions"
```

---

## Next Plan

[Plan 09: Canvas + node components](2026-09-27-flow-visual-workflow-builder-plan-09-frontend-canvas-nodes.md) —
replaces `FlowPane`'s loaded-state stub with the real React Flow canvas and
the `RequestNode`/`InputNode`/`OutputNode` components.

## Post-Implementation Review

Before starting Plan 09, dispatch a subagent (Agent tool,
`subagent_type: "general-purpose"`, `model: "opus"`) with this brief:

> Review every file this plan created or modified:
> `src/lib/tauri-api.ts`, `src/lib/queries/__tests__/flow-api.test.ts`,
> `src/types/pane-types.ts`, `src/stores/pane-store.ts`,
> `src/stores/__tests__/pane-store.test.ts`,
> `src/components/panes/EditorGroup.tsx`, `src/components/flow/FlowPane.tsx`.
>
> Check for:
> 1. Gaps versus this plan's stated interfaces — do the Flow domain types
>    match the plan index's locked contract (accounting for this plan's
>    documented deviations: `tabType`/`collectionName` instead of
>    `type`/`collectionRoot`, types living in `tauri-api.ts` instead of a
>    separate `flow-types.ts`), and does `FlowTab` expose everything Plan 09
>    and Plan 10 will need (`nodes`, `edges`, `nodeStatus`, `runState`)?
> 2. Code quality versus this plan's Review Focus section (picker-state
>    fallback on a failed `getFlow`, safe no-op for an unknown node id in
>    `patchFlowNodeStatus`, no cross-tab state contamination).
> 3. DDD/frontend guardrail conformance — Zustand actions never fully
>    destructure store state, no raw HTML form elements introduced,
>    `lucide-react`-only icons (none needed yet in this plan, but check
>    nothing slipped in).
>
> You have explicit authority to apply fixes directly for anything you find.
> After fixing, re-run `yarn vitest run src/lib/queries/__tests__/flow-api.test.ts`,
> `yarn vitest run src/stores/__tests__/pane-store.test.ts -t "Flow tab actions"`,
> and `yarn tsc --noEmit`, and confirm they still pass. Report what you found
> and fixed.

Only proceed to Plan 09 once this review comes back clean (or its fixes are
applied and re-verified).
