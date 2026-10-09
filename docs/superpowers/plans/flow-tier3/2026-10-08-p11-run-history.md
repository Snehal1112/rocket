# Flow Run History Implementation Plan

> **Execute this plan:** P11. Before starting it, make sure these are merged to main: P2 (already merged) and decision D3 (in-memory history) confirmed. After it is merged, the next plan to execute is P12. Status and the full order are in `00-plan-index.md`.

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep the last five finished runs of a flow tab in memory and let the user view an earlier run's node results on the canvas, with a banner that says the graph may have changed.

**Architecture:** Each finished run becomes a `FlowRunRecord` (the run's `FlowLastRun` result plus a snapshot of the tab's `nodeStatus` and `nodeDetail`) stored in `FlowTab.runHistory`, newest first, capped at five. `FlowTab.viewedRunId` (null means live) picks which maps the canvas, the properties panel and the result strip show. The record is built in `FlowPane` from the same callback plan P2 uses for the result strip, so no second toolbar callback exists. Frontend only, in memory only.

**Tech Stack:** React, TypeScript, Zustand (`pane-store`), Vitest and Testing Library, shadcn `Select` and `Button`, lucide-react.

**Spec:** Roadmap item F-36 in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` (section P11) and the plan index (`00-plan-index.md`, plan P11).

**Depends on P2 (and so on P1), assumed merged.** This plan edits code as P2 leaves it: `FlowTab.lastRun`, `FlowLastRun`, `FlowRunResult`, the store action `setFlowRunResult`, the toolbar prop `onRunResult`, `FlowPane.handleRunResult`, `RunResultStrip` and the strip block in `FlowPane`'s JSX. Anchors are given by function or prop name because line numbers move.

**Decision assumed (open decision D3 in the plan index):** run history is in memory only, keeps the last 5 runs per tab, and keeps full detail for all of them (no slimming of older records). The human can change this before the plan runs. Nothing is written to disk: step values, logs and exchanges are masked on a best-effort basis (see `crates/rocket-app/CLAUDE.md`), so persisting them would widen the leak surface. Memory is bounded at five runs per open flow tab; an `exchange` can be up to 256 KB each way per request node, so a follow-up could strip `exchange` and `logs` from records older than the newest two if memory ever matters.

## Global Constraints

- shadcn/ui primitives only, `lucide-react` icons only, no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`.
- Zustand: never fully destructure store state at component top level. Use narrow selectors.
- No Rust changes, no new IPC command, nothing persisted to disk.
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format and are created through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check` (if it only reports import order or formatting, run `yarn lint` and `yarn format`, review the diff, and re-check), and the targeted `yarn test <pattern>` listed in the task.
- Only one implementer at a time touches `FlowPane.tsx` and `FlowToolbar.tsx`.
- Not in scope: persisting history, comparing two runs, partial-run (`partial`) records (reserved for plans P19 and P20), exporting a run (plan P15).

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. A record must be a frozen snapshot. The next run wipes the live `nodeStatus` and `nodeDetail`, and a late summary for an older run must not be recorded with a newer run's maps. Tests pinned in Task 1 (`buildRunRecord`, store).
2. The same run can be recorded twice (the counts-only finished event, then the timed summary). History must hold one entry per run id, replaced in place, and still cap at five. Tests pinned in Task 1.
3. Starting a new run must return the view to live and disable the selector while a run is active, in the store and in the UI. Tests pinned in Task 1 (store) and Task 3 (UI).
4. The viewed record can name nodes that no longer exist, and its failed-node chip must be disabled then. An unknown `viewedRunId` must behave like live. Tests pinned in Task 3.
5. History must never leak into what Save writes or mark the tab dirty. Viewing a run must not change `isDirty`. Tests pinned in Task 1 (`flowPayloadFromTab`) and Task 3 (Save while viewing).

---

## File Structure

| File | Responsibility |
|---|---|
| `src/types/pane-types.ts` (modify) | `FlowRunRecord`, `FlowTab.runHistory`, `FlowTab.viewedRunId`, `FlowLastRun.environmentName`. |
| `src/lib/flow-run-history.ts` (new) | `MAX_RUN_HISTORY`, `appendRunRecord`, `buildRunRecord`, `formatClock`, `runRecordLabel`. |
| `src/stores/pane-store.ts` (modify) | `recordFlowRun`, `setViewedFlowRun`. `setFlowRunState('running')` resets `viewedRunId`. |
| `src/components/flow/FlowToolbar.tsx` (modify) | Reports the run's environment, and reports an `error` result when a started run rejects. |
| `src/components/flow/RunHistorySelect.tsx` (new) | shadcn `Select` listing the recorded runs. |
| `src/components/flow/ViewedRunBanner.tsx` (new) | "Viewing the run from HH:MM:SS" note with a Back to latest button. |
| `src/components/flow/FlowPane.tsx` (modify) | Records finished runs, computes the shown maps, renders the selector and banner. |

Placement note: the selector goes into `FlowPane`'s top-right container directly before `<FlowToolbar`, not into `FlowToolbar.tsx`. `FlowToolbar` is already the busiest file in this series, and the selector needs no toolbar state. If plan P12 (issue count) has merged, both sit before `<FlowToolbar`; their order does not matter.

Existing tests to know: `src/components/flow/__tests__/FlowToolbar.test.tsx` (`renderToolbar`, `started`, `resolveRun`, the `describe('run result')` block from P2), `src/components/flow/__tests__/FlowPane.runStrip.test.tsx` (P2: mock setup, `Harness`, `seed`), `src/stores/__tests__/pane-store.flowRun.test.ts` (P2: store test setup).

---

### Task 1: Types, pure helpers and store actions

**Files:**
- Modify: `src/types/pane-types.ts`
- Create: `src/lib/flow-run-history.ts`
- Create: `src/lib/__tests__/flow-run-history.test.ts`
- Modify: `src/stores/pane-store.ts` (imports, interface, `setFlowRunState`, two new actions)
- Create: `src/stores/__tests__/pane-store.flowHistory.test.ts`

**Interfaces:**
- Consumes: `FlowLastRun`, `FlowTab.lastRun` and `setFlowRunState` from P2; `flowPayloadFromTab` from P1.
- Produces: `FlowRunRecord = { runId: string; finishedAt: number; environmentName: string | null; result: FlowLastRun; nodeStatus: Record<string, FlowNodeStatus>; nodeDetail: Record<string, FlowNodeDetail> }`.
- Produces: `FlowLastRun.environmentName?: string | null`, `FlowTab.runHistory?: FlowRunRecord[]` (newest first), `FlowTab.viewedRunId?: string | null`.
- Produces: `appendRunRecord(history: FlowRunRecord[] | undefined, record: FlowRunRecord): FlowRunRecord[]`.
- Produces: `buildRunRecord(tab: FlowTab | null, runId: string, finishedAt: number): FlowRunRecord | null`.
- Produces: `formatClock(ms: number): string` (local time, `HH:MM:SS`) and `runRecordLabel(record: FlowRunRecord): string`.
- Produces: store actions `recordFlowRun(tabId, record)` and `setViewedFlowRun(tabId, runId | null)`.

- [ ] **Step 1: Write the failing test for the helpers**

Create `src/lib/__tests__/flow-run-history.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { FlowLastRun, FlowRunRecord, FlowTab } from '@/types/pane-types';
import { flowPayloadFromTab } from '../flow-save';
import {
  appendRunRecord,
  buildRunRecord,
  formatClock,
  MAX_RUN_HISTORY,
  runRecordLabel,
} from '../flow-run-history';

