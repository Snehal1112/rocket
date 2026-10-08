import type { UnlistenFn } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { getActiveGlobalEnvName } from '@/lib/execute-request';
import { newFlowRunId } from '@/lib/flow-run-id';
import { type FlowRunResult, resultFromFinishedEvent, summarizeRun } from '@/lib/flow-run-result';
import {
  cancelFlowRun,
  type FlowAuthToken,
  type FlowDebugRequest,
  type FlowLiveProgress,
  type FlowLogEntry,
  type FlowRunStartedEvent,
  type FlowStepCompletedEvent,
  type FlowStepProgressEvent,
  type FlowStepResult,
  onFlowRunFinished,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepProgress,
  onFlowStepStarted,
  runFlow,
} from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';

interface FlowToolbarProps {
  collection: string;
  flowName: string;
  // The tab this toolbar belongs to. Lets Ctrl+Enter start this tab's run.
  tabId?: string;
  environmentName: string | null;
  onPatchStatus: (nodeId: string, status: string, detail?: FlowNodeDetail) => void;
  // Receives progress text for a running node, such as "attempt 3/30".
  onPatchProgress?: (nodeId: string, message: string, live?: FlowLiveProgress) => void;
  // Receives the callback URL of each Wait node when this toolbar's run starts.
  onCallbackUrls?: (urls: Record<string, string>) => void;
  onRunStateChange: (state: 'running' | 'done', runId?: string) => void;
  // The tab's stored run state. The toolbar unmounts when its tab is hidden,
  // so a remounted toolbar reads an in-progress run from here.
  tabRunState?: 'idle' | 'running' | 'done';
  tabRunId?: string;
  // Id of a run this tab sent whose flow-run-started has not arrived yet.
  tabPendingRunId?: string;
  // Receives the run id the toolbar chose, right before the run is sent. The
  // tab stores it, so Stop and a remounted toolbar know the run early.
  onRunRequested?: (runId: string) => void;
  // Runs before a new run starts. `run_flow` runs the flow saved on disk, so
  // this saves unsaved canvas edits first. Returning false aborts the run.
  onBeforeRun?: () => Promise<boolean>;
  // Runs after onBeforeRun and before the run starts. Authenticates Auth nodes
  // and returns the tokens to hand to the run. Returning null, or rejecting,
  // aborts the run.
  onPrepareAuth?: () => Promise<Record<string, FlowAuthToken> | null>;
  // Receives each step's script console output once the run ends.
  onStepLogs?: (nodeId: string, logs: FlowLogEntry[]) => void;
  // Receives each debug node's sent request once the run ends.
  onStepDebug?: (nodeId: string, debug: FlowDebugRequest) => void;
  // Receives the outcome of a finished run: after the final summary is
  // applied, or, for a run this mount only resumed, when its finished event
  // arrives. Not called for a run that is rejected before it starts.
  onRunResult?: (result: FlowRunResult) => void;
}

// Maps a streamed step event (snake_case) to the per-node detail the tab stores.
function detailFromEvent(event: FlowStepCompletedEvent): FlowNodeDetail {
  return {
    statusCode: event.status_code ?? undefined,
    durationMs: event.duration_ms ?? undefined,
    error: event.error ?? undefined,
    value: event.value ?? undefined,
    skipReason: event.skip_reason ?? undefined,
    branch: event.branch ?? undefined,
    attempts: event.attempts ?? undefined,
    exchange: event.exchange ?? undefined,
    logs: event.logs?.length ? event.logs : undefined,
    trace: event.trace ?? undefined,
  };
}

// Maps a run_flow summary step (camelCase) to the same detail shape.
function detailFromStep(step: FlowStepResult): FlowNodeDetail {
  return {
    statusCode: step.statusCode ?? undefined,
    durationMs: step.durationMs ?? undefined,
    error: step.error ?? undefined,
    value: step.value ?? undefined,
    skipReason: step.skipReason ?? undefined,
    branch: step.branch ?? undefined,
    attempts: step.attempts ?? undefined,
    exchange: step.exchange ?? undefined,
    logs: step.logs?.length ? step.logs : undefined,
    trace: step.trace ?? undefined,
  };
}

// Forwards one progress event. The structured part is passed only when the
// backend sent it, so older payloads call the handler as before.
function forwardProgress(
  handler: FlowToolbarProps['onPatchProgress'],
  event: FlowStepProgressEvent,
) {
  if (!handler) return;
  if (event.live) handler(event.node_id, event.message, event.live);
  else handler(event.node_id, event.message);
}

