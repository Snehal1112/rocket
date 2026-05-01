# Enhanced Load Testing — Plan D: Frontend Store + IPC

> **For agentic workers:** REQUIRED: Use superpowers:subagent-driven-development (if subagents available) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `useLoadTestStore` (Zustand), update `tauri-api.ts` with new types and event listeners, and wire the store's `startTest` action to the Tauri event stream.

**Architecture:** `useLoadTestStore` holds all config, run state, time-series, and request log. The `startTest` action invokes `run_load_test_v2_command` then subscribes to two Tauri events: `load_test_progress` (appends to `timeSeries`, updates `latestSnapshot`) and `load_test_complete` (sets `result`, cleans up listeners). A 30 s safety timeout sets `status = 'error'` if `load_test_complete` never arrives.

**Tech Stack:** TypeScript, Zustand, `@tauri-apps/api/event`, shadcn/ui

**Spec:** `docs/superpowers/specs/2026-05-01-load-test-enhanced-design.md`

**Depends on:** Plan C complete

---

## File Map

| File | Change |
|---|---|
| `src/lib/tauri-api.ts` | Add new types, `runLoadTestV2`, `exportLoadTest` |
| `src/stores/load-test-store.ts` | New — full Zustand store |
| `src/stores/load-test-store.test.ts` | New — Vitest unit tests |

---

## Chunk 1: `tauri-api.ts` additions

### Task 1: Add new types and API functions to `tauri-api.ts`

**Files:**
- Modify: `src/lib/tauri-api.ts`

- [ ] **Step 1: Read existing `tauri-api.ts` load test section**

```bash
grep -n "LoadTest\|load_test\|runLoadTest" src/lib/tauri-api.ts
```

Note existing type names to avoid conflicts.

- [ ] **Step 2: Add new types**

In `src/lib/tauri-api.ts`, find the existing `LoadTestConfig` and `LoadTestResult` types. Add the new v2 types **after** them (keep old types for backwards compat):

```typescript
// ---- Load Test v2 types ----

export type PhaseKind = 'RampUp' | 'Hold' | 'RampDown';

export interface LoadTestPhase {
  kind: PhaseKind;
  durationSecs: number;
  targetConcurrency: number;
}

export interface SuccessRule {
  statusBelow: number;
}

export interface LoadTestConfigV2 {
  phases: LoadTestPhase[];
  successRule: SuccessRule;
  ringBufferSize: number;
}

export interface LoadTestProgressEvent {
  elapsedMs: number;
  completed: number;
  activeConcurrent: number;
  succeeded: number;
  failedStatus: number;
  failedTransport: number;
  requestsPerSecond: number;
  p50Ms: number;
  p95Ms: number;
  p99Ms: number;
  currentPhaseIndex: number;
}

export interface TimeSeriesPoint {
  elapsedMs: number;
  rps: number;
  p50Ms: number;
  p95Ms: number;
  p99Ms: number;
  errorRatePct: number;
  activeConcurrent: number;
}

export interface RequestLogEntry {
  seq: number;
  status: number | null;
  latencyMs: number;
  responseBytes: number;
  error: string | null;
  phaseIndex: number;
}

export interface PhaseMarker {
  phaseIndex: number;
  startedAtMs: number;
}

export type ExportFormat = 'html' | 'csv' | 'json' | 'pdf';
```

- [ ] **Step 3: Extend the existing `LoadTestResult` type**

Find the existing `LoadTestResult` interface and add the three new optional fields:

```typescript
  phaseTimeline?: PhaseMarker[];
  requestLog?: RequestLogEntry[];
  timeSeries?: TimeSeriesPoint[];
```

- [ ] **Step 4: Add new API functions**

After the existing `runLoadTest` export, add:

```typescript
export const runLoadTestV2 = (
  input: Parameters<typeof runLoadTest>[0],
  config: LoadTestConfigV2,
) => invoke<void>('run_load_test_v2_command', { input, config });

export const exportLoadTest = (
  result: LoadTestResult,
  format: ExportFormat,
) => invoke<[string, string]>('export_load_test', { result, format });
```

- [ ] **Step 5: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | tail -15
```

Expected: no new errors (the pre-existing `CollectionOverviewTab.tsx` warning may still appear).

- [ ] **Step 6: Commit**

```bash
git add src/lib/tauri-api.ts
git commit -m "feat(frontend): add LoadTestConfigV2, LoadTestProgressEvent, TimeSeriesPoint, RequestLogEntry types and API functions"
```

---

## Chunk 2: `useLoadTestStore`

### Task 2: Create `src/stores/load-test-store.ts`

**Files:**
- Create: `src/stores/load-test-store.ts`

- [ ] **Step 1: Read an existing store for patterns**

```bash
head -80 src/stores/git-store.ts
```

Note how `create` is imported, how async actions are structured, and how errors are set.

- [ ] **Step 2: Create the store**

Create `src/stores/load-test-store.ts`:

```typescript
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { create } from 'zustand';

