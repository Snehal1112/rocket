# Simple Load Test — Route Through V2 Streaming

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Simple mode emit live per-request log entries during a run by routing it through the existing V2 streaming path (`run_load_test_v2_command`) instead of the legacy blocking `run_load_test_command`.

**Architecture:** Convert `simpleConfig` to a single `Hold` phase V2 config inside `startTest`. Listen to `load_test_progress` / `load_test_complete` events exactly as Advanced mode does. The V1 command and all Rust code remain untouched — this is a pure frontend store change.

**Tech Stack:** TypeScript, Zustand, `@tauri-apps/api/event`, Vitest

---

## File Map

| File | Change |
|---|---|
| `src/stores/load-test-store.ts` | Replace the Simple mode branch in `startTest` — call `runLoadTestV2` with a derived V2 config; set up event listeners |
| `src/stores/__tests__/load-test-store.test.ts` | Update existing Simple-mode test; add new tests for live `requestLog` population and `timeSeries` accumulation |

No new files are created.

---

### Task 1: Update the Simple mode branch in `startTest`

**Files:**
- Modify: `src/stores/load-test-store.ts:150-162`

The current Simple mode block calls `runLoadTest` (V1) and blocks until done. Replace it to convert `simpleConfig` to a one-phase V2 config, attach the same event listeners used by Advanced mode, then call `runLoadTestV2`.

- [ ] **Step 1: Open and read the current Simple branch**

Lines 150–162 of `src/stores/load-test-store.ts`:

```ts
if (get().mode === 'simple') {
  abortController = new AbortController();
  try {
    const result = await runLoadTest(httpInput, get().simpleConfig);
    set({ status: 'complete', result, requestLog: result.requestLog ?? [] });
  } catch (err) {
    if ((err as Error)?.name === 'AbortError') return;
    set({ status: 'error', error: String(err) });
  } finally {
    abortController = null;
  }
  return;
}
```

- [ ] **Step 2: Replace the Simple mode block**

Replace exactly those lines with the block below. The `runLoadTest` import becomes unused after this — remove it from the import at line 14.

```ts
if (get().mode === 'simple') {
  // Convert flat config to a single Hold phase so Simple mode gets
  // live streaming via the same V2 path as Advanced mode.
  const { concurrency, totalRequests, intervalMs, durationCapSecs, successStatusBelow: _ignored } =
    { ...get().simpleConfig, successStatusBelow: get().successStatusBelow };

  const simpleV2Config: LoadTestConfigV2 = {
    phases: [
      {
        kind: 'Hold',
        durationSecs: durationCapSecs ?? Math.ceil((totalRequests / Math.max(concurrency, 1)) * 2),
        target: { kind: 'concurrency', value: concurrency },
      },
    ],
    successRule: { statusBelow: get().successStatusBelow },
    ringBufferSize: totalRequests,
  };

  unlistenProgress = await listen<LoadTestProgressEvent>('load_test_progress', (event) => {
    set((state) => ({
      latestSnapshot: event.payload,
      requestLog: event.payload.recentLog,
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

  const totalPhaseMs = simpleV2Config.phases.reduce((s, p) => s + p.durationSecs * 1000, 0);
  safetyTimer = setTimeout(() => {
    if (get().status === 'running') {
      set({
        status: 'error',
        error: `Load test timed out after ${(totalPhaseMs + SAFETY_BUFFER_MS) / 1000}s.`,
      });
      get().stopTest();
    }
  }, totalPhaseMs + SAFETY_BUFFER_MS);

  try {
    await runLoadTestV2(httpInput, simpleV2Config);
  } catch (err) {
    if (safetyTimer) clearTimeout(safetyTimer);
    set({ status: 'error', error: String(err) });
    get().stopTest();
  }
  return;
}
```

- [ ] **Step 3: Remove the now-unused `runLoadTest` import**

In the import at the top of `load-test-store.ts`, remove `runLoadTest` from the `@/lib/tauri-api` import line. It should read (other items kept as-is):