// Maps the run-started callbacks to node id and URL.
function callbackUrlsFrom(event: FlowRunStartedEvent): Record<string, string> {
  return Object.fromEntries((event.callbacks ?? []).map((c) => [c.nodeId, c.url]));
}

// Ids of runs whose own handleRun still listens, even after its toolbar unmounted.
const ownedRunIds = new Set<string>();

export function FlowToolbar({
  collection,
  flowName,
  tabId,
  environmentName,
  onPatchStatus,
  onPatchProgress,
  onCallbackUrls,
  onRunStateChange,
  tabRunState,
  tabRunId,
  tabPendingRunId,
  onRunRequested,
  onBeforeRun,
  onPrepareAuth,
  onStepLogs,
  onStepDebug,
  onRunResult,
}: FlowToolbarProps) {
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  // A run started by an earlier mount of this toolbar, still in progress. A
  // run that has not announced itself yet is found by the tab's pending id.
  let resumedRunId: string | null = null;
  if (activeRunId === null) {
    resumedRunId = tabRunState === 'running' ? (tabRunId ?? null) : (tabPendingRunId ?? null);
  }
  const liveRunId = activeRunId ?? resumedRunId;
  // The parent passes a new callback each render. A ref keeps the resumed
  // subscription below from resubscribing on every status patch.
  const onPatchStatusRef = useRef(onPatchStatus);
  onPatchStatusRef.current = onPatchStatus;
  const onPatchProgressRef = useRef(onPatchProgress);
  onPatchProgressRef.current = onPatchProgress;
  const onRunResultRef = useRef(onRunResult);
  onRunResultRef.current = onRunResult;
  const onRunStateChangeRef = useRef(onRunStateChange);
  onRunStateChangeRef.current = onRunStateChange;
  const onCallbackUrlsRef = useRef(onCallbackUrls);
  onCallbackUrlsRef.current = onCallbackUrls;
  // A ref, not state: `activeRunId` is only set after the save, the sign-in
  // and the event subscriptions, which all await, so between a click and that
  // point the Run button's `disabled` prop alone does not prevent a second,
  // concurrent `handleRun` call (e.g. a fast double-click). Two concurrent
  // calls would each subscribe their own listener pair and overwrite
  // `unlistenRefs.current`, permanently orphaning whichever pair loses the
  // race — a real leaked-listener bug, not just a double `runFlow` call.
  // A ref guard, checked and set synchronously before any `await`, closes
  // that window regardless of render timing.
  const isStartingRef = useRef(false);
  const unlistenRefs = useRef<UnlistenFn[]>([]);
  // True while the pre-run sign-in step is pending. State, so the button shows it.
  const [preparing, setPreparing] = useState(false);
  // Abandons the pending sign-in wait of the current attempt. Null when none.
  const abandonPrepareRef = useRef<(() => void) | null>(null);
  const mountedRef = useRef(true);

  // Unmounting abandons a pending sign-in, so its result starts no run.
  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
      abandonPrepareRef.current?.();
    };
  }, []);

  const cleanupListeners = useCallback(() => {
    for (const unlisten of unlistenRefs.current) unlisten();
    unlistenRefs.current = [];
  }, []);

  // Keep streaming step results for a run this mount did not start. The
  // mount that started it still applies the final summary when it ends. A
  // run found by its pending id is marked running when it announces itself.
  useEffect(() => {
    // The mount that is starting a run follows it itself.
    if (!resumedRunId || isStartingRef.current) return;
    // A start that outlived its toolbar still follows its run itself.
    if (ownedRunIds.has(resumedRunId)) return;
    let unlistenRun: UnlistenFn | undefined;
    let unlistenStep: UnlistenFn | undefined;
    let unlistenStarted: UnlistenFn | undefined;
    let unlistenProgress: UnlistenFn | undefined;
    let unlistenFinished: UnlistenFn | undefined;
    let disposed = false;
    void onFlowRunStarted((event) => {
      if (disposed) return;
      if (event.run_id !== resumedRunId) return;
      onRunStateChangeRef.current('running', event.run_id);
      // After the run state, because a new run drops older URLs.
      const urls = callbackUrlsFrom(event);
      if (Object.keys(urls).length > 0) onCallbackUrlsRef.current?.(urls);
    }).then((fn) => {
      if (disposed) fn();
      else unlistenRun = fn;
    });
    void onFlowStepStarted((event) => {
      if (disposed) return;
      if (event.run_id !== resumedRunId) return;
      onPatchStatusRef.current(event.node_id, 'running');
    }).then((fn) => {
      if (disposed) fn();
      else unlistenStarted = fn;
    });
    void onFlowStepCompleted((event) => {
      if (disposed) return;
      if (event.run_id !== resumedRunId) return;
      onPatchStatusRef.current(event.node_id, event.status, detailFromEvent(event));
    }).then((fn) => {
      if (disposed) fn();
      else unlistenStep = fn;
    });
    void onFlowStepProgress((event) => {
      if (disposed) return;
      if (event.run_id !== resumedRunId) return;
      forwardProgress(onPatchProgressRef.current, event);
    }).then((fn) => {
      if (disposed) fn();
      else unlistenProgress = fn;
    });
    void onFlowRunFinished((event) => {
      if (disposed) return;
      if (event.run_id !== resumedRunId) return;
      // The mount that started the run applies the timed summary later, which
      // replaces this counts-only result.
      onRunResultRef.current?.(resultFromFinishedEvent(event));
      onRunStateChangeRef.current('done', event.run_id);
    }).then((fn) => {
      if (disposed) fn();
      else unlistenFinished = fn;
    });
    return () => {
      disposed = true;
      unlistenRun?.();
      unlistenStarted?.();
      unlistenStep?.();
      unlistenProgress?.();
      unlistenFinished?.();
    };
  }, [resumedRunId]);

  const handleRun = async () => {
    if (isStartingRef.current || liveRunId !== null) return;
    isStartingRef.current = true;
    // The environment can change while the run is going, so keep the one it started with.
    const runEnvironment = environmentName;
    if (onBeforeRun) {
      let ready = false;
      try {
        ready = await onBeforeRun();
      } catch {
        ready = false;
      }
      if (!ready) {
        isStartingRef.current = false;
        return;
      }
    }
    let authTokens: Record<string, FlowAuthToken> | undefined;
    if (onPrepareAuth) {
      // Stop (or unmount) resolves this, so the toolbar stops waiting on a
      // sign-in the user abandoned. The sign-in promise itself may never settle.
      const abandoned = Symbol('abandoned');
      let abandon: () => void = () => {
        // Reassigned by the promise executor below.
      };
      const abandonPromise = new Promise<typeof abandoned>((resolve) => {
        abandon = () => resolve(abandoned);
      });
      abandonPrepareRef.current = abandon;
      setPreparing(true);
      // Called synchronously; a sync throw becomes a rejection handled below.
      // Promise.race also handles a late rejection after abandoning.
      const pending = new Promise<Record<string, FlowAuthToken> | null>((resolve) =>
        resolve(onPrepareAuth()),
      );
      try {
        const prepared = await Promise.race([pending, abandonPromise]);
        if (prepared === abandoned) {
          isStartingRef.current = false;
          if (mountedRef.current) {
            setPreparing(false);
            toast.info('Sign-in cancelled');
          }
          return;
        }
        abandonPrepareRef.current = null;
        if (mountedRef.current) setPreparing(false);
        if (prepared === null) {
          isStartingRef.current = false;
          return;
        }
        authTokens = prepared;
      } catch (err) {
        abandonPrepareRef.current = null;
        isStartingRef.current = false;
        if (mountedRef.current) {
          setPreparing(false);
          toast.error(err instanceof Error ? err.message : String(err));
        }
        return;
      }
    }
    cleanupListeners();
    // Chosen here, before anything is sent, so each event can be matched to
    // this run alone. Another tab running the same flow has its own id.
    const runId = newFlowRunId();
    // Set by flow-run-started for this id. Until then, the wall-clock start.
    let started = false;
    let startedAt = performance.now();
    // Set once run_flow settles. Events that arrive after it change nothing.
    let ended = false;
    // A function, because TypeScript keeps `started` narrowed to false in the
    // catch block below, while the event handler sets it later.
    const hasStarted = (): boolean => started;
    const isOurs = (eventRunId: string) => !ended && eventRunId === runId;
    // The listeners below outlive an unmounted toolbar, because their writes
    // go to the tab by id. A toolbar mounted later must not follow this run too.
    ownedRunIds.add(runId);

    // Subscribe first. run_flow only resolves when the run ends, so every
    // event is emitted while its promise is still pending.
    const unlistenStarted = await onFlowRunStarted((event) => {
      if (started || !isOurs(event.run_id)) return;
      started = true;
      startedAt = performance.now();
      onRunStateChange('running', runId);
      // After the run state, because a new run drops older URLs.
      const urls = callbackUrlsFrom(event);
      if (Object.keys(urls).length > 0) onCallbackUrls?.(urls);
    });
    const unlistenStepStarted = await onFlowStepStarted((event) => {
      if (!isOurs(event.run_id)) return;
      onPatchStatus(event.node_id, 'running');
    });
    const unlistenStep = await onFlowStepCompleted((event) => {
      if (!isOurs(event.run_id)) return;
      onPatchStatus(event.node_id, event.status, detailFromEvent(event));
    });
    const unlistenProgress = await onFlowStepProgress((event) => {
      if (!isOurs(event.run_id)) return;
      forwardProgress(onPatchProgressRef.current, event);
    });
    // Kept even after an unmount and freed when run_flow settles, so a run
    // whose toolbar was hidden still learns that it started.
    unlistenRefs.current = [unlistenStarted, unlistenStepStarted, unlistenStep, unlistenProgress];
    // Known before the request goes out, so Stop works and a remounted
    // toolbar can follow the run before flow-run-started arrives.
    setActiveRunId(runId);
    onRunRequested?.(runId);

    try {
      // Read fresh at click-time, not from a prop snapshotted at an earlier
      // render — the active global environment can change (via the
      // environment switcher's Global tab) while this tab sits mounted but
      // idle, and a stale value here would silently resolve the run against
      // the wrong global environment. Mirrors how runner-execute.ts reads
      // this at execution time rather than caching it.
      const globalEnvName = getActiveGlobalEnvName();
      // Tokens are sent only when there are some, so a flow without Auth nodes
      // sends no authTokens key.
      const tokens = authTokens && Object.keys(authTokens).length > 0 ? authTokens : undefined;
      const summary = await runFlow(
        collection,
        flowName,
        environmentName,
        globalEnvName ?? null,
        tokens,
        { runId },
      );
      ended = true;
      // The summary is the authoritative final state. Event delivery is not
      // guaranteed to finish before the command response arrives.
      for (const step of summary.steps) {
        onPatchStatus(step.nodeId, step.status, detailFromStep(step));
        if (step.logs?.length) onStepLogs?.(step.nodeId, step.logs);
        if (step.debugRequest) onStepDebug?.(step.nodeId, step.debugRequest);
      }
      onRunResult?.({
        ...summarizeRun(summary, Math.round(performance.now() - startedAt)),
        environmentName: runEnvironment,
      });
      onRunStateChange('done', summary.runId);
    } catch (err) {
      ended = true;
      // A run that cannot start rejects before any event is emitted.
      toast.error(`Could not run flow: ${String(err)}`);
      // A run that had started leaves a result, so its partial results stay viewable.
      if (hasStarted()) {
        onRunResult?.({
          runId,
          stoppedReason: 'error',
          totalMs: Math.round(performance.now() - startedAt),
          failedCount: 0,
          skippedCount: 0,
          environmentName: runEnvironment,
        });
      }
      onRunStateChange('done');
    } finally {
      ownedRunIds.delete(runId);
      setActiveRunId(null);
      cleanupListeners();
      isStartingRef.current = false;
    }
  };

  // The global Ctrl+Enter handler dispatches this event. The ref keeps the
  // listener from using a stale handler, and handleRun guards double starts.
  const handleRunRef = useRef(handleRun);
  handleRunRef.current = handleRun;
  useEffect(() => {
    if (!tabId) return;
    const handler = (e: Event) => {
      const detail = (e as CustomEvent<{ tabId?: string }>).detail;
      if (detail?.tabId === tabId) void handleRunRef.current();
    };
    window.addEventListener('rocket:flow-run', handler);
    return () => window.removeEventListener('rocket:flow-run', handler);
  }, [tabId]);

  const handleStop = () => {
    if (abandonPrepareRef.current) {
      abandonPrepareRef.current();
      abandonPrepareRef.current = null;
      return;
    }
    if (!liveRunId) return;
    // Cancelling a run that just finished is a no-op on the backend.
    cancelFlowRun(liveRunId).catch((err) => console.error('[FlowToolbar] cancel failed', err));
  };

  return (
    <div className='flex items-center gap-2'>
      <Button size='sm' onClick={() => void handleRun()} disabled={liveRunId !== null || preparing}>
        {preparing ? 'Signing in…' : 'Run'}
      </Button>
      <Button
        size='sm'
        variant='outline'
        onClick={handleStop}
        disabled={liveRunId === null && !preparing}
      >
        Stop
      </Button>
    </div>
  );
}