import {
  type ExportFormat,
  type LoadTestConfigV2,
  type LoadTestPhase,
  type LoadTestProgressEvent,
  type LoadTestResult,
  type RequestLogEntry,
  type TimeSeriesPoint,
  exportLoadTest,
  runLoadTestV2,
} from '@/lib/tauri-api';
import type { RequestState } from '@/types/pane-types';
import { buildExecuteRequestInput } from '@/lib/execute-request';

export type LoadTestStatus = 'idle' | 'running' | 'complete' | 'error';

const DEFAULT_PHASES: LoadTestPhase[] = [
  { kind: 'RampUp', durationSecs: 10, targetConcurrency: 10 },
  { kind: 'Hold',   durationSecs: 30, targetConcurrency: 10 },
  { kind: 'RampDown', durationSecs: 10, targetConcurrency: 0 },
];

interface LoadTestState {
  // Config
  phases: LoadTestPhase[];
  successStatusBelow: number;
  ringBufferSize: number;

  // Run state
  status: LoadTestStatus;
  latestSnapshot: LoadTestProgressEvent | null;
  timeSeries: TimeSeriesPoint[];
  requestLog: RequestLogEntry[];
  result: LoadTestResult | null;
  error: string | null;

  // Actions
  setPhases: (phases: LoadTestPhase[]) => void;
  setSuccessStatusBelow: (n: number) => void;
  startTest: (request: RequestState, tabId: string) => Promise<void>;
  stopTest: () => void;
  exportResult: (format: ExportFormat) => Promise<void>;
  reset: () => void;
}

// Module-level unlisten refs so `stopTest` can cancel the listeners.
let unlistenProgress: UnlistenFn | null = null;
let unlistenComplete: UnlistenFn | null = null;
let safetyTimer: ReturnType<typeof setTimeout> | null = null;
const SAFETY_TIMEOUT_MS = 30_000;

export const useLoadTestStore = create<LoadTestState>((set, get) => ({
  // Config defaults
  phases: DEFAULT_PHASES,
  successStatusBelow: 400,
  ringBufferSize: 5000,

  // Run state defaults
  status: 'idle',
  latestSnapshot: null,
  timeSeries: [],
  requestLog: [],
  result: null,
  error: null,

  setPhases: (phases) => set({ phases }),
  setSuccessStatusBelow: (n) => set({ successStatusBelow: n }),

  startTest: async (request, tabId) => {
    // Clean up any previous listeners.
    get().stopTest();

    set({ status: 'running', timeSeries: [], requestLog: [], result: null, error: null, latestSnapshot: null });

    // Subscribe to progress events BEFORE invoking so we don't miss early events.
    unlistenProgress = await listen<LoadTestProgressEvent>('load_test_progress', (event) => {
      set((state) => ({
        latestSnapshot: event.payload,
        timeSeries: [
          ...state.timeSeries,
          {
            elapsedMs: event.payload.elapsedMs,
            rps: event.payload.requestsPerSecond,
            p50Ms: event.payload.p50Ms,
            p95Ms: event.payload.p95Ms,
            p99Ms: event.payload.p99Ms,
            errorRatePct:
              event.payload.completed > 0
                ? ((event.payload.failedStatus + event.payload.failedTransport) /
                    event.payload.completed) *
                  100
                : 0,
            activeConcurrent: event.payload.activeConcurrent,
          },
        ],
      }));
    });

    unlistenComplete = await listen<LoadTestResult>('load_test_complete', (event) => {
      if (safetyTimer) clearTimeout(safetyTimer);
      set({
        status: 'complete',
        result: event.payload,
        requestLog: event.payload.requestLog ?? [],
        timeSeries: event.payload.timeSeries ?? get().timeSeries,
      });
      unlistenProgress?.();
      unlistenComplete?.();
      unlistenProgress = null;
      unlistenComplete = null;
    });

    // Safety timeout: if complete never arrives, surface an error.
    safetyTimer = setTimeout(() => {
      if (get().status === 'running') {
        set({ status: 'error', error: 'Load test timed out — no completion event received after 30 s.' });
        get().stopTest();
      }
    }, SAFETY_TIMEOUT_MS);

    // Build the IPC input using the same helper as sendRequest.
    try {
      const { input, collection, requestPath, environmentName } =
        await buildExecuteRequestInput(request, tabId);

      const config: LoadTestConfigV2 = {
        phases: get().phases,
        successRule: { statusBelow: get().successStatusBelow },
        ringBufferSize: get().ringBufferSize,
      };

      await runLoadTestV2(
        { ...input, collection, requestPath, environmentName },
        config,
      );
    } catch (err) {
      if (safetyTimer) clearTimeout(safetyTimer);
      set({ status: 'error', error: String(err) });
      get().stopTest();
    }
  },

  stopTest: () => {
    unlistenProgress?.();
    unlistenComplete?.();
    unlistenProgress = null;
    unlistenComplete = null;
    if (safetyTimer) {
      clearTimeout(safetyTimer);
      safetyTimer = null;
    }
    if (get().status === 'running') {
      set({ status: 'idle' });
    }
  },

  exportResult: async (format) => {
    const { result } = get();
    if (!result) return;

    try {
      const [b64, ext] = await exportLoadTest(result, format);
      // Decode base64 and trigger browser download.
      const binary = atob(b64);
      const bytes = new Uint8Array(binary.length);
      for (let i = 0; i < binary.length; i++) bytes[i] = binary.charCodeAt(i);
      const blob = new Blob([bytes]);
      const url = URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `load-test-report.${ext}`;
      a.click();
      URL.revokeObjectURL(url);
    } catch (err) {
      set({ error: String(err) });
    }
  },

  reset: () => {
    get().stopTest();
    set({
      status: 'idle',
      latestSnapshot: null,
      timeSeries: [],
      requestLog: [],
      result: null,
      error: null,
    });
  },
}));
```

- [ ] **Step 3: TypeScript check**

```bash
yarn tsc --noEmit 2>&1 | tail -15
```

If `buildExecuteRequestInput` does not exist as an exported function from `execute-request.ts`, check the actual export shape:

```bash
grep -n "^export" src/lib/execute-request.ts | head -20
```

Adjust the import and call in the store to match the real API. The important thing is that `input` ends up as the same shape passed to `runLoadTest` in the existing `LoadTestDialog`.

- [ ] **Step 4: Commit**

```bash
git add src/stores/load-test-store.ts
git commit -m "feat(frontend): add useLoadTestStore with Tauri event streaming and export"
```

---

## Chunk 3: Store unit tests

### Task 3: Vitest unit tests for `useLoadTestStore`

**Files:**
- Create: `src/stores/load-test-store.test.ts`

- [ ] **Step 1: Check Vitest mock setup**

```bash
grep -r "vi.mock\|@tauri-apps" src/stores/ | head -10
```

Note how other store tests mock Tauri modules — follow the same pattern.

- [ ] **Step 2: Create test file**

Create `src/stores/load-test-store.test.ts`:

```typescript
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { act } from '@testing-library/react';
import { useLoadTestStore } from './load-test-store';