```ts
import {
  type ExportFormat,
  exportLoadTest,
  type LoadTestConfig,
  type LoadTestConfigV2,
  type LoadTestPhase,
  type LoadTestProgressEvent,
  type LoadTestResult,
  type RequestLogEntry,
  runLoadTestV2,
  type TargetUnit,
  type TimeSeriesPoint,
} from '@/lib/tauri-api';
```

- [ ] **Step 4: Type-check**

```bash
yarn tsc --noEmit
```

Expected: no errors.

- [ ] **Step 5: Commit**

```bash
git add src/stores/load-test-store.ts
git commit -m "feat(load-test): route simple mode through v2 streaming for live request log"
```

---

### Task 2: Update tests

**Files:**
- Modify: `src/stores/__tests__/load-test-store.test.ts`

The existing `startTest in simple mode transitions to complete with result` test mocks `runLoadTest`. After Task 1, Simple mode no longer calls `runLoadTest` — it calls `runLoadTestV2` and listens for events. The test must be rewritten to fire events instead.

- [ ] **Step 1: Write failing tests first**

Replace the existing simple-mode `startTest` test and add two new ones. The full updated `describe` block below replaces everything from line 130 to the end of the file.

```ts
describe('startTest — simple mode', () => {
  it('calls runLoadTestV2 (not runLoadTest) when mode is simple', async () => {
    const { runLoadTestV2, runLoadTest } = await import('@/lib/tauri-api');
    useLoadTestStore.getState().setMode('simple');
    const fakeRequest = {
      method: 'GET',
      url: 'http://test.local',
      headers: [],
      queryParams: [],
      pathParams: [],
      body: { bodyType: 'none' },
      auth: { authType: 'none' },
      settings: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
    } as unknown as RequestState;

    await useLoadTestStore.getState().startTest(fakeRequest, 'tab-1');

    expect(runLoadTestV2).toHaveBeenCalled();
    expect(runLoadTest).not.toHaveBeenCalled();
  });

  it('populates requestLog from load_test_complete event in simple mode', async () => {
    const { listen } = await import('@tauri-apps/api/event');
    const listenMock = vi.mocked(listen);

    const sampleLog = [
      { seq: 0, status: 200, latencyMs: 12.5, responseBytes: 128, error: null, phaseIndex: 0 },
      { seq: 1, status: 404, latencyMs: 8.0, responseBytes: 64, error: null, phaseIndex: 0 },
    ];

    // Capture the complete-event handler so we can fire it manually.
    let completeHandler: ((event: { payload: unknown }) => void) | null = null;
    listenMock.mockImplementation(async (eventName, handler) => {
      if (eventName === 'load_test_complete') {
        completeHandler = handler as typeof completeHandler;
      }
      return () => undefined;
    });

    useLoadTestStore.getState().setMode('simple');
    const fakeRequest = {
      method: 'GET', url: 'http://test.local', headers: [], queryParams: [], pathParams: [],
      body: { bodyType: 'none' }, auth: { authType: 'none' },
      settings: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
    } as unknown as RequestState;

    const runPromise = useLoadTestStore.getState().startTest(fakeRequest, 'tab-2');

    // Fire the complete event after listeners are registered.
    await vi.waitFor(() => expect(completeHandler).not.toBeNull());
    completeHandler!({
      payload: {
        totalRequests: 2, succeeded: 1, failed: 1, failedTransport: 0, failedStatus: 1,
        minLatencyMs: 8, avgLatencyMs: 10.25, p50LatencyMs: 10, p95LatencyMs: 12, p99LatencyMs: 12,
        maxLatencyMs: 12.5, requestsPerSecond: 10, totalDurationMs: 200,
        requestLog: sampleLog, timeSeries: [], phaseTimeline: [],
      },
    });

    await runPromise;

    const state = useLoadTestStore.getState();
    expect(state.status).toBe('complete');
    expect(state.requestLog).toHaveLength(2);
    expect(state.requestLog[0].seq).toBe(0);
    expect(state.requestLog[1].status).toBe(404);
  });

  it('populates requestLog incrementally from load_test_progress events', async () => {
    const { listen } = await import('@tauri-apps/api/event');
    const listenMock = vi.mocked(listen);

    let progressHandler: ((event: { payload: unknown }) => void) | null = null;
    let completeHandler: ((event: { payload: unknown }) => void) | null = null;

    listenMock.mockImplementation(async (eventName, handler) => {
      if (eventName === 'load_test_progress') progressHandler = handler as typeof progressHandler;
      if (eventName === 'load_test_complete') completeHandler = handler as typeof completeHandler;
      return () => undefined;
    });

    useLoadTestStore.getState().setMode('simple');
    const fakeRequest = {
      method: 'GET', url: 'http://test.local', headers: [], queryParams: [], pathParams: [],
      body: { bodyType: 'none' }, auth: { authType: 'none' },
      settings: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
    } as unknown as RequestState;

    const runPromise = useLoadTestStore.getState().startTest(fakeRequest, 'tab-3');

    await vi.waitFor(() => expect(progressHandler).not.toBeNull());

    // Fire a progress event — should update requestLog immediately.
    progressHandler!({
      payload: {
        elapsedMs: 500, completed: 1, activeConcurrent: 1, succeeded: 1,
        failedStatus: 0, failedTransport: 0, requestsPerSecond: 2,
        p50Ms: 10, p95Ms: 15, p99Ms: 18, currentPhaseIndex: 0,
        recentLog: [{ seq: 0, status: 200, latencyMs: 10, responseBytes: 100, error: null, phaseIndex: 0 }],
      },
    });

    expect(useLoadTestStore.getState().requestLog).toHaveLength(1);
    expect(useLoadTestStore.getState().requestLog[0].seq).toBe(0);

    // Resolve by firing complete.
    await vi.waitFor(() => expect(completeHandler).not.toBeNull());
    completeHandler!({
      payload: {
        totalRequests: 1, succeeded: 1, failed: 0, failedTransport: 0, failedStatus: 0,
        minLatencyMs: 10, avgLatencyMs: 10, p50LatencyMs: 10, p95LatencyMs: 10, p99LatencyMs: 10,
        maxLatencyMs: 10, requestsPerSecond: 2, totalDurationMs: 500,
        requestLog: [{ seq: 0, status: 200, latencyMs: 10, responseBytes: 100, error: null, phaseIndex: 0 }],
        timeSeries: [], phaseTimeline: [],
      },
    });

    await runPromise;
    expect(useLoadTestStore.getState().status).toBe('complete');
  });
});
```

