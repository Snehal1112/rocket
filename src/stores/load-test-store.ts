import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { create } from 'zustand';

import { resolveRequestFields } from '@/lib/execute-request';
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
import type { RequestState } from '@/types/pane-types';

export type LoadTestMode = 'simple' | 'advanced';
export type LoadTestStatus = 'idle' | 'running' | 'complete' | 'error';

const DEFAULT_SIMPLE_CONFIG: LoadTestConfig = {
  concurrency: 25,
  totalRequests: 500,
  intervalMs: 0,
  durationCapSecs: 60,
};

const DEFAULT_PHASES_CONCURRENCY: LoadTestPhase[] = [
  { kind: 'RampUp', durationSecs: 10, target: { kind: 'concurrency', value: 10 } },
  { kind: 'Hold', durationSecs: 30, target: { kind: 'concurrency', value: 10 } },
  { kind: 'RampDown', durationSecs: 10, target: { kind: 'concurrency', value: 0 } },
];

const DEFAULT_PHASES_RPS: LoadTestPhase[] = [
  { kind: 'RampUp', durationSecs: 10, target: { kind: 'rps', value: 50 } },
  { kind: 'Hold', durationSecs: 30, target: { kind: 'rps', value: 50 } },
  { kind: 'RampDown', durationSecs: 10, target: { kind: 'rps', value: 0 } },
];

const SAFETY_BUFFER_MS = 60_000;

interface LoadTestState {
  // Mode
  mode: LoadTestMode;

  // Simple config
  simpleConfig: LoadTestConfig;

  // Advanced config
  targetUnit: TargetUnit;
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
  setMode: (mode: LoadTestMode) => void;
  setSimpleConfig: (patch: Partial<LoadTestConfig>) => void;
  setTargetUnit: (unit: TargetUnit) => void;
  setPhases: (phases: LoadTestPhase[]) => void;
  setSuccessStatusBelow: (n: number) => void;
  setRingBufferSize: (n: number) => void;
  startTest: (request: RequestState, tabId: string) => Promise<void>;
  stopTest: () => void;
  exportResult: (format: ExportFormat) => Promise<void>;
  reset: () => void;
}

// Module-level refs so stopTest can cancel listeners/timers from any call site.
let unlistenProgress: UnlistenFn | null = null;
let unlistenComplete: UnlistenFn | null = null;
let safetyTimer: ReturnType<typeof setTimeout> | null = null;
let abortController: AbortController | null = null;

export const useLoadTestStore = create<LoadTestState>((set, get) => ({
  mode: 'simple',

  simpleConfig: DEFAULT_SIMPLE_CONFIG,

  targetUnit: 'concurrency',
  phases: DEFAULT_PHASES_CONCURRENCY,
  successStatusBelow: 400,
  ringBufferSize: 5000,

  status: 'idle',
  latestSnapshot: null,
  timeSeries: [],
  requestLog: [],
  result: null,
  error: null,

  setMode: (mode) => set({ mode }),
  setSimpleConfig: (patch) => set((s) => ({ simpleConfig: { ...s.simpleConfig, ...patch } })),
  setTargetUnit: (unit) => {
    // Switching unit replaces the phase array with safe defaults for the new
    // unit because numeric values are not interchangeable: 10 concurrent
    // users is not 10 req/sec.
    const phases = unit === 'rps' ? DEFAULT_PHASES_RPS : DEFAULT_PHASES_CONCURRENCY;
    set({ targetUnit: unit, phases });
  },
  setPhases: (phases) => set({ phases }),
  setSuccessStatusBelow: (n) => set({ successStatusBelow: n }),
  setRingBufferSize: (n) => set({ ringBufferSize: n }),

  startTest: async (request, tabId) => {
    get().stopTest();

    set({
      status: 'running',
      timeSeries: [],
      requestLog: [],
      result: null,
      error: null,
      latestSnapshot: null,
    });

    const resolved = await resolveRequestFields(tabId, request).catch((err) => {
      set({ status: 'error', error: String(err) });
      return null;
    });
    if (!resolved) return;

    const httpInput = {
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
    };

    if (get().mode === 'simple') {
      // Convert flat config to a single Hold phase so Simple mode gets
      // live streaming via the same V2 path as Advanced mode.
      const {
        concurrency,
        totalRequests,
        intervalMs: _intervalMs,
        durationCapSecs,
      } = get().simpleConfig;

      const simpleV2Config: LoadTestConfigV2 = {
        phases: [
          {
            kind: 'Hold',
            durationSecs:
              durationCapSecs ?? Math.ceil((totalRequests / Math.max(concurrency, 1)) * 2),
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
        // Stop early once totalRequests have been completed. V2 is duration-based
        // so we mark complete first (preventing stopTest from resetting to idle),
        // then clean up listeners and timers.
        if (event.payload.completed >= totalRequests) {
          set({ status: 'complete' });
          get().stopTest();
        }
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

    // Advanced mode — streaming via v2.
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

    const totalPhaseMs = get().phases.reduce((s, p) => s + p.durationSecs * 1000, 0);
    safetyTimer = setTimeout(() => {
      if (get().status === 'running') {
        set({
          status: 'error',
          error: `Load test timed out after ${(totalPhaseMs + SAFETY_BUFFER_MS) / 1000}s.`,
        });
        get().stopTest();
      }
    }, totalPhaseMs + SAFETY_BUFFER_MS);

    const targetUnit = get().targetUnit;
    const allMatch = get().phases.every((p) => p.target.kind === targetUnit);
    if (!allMatch) {
      if (safetyTimer) clearTimeout(safetyTimer);
      set({
        status: 'error',
        error:
          'All phases must match the selected workload type. Reset phases or switch the toggle.',
      });
      get().stopTest();
      return;
    }

    const advancedConfig: LoadTestConfigV2 = {
      phases: get().phases,
      successRule: { statusBelow: get().successStatusBelow },
      ringBufferSize: get().ringBufferSize,
    };

    try {
      await runLoadTestV2(httpInput, advancedConfig);
    } catch (err) {
      if (safetyTimer) clearTimeout(safetyTimer);
      set({ status: 'error', error: String(err) });
      get().stopTest();
    }
  },

  stopTest: () => {
    abortController?.abort();
    abortController = null;
    unlistenProgress?.();
    unlistenComplete?.();
    unlistenProgress = null;
    unlistenComplete = null;
    if (safetyTimer) {
      clearTimeout(safetyTimer);
      safetyTimer = null;
    }
    if (get().status === 'running') set({ status: 'idle' });
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
