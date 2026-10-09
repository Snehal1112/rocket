# Flow Run-Result Strip Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** After a flow run ends, show a one-line result strip (outcome, total time, failed and skipped counts, and a clickable failed-node chip) that survives hiding and reopening the tab.

**Architecture:** The result lives on the tab as `FlowTab.lastRun`, written by a new store action `setFlowRunResult` and cleared when the next run starts. `FlowToolbar` builds the result from the final `run_flow` summary (or, for a run an earlier toolbar mount started, from the `flow-run-finished` event) and hands it to `FlowPane` through a new `onRunResult` prop. `FlowPane` adds the failed node's label and renders a new `RunResultStrip`. Frontend only, no backend change.

**Tech Stack:** React, TypeScript, Zustand (`pane-store`), Vitest and Testing Library, shadcn `Button`, lucide-react.

**Spec:** Roadmap item F-31 in `.claude/flow-roadmap.md`. Design notes: `docs/superpowers/plans/flow-tier3/01-design-notes.md` (section P2) and the plan index (`00-plan-index.md`, plan P2).

**Depends on P1 (assumed merged).** All edits to `FlowToolbar.tsx` and `FlowPane.tsx` are written against the code after P1: `FlowToolbar` already has the `tabId` prop, the `rocket:flow-run` listener and a disabled-when-idle Stop button; `FlowPane` already imports `flowPayloadFromTab`, mounts `FlowSaveShortcut`, passes `tabId={tab.id}` to `FlowToolbar` and toasts in `handleBeforeRun`. Line numbers in this plan come from HEAD b047bbc6 (before P1) and have shifted, so every edit is anchored by function or prop name.

**Deviations from the design notes (checked against the code):**
- The strip uses `role='group'` with `aria-label='Last run result'`, not `role='status'`. A live region that is inserted into the DOM together with its text is not reliably announced, and plan P16 adds an always-mounted announcer that already says "Run finished: ...". Two regions would read the result twice.
- Cancelled runs report no failed node and zero counts, in both the summary path and the event path. The backend reports cancelled nodes as `failed` with error "cancelled" (roadmap F-11), so the counts would otherwise be wrong.
- `skippedCount` counts nodes skipped because an upstream node failed. Nodes skipped because their branch was not taken are normal and are excluded (`skipReason === 'branch_not_taken'`).

## Global Constraints

- shadcn/ui primitives only, `lucide-react` icons only, no raw `<button>`, `<input>`, `<dialog>`, `<select>`, `<form>`.
- Zustand: never fully destructure store state at component top level. Use narrow selectors.
- No Rust changes in this plan. No new IPC command.
- Code comments are short full sentences that end with a punctuation mark.
- Commits use conventional commit format and are created through the `dev-workflow-skills:1-git-commit` skill. Stage explicit paths only. Never `git add -A`, `--all` or `.`.
- Gates before each commit: `yarn tsc --noEmit`, `yarn check` (if it only reports import order or formatting, run `yarn lint` and `yarn format`, review the diff, and re-check), and the targeted `yarn test <pattern>` listed in the task.
- Only one implementer at a time touches `FlowPane.tsx` and `FlowToolbar.tsx`.
- Not in scope: a total-time field from the backend, run history (plan P11 builds on `lastRun`), a Re-run button, per-node timing chips (plan P8).

## Review Focus

Failure modes the spec implies but the obvious tests would miss, most likely first:

1. A cancelled run must not blame a node. The backend reports cancelled nodes as failed with error "cancelled". Test pinned in Task 1 (helpers) and Task 2 (toolbar).
2. A toolbar remounted mid-run must update the result from `flow-run-finished` for its own run id only, and a later richer summary for the same run must win over the counts-only result. Tests pinned in Task 1 (`mergeRunResult`) and Task 2 (toolbar).
3. Starting a new run must clear the old strip, and a late result from an older run must not overwrite a newer run. Tests pinned in Task 1 (store).
4. After a run, the failed node may be deleted or renamed. The chip must show the label captured at finish time and be disabled when the node is gone. Test pinned in Task 3.
5. A run that is rejected before it starts must show no strip and must not call `onRunResult`. Test pinned in Task 2.

---

## File Structure

| File | Responsibility |
|---|---|
| `src/types/pane-types.ts` (modify) | `FlowLastRun` type and `FlowTab.lastRun`. |
| `src/lib/flow-run-result.ts` (new) | Pure helpers: `summarizeRun`, `resultFromFinishedEvent`, `mergeRunResult`, `formatRunDuration`, `FlowRunResult` type. |
| `src/stores/pane-store.ts` (modify) | `setFlowRunResult` action. `setFlowRunState('running')` clears `lastRun`. |
| `src/components/flow/FlowToolbar.tsx` (modify) | `onRunResult` prop, wall-clock timing, `flow-run-finished` subscription in the resume effect. |
| `src/components/flow/RunResultStrip.tsx` (new) | The strip: icon, headline, counts, failed-node chip. |
| `src/components/flow/FlowPane.tsx` (modify) | `handleRunResult` (adds the failed label), `handleOpenNodeOnLastRun`, renders the strip. |

Existing tests to know: `src/components/flow/__tests__/FlowToolbar.test.tsx` (`renderToolbar`, `started`, `resolveRun`, the `tauriApi` mock list), `src/components/flow/__tests__/FlowPane.properties.test.tsx` (the `Harness` that re-renders `FlowPane` from the store, jsdom polyfills), `src/stores/__tests__/pane-store.test.ts` (`openFlowTab`, `setFlowRunState` tests).

---

### Task 1: `FlowTab.lastRun`, pure helpers and the store action