const result = (over: Partial<FlowLastRun> = {}): FlowLastRun => ({
  runId: 'run-1',
  stoppedReason: 'completed',
  totalMs: 1000,
  failedCount: 0,
  skippedCount: 0,
  ...over,
});

const record = (runId: string, over: Partial<FlowRunRecord> = {}): FlowRunRecord => ({
  runId,
  finishedAt: new Date(2026, 9, 8, 14, 2, 11).getTime(),
  environmentName: null,
  result: result({ runId }),
  nodeStatus: {},
  nodeDetail: {},
  ...over,
});

const tab = (over: Partial<FlowTab> = {}): FlowTab => ({
  id: 't1',
  title: 'Flow: my-flow',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [],
  edges: [],
  nodeStatus: { a: 'success' },
  nodeDetail: { a: { durationMs: 5 } },
  runState: 'running',
  runId: 'run-1',
  lastRun: result({ environmentName: 'dev' }),
  ...over,
});

describe('appendRunRecord', () => {
  it('puts the newest run first and starts from no history', () => {
    expect(appendRunRecord(undefined, record('r1')).map((r) => r.runId)).toEqual(['r1']);
    const next = appendRunRecord([record('r1')], record('r2'));
    expect(next.map((r) => r.runId)).toEqual(['r2', 'r1']);
  });

  it('keeps only the newest MAX_RUN_HISTORY runs', () => {
    let history: FlowRunRecord[] | undefined;
    for (let i = 1; i <= MAX_RUN_HISTORY + 2; i += 1) {
      history = appendRunRecord(history, record(`r${i}`));
    }
    expect(history).toHaveLength(MAX_RUN_HISTORY);
    expect(history?.[0].runId).toBe(`r${MAX_RUN_HISTORY + 2}`);
    expect(history?.some((r) => r.runId === 'r1')).toBe(false);
  });

  it('replaces a record of the same run in place instead of adding a second one', () => {
    const history = [record('r3'), record('r2'), record('r1')];
    const replaced = record('r2', { environmentName: 'prod' });
    const next = appendRunRecord(history, replaced);
    expect(next.map((r) => r.runId)).toEqual(['r3', 'r2', 'r1']);
    expect(next[1].environmentName).toBe('prod');
  });
});

describe('buildRunRecord', () => {
  it('snapshots the tab maps and takes the environment from the result', () => {
    const t = tab();
    const built = buildRunRecord(t, 'run-1', 123);
    expect(built).toEqual({
      runId: 'run-1',
      finishedAt: 123,
      environmentName: 'dev',
      result: t.lastRun,
      nodeStatus: { a: 'success' },
      nodeDetail: { a: { durationMs: 5 } },
    });
  });

  it('uses empty maps and a null environment when the tab has none', () => {
    const built = buildRunRecord(tab({ nodeDetail: undefined, lastRun: result() }), 'run-1', 1);
    expect(built?.nodeDetail).toEqual({});
    expect(built?.environmentName).toBeNull();
  });

  it('returns null without a tab, without a result, or when the run ids do not match', () => {
    expect(buildRunRecord(null, 'run-1', 1)).toBeNull();
    expect(buildRunRecord(tab({ lastRun: undefined }), 'run-1', 1)).toBeNull();
    // A late result of an older run must not capture a newer run's maps.
    expect(buildRunRecord(tab({ runId: 'run-2' }), 'run-1', 1)).toBeNull();
    expect(buildRunRecord(tab({ lastRun: result({ runId: 'run-0' }) }), 'run-1', 1)).toBeNull();
  });

  it('still records when the tab never learned the run id', () => {
    expect(buildRunRecord(tab({ runId: undefined }), 'run-1', 1)?.runId).toBe('run-1');
  });
});

describe('formatClock', () => {
  it('formats local time as HH:MM:SS with zero padding', () => {
    expect(formatClock(new Date(2026, 9, 8, 14, 2, 11).getTime())).toBe('14:02:11');
    expect(formatClock(new Date(2026, 9, 8, 3, 4, 5).getTime())).toBe('03:04:05');
  });
});

describe('runRecordLabel', () => {
  const clock = formatClock(record('x').finishedAt);

  it('says completed for a clean run and counts failures otherwise', () => {
    expect(runRecordLabel(record('r1'))).toBe(`${clock} · completed`);
    expect(runRecordLabel(record('r1', { result: result({ failedCount: 2 }) }))).toBe(
      `${clock} · 2 failed`,
    );
  });

  it('labels cancelled, error and unknown stop reasons', () => {
    expect(runRecordLabel(record('r1', { result: result({ stoppedReason: 'cancelled' }) }))).toBe(
      `${clock} · cancelled`,
    );
    expect(runRecordLabel(record('r1', { result: result({ stoppedReason: 'error' }) }))).toBe(
      `${clock} · error`,
    );
    expect(runRecordLabel(record('r1', { result: result({ stoppedReason: 'timeout' }) }))).toBe(
      `${clock} · timeout`,
    );
  });

  it('appends the environment when there is one', () => {
    expect(runRecordLabel(record('r1', { environmentName: 'staging' }))).toBe(
      `${clock} · completed · staging`,
    );
  });
});

