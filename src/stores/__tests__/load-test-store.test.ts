import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RequestState } from '@/types/pane-types';
import { useLoadTestStore } from '../load-test-store';

const MOCK_RESULT = {
  totalRequests: 10,
  succeeded: 10,
  failed: 0,
  failedTransport: 0,
  failedStatus: 0,
  minLatencyMs: 5,
  avgLatencyMs: 10,
  p50LatencyMs: 10,
  p95LatencyMs: 15,
  p99LatencyMs: 18,
  maxLatencyMs: 20,
  requestsPerSecond: 50,
  totalDurationMs: 200,
  requestLog: [],
  timeSeries: [],
};

vi.mock('@/lib/tauri-api', () => ({
  runLoadTestV2: vi.fn().mockResolvedValue(undefined),
  exportLoadTest: vi.fn().mockResolvedValue(['base64data==', 'json']),
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: vi.fn().mockResolvedValue(() => undefined),
}));

vi.mock('@/lib/execute-request', () => ({
  resolveRequestFields: vi.fn().mockResolvedValue({
    url: 'http://localhost',
    headers: [],
    queryParams: [],
    body: undefined,
    auth: { type: 'none' },
    collection: undefined,
    environmentName: undefined,
    requestPath: undefined,
  }),
}));

describe('useLoadTestStore', () => {
  beforeEach(() => {
    useLoadTestStore.getState().reset();
    vi.clearAllMocks();
  });

  it('starts in idle state with simple mode', () => {
    const { status, mode, requestLog, result } = useLoadTestStore.getState();
    expect(status).toBe('idle');
    expect(mode).toBe('simple');
    expect(requestLog).toHaveLength(0);
    expect(result).toBeNull();
  });

  it('setMode switches between simple and advanced', () => {
    useLoadTestStore.getState().setMode('advanced');
    expect(useLoadTestStore.getState().mode).toBe('advanced');
    useLoadTestStore.getState().setMode('simple');
    expect(useLoadTestStore.getState().mode).toBe('simple');
  });

  it('setSimpleConfig patches simple config', () => {
    useLoadTestStore.getState().setSimpleConfig({ concurrency: 50 });
    expect(useLoadTestStore.getState().simpleConfig.concurrency).toBe(50);
  });

  it('setSimpleConfig preserves other fields', () => {
    const before = useLoadTestStore.getState().simpleConfig;
    useLoadTestStore.getState().setSimpleConfig({ totalRequests: 1000 });
    const after = useLoadTestStore.getState().simpleConfig;
    expect(after.totalRequests).toBe(1000);
    expect(after.concurrency).toBe(before.concurrency);
  });

  it('setPhases updates phase list', () => {
    const newPhases = [
      {
        kind: 'Hold' as const,
        durationSecs: 60,
        target: { kind: 'concurrency' as const, value: 5 },
      },
    ];
    useLoadTestStore.getState().setPhases(newPhases);
    expect(useLoadTestStore.getState().phases).toEqual(newPhases);
  });

  it('setSuccessStatusBelow updates threshold', () => {
    useLoadTestStore.getState().setSuccessStatusBelow(500);
    expect(useLoadTestStore.getState().successStatusBelow).toBe(500);
  });

  it('reset clears run state', () => {
    useLoadTestStore.setState({ status: 'complete', result: MOCK_RESULT });
    useLoadTestStore.getState().reset();
    expect(useLoadTestStore.getState().status).toBe('idle');
    expect(useLoadTestStore.getState().result).toBeNull();
  });

  it('setTargetUnit to rps replaces phases with rps defaults', () => {
    useLoadTestStore.getState().setTargetUnit('rps');
    const state = useLoadTestStore.getState();
    expect(state.targetUnit).toBe('rps');
    expect(state.phases.length).toBeGreaterThan(0);
    expect(state.phases.every((p) => p.target.kind === 'rps')).toBe(true);
  });

  it('setTargetUnit back to concurrency replaces phases again', () => {
    useLoadTestStore.getState().setTargetUnit('rps');
    useLoadTestStore.getState().setTargetUnit('concurrency');
    const state = useLoadTestStore.getState();
    expect(state.targetUnit).toBe('concurrency');
    expect(state.phases.every((p) => p.target.kind === 'concurrency')).toBe(true);
  });

  describe('startTest — simple mode', () => {
    it('calls runLoadTestV2 when mode is simple', async () => {
      const { runLoadTestV2 } = await import('@/lib/tauri-api');
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
    });

    it('populates requestLog from load_test_complete event in simple mode', async () => {
      const { listen } = await import('@tauri-apps/api/event');
      const listenMock = vi.mocked(listen);

      const sampleLog = [
        { seq: 0, status: 200, latencyMs: 12.5, responseBytes: 128, error: null, phaseIndex: 0 },
        { seq: 1, status: 404, latencyMs: 8.0, responseBytes: 64, error: null, phaseIndex: 0 },
      ];

      let completeHandler: ((event: { payload: unknown }) => void) | null = null;
      listenMock.mockImplementation(async (eventName, handler) => {
        if (eventName === 'load_test_complete') {
          completeHandler = handler as typeof completeHandler;
        }
        return () => undefined;
      });

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

      const runPromise = useLoadTestStore.getState().startTest(fakeRequest, 'tab-2');

      await vi.waitFor(() => expect(completeHandler).not.toBeNull());
      const handler = completeHandler as ((event: { payload: unknown }) => void) | null;
      if (!handler) throw new Error('completeHandler not set');
      handler({
        payload: {
          totalRequests: 2,
          succeeded: 1,
          failed: 1,
          failedTransport: 0,
          failedStatus: 1,
          minLatencyMs: 8,
          avgLatencyMs: 10.25,
          p50LatencyMs: 10,
          p95LatencyMs: 12,
          p99LatencyMs: 12,
          maxLatencyMs: 12.5,
          requestsPerSecond: 10,
          totalDurationMs: 200,
          requestLog: sampleLog,
          timeSeries: [],
          phaseTimeline: [],
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
        method: 'GET',
        url: 'http://test.local',
        headers: [],
        queryParams: [],
        pathParams: [],
        body: { bodyType: 'none' },
        auth: { authType: 'none' },
        settings: { followRedirects: true, timeoutMs: 30000, verifySsl: true },
      } as unknown as RequestState;

      const runPromise = useLoadTestStore.getState().startTest(fakeRequest, 'tab-3');

      await vi.waitFor(() => expect(progressHandler).not.toBeNull());

      const pHandler = progressHandler as ((event: { payload: unknown }) => void) | null;
      if (!pHandler) throw new Error('progressHandler not set');
      pHandler({
        payload: {
          elapsedMs: 500,
          completed: 1,
          activeConcurrent: 1,
          succeeded: 1,
          failedStatus: 0,
          failedTransport: 0,
          requestsPerSecond: 2,
          p50Ms: 10,
          p95Ms: 15,
          p99Ms: 18,
          currentPhaseIndex: 0,
          recentLog: [
            { seq: 0, status: 200, latencyMs: 10, responseBytes: 100, error: null, phaseIndex: 0 },
          ],
        },
      });

      expect(useLoadTestStore.getState().requestLog).toHaveLength(1);
      expect(useLoadTestStore.getState().requestLog[0].seq).toBe(0);

      await vi.waitFor(() => expect(completeHandler).not.toBeNull());
      const cHandler = completeHandler as ((event: { payload: unknown }) => void) | null;
      if (!cHandler) throw new Error('completeHandler not set');
      cHandler({
        payload: {
          totalRequests: 1,
          succeeded: 1,
          failed: 0,
          failedTransport: 0,
          failedStatus: 0,
          minLatencyMs: 10,
          avgLatencyMs: 10,
          p50LatencyMs: 10,
          p95LatencyMs: 10,
          p99LatencyMs: 10,
          maxLatencyMs: 10,
          requestsPerSecond: 2,
          totalDurationMs: 500,
          requestLog: [
            { seq: 0, status: 200, latencyMs: 10, responseBytes: 100, error: null, phaseIndex: 0 },
          ],
          timeSeries: [],
          phaseTimeline: [],
        },
      });

      await runPromise;
      expect(useLoadTestStore.getState().status).toBe('complete');
    });
  });
});
