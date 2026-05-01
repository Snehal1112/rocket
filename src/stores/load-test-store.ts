import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { create } from 'zustand';

import { resolveRequestFields } from '@/lib/execute-request';
import {
  type ExportFormat,
  exportLoadTest,
  type LoadTestConfigV2,
  type LoadTestPhase,
  type LoadTestProgressEvent,
  type LoadTestResult,
  type RequestLogEntry,
  runLoadTestV2,
  type TimeSeriesPoint,
} from '@/lib/tauri-api';
import type { RequestState } from '@/types/pane-types';

export type LoadTestStatus = 'idle' | 'running' | 'complete' | 'error';

const DEFAULT_PHASES: LoadTestPhase[] = [
  { kind: 'RampUp', durationSecs: 10, targetConcurrency: 10 },
  { kind: 'Hold', durationSecs: 30, targetConcurrency: 10 },
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

// Safety buffer added on top of total phase duration before declaring a timeout.
const SAFETY_BUFFER_MS = 60_000;

export const useLoadTestStore = create<LoadTestState>((set, get) => ({
  phases: DEFAULT_PHASES,
  successStatusBelow: 400,
  ringBufferSize: 5000,

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

    set({
      status: 'running',
      timeSeries: [],
      requestLog: [],
      result: null,
      error: null,
      latestSnapshot: null,
    });

    // Subscribe before invoking so we don't miss early events.
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

    // Safety timeout = total planned phase duration + fixed buffer.
    // This ensures the timer never fires before the test can legitimately finish.
    const totalPhaseMs = get().phases.reduce((s, p) => s + p.durationSecs * 1000, 0);
    const safetyTimeoutMs = totalPhaseMs + SAFETY_BUFFER_MS;
    safetyTimer = setTimeout(() => {
      if (get().status === 'running') {
        const totalSecs = safetyTimeoutMs / 1000;
        set({
          status: 'error',
          error: `Load test timed out — no completion event received after ${totalSecs} s.`,
        });
        get().stopTest();
      }
    }, safetyTimeoutMs);

    try {
      const resolved = await resolveRequestFields(tabId, request);
      const config: LoadTestConfigV2 = {
        phases: get().phases,
        successRule: { statusBelow: get().successStatusBelow },
        ringBufferSize: get().ringBufferSize,
      };

      await runLoadTestV2(
        {
          method: request.method,
          url: resolved.url,
          headers: resolved.headers,
          queryParams: resolved.queryParams,
          body: resolved.body ?? null,
          auth: resolved.auth,
          options: {
            followRedirects: request.settings.followRedirects,
            timeoutMs: request.settings.timeoutMs,
            verifySsl: request.settings.verifySsl,
          },
          collection: resolved.collection,
          environmentName: resolved.environmentName,
          requestPath: resolved.requestPath,
        },
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