- [ ] **Step 2: Remove the old simple-mode test**

Delete lines 130–147 from the test file (the old `it('startTest in simple mode transitions to complete with result', ...)` block) — they are fully replaced by the three tests above. Also remove `runLoadTest` from the `vi.mock('@/lib/tauri-api', ...)` factory since it is no longer called (lines 24–40). The mock factory should become:

```ts
vi.mock('@/lib/tauri-api', () => ({
  runLoadTestV2: vi.fn().mockResolvedValue(undefined),
  exportLoadTest: vi.fn().mockResolvedValue(['base64data==', 'json']),
}));
```

- [ ] **Step 3: Run the tests**

```bash
yarn test load-test-store
```

Expected: all tests pass. If the `calls runLoadTestV2` test fails with "runLoadTest was called", confirm Task 1 was applied correctly.

- [ ] **Step 4: Commit**

```bash
git add src/stores/__tests__/load-test-store.test.ts
git commit -m "test(load-test): update store tests for simple-mode v2 routing"
```

---

## Self-Review

**Spec coverage:**
- Simple mode now calls `runLoadTestV2` → covered in Task 1.
- `requestLog` fills live via `load_test_progress` → covered in Task 1 (event listener) and Task 2 (progress test).
- `requestLog` finalised via `load_test_complete` → covered in both tasks.
- Safety timer and `stopTest` cleanup paths work the same way → identical logic to Advanced mode, reused verbatim.
- V1 command and all Rust code untouched → no Rust files modified.

**Placeholder scan:** No TBDs, no "handle edge cases" language, no missing code blocks.

**Type consistency:** `LoadTestConfigV2`, `LoadTestProgressEvent`, `LoadTestResult`, `RequestLogEntry` types all sourced from `@/lib/tauri-api` — same types used in both the impl and tests.