**Files:**
- Modify: `src/types/pane-types.ts` (after `FlowNodeDetail`, and `FlowTab` near line 187)
- Create: `src/lib/flow-run-result.ts`
- Create: `src/lib/__tests__/flow-run-result.test.ts`
- Modify: `src/stores/pane-store.ts` (imports near lines 17-55, interface near line 291, `setFlowRunState` near line 969)
- Create: `src/stores/__tests__/pane-store.flowRun.test.ts`

**Interfaces:**
- Produces: `FlowLastRun` (in `pane-types.ts`):
  `{ runId: string; stoppedReason: string; totalMs: number | null; failedNodeId?: string; failedLabel?: string; failedCount: number; skippedCount: number }`.
- Produces: `FlowRunResult = Omit<FlowLastRun, 'failedLabel'>` (what the toolbar reports; `FlowPane` adds the label).
- Produces: `summarizeRun(summary: FlowRunSummary, totalMs: number | null): FlowRunResult`.
- Produces: `resultFromFinishedEvent(event: FlowRunFinishedEvent): FlowRunResult`.
- Produces: `mergeRunResult(prev: FlowLastRun | undefined, next: FlowLastRun): FlowLastRun`.
- Produces: `formatRunDuration(ms: number): string`.
- Produces: store action `setFlowRunResult(tabId: string, lastRun: FlowLastRun | undefined): void`.

- [ ] **Step 1: Write the failing test for the helpers**