describe('what Save writes', () => {
  it('never includes history, the viewed run or the last result', () => {
    const t = tab({
      runHistory: [record('r1')],
      viewedRunId: 'r1',
      nodes: [{ id: 'a', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } }],
    });
    expect(flowPayloadFromTab(t)).toEqual({
      collection: 'demo',
      flow: { name: 'my-flow', nodes: t.nodes, edges: [] },
    });
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn test src/lib/__tests__/flow-run-history.test.ts`
Expected: FAIL, cannot resolve `../flow-run-history`.

- [ ] **Step 3: Add the types**

In `src/types/pane-types.ts`:

1. In `FlowLastRun` (added by P2), after `skippedCount: number;`, add:

```ts
  /** Environment the run used. Absent when the client could not tell. */
  environmentName?: string | null;
```

2. Directly below the `FlowLastRun` interface, add:

```ts
/** One finished run kept in memory so its results can be viewed again. */
export interface FlowRunRecord {
  runId: string;
  /** Epoch ms when this client recorded the finish. */
  finishedAt: number;
  environmentName: string | null;
  result: FlowLastRun;
  /** Frozen copy of the tab's node results at the end of the run. */
  nodeStatus: Record<string, import('@/lib/tauri-api').FlowNodeStatus>;
  nodeDetail: Record<string, FlowNodeDetail>;
}
```

3. In `FlowTab`, after the `lastRun?: FlowLastRun;` field, add:

```ts
  /** The last few finished runs, newest first. In memory only, never saved. */
  runHistory?: FlowRunRecord[];
  /** The past run whose results are shown. Null or absent means the live results. */
  viewedRunId?: string | null;
```

- [ ] **Step 4: Write the helpers**

Create `src/lib/flow-run-history.ts`:

```ts
import type { FlowRunRecord, FlowTab } from '@/types/pane-types';

export const MAX_RUN_HISTORY = 5;

// Adds a record, newest first. A record of a run that is already in the list
// replaces it in place, because the finished event and the final summary can
// both report the same run.
export function appendRunRecord(
  history: FlowRunRecord[] | undefined,
  record: FlowRunRecord,
): FlowRunRecord[] {
  const current = history ?? [];
  const index = current.findIndex((r) => r.runId === record.runId);
  if (index >= 0) return current.map((r, i) => (i === index ? record : r));
  return [record, ...current].slice(0, MAX_RUN_HISTORY);
}

// Snapshots the tab's results for one finished run. The store never mutates
// these maps, it replaces them, so sharing the references is a safe copy.
// Returns null unless the tab's result is that run's and the tab has not moved
// on to another run.
export function buildRunRecord(
  tab: FlowTab | null,
  runId: string,
  finishedAt: number,
): FlowRunRecord | null {
  if (!tab?.lastRun || tab.lastRun.runId !== runId) return null;
  if (tab.runId !== undefined && tab.runId !== runId) return null;
  return {
    runId,
    finishedAt,
    environmentName: tab.lastRun.environmentName ?? null,
    result: tab.lastRun,
    nodeStatus: tab.nodeStatus,
    nodeDetail: tab.nodeDetail ?? {},
  };
}

const pad = (n: number) => String(n).padStart(2, '0');

// Local time as HH:MM:SS. Written by hand so the format never depends on the locale.
export function formatClock(ms: number): string {
  const d = new Date(ms);
  return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function outcomeLabel(record: FlowRunRecord): string {
  const { stoppedReason, failedCount } = record.result;
  if (stoppedReason !== 'completed') return stoppedReason;
  return failedCount > 0 ? `${failedCount} failed` : 'completed';
}

// Text of a selector entry, such as "14:02:11 · 2 failed · staging".
export function runRecordLabel(record: FlowRunRecord): string {
  const parts = [formatClock(record.finishedAt), outcomeLabel(record)];
  if (record.environmentName) parts.push(record.environmentName);
  return parts.join(' · ');
}
```

- [ ] **Step 5: Run the helper tests to verify they pass**

Run: `yarn test src/lib/__tests__/flow-run-history.test.ts`
Expected: PASS (all tests, including the `flowPayloadFromTab` one, which needs P1's `src/lib/flow-save.ts`).

- [ ] **Step 6: Write the failing store tests**

Create `src/stores/__tests__/pane-store.flowHistory.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { MAX_RUN_HISTORY } from '@/lib/flow-run-history';
import { findTabInTree } from '@/lib/pane-utils';
import { type FlowRunRecord, type FlowTab, isFlowTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), getFlow: vi.fn(), endAgentSession: vi.fn() };
});

const tabId = 'flow-hist-tab';

const flowTab = (id = tabId): FlowTab => ({
  id,
  title: 'Flow: my-flow',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [{ id: 'a', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } }],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
});

const record = (runId: string): FlowRunRecord => ({
  runId,
  finishedAt: 1000,
  environmentName: null,
  result: { runId, stoppedReason: 'completed', totalMs: 10, failedCount: 0, skippedCount: 0 },
  nodeStatus: { a: 'success' },
  nodeDetail: { a: { durationMs: 3 } },
});

function stored(id = tabId): FlowTab {
  const found = findTabInTree(usePaneStore.getState().root, id);
  if (!found || !isFlowTab(found.tab)) throw new Error('Expected the flow tab');
  return found.tab;
}

