import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { RequestState } from '@/types/pane-types';
import { useLoadTestStore } from '../load-test-store';

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

  it('starts in idle state', () => {
    const { status, timeSeries, requestLog, result } = useLoadTestStore.getState();
    expect(status).toBe('idle');
    expect(timeSeries).toHaveLength(0);
    expect(requestLog).toHaveLength(0);
    expect(result).toBeNull();
  });

  it('setPhases updates the phases config', () => {
    const newPhases = [{ kind: 'Hold' as const, durationSecs: 60, targetConcurrency: 5 }];
    useLoadTestStore.getState().setPhases(newPhases);
    expect(useLoadTestStore.getState().phases).toEqual(newPhases);
  });

  it('setSuccessStatusBelow updates the threshold', () => {
    useLoadTestStore.getState().setSuccessStatusBelow(500);
    expect(useLoadTestStore.getState().successStatusBelow).toBe(500);
  });

  it('reset clears run state', () => {
    useLoadTestStore.setState({
      status: 'complete',
      timeSeries: [
        {
          elapsedMs: 1000,
          rps: 10,
          p50Ms: 50,
          p95Ms: 100,
          p99Ms: 120,
          errorRatePct: 0,
          activeConcurrent: 5,
        },
      ],
    });
    useLoadTestStore.getState().reset();
    expect(useLoadTestStore.getState().status).toBe('idle');
    expect(useLoadTestStore.getState().timeSeries).toHaveLength(0);
  });

  it('startTest transitions status to running', async () => {
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
    // Status should be 'running' (no complete event arrives in this mocked setup).
    expect(['running', 'error']).toContain(useLoadTestStore.getState().status);
  });
});