Create `src/lib/__tests__/flow-run-result.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import type { FlowRunFinishedEvent, FlowRunSummary, FlowStepResult } from '@/lib/tauri-api';
import type { FlowLastRun } from '@/types/pane-types';
import {
  formatRunDuration,
  mergeRunResult,
  resultFromFinishedEvent,
  summarizeRun,
} from '../flow-run-result';

const step = (
  nodeId: string,
  status: FlowStepResult['status'],
  over: Partial<FlowStepResult> = {},
): FlowStepResult => ({
  nodeId,
  status,
  statusCode: null,
  durationMs: null,
  error: null,
  value: null,
  ...over,
});

const summary = (
  steps: FlowStepResult[],
  stoppedReason = 'completed',
): FlowRunSummary => ({ runId: 'run-1', steps, stoppedReason });

describe('summarizeRun', () => {
  it('reports a clean run with no failed node', () => {
    const result = summarizeRun(summary([step('a', 'success'), step('b', 'success')]), 1200);
    expect(result).toEqual({
      runId: 'run-1',
      stoppedReason: 'completed',
      totalMs: 1200,
      failedCount: 0,
      skippedCount: 0,
    });
    expect(result).not.toHaveProperty('failedNodeId');
  });

  it('names the first failed step and counts every failure', () => {
    const result = summarizeRun(
      summary([
        step('a', 'success'),
        step('b', 'failed', { error: 'boom' }),
        step('c', 'failed', { error: 'later' }),
      ]),
      10,
    );
    expect(result.failedNodeId).toBe('b');
    expect(result.failedCount).toBe(2);
  });

  it('counts upstream skips, leaves out not-taken branches, and treats a missing reason as upstream', () => {
    const result = summarizeRun(
      summary([
        step('a', 'failed', { error: 'x' }),
        step('b', 'skipped', { skipReason: 'upstream_failed' }),
        step('c', 'skipped'),
        step('d', 'skipped', { skipReason: 'branch_not_taken' }),
      ]),
      null,
    );
    expect(result.skippedCount).toBe(2);
    expect(result.totalMs).toBeNull();
  });

  it('blames no node and counts nothing for a cancelled run', () => {
    const result = summarizeRun(
      summary(
        [step('a', 'success'), step('b', 'failed', { error: 'cancelled' }), step('c', 'skipped')],
        'cancelled',
      ),
      500,
    );
    expect(result).toEqual({
      runId: 'run-1',
      stoppedReason: 'cancelled',
      totalMs: 500,
      failedCount: 0,
      skippedCount: 0,
    });
  });
});

const finished = (over: Partial<FlowRunFinishedEvent> = {}): FlowRunFinishedEvent => ({
  type: 'flowRunFinished',
  run_id: 'run-9',
  stopped_reason: 'completed',
  node_count: 6,
  failed_count: 1,
  skipped_count: 3,
  not_taken_count: 2,
  ...over,
});

describe('resultFromFinishedEvent', () => {
  it('uses the counts only, with no timing and no failed node', () => {
    expect(resultFromFinishedEvent(finished())).toEqual({
      runId: 'run-9',
      stoppedReason: 'completed',
      totalMs: null,
      failedCount: 1,
      skippedCount: 1,
    });
  });

  it('treats a missing not_taken_count as zero and never goes negative', () => {
    expect(resultFromFinishedEvent(finished({ not_taken_count: undefined })).skippedCount).toBe(3);
    expect(resultFromFinishedEvent(finished({ skipped_count: 1, not_taken_count: 4 })).skippedCount).toBe(0);
  });

  it('zeroes the counts of a cancelled run', () => {
    const result = resultFromFinishedEvent(finished({ stopped_reason: 'cancelled' }));
    expect(result.failedCount).toBe(0);
    expect(result.skippedCount).toBe(0);
  });
});

describe('mergeRunResult', () => {
  const timed: FlowLastRun = {
    runId: 'run-1',
    stoppedReason: 'completed',
    totalMs: 900,
    failedNodeId: 'b',
    failedLabel: 'Login',
    failedCount: 1,
    skippedCount: 0,
  };
  const countsOnly: FlowLastRun = {
    runId: 'run-1',
    stoppedReason: 'completed',
    totalMs: null,
    failedCount: 1,
    skippedCount: 0,
  };

  it('keeps the timed result when a counts-only one arrives for the same run', () => {
    expect(mergeRunResult(timed, countsOnly)).toBe(timed);
  });

  it('replaces a counts-only result with the timed one for the same run', () => {
    expect(mergeRunResult(countsOnly, timed)).toBe(timed);
  });

  it('takes the new result for a different run, or when there is none yet', () => {
    expect(mergeRunResult(timed, { ...countsOnly, runId: 'run-2' }).runId).toBe('run-2');
    expect(mergeRunResult(undefined, countsOnly)).toBe(countsOnly);
  });
});

describe('formatRunDuration', () => {
  it('formats milliseconds, seconds and minutes', () => {
    expect(formatRunDuration(0)).toBe('0 ms');
    expect(formatRunDuration(850)).toBe('850 ms');
    expect(formatRunDuration(2300)).toBe('2.3 s');
    expect(formatRunDuration(125_000)).toBe('2 m 5 s');
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `yarn test src/lib/__tests__/flow-run-result.test.ts`
Expected: FAIL, cannot resolve `../flow-run-result`.

- [ ] **Step 3: Add the type to `pane-types.ts`**

In `src/types/pane-types.ts`, directly above `export interface FlowTab extends BaseTab {`, add:

```ts
/** Outcome of the last finished run, shown in the run-result strip. */
export interface FlowLastRun {
  runId: string;
  /** `completed`, `cancelled`, `error`, or another backend reason. */
  stoppedReason: string;
  /** Wall-clock time in ms. Null when this client did not time the run. */
  totalMs: number | null;
  /** First node that failed. Absent for a clean or cancelled run. */
  failedNodeId?: string;
  /** Label of that node when the run ended, kept even if the node is renamed later. */
  failedLabel?: string;
  failedCount: number;
  /** Nodes skipped because an upstream node failed. Not-taken branches are excluded. */
  skippedCount: number;
}
```

and inside `FlowTab`, after `runId?: string;`, add:

```ts
  /** Result of the last finished run. Cleared when the next run starts. */
  lastRun?: FlowLastRun;
```

- [ ] **Step 4: Write the helpers**

Create `src/lib/flow-run-result.ts`:

```ts
import type { FlowRunFinishedEvent, FlowRunSummary } from '@/lib/tauri-api';
import type { FlowLastRun } from '@/types/pane-types';

/** What the toolbar reports. `FlowPane` adds the failed node's label. */
export type FlowRunResult = Omit<FlowLastRun, 'failedLabel'>;

const isCancelled = (stoppedReason: string) => stoppedReason === 'cancelled';

// A cancelled run reports its cancelled nodes as failed (roadmap F-11), so it
// names no failed node and carries no counts.
export function summarizeRun(summary: FlowRunSummary, totalMs: number | null): FlowRunResult {
  const base = { runId: summary.runId, stoppedReason: summary.stoppedReason, totalMs };
  if (isCancelled(summary.stoppedReason)) return { ...base, failedCount: 0, skippedCount: 0 };
  const failed = summary.steps.filter((s) => s.status === 'failed');
  const skipped = summary.steps.filter(
    (s) => s.status === 'skipped' && s.skipReason !== 'branch_not_taken',
  );
  const result: FlowRunResult = {
    ...base,
    failedCount: failed.length,
    skippedCount: skipped.length,
  };
  if (failed[0]) result.failedNodeId = failed[0].nodeId;
  return result;
}

// Used by a toolbar that only resumed the run. The event has counts, no steps
// and no timing.
export function resultFromFinishedEvent(event: FlowRunFinishedEvent): FlowRunResult {
  const base = { runId: event.run_id, stoppedReason: event.stopped_reason, totalMs: null };
  if (isCancelled(event.stopped_reason)) return { ...base, failedCount: 0, skippedCount: 0 };
  return {
    ...base,
    failedCount: event.failed_count,
    skippedCount: Math.max(0, event.skipped_count - (event.not_taken_count ?? 0)),
  };
}

// The summary and the finished event can both report the same run. The timed
// summary result is richer, so a counts-only result never replaces it.
export function mergeRunResult(prev: FlowLastRun | undefined, next: FlowLastRun): FlowLastRun {
  if (prev && prev.runId === next.runId && prev.totalMs !== null && next.totalMs === null) {
    return prev;
  }
  return next;
}

export function formatRunDuration(ms: number): string {
  if (ms < 1000) return `${Math.round(ms)} ms`;
  if (ms < 60_000) return `${(ms / 1000).toFixed(1)} s`;
  const total = Math.round(ms / 1000);
  return `${Math.floor(total / 60)} m ${total % 60} s`;
}
```

- [ ] **Step 5: Run the helper tests to verify they pass**

Run: `yarn test src/lib/__tests__/flow-run-result.test.ts`
Expected: PASS (all tests).

- [ ] **Step 6: Write the failing store tests**

Create `src/stores/__tests__/pane-store.flowRun.test.ts`:

```ts
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findTabInTree } from '@/lib/pane-utils';
import { type FlowLastRun, type FlowTab, isFlowTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), getFlow: vi.fn(), endAgentSession: vi.fn() };
});

const tabId = 'flow-run-tab';

const flowTab: FlowTab = {
  id: tabId,
  title: 'Flow: my-flow',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [{ id: 'a', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } }],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

const run = (over: Partial<FlowLastRun> = {}): FlowLastRun => ({
  runId: 'run-1',
  stoppedReason: 'completed',
  totalMs: 1000,
  failedCount: 0,
  skippedCount: 0,
  ...over,
});

function stored(): FlowTab {
  const found = findTabInTree(usePaneStore.getState().root, tabId);
  if (!found || !isFlowTab(found.tab)) throw new Error('Expected the flow tab');
  return found.tab;
}