describe('pane-store flow run history', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(flowTab());
  });

  it('recordFlowRun keeps the newest run first', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().recordFlowRun(tabId, record('r2'));
    expect(stored().runHistory?.map((r) => r.runId)).toEqual(['r2', 'r1']);
  });

  it('recordFlowRun caps the history', () => {
    for (let i = 1; i <= MAX_RUN_HISTORY + 1; i += 1) {
      usePaneStore.getState().recordFlowRun(tabId, record(`r${i}`));
    }
    expect(stored().runHistory).toHaveLength(MAX_RUN_HISTORY);
  });

  it('recording the same run twice keeps one entry', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().recordFlowRun(tabId, { ...record('r1'), finishedAt: 2000 });
    expect(stored().runHistory).toHaveLength(1);
    expect(stored().runHistory?.[0].finishedAt).toBe(2000);
  });

  it('recording does not mark the tab dirty', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    expect(stored().isDirty).toBe(false);
  });

  it('a record keeps its results when the next run wipes the live ones', () => {
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-1');
    usePaneStore.getState().patchFlowNodeStatus(tabId, 'a', 'success', { durationMs: 3 });
    usePaneStore.getState().recordFlowRun(tabId, record('run-1'));
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-2');
    expect(stored().nodeStatus).toEqual({});
    expect(stored().runHistory?.[0].nodeStatus).toEqual({ a: 'success' });
    expect(stored().runHistory?.[0].nodeDetail).toEqual({ a: { durationMs: 3 } });
  });

  it('history belongs to one tab', () => {
    usePaneStore.getState().openTab(flowTab('flow-hist-other'));
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    expect(stored('flow-hist-other').runHistory).toBeUndefined();
  });

  it('recordFlowRun for an unknown tab does nothing', () => {
    usePaneStore.getState().recordFlowRun('missing', record('r1'));
    expect(stored().runHistory).toBeUndefined();
  });

  it('setViewedFlowRun shows a recorded run and null returns to live', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().setViewedFlowRun(tabId, 'r1');
    expect(stored().viewedRunId).toBe('r1');
    usePaneStore.getState().setViewedFlowRun(tabId, null);
    expect(stored().viewedRunId).toBeNull();
  });

  it('setViewedFlowRun ignores a run that is not in the history', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().setViewedFlowRun(tabId, 'nope');
    expect(stored().viewedRunId ?? null).toBeNull();
  });

  it('setViewedFlowRun does not switch the view while a run is active', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-2');
    usePaneStore.getState().setViewedFlowRun(tabId, 'r1');
    expect(stored().viewedRunId ?? null).toBeNull();
  });

  it('starting a run returns the view to live and keeps the history', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().setViewedFlowRun(tabId, 'r1');
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-2');
    expect(stored().viewedRunId).toBeNull();
    expect(stored().runHistory).toHaveLength(1);
  });

  it('viewing a run does not mark the tab dirty', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().setViewedFlowRun(tabId, 'r1');
    expect(stored().isDirty).toBe(false);
  });
});
```

- [ ] **Step 7: Run them to verify they fail**

Run: `yarn test src/stores/__tests__/pane-store.flowHistory.test.ts`
Expected: FAIL (`recordFlowRun is not a function`).

- [ ] **Step 8: Add the store actions**

In `src/stores/pane-store.ts`:

1. Add `FlowRunRecord,` to the `@/types/pane-types` type import (after `FlowNodeDetail,`), and add next to the other `@/lib/` imports:

```ts
import { appendRunRecord } from '@/lib/flow-run-history';
```

2. In the `PaneStore` interface, after `setFlowRunResult`, add:

```ts
  /** Adds a finished run to the tab's history (newest first, capped). */
  recordFlowRun: (tabId: string, record: FlowRunRecord) => void;
  /** Shows a recorded run's results. Null returns to the live results. */
  setViewedFlowRun: (tabId: string, runId: string | null) => void;
```

3. In `setFlowRunState`, add `viewedRunId: null` to the `'running'` branch, so it reads:

```ts
        if (runState === 'running') {
          return {
            ...tab,
            runState,
            runId,
            nodeStatus: {},
            nodeDetail: {},
            lastRun: undefined,
            viewedRunId: null,
          };
        }
```

4. After the `setFlowRunResult` implementation, add:

```ts
  recordFlowRun(tabId, record) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) =>
        isFlowTab(tab) ? { ...tab, runHistory: appendRunRecord(tab.runHistory, record) } : tab,
      ),
    });
  },

  setViewedFlowRun(tabId, runId) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isFlowTab(tab)) return tab;
        if (runId === null) return { ...tab, viewedRunId: null };
        // The live results are being written during a run, so the view stays live.
        if (tab.runState === 'running') return tab;
        if (!tab.runHistory?.some((r) => r.runId === runId)) return tab;
        return { ...tab, viewedRunId: runId };
      }),
    });
  },
```

- [ ] **Step 9: Run the store tests to verify they pass**

Run: `yarn test src/stores/__tests__ src/lib/__tests__/flow-run-history.test.ts`
Expected: PASS, including the P2 store tests and the existing `pane-store.test.ts`.

- [ ] **Step 10: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/types/pane-types.ts src/lib/flow-run-history.ts src/lib/__tests__/flow-run-history.test.ts src/stores/pane-store.ts src/stores/__tests__/pane-store.flowHistory.test.ts`
Suggested subject: `feat(flow): keep the last five runs of a flow tab in memory`.

---

### Task 2: Record finished runs

**Files:**
- Modify: `src/components/flow/FlowToolbar.tsx` (`handleRun`)
- Modify: `src/components/flow/__tests__/FlowToolbar.test.tsx` (extend `describe('run result')`)
- Modify: `src/components/flow/FlowPane.tsx` (selector, `handleRunResult`)
- Create: `src/components/flow/__tests__/FlowPane.runHistoryRecord.test.tsx`

**Interfaces:**
- Consumes: `buildRunRecord`, `recordFlowRun`, `FlowLastRun.environmentName` from Task 1; `onRunResult`, `summarizeRun` from P2.
- Produces: results reported by the toolbar now carry `environmentName` (the value the toolbar had when Run was clicked). A run that started and then rejected reports `{ stoppedReason: 'error', failedCount: 0, skippedCount: 0 }`.

- [ ] **Step 1: Write the failing toolbar tests**

In `src/components/flow/__tests__/FlowToolbar.test.tsx`, inside the `describe('run result', ...)` block that plan P2 added, append:

```tsx
    it('reports the environment the run was started with, even if it changes mid-run', async () => {
      const onRunResult = vi.fn();
      const element = (environmentName: string | null) => (
        <FlowToolbar
          collection='my-collection'
          flowName='my-flow'
          environmentName={environmentName}
          onPatchStatus={onPatchStatus}
          onRunStateChange={onRunStateChange}
          onRunResult={onRunResult}
        />
      );
      const view = render(element('staging'));
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(startedHandler).toBeDefined());
      started('run-1');
      view.rerender(element('prod'));
      resolveRun({ runId: 'run-1', stoppedReason: 'completed', steps: [] });
      await waitFor(() => expect(onRunResult).toHaveBeenCalledTimes(1));
      expect(onRunResult).toHaveBeenCalledWith(
        expect.objectContaining({ runId: 'run-1', environmentName: 'staging' }),
      );
    });

    it('reports an error result when a run that had started is rejected', async () => {
      const onRunResult = vi.fn();
      let rejectRun: (reason: unknown) => void = () => undefined;
      vi.mocked(tauriApi.runFlow).mockImplementationOnce(
        () =>
          new Promise((_, reject) => {
            rejectRun = reject;
          }),
      );
      renderToolbar({ onRunResult });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(startedHandler).toBeDefined());
      started('run-1');
      rejectRun('socket closed');
      await waitFor(() => expect(onRunResult).toHaveBeenCalledTimes(1));
      expect(onRunResult).toHaveBeenCalledWith(
        expect.objectContaining({
          runId: 'run-1',
          stoppedReason: 'error',
          failedCount: 0,
          skippedCount: 0,
          totalMs: expect.any(Number),
        }),
      );
      expect(toast.error).toHaveBeenCalled();
    });
```

