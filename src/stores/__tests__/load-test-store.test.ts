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
  runLoadTest: vi.fn().mockResolvedValue({
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
  }),
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

  it('startTest in simple mode transitions to complete with result', async () => {
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
    const state = useLoadTestStore.getState();
    expect(state.status).toBe('complete');
    expect(state.result?.totalRequests).toBe(10);
  });
});