describe('pane-store flow run result', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(flowTab);
  });

  it('setFlowRunResult stores the result on the tab', () => {
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-1');
    usePaneStore.getState().setFlowRunResult(tabId, run());
    expect(stored().lastRun).toEqual(run());
  });

  it('setFlowRunResult with undefined clears it', () => {
    usePaneStore.getState().setFlowRunResult(tabId, run());
    usePaneStore.getState().setFlowRunResult(tabId, undefined);
    expect(stored().lastRun).toBeUndefined();
  });

  it('a new run clears the previous result together with the node results', () => {
    usePaneStore.getState().setFlowRunResult(tabId, run());
    usePaneStore.getState().setFlowRunState(tabId, 'done', 'run-1');
    expect(stored().lastRun).toBeDefined();
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-2');
    expect(stored().lastRun).toBeUndefined();
  });

  it('finishing a run keeps the result', () => {
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-1');
    usePaneStore.getState().setFlowRunResult(tabId, run());
    usePaneStore.getState().setFlowRunState(tabId, 'done', 'run-1');
    expect(stored().lastRun?.runId).toBe('run-1');
  });

  it('ignores a result from an older run while a newer run is active', () => {
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-2');
    usePaneStore.getState().setFlowRunResult(tabId, run({ runId: 'run-1' }));
    expect(stored().lastRun).toBeUndefined();
  });

  it('keeps a timed result when a counts-only result for the same run arrives later', () => {
    usePaneStore.getState().setFlowRunResult(tabId, run({ failedNodeId: 'a', failedLabel: 'Out' }));
    usePaneStore.getState().setFlowRunResult(tabId, run({ totalMs: null }));
    expect(stored().lastRun?.totalMs).toBe(1000);
    expect(stored().lastRun?.failedLabel).toBe('Out');
  });

  it('is a no-op for an unknown tab id', () => {
    usePaneStore.getState().setFlowRunResult('missing', run());
    expect(stored().lastRun).toBeUndefined();
  });
});
```

- [ ] **Step 7: Run them to verify they fail**

Run: `yarn test src/stores/__tests__/pane-store.flowRun.test.ts`
Expected: FAIL (`setFlowRunResult is not a function`).

- [ ] **Step 8: Add the store action**

In `src/stores/pane-store.ts`:

1. Add `FlowLastRun,` to the `@/types/pane-types` type import (before `FlowNodeDetail,`), and add next to the other `@/lib/` imports:

```ts
import { mergeRunResult } from '@/lib/flow-run-result';
```

2. In the `PaneStore` interface, after the `setFlowRunState` line, add:

```ts
  /** Stores the finished run's result. Pass undefined to clear it. */
  setFlowRunResult: (tabId: string, lastRun: FlowLastRun | undefined) => void;
```

3. In `setFlowRunState`, change the `runState === 'running'` branch to:

```ts
        if (runState === 'running') {
          return { ...tab, runState, runId, nodeStatus: {}, nodeDetail: {}, lastRun: undefined };
        }
```

4. After the `setFlowRunState` implementation, add:

```ts
  setFlowRunResult(tabId, lastRun) {
    set({
      root: updateTabInTree(get().root, tabId, (tab) => {
        if (!isFlowTab(tab)) return tab;
        if (!lastRun) return { ...tab, lastRun: undefined };
        // A late result from an older run must not show during a newer run.
        if (tab.runState === 'running' && tab.runId !== undefined && tab.runId !== lastRun.runId) {
          return tab;
        }
        return { ...tab, lastRun: mergeRunResult(tab.lastRun, lastRun) };
      }),
    });
  },
```

- [ ] **Step 9: Run the store tests to verify they pass**

Run: `yarn test src/stores/__tests__/pane-store.flowRun.test.ts src/stores/__tests__/pane-store.test.ts`
Expected: PASS. The existing `setFlowRunState clears the last run results when a new run starts` test still passes.

- [ ] **Step 10: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/types/pane-types.ts src/lib/flow-run-result.ts src/lib/__tests__/flow-run-result.test.ts src/stores/pane-store.ts src/stores/__tests__/pane-store.flowRun.test.ts`
Suggested subject: `feat(flow): keep the last run result on the flow tab`.

---

### Task 2: Toolbar reports the result and recovers it after a remount

**Files:**
- Modify: `src/components/flow/FlowToolbar.tsx` (imports, props, refs, resume effect, `handleRun`)
- Modify: `src/components/flow/__tests__/FlowToolbar.test.tsx` (mock list, `beforeEach`, new describe)

**Interfaces:**
- Consumes: `summarizeRun`, `resultFromFinishedEvent`, `FlowRunResult` from Task 1; `onFlowRunFinished` from `@/lib/tauri-api` (exists, typed, unused today).
- Produces: `FlowToolbar` prop `onRunResult?: (result: FlowRunResult) => void`. Called once per finished run: after the final summary is applied and before `onRunStateChange('done', ...)`, or, in a resumed toolbar, when the run's `flow-run-finished` event arrives (followed by `onRunStateChange('done', runId)`).

- [ ] **Step 1: Write the failing toolbar tests**

In `src/components/flow/__tests__/FlowToolbar.test.tsx`:

1. Add `onFlowRunFinished: vi.fn(),` to the object returned by the `vi.mock('@/lib/tauri-api', ...)` factory (after `onFlowStepProgress: vi.fn(),`).

2. Below `let startedStepHandler ...`, add:

```tsx
let finishedHandler: Parameters<typeof tauriApi.onFlowRunFinished>[0] | undefined;
```