The P2 test `reports nothing when the run is rejected before it starts` stays as it is: no run id was seen, so nothing is reported.

Add the `FlowToolbar` import if the file does not already have it: it does (`import { FlowToolbar } from '../FlowToolbar';`).

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: the two new tests FAIL; everything else passes.

- [ ] **Step 3: Implement the toolbar changes**

In `src/components/flow/FlowToolbar.tsx`, in `handleRun`:

1. Directly after `isStartingRef.current = true;`, add:

```tsx
    // The environment can change while the run is going, so keep the one it started with.
    const runEnvironment = environmentName;
```

2. After the line `let runId: string | null = null;` (and P2's `let startedAt = ...;`), add:

```tsx
    // A function, because TypeScript narrows `runId` to null in the catch block
    // below, while the event handlers assign it later.
    const currentRunId = (): string | null => runId;
```

3. Replace P2's line `onRunResult?.(summarizeRun(summary, Math.round(performance.now() - startedAt)));` with:

```tsx
      onRunResult?.({
        ...summarizeRun(summary, Math.round(performance.now() - startedAt)),
        environmentName: runEnvironment,
      });
```

4. In the `catch (err)` block, after the `toast.error(...)` line and before `onRunStateChange('done');`, add:

```tsx
      // A run that had started leaves a result, so its partial results stay viewable.
      const startedRunId = currentRunId();
      if (startedRunId !== null) {
        onRunResult?.({
          runId: startedRunId,
          stoppedReason: 'error',
          totalMs: Math.round(performance.now() - startedAt),
          failedCount: 0,
          skippedCount: 0,
          environmentName: runEnvironment,
        });
      }
```

- [ ] **Step 4: Run to verify the toolbar tests pass**

Run: `yarn test src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: PASS, including all P2 tests (they use `expect.objectContaining`, so the extra `environmentName` key is fine).

- [ ] **Step 5: Write the failing FlowPane recording test**

Create `src/components/flow/__tests__/FlowPane.runHistoryRecord.test.tsx`:

```tsx
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findTabInTree } from '@/lib/pane-utils';
import {
  type FlowRunSummary,
  listCollections,
  listFlows,
  onFlowRunFinished,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepProgress,
  onFlowStepStarted,
  runFlow,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listFlows: vi.fn(),
    runFlow: vi.fn(),
    onFlowRunStarted: vi.fn(),
    onFlowRunFinished: vi.fn(),
    onFlowStepStarted: vi.fn(),
    onFlowStepCompleted: vi.fn(),
    onFlowStepProgress: vi.fn(),
  };
});
vi.mock('@/lib/execute-request', () => ({ getActiveGlobalEnvName: vi.fn() }));
vi.mock('@/lib/flow-auth-preflight', () => ({ collectFlowAuthTokens: vi.fn(async () => ({})) }));
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));

const tabId = 'flow-record-1';

const baseTab: FlowTab = {
  id: tabId,
  tabType: 'flow',
  title: 'Flow: rec',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'rec',
  nodes: [{ id: 'out1', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } }],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

function storedTab(): FlowTab {
  const found = findTabInTree(usePaneStore.getState().root, tabId);
  if (!found || !isFlowTab(found.tab)) throw new Error('Expected the flow tab');
  return found.tab;
}

function Harness() {
  const tab = usePaneStore((s) => {
    if (s.root.type !== 'leaf') return null;
    const t = s.root.tabs.find((x) => x.id === tabId);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

const summaryFor = (runId: string, status: 'success' | 'failed'): FlowRunSummary => ({
  runId,
  stoppedReason: 'completed',
  steps: [
    {
      nodeId: 'out1',
      status,
      statusCode: null,
      durationMs: 4,
      error: status === 'failed' ? 'boom' : null,
      value: status === 'success' ? '"ok"' : null,
    },
  ],
});

let startedHandler: Parameters<typeof onFlowRunStarted>[0] | undefined;
let resolveRun: (summary: FlowRunSummary) => void = () => undefined;

// Clicks Run, lets the toolbar subscribe, fires the run-started event, then ends the run.
async function runOnce(runId: string, status: 'success' | 'failed') {
  vi.mocked(runFlow).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        resolveRun = resolve;
      }),
  );
  startedHandler = undefined;
  await waitFor(() => expect(screen.getByRole('button', { name: 'Run' })).toBeEnabled());
  await userEvent.click(screen.getByRole('button', { name: 'Run' }));
  await waitFor(() => expect(startedHandler).toBeDefined());
  act(() => {
    startedHandler?.({
      type: 'flowRunStarted',
      run_id: runId,
      flow_name: 'rec',
      collection: 'demo',
      total_nodes: 1,
    });
  });
  await act(async () => {
    resolveRun(summaryFor(runId, status));
  });
}