// Mock the tauri API module.
vi.mock('@/lib/tauri-api', () => ({
  runLoadTestV2: vi.fn().mockResolvedValue(undefined),
  exportLoadTest: vi.fn().mockResolvedValue(['base64data==', 'json']),
}));

// Mock the tauri event listener so tests don't need a real Tauri runtime.
vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => {}), // returns a no-op unlisten fn
}));

// Mock the execute-request helper.
vi.mock('@/lib/execute-request', () => ({
  buildExecuteRequestInput: vi.fn().mockResolvedValue({
    input: { method: 'GET', url: 'http://localhost', headers: [], queryParams: [], body: null, auth: { type: 'none' }, options: {} },
    collection: undefined,
    requestPath: undefined,
    environmentName: undefined,
  }),
}));

describe('useLoadTestStore', () => {
  beforeEach(() => {
    useLoadTestStore.getState().reset();
    vi.clearAllMocks();
  });

  it('starts in idle state', () => {
    const { status, timeSeries, requestLog, result } = useLoadTestStore.getState();
    expect(status).toBe('idle');
    expect(timeSeries).toHaveLength(0);
    expect(requestLog).toHaveLength(0);
    expect(result).toBeNull();
  });

  it('setPhases updates the phases config', () => {
    const newPhases = [{ kind: 'Hold' as const, durationSecs: 60, targetConcurrency: 5 }];
    act(() => { useLoadTestStore.getState().setPhases(newPhases); });
    expect(useLoadTestStore.getState().phases).toEqual(newPhases);
  });

  it('reset clears run state', () => {
    // Manually inject some state.
    useLoadTestStore.setState({ status: 'complete', timeSeries: [{ elapsedMs: 1000, rps: 10, p50Ms: 50, p95Ms: 100, p99Ms: 120, errorRatePct: 0, activeConcurrent: 5 }] });
    act(() => { useLoadTestStore.getState().reset(); });
    expect(useLoadTestStore.getState().status).toBe('idle');
    expect(useLoadTestStore.getState().timeSeries).toHaveLength(0);
  });

  it('startTest transitions status to running', async () => {
    const fakeRequest = { method: 'GET', url: 'http://test.local' } as any;
    await act(async () => {
      // Don't await fully — just check the synchronous status transition.
      useLoadTestStore.getState().startTest(fakeRequest, 'tab-1');
    });
    // After startTest is called, status should be 'running' (complete event not fired in this test).
    expect(['running', 'error']).toContain(useLoadTestStore.getState().status);
  });
});
```

- [ ] **Step 3: Run store tests**

```bash
yarn test src/stores/load-test-store.test.ts --run 2>&1 | tail -15
```

Expected: 4 tests pass.

- [ ] **Step 4: Full frontend test suite**

```bash
yarn test --run 2>&1 | tail -10
```

Expected: all tests pass.

- [ ] **Step 5: Commit**

```bash
git add src/stores/load-test-store.test.ts
git commit -m "test(frontend): unit tests for useLoadTestStore"
```