3. In `beforeEach`, add `finishedHandler = undefined;` next to the other resets, and, after the `onFlowStepCompleted` mock implementation, add:

```tsx
    vi.mocked(tauriApi.onFlowRunFinished).mockImplementation(async (h) => {
      finishedHandler = h;
      return () => {
        // Fake unlisten — no real Tauri listener to tear down in tests.
      };
    });
    vi.mocked(tauriApi.onFlowRunFinished).mockClear();
```

The `mockClear` after `mockImplementation` keeps the implementation and resets the call count, which the "does not subscribe" test below reads.

4. Inside `describe('FlowToolbar', ...)`, at the end, add:

```tsx
  describe('run result', () => {
    const base = { statusCode: null, durationMs: null, error: null, value: null };

    const finishedEvent = (
      over: Partial<tauriApi.FlowRunFinishedEvent> = {},
    ): tauriApi.FlowRunFinishedEvent => ({
      type: 'flowRunFinished',
      run_id: 'run-9',
      stopped_reason: 'completed',
      node_count: 3,
      failed_count: 1,
      skipped_count: 2,
      not_taken_count: 1,
      ...over,
    });

    it('reports the result after the final summary and before the run is marked done', async () => {
      const onRunResult = vi.fn();
      renderToolbar({ onRunResult });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(startedHandler).toBeDefined());
      started('run-1');
      resolveRun({
        runId: 'run-1',
        stoppedReason: 'completed',
        steps: [
          { ...base, nodeId: 'a', status: 'success' },
          { ...base, nodeId: 'b', status: 'failed', error: 'boom' },
          { ...base, nodeId: 'c', status: 'skipped', skipReason: 'upstream_failed' },
          { ...base, nodeId: 'd', status: 'skipped', skipReason: 'branch_not_taken' },
        ],
      });
      await waitFor(() => expect(onRunResult).toHaveBeenCalledTimes(1));
      expect(onRunResult).toHaveBeenCalledWith(
        expect.objectContaining({
          runId: 'run-1',
          stoppedReason: 'completed',
          failedNodeId: 'b',
          failedCount: 1,
          skippedCount: 1,
          totalMs: expect.any(Number),
        }),
      );
      expect(onRunResult.mock.calls[0][0].totalMs).toBeGreaterThanOrEqual(0);
      const doneIndex = onRunStateChange.mock.calls.findIndex((c) => c[0] === 'done');
      expect(onRunResult.mock.invocationCallOrder[0]).toBeLessThan(
        onRunStateChange.mock.invocationCallOrder[doneIndex],
      );
    });

    it('names no failed node for a cancelled run', async () => {
      const onRunResult = vi.fn();
      renderToolbar({ onRunResult });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(startedHandler).toBeDefined());
      started('run-1');
      resolveRun({
        runId: 'run-1',
        stoppedReason: 'cancelled',
        steps: [{ ...base, nodeId: 'b', status: 'failed', error: 'cancelled' }],
      });
      await waitFor(() => expect(onRunResult).toHaveBeenCalledTimes(1));
      const result = onRunResult.mock.calls[0][0];
      expect(result.stoppedReason).toBe('cancelled');
      expect(result.failedCount).toBe(0);
      expect(result).not.toHaveProperty('failedNodeId');
    });

    it('reports nothing when the run is rejected before it starts', async () => {
      const onRunResult = vi.fn();
      vi.mocked(tauriApi.runFlow).mockRejectedValue('Invalid input: bad graph');
      renderToolbar({ onRunResult });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(toast.error).toHaveBeenCalled());
      expect(onRunResult).not.toHaveBeenCalled();
    });

    it('a remounted toolbar reports the result from the finished event of its run', async () => {
      const onRunResult = vi.fn();
      renderToolbar({ onRunResult, tabRunState: 'running', tabRunId: 'run-9' });
      await waitFor(() => expect(finishedHandler).toBeDefined());
      finishedHandler?.(finishedEvent());
      expect(onRunResult).toHaveBeenCalledWith({
        runId: 'run-9',
        stoppedReason: 'completed',
        totalMs: null,
        failedCount: 1,
        skippedCount: 1,
      });
      expect(onRunStateChange).toHaveBeenCalledWith('done', 'run-9');
    });

    it('a remounted toolbar ignores the finished event of another run', async () => {
      const onRunResult = vi.fn();
      renderToolbar({ onRunResult, tabRunState: 'running', tabRunId: 'run-9' });
      await waitFor(() => expect(finishedHandler).toBeDefined());
      finishedHandler?.(finishedEvent({ run_id: 'run-other' }));
      expect(onRunResult).not.toHaveBeenCalled();
      expect(onRunStateChange).not.toHaveBeenCalled();
    });

    it('does not subscribe to the finished event when no run is being resumed', async () => {
      renderToolbar({ onRunResult: vi.fn() });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
      expect(tauriApi.onFlowRunFinished).not.toHaveBeenCalled();
    });
  });
```

Note: `toast` is mocked as `{ error, info }` in this file; the rejected-run test only reads `toast.error`.

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: the five `run result` tests that expect `onRunResult` calls FAIL; the "does not subscribe" test passes; every pre-existing test still passes (the new `onFlowRunFinished` mock is harmless).

- [ ] **Step 3: Implement the toolbar changes**

In `src/components/flow/FlowToolbar.tsx`:

1. Imports. Add `onFlowRunFinished,` to the `@/lib/tauri-api` import (before `onFlowRunStarted,`) and add below the `execute-request` import:

```tsx
import { type FlowRunResult, resultFromFinishedEvent, summarizeRun } from '@/lib/flow-run-result';
```

2. In `FlowToolbarProps`, after `onStepDebug`, add:

```tsx
  // Receives the outcome of a finished run: after the final summary is
  // applied, or, for a run this mount only resumed, when its finished event
  // arrives. Not called for a run that is rejected before it starts.
  onRunResult?: (result: FlowRunResult) => void;
```

and add `onRunResult,` to the destructured parameters (after `onStepDebug,`).

3. After the `onPatchProgressRef` lines, add:

```tsx
  const onRunResultRef = useRef(onRunResult);
  onRunResultRef.current = onRunResult;
  const onRunStateChangeRef = useRef(onRunStateChange);
  onRunStateChangeRef.current = onRunStateChange;
```

4. In the resume effect (the one guarded by `if (!resumedRunId) return;`), add a fourth subscription. Declare `let unlistenFinished: UnlistenFn | undefined;` next to the other three, add this block after the `onFlowStepProgress` block:

```tsx
    void onFlowRunFinished((event) => {
      if (event.run_id !== resumedRunId) return;
      // The mount that started the run applies the timed summary later, which
      // replaces this counts-only result.
      onRunResultRef.current?.(resultFromFinishedEvent(event));
      onRunStateChangeRef.current('done', event.run_id);
    }).then((fn) => {
      if (disposed) fn();
      else unlistenFinished = fn;
    });
```

and add `unlistenFinished?.();` to the cleanup function.

5. In `handleRun`, change the line `let runId: string | null = null;` block so the start time sits next to it:

```tsx
    let runId: string | null = null;
    // Wall-clock start. The run-started event moves it to the real start.
    let startedAt = performance.now();
```

In the `onFlowRunStarted` handler, after `runId = event.run_id;`, add:

```tsx
      startedAt = performance.now();
```

Then, in the `try` block, between the `for (const step of summary.steps) { ... }` loop and `onRunStateChange('done', summary.runId);`, add:

```tsx
      onRunResult?.(summarizeRun(summary, Math.round(performance.now() - startedAt)));
```

- [ ] **Step 4: Run to verify the tests pass**

Run: `yarn test src/components/flow/__tests__/FlowToolbar.test.tsx`
Expected: PASS, including every pre-existing test.

- [ ] **Step 5: Gates and commit**

Run: `yarn tsc --noEmit && yarn check`
Expected: no errors.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/FlowToolbar.tsx src/components/flow/__tests__/FlowToolbar.test.tsx`
Suggested subject: `feat(flow): report the run result from the toolbar`.

---

### Task 3: The strip UI and click-to-select

**Files:**
- Create: `src/components/flow/RunResultStrip.tsx`
- Modify: `src/components/flow/FlowPane.tsx` (imports, selector, two callbacks after `handleDeleteNode`, `onRunResult` prop on `<FlowToolbar`, strip JSX after the toolbar container)
- Create: `src/components/flow/__tests__/FlowPane.runStrip.test.tsx`

**Interfaces:**
- Consumes: `FlowRunResult`, `formatRunDuration` from Task 1; `setFlowRunResult` store action; `onRunResult` toolbar prop from Task 2.
- Produces: `<RunResultStrip result={FlowLastRun} canSelectFailed={boolean} onSelectFailed={(nodeId: string) => void} />`.

- [ ] **Step 1: Write the failing FlowPane tests**

Create `src/components/flow/__tests__/FlowPane.runStrip.test.tsx`:

```tsx
import { act, render, screen, waitFor } from '@testing-library/react';
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
  runFlow,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowLastRun, type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

// The Request editor pulls in Monaco, which jsdom cannot load.
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

// Radix menus call pointer-capture and scrollIntoView APIs that jsdom lacks.
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

// jsdom reports every rect as 0,0,0,0 and userEvent clicks at 0,0, so the resize
// handle would count as hit by every click. Park the handle away from the pointer.
const realGetRect = Element.prototype.getBoundingClientRect;
Element.prototype.getBoundingClientRect = function getRect() {
  if (this instanceof HTMLElement && this.dataset.slot === 'resizable-handle') {
    return new DOMRect(5000, 5000, 1, 100);
  }
  return realGetRect.call(this);
};

const tabId = 'flow-strip-1';