describe('FlowPane run history recording', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    const unlisten = async () => () => undefined;
    vi.mocked(onFlowRunStarted).mockImplementation(async (handler) => {
      startedHandler = handler;
      return () => undefined;
    });
    vi.mocked(onFlowRunFinished).mockImplementation(unlisten);
    vi.mocked(onFlowStepStarted).mockImplementation(unlisten);
    vi.mocked(onFlowStepCompleted).mockImplementation(unlisten);
    vi.mocked(onFlowStepProgress).mockImplementation(unlisten);
  });

  it('records a snapshot of each finished run, newest first', async () => {
    render(<Harness />);
    await runOnce('run-1', 'failed');
    await waitFor(() => expect(storedTab().runHistory).toHaveLength(1));
    await runOnce('run-2', 'success');
    await waitFor(() => expect(storedTab().runHistory).toHaveLength(2));

    const [newest, older] = storedTab().runHistory ?? [];
    expect(newest.runId).toBe('run-2');
    expect(newest.nodeStatus).toEqual({ out1: 'success' });
    expect(older.runId).toBe('run-1');
    expect(older.nodeStatus).toEqual({ out1: 'failed' });
    expect(older.nodeDetail.out1.error).toBe('boom');
    expect(older.result.failedCount).toBe(1);
  });

  it('does not record a run that was rejected before it started', async () => {
    vi.mocked(runFlow).mockRejectedValueOnce('Invalid input: bad graph');
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(runFlow).toHaveBeenCalled());
    await waitFor(() => expect(storedTab().runState).toBe('done'));
    expect(storedTab().runHistory ?? []).toHaveLength(0);
  });
});
```

- [ ] **Step 6: Run it to verify it fails**

Run: `yarn test src/components/flow/__tests__/FlowPane.runHistoryRecord.test.tsx`
Expected: the first test FAILS (`runHistory` stays undefined); the second passes.

- [ ] **Step 7: Record from `FlowPane`**

In `src/components/flow/FlowPane.tsx`:

1. Add the import next to the other `@/lib/` imports:

```tsx
import { buildRunRecord } from '@/lib/flow-run-history';
```

2. After the `setFlowRunResult` selector (added by P2), add:

```tsx
  const recordFlowRun = usePaneStore((s) => s.recordFlowRun);
```

3. Replace P2's `handleRunResult` with:

```tsx
  // Adds the failed node's label while the node still exists, so the strip
  // keeps naming it after a rename or delete. Then snapshots the run for the
  // history selector.
  const handleRunResult = useCallback(
    (result: FlowRunResult) => {
      const failed = result.failedNodeId
        ? latestFlowTab()?.nodes.find((n) => n.id === result.failedNodeId)
        : undefined;
      const failedLabel = failed ? failed.kind.label || failed.id : undefined;
      setFlowRunResult(tabId, { ...result, ...(failedLabel ? { failedLabel } : {}) });
      // The store has just applied the result, so the tab now holds this run's maps.
      const record = buildRunRecord(latestFlowTab(), result.runId, Date.now());
      if (record) recordFlowRun(tabId, record);
    },
    [latestFlowTab, recordFlowRun, setFlowRunResult, tabId],
  );
```

- [ ] **Step 8: Run to verify the tests pass**

Run: `yarn test src/components/flow src/stores src/lib/__tests__/flow-run-history.test.ts`
Expected: PASS.

- [ ] **Step 9: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/FlowToolbar.tsx src/components/flow/__tests__/FlowToolbar.test.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowPane.runHistoryRecord.test.tsx`
Suggested subject: `feat(flow): record each finished run in the tab history`.

---

### Task 3: Selector, viewed-run banner and edge cases

**Files:**
- Create: `src/components/flow/RunHistorySelect.tsx`
- Create: `src/components/flow/ViewedRunBanner.tsx`
- Modify: `src/components/flow/FlowPane.tsx` (shown maps, JSX)
- Create: `src/components/flow/__tests__/FlowPane.runHistory.test.tsx`

**Interfaces:**
- Consumes: `FlowRunRecord`, `runRecordLabel`, `formatClock`, `setViewedFlowRun` from Task 1.
- Produces: `<RunHistorySelect history={FlowRunRecord[]} viewedRunId={string | null} disabled={boolean} onChange={(runId: string | null) => void} />`. Renders nothing for fewer than two records. Choosing the newest record reports `null` (live).
- Produces: `<ViewedRunBanner record={FlowRunRecord} onBack={() => void} />`.

- [ ] **Step 1: Write the failing FlowPane tests**

Create `src/components/flow/__tests__/FlowPane.runHistory.test.tsx`:

```tsx
import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  listCollections,
  listFlows,
  onFlowRunFinished,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepProgress,
  onFlowStepStarted,
  saveFlow,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowRunRecord, type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listFlows: vi.fn(),
    saveFlow: vi.fn(),
    onFlowRunStarted: vi.fn(),
    onFlowRunFinished: vi.fn(),
    onFlowStepStarted: vi.fn(),
    onFlowStepCompleted: vi.fn(),
    onFlowStepProgress: vi.fn(),
  };
});
vi.mock('@/lib/execute-request', () => ({ getActiveGlobalEnvName: vi.fn() }));
vi.mock('@/lib/flow-auth-preflight', () => ({ collectFlowAuthTokens: vi.fn(async () => ({})) }));
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));
vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => (
    <input
      aria-label={props['aria-label']}
      value={props.value}
      onChange={(e) => props.onChange(e.target.value)}
    />
  ),
}));

// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;

// jsdom has no DOMMatrixReadOnly, which React Flow reads when it re-measures nodes.
vi.stubGlobal(
  'DOMMatrixReadOnly',
  class {
    m22 = 1;
  },
);

const tabId = 'flow-hist-ui-1';
const at = (h: number, m: number, s: number) => new Date(2026, 9, 8, h, m, s).getTime();

const newest: FlowRunRecord = {
  runId: 'run-2',
  finishedAt: at(14, 5, 0),
  environmentName: 'dev',
  result: { runId: 'run-2', stoppedReason: 'completed', totalMs: 900, failedCount: 0, skippedCount: 0 },
  nodeStatus: { out1: 'success' },
  nodeDetail: { out1: { value: '"ok"' } },
};

const older: FlowRunRecord = {
  runId: 'run-1',
  finishedAt: at(14, 1, 0),
  environmentName: null,
  result: {
    runId: 'run-1',
    stoppedReason: 'completed',
    totalMs: 1500,
    failedNodeId: 'out1',
    failedLabel: 'Result',
    failedCount: 1,
    skippedCount: 0,
  },
  // `ghost` is a node that was deleted after the run.
  nodeStatus: { out1: 'failed', ghost: 'failed' },
  nodeDetail: { out1: { error: 'boom' }, ghost: { error: 'gone' } },
};

const baseTab: FlowTab = {
  id: tabId,
  tabType: 'flow',
  title: 'Flow: hist',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'hist',
  nodes: [{ id: 'out1', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } }],
  edges: [],
  nodeStatus: newest.nodeStatus,
  nodeDetail: newest.nodeDetail,
  runState: 'done',
  runId: 'run-2',
  lastRun: newest.result,
  runHistory: [newest, older],
};

function seed(over: Partial<FlowTab> = {}) {
  usePaneStore.getState().reset();
  usePaneStore.getState().openTab({ ...baseTab, ...over });
}

function storedTab(): FlowTab {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected a leaf');
  const tab = root.tabs.find((t) => t.id === tabId);
  if (!tab || !isFlowTab(tab)) throw new Error('Expected the flow tab');
  return tab;
}

function Harness() {
  const tab = usePaneStore((s) => {
    if (s.root.type !== 'leaf') return null;
    const t = s.root.tabs.find((x) => x.id === tabId);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

const cardStatus = () => screen.getByTestId('output-node-card').getAttribute('data-status');

async function choose(optionName: RegExp) {
  await userEvent.click(screen.getByRole('combobox', { name: 'Run history' }));
  await userEvent.click(await screen.findByRole('option', { name: optionName }));
}

describe('FlowPane run history', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    const unlisten = async () => () => undefined;
    vi.mocked(onFlowRunStarted).mockImplementation(unlisten);
    vi.mocked(onFlowRunFinished).mockImplementation(unlisten);
    vi.mocked(onFlowStepStarted).mockImplementation(unlisten);
    vi.mocked(onFlowStepCompleted).mockImplementation(unlisten);
    vi.mocked(onFlowStepProgress).mockImplementation(unlisten);
  });

  it('shows no selector until there are two recorded runs', () => {
    seed({ runHistory: [newest] });
    render(<Harness />);
    expect(screen.queryByRole('combobox', { name: 'Run history' })).not.toBeInTheDocument();
  });

  it('lists the runs and shows the latest one live', async () => {
    seed();
    render(<Harness />);
    expect(cardStatus()).toBe('success');
    await userEvent.click(screen.getByRole('combobox', { name: 'Run history' }));
    expect(await screen.findByRole('option', { name: /Latest/ })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /1 failed/ })).toBeInTheDocument();
  });

  it('switches the canvas, panel result and strip to a past run and back', async () => {
    seed();
    render(<Harness />);
    await choose(/1 failed/);
    expect(cardStatus()).toBe('failed');
    expect(screen.getByTestId('viewed-run-banner')).toHaveTextContent(
      /Viewing the run from 14:01:00\. The graph may have changed since\./,
    );
    expect(screen.getByTestId('run-result-strip')).toHaveTextContent(
      'Run finished with 1 failed in 1.5 s',
    );
    expect(storedTab().isDirty).toBe(false);

    await choose(/Latest/);
    expect(cardStatus()).toBe('success');
    expect(screen.queryByTestId('viewed-run-banner')).not.toBeInTheDocument();
  });

  it('Back to latest returns to the live results', async () => {
    seed({ viewedRunId: 'run-1' });
    render(<Harness />);
    expect(cardStatus()).toBe('failed');
    await userEvent.click(screen.getByRole('button', { name: 'Back to latest' }));
    expect(cardStatus()).toBe('success');
    expect(storedTab().viewedRunId).toBeNull();
  });

  it('a new run returns to live and disables the selector', () => {
    seed({ viewedRunId: 'run-1' });
    render(<Harness />);
    expect(screen.getByTestId('viewed-run-banner')).toBeInTheDocument();
    act(() => {
      usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-3');
    });
    expect(screen.queryByTestId('viewed-run-banner')).not.toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Run history' })).toBeDisabled();
  });

  it('treats a viewed run that is not in the history as the live run', () => {
    seed({ viewedRunId: 'nope' });
    render(<Harness />);
    expect(screen.queryByTestId('viewed-run-banner')).not.toBeInTheDocument();
    expect(cardStatus()).toBe('success');
  });

  it('ignores results of nodes that no longer exist and disables the failed-node chip', () => {
    seed({ viewedRunId: 'run-1' });
    render(<Harness />);
    expect(screen.getByRole('button', { name: 'Select failed node Result' })).toBeEnabled();
    act(() => {
      usePaneStore.getState().updateFlowNodes(tabId, []);
    });
    expect(screen.getByTestId('viewed-run-banner')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Select failed node Result' })).toBeDisabled();
  });

  it('Save writes only the graph while a past run is shown', async () => {
    seed({ viewedRunId: 'run-1' });
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(saveFlow).toHaveBeenCalledWith('demo', {
      name: 'hist',
      nodes: baseTab.nodes,
      edges: [],
    });
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/components/flow/__tests__/FlowPane.runHistory.test.tsx`
Expected: FAIL (no `Run history` combobox).

- [ ] **Step 3: Create the selector**

Create `src/components/flow/RunHistorySelect.tsx`:

```tsx
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { runRecordLabel } from '@/lib/flow-run-history';
import type { FlowRunRecord } from '@/types/pane-types';

interface RunHistorySelectProps {
  // Newest first.
  history: FlowRunRecord[];
  // The shown past run, or null for the live results.
  viewedRunId: string | null;
  // True while a run is active, because the live results are still being written.
  disabled: boolean;
  // Receives a past run's id, or null when the newest run is chosen.
  onChange: (runId: string | null) => void;
}

// Lists the recorded runs. The newest entry is the live one, so choosing it
// reports null instead of its id.
export function RunHistorySelect({ history, viewedRunId, disabled, onChange }: RunHistorySelectProps) {
  if (history.length < 2) return null;
  const latestId = history[0].runId;
  const value =
    viewedRunId !== null && history.some((r) => r.runId === viewedRunId) ? viewedRunId : latestId;
  return (
    <Select
      value={value}
      disabled={disabled}
      onValueChange={(runId) => onChange(runId === latestId ? null : runId)}
    >
      <SelectTrigger className='nokey h-8 w-48 text-xs' aria-label='Run history'>
        <SelectValue />
      </SelectTrigger>
      <SelectContent>
        {history.map((record, index) => (
          <SelectItem key={record.runId} value={record.runId} className='text-xs'>
            {index === 0 ? `Latest · ${runRecordLabel(record)}` : runRecordLabel(record)}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
```

- [ ] **Step 4: Create the banner**

Create `src/components/flow/ViewedRunBanner.tsx`:

```tsx
import { History } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { formatClock } from '@/lib/flow-run-history';
import type { FlowRunRecord } from '@/types/pane-types';

interface ViewedRunBannerProps {
  record: FlowRunRecord;
  onBack: () => void;
}

// Shown while a past run's results replace the live ones on the canvas.
export function ViewedRunBanner({ record, onBack }: ViewedRunBannerProps) {
  return (
    <div
      role='note'
      data-testid='viewed-run-banner'
      className='nokey flex max-w-full items-center gap-2 rounded-md border border-amber-500/50 bg-amber-500/10 px-2.5 py-1 text-xs'
    >
      <History className='h-3.5 w-3.5 shrink-0 text-amber-600' aria-hidden='true' />
      <span className='truncate'>
        Viewing the run from {formatClock(record.finishedAt)}. The graph may have changed since.
      </span>
      <Button type='button' size='sm' variant='outline' className='h-6 px-2 text-xs' onClick={onBack}>
        Back to latest
      </Button>
    </div>
  );
}
```

- [ ] **Step 5: Wire them into `FlowPane`**

In `src/components/flow/FlowPane.tsx`:

1. Add the imports:

```tsx
import { RunHistorySelect } from './RunHistorySelect';
import { ViewedRunBanner } from './ViewedRunBanner';
```

2. Next to the `recordFlowRun` selector, add:

```tsx
  const setViewedFlowRun = usePaneStore((s) => s.setViewedFlowRun);
```

3. Directly after the line `const panelNode = panelNodeId ? tab.nodes.find((n) => n.id === panelNodeId) : undefined;`, add:

```tsx
  // A past run chosen in the selector replaces the live results on screen. An id
  // that is no longer in the history counts as live. The toolbar and the run
  // announcer keep reading the live maps.
  const viewedRecord = tab.viewedRunId
    ? ((tab.runHistory ?? []).find((r) => r.runId === tab.viewedRunId) ?? null)
    : null;
  const shownStatus = viewedRecord ? viewedRecord.nodeStatus : tab.nodeStatus;
  const shownDetail = viewedRecord ? viewedRecord.nodeDetail : tab.nodeDetail;
  const shownRun = viewedRecord ? viewedRecord.result : tab.lastRun;
```

4. In the top-right container, directly before `<FlowToolbar`, add:

```tsx
            <RunHistorySelect
              history={tab.runHistory ?? []}
              viewedRunId={viewedRecord ? viewedRecord.runId : null}
              disabled={tab.runState === 'running'}
              onChange={(runId) => setViewedFlowRun(tab.id, runId)}
            />
```

5. Replace P2's strip block with:

```tsx
          {shownRun && (
            <div className='absolute top-12 right-2 z-10 max-w-[60%]'>
              <RunResultStrip
                result={shownRun}
                canSelectFailed={tab.nodes.some((n) => n.id === shownRun.failedNodeId)}
                onSelectFailed={handleOpenNodeOnLastRun}
              />
            </div>
          )}
          {viewedRecord && (
            <div className='absolute top-14 left-3 z-10 max-w-[45%]'>
              <ViewedRunBanner
                record={viewedRecord}
                onBack={() => setViewedFlowRun(tab.id, null)}
              />
            </div>
          )}
```

6. On `<FlowCanvas`, change `nodeStatus={tab.nodeStatus}` to `nodeStatus={shownStatus}` and `nodeDetail={tab.nodeDetail}` to `nodeDetail={shownDetail}`.

7. On `<NodePropertiesPanel`, change these four props:

```tsx
              status={shownStatus[panelNode.id] ?? 'idle'}
              detail={shownDetail?.[panelNode.id]}
              nodeStatus={shownStatus}
              nodeDetail={shownDetail}
```

Do not change `<FlowToolbar`'s props: `tabRunState` and `tabRunId` stay on the live tab.

- [ ] **Step 6: Run to verify the tests pass**

Run: `yarn test src/components/flow src/stores src/lib/__tests__/flow-run-history.test.ts`
Expected: PASS. If a Radix Select option query times out in jsdom, confirm the polyfill lines at the top of the new test file are present; they are the same ones other Select tests use.

- [ ] **Step 7: Full gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/stores src/lib/__tests__/flow-run-history.test.ts src/lib/__tests__/flow-run-result.test.ts`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/RunHistorySelect.tsx src/components/flow/ViewedRunBanner.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowPane.runHistory.test.tsx`
Suggested subject: `feat(flow): view an earlier run from the history selector`.

---

## Self-Review

- **Spec coverage (F-36):** in-memory last-5 history with full detail (Task 1, decision D3); `recordFlowRun` called when a run finishes, including a started run that rejects with `stoppedReason: 'error'` (Task 2); shadcn `Select` with entries like "14:02:11 · completed · 2 failed" style labels (Task 3 and `runRecordLabel`); `FlowPane` shows the viewed record's maps for the canvas, panel and strip, banner "Viewing the run from HH:MM:SS. The graph may have changed since.", nodes that no longer exist ignored, starting a run resets the view (Tasks 1 and 3); not persisted (Global Constraints, Task 1 `flowPayloadFromTab` test); history survives hiding a tab and a collection switch because it lives on the tab object, which `switchCollection` snapshots whole.
- **Placeholders:** none. Every code step shows code.
- **Type consistency:** `FlowRunRecord` is used with the same fields in the types, `flow-run-history.ts`, the store and the three test files. `buildRunRecord(tab, runId, finishedAt)` has the same argument order in the helper, its tests and `handleRunResult`. `setViewedFlowRun(tabId, string | null)` matches the interface, the implementation, the selector `onChange` and the banner `onBack`. The selector accessible name is `Run history` in the component and the tests; the banner testid is `viewed-run-banner` in the component and the tests.
- **Review Focus coverage:** item 1 is the `buildRunRecord` mismatch test and the store "keeps its results when the next run wipes the live ones" test; item 2 is `appendRunRecord` replace-in-place and the store "same run twice" test; item 3 is the store "starting a run returns the view to live" and "does not switch the view while a run is active" tests plus the UI "a new run returns to live and disables the selector" test; item 4 is "ignores results of nodes that no longer exist" and "treats a viewed run that is not in the history as the live run"; item 5 is the `flowPayloadFromTab` test, the "does not mark the tab dirty" tests and "Save writes only the graph".
- **Deviation from the design notes:** the notes call `recordFlowRun` from the toolbar with a record that carries `partial`. This plan builds the record in `FlowPane` from the tab's own maps (so the toolbar needs no map access and no `detailFromStep` duplicate) and leaves `partial` to plans P19 and P20.
- **Interface for later plans:** plan P15 (export) can read `tab.runHistory` and `tab.lastRun`. Plan P16's announcer must keep reading the live `tab.nodeStatus`, not the shown maps.