const baseTab: FlowTab = {
  id: tabId,
  tabType: 'flow',
  title: 'Flow: strip',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'strip',
  nodes: [
    {
      id: 'req1',
      position: { x: 0, y: 0 },
      kind: {
        kind: 'Request',
        label: 'Fetch',
        source: { type: 'Inline', request: { method: 'GET', url: '', headers: [] } },
      },
    },
    { id: 'out1', position: { x: 300, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
  ],
  edges: [],
  nodeStatus: {},
  runState: 'done',
};

const failedRun: FlowLastRun = {
  runId: 'run-1',
  stoppedReason: 'completed',
  totalMs: 2300,
  failedNodeId: 'req1',
  failedLabel: 'Fetch',
  failedCount: 1,
  skippedCount: 2,
};

function seed(over: Partial<FlowTab> = {}) {
  usePaneStore.getState().reset();
  usePaneStore.getState().openTab({ ...baseTab, ...over });
}

// FlowPane receives the tab as a prop. Re-render it from the store after each
// store change, the way PaneRenderer does in the app.
function Harness() {
  const tab = usePaneStore((s) => {
    if (s.root.type !== 'leaf') return null;
    const t = s.root.tabs.find((x) => x.id === tabId);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

// A tab that is running mounts a toolbar that subscribes to run events, so
// every listener needs a fake in both describes.
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listCollections).mockResolvedValue([]);
  vi.mocked(listFlows).mockResolvedValue([]);
  const unlisten = async () => () => undefined;
  vi.mocked(onFlowRunStarted).mockImplementation(unlisten);
  vi.mocked(onFlowRunFinished).mockImplementation(unlisten);
  vi.mocked(onFlowStepStarted).mockImplementation(unlisten);
  vi.mocked(onFlowStepCompleted).mockImplementation(unlisten);
  vi.mocked(onFlowStepProgress).mockImplementation(unlisten);
});

describe('FlowPane run-result strip', () => {
  it('shows no strip before any run', () => {
    seed();
    render(<Harness />);
    expect(screen.queryByTestId('run-result-strip')).not.toBeInTheDocument();
  });

  it('shows the outcome, time and counts of a failed run', () => {
    seed({ lastRun: failedRun });
    render(<Harness />);
    const strip = screen.getByTestId('run-result-strip');
    expect(strip).toHaveTextContent('Run finished with 1 failed in 2.3 s');
    expect(strip).toHaveTextContent('2 skipped');
    expect(screen.getByRole('button', { name: 'Select failed node Fetch' })).toBeEnabled();
  });

  it('shows a clean run without a failed-node chip', () => {
    seed({
      lastRun: { runId: 'run-2', stoppedReason: 'completed', totalMs: 850, failedCount: 0, skippedCount: 0 },
    });
    render(<Harness />);
    expect(screen.getByTestId('run-result-strip')).toHaveTextContent('Run completed in 850 ms');
    expect(screen.queryByRole('button', { name: /Select failed node/ })).not.toBeInTheDocument();
  });

  it('does not blame a node for a cancelled run', () => {
    seed({
      lastRun: { runId: 'run-3', stoppedReason: 'cancelled', totalMs: 1200, failedCount: 0, skippedCount: 0 },
    });
    render(<Harness />);
    expect(screen.getByTestId('run-result-strip')).toHaveTextContent('Run cancelled in 1.2 s');
    expect(screen.queryByRole('button', { name: /Select failed node/ })).not.toBeInTheDocument();
  });

  it('selecting the failed node opens its panel on the Last run tab', async () => {
    seed({ lastRun: failedRun });
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Select failed node Fetch' }));
    expect(await screen.findByTestId('node-properties-panel')).toHaveTextContent('Fetch');
    expect(screen.getByRole('tab', { name: 'Last run' })).toHaveAttribute('aria-selected', 'true');
  });

  it('keeps the captured label and disables the chip once the node is deleted', () => {
    seed({ lastRun: failedRun });
    render(<Harness />);
    act(() => {
      usePaneStore.getState().updateFlowNodes(
        tabId,
        baseTab.nodes.filter((n) => n.id !== 'req1'),
      );
    });
    expect(screen.getByRole('button', { name: 'Select failed node Fetch' })).toBeDisabled();
  });

  it('clears the strip when a new run starts', () => {
    seed({ lastRun: failedRun });
    render(<Harness />);
    act(() => {
      usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-4');
    });
    expect(screen.queryByTestId('run-result-strip')).not.toBeInTheDocument();
  });
});

describe('FlowPane run-result wiring', () => {
  it('shows the strip, with the failed node label, after Run finishes', async () => {
    vi.mocked(runFlow).mockResolvedValue({
      runId: 'run-5',
      stoppedReason: 'completed',
      steps: [
        {
          nodeId: 'out1',
          status: 'failed',
          statusCode: null,
          durationMs: null,
          error: 'boom',
          value: null,
        },
      ],
    });
    seed({ runState: 'idle' });
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() =>
      expect(screen.getByTestId('run-result-strip')).toHaveTextContent('Run finished with 1 failed'),
    );
    expect(screen.getByRole('button', { name: 'Select failed node Result' })).toBeEnabled();
  });
});
```

- [ ] **Step 2: Run them to verify they fail**

Run: `yarn test src/components/flow/__tests__/FlowPane.runStrip.test.tsx`
Expected: FAIL (no `run-result-strip` element).

- [ ] **Step 3: Create the strip component**

Create `src/components/flow/RunResultStrip.tsx`:

```tsx
import { Ban, CheckCircle2, Clock, type LucideIcon, XCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { formatRunDuration } from '@/lib/flow-run-result';
import { cn } from '@/lib/utils';
import type { FlowLastRun } from '@/types/pane-types';

interface RunResultStripProps {
  result: FlowLastRun;
  // False when the failed node is no longer on the canvas.
  canSelectFailed: boolean;
  onSelectFailed: (nodeId: string) => void;
}

function describeRun(result: FlowLastRun): { text: string; Icon: LucideIcon; tone: string } {
  const took = result.totalMs === null ? '' : ` in ${formatRunDuration(result.totalMs)}`;
  switch (result.stoppedReason) {
    case 'cancelled':
      return { text: `Run cancelled${took}`, Icon: Ban, tone: 'text-muted-foreground' };
    case 'completed':
      return result.failedCount > 0
        ? { text: `Run finished with ${result.failedCount} failed${took}`, Icon: XCircle, tone: 'text-red-600' }
        : { text: `Run completed${took}`, Icon: CheckCircle2, tone: 'text-green-600' };
    case 'error':
      return { text: `Run ended with an error${took}`, Icon: XCircle, tone: 'text-red-600' };
    default:
      return {
        text: `Run stopped (${result.stoppedReason})${took}`,
        Icon: Clock,
        tone: 'text-amber-600',
      };
  }
}

// One line under the toolbar: how the last run ended, and which node failed first.
// It is a plain group, not a live region. The run announcer owns spoken updates.
export function RunResultStrip({ result, canSelectFailed, onSelectFailed }: RunResultStripProps) {
  const { text, Icon, tone } = describeRun(result);
  const skipped = result.skippedCount > 0 ? ` · ${result.skippedCount} skipped` : '';
  const failedNodeId = result.failedNodeId;
  const failedLabel = result.failedLabel || failedNodeId;
  return (
    <div
      role='group'
      aria-label='Last run result'
      data-testid='run-result-strip'
      className='nokey flex max-w-full items-center gap-2 rounded-md border bg-card px-2.5 py-1 text-xs shadow-sm'
    >
      <Icon className={cn('h-3.5 w-3.5 shrink-0', tone)} aria-hidden='true' />
      <span className='truncate'>
        {text}
        {skipped}
      </span>
      {failedNodeId && (
        <Button
          type='button'
          variant='link'
          size='sm'
          className='h-auto min-w-0 max-w-44 p-0 text-xs'
          disabled={!canSelectFailed}
          aria-label={`Select failed node ${failedLabel}`}
          onClick={() => onSelectFailed(failedNodeId)}
        >
          <span className='truncate'>Failed at {failedLabel}</span>
        </Button>
      )}
    </div>
  );
}
```

- [ ] **Step 4: Wire it into `FlowPane`**

In `src/components/flow/FlowPane.tsx`:

1. Imports. Add (keep Biome's order):

```tsx
import type { FlowRunResult } from '@/lib/flow-run-result';
import { RunResultStrip } from './RunResultStrip';
```

2. Next to the other store selectors (after `setFlowRunState`), add:

```tsx
  const setFlowRunResult = usePaneStore((s) => s.setFlowRunResult);
```

3. After the `handleDeleteNode` definition (still above the `useEffect` that lists collections, so both stay above the picker early return), add:

```tsx
  // Adds the failed node's label while the node still exists, so the strip
  // keeps naming it after a rename or delete.
  const handleRunResult = useCallback(
    (result: FlowRunResult) => {
      const failed = result.failedNodeId
        ? latestFlowTab()?.nodes.find((n) => n.id === result.failedNodeId)
        : undefined;
      const failedLabel = failed ? failed.kind.label || failed.id : undefined;
      setFlowRunResult(tabId, { ...result, ...(failedLabel ? { failedLabel } : {}) });
    },
    [latestFlowTab, setFlowRunResult, tabId],
  );

  // Selects the node and opens its panel on the Last run tab.
  const handleOpenNodeOnLastRun = useCallback(
    (nodeId: string) => {
      setPanelTab('last-run');
      handleSelectedNodeIdsChange(new Set([nodeId]));
      handleOpenProperties(nodeId);
    },
    [handleOpenProperties, handleSelectedNodeIdsChange],
  );
```

4. On the `<FlowToolbar` element, add the prop after `onStepDebug={...}`:

```tsx
              onRunResult={handleRunResult}
```

5. Directly after the closing `</div>` of the `absolute top-2 right-2 z-10` container (the one holding `CallbackHostSetting`, `FlowToolbar` and the Save button), add:

```tsx
          {tab.lastRun && (
            <div className='absolute top-12 right-2 z-10 max-w-[60%]'>
              <RunResultStrip
                result={tab.lastRun}
                canSelectFailed={tab.nodes.some((n) => n.id === tab.lastRun?.failedNodeId)}
                onSelectFailed={handleOpenNodeOnLastRun}
              />
            </div>
          )}
```

- [ ] **Step 5: Run to verify the tests pass**

Run: `yarn test src/components/flow src/stores/__tests__/pane-store.flowRun.test.ts src/lib/__tests__/flow-run-result.test.ts`
Expected: PASS. If an existing `FlowPane` test now fails because it renders a tab with a stale `lastRun`, that is a real regression: none of the existing fixtures set `lastRun`.

- [ ] **Step 6: Full gates and commit**

Run: `yarn tsc --noEmit && yarn check && yarn test src/components/flow src/stores src/lib/__tests__/flow-run-result.test.ts`
Expected: all pass.

Commit with the `dev-workflow-skills:1-git-commit` skill. Stage only:
`src/components/flow/RunResultStrip.tsx src/components/flow/FlowPane.tsx src/components/flow/__tests__/FlowPane.runStrip.test.tsx`
Suggested subject: `feat(flow): show a run-result strip with click-to-select failed node`.

---

## Self-Review

- **Spec coverage (F-31):** result on the tab and store action (Task 1); wall-clock total from the run-started handler to the resolved `runFlow` (Task 2); failed node = first failed step, skipped for a cancelled run (Tasks 1 and 2); `onFlowRunFinished` recovery filtered by the resumed run id, idempotent by run id (Tasks 1 and 2); a run rejected before any event shows nothing (Task 2); strip with lucide icons, a shadcn `Button` chip, click selects the node and opens the Last run tab (`'last-run'` is the real `PanelTab` value), long labels truncated (Task 3).
- **Placeholders:** none. Every code step shows code.
- **Type consistency:** `FlowLastRun` (store, strip, tests) and `FlowRunResult = Omit<FlowLastRun, 'failedLabel'>` (helpers, toolbar prop, `handleRunResult`). The strip testid is `run-result-strip` in the component and both test files. The chip's accessible name is `Select failed node <label>` in the component and the tests. `setFlowRunResult(tabId, FlowLastRun | undefined)` matches in the interface, the implementation and the tests.
- **Review Focus coverage:** item 1 is the cancelled tests in Tasks 1 and 2; item 2 is `mergeRunResult` plus the two remount tests; item 3 is the "new run clears" and "older run" store tests; item 4 is the deleted-node strip test; item 5 is the rejected-run toolbar test.
- **Interface for later plans:** plan P11 (run history) consumes `FlowTab.lastRun`, `FlowLastRun` and `handleRunResult`. Plan P16 keeps its own announcer and relies on the strip not being a live region.
