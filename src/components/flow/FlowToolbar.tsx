import type { UnlistenFn } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { getActiveGlobalEnvName } from '@/lib/execute-request';
import {
  cancelFlowRun,
  type FlowDebugRequest,
  type FlowLogEntry,
  type FlowStepCompletedEvent,
  type FlowStepResult,
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
  environmentName: string | null;
  onPatchStatus: (nodeId: string, status: string, detail?: FlowNodeDetail) => void;
  // Receives progress text for a running node, such as "attempt 3/30".
  onPatchProgress?: (nodeId: string, message: string) => void;
  onRunStateChange: (state: 'running' | 'done', runId?: string) => void;
  // The tab's stored run state. The toolbar unmounts when its tab is hidden,
  // so a remounted toolbar reads an in-progress run from here.
  tabRunState?: 'idle' | 'running' | 'done';
  tabRunId?: string;
  // Runs before a new run starts. `run_flow` runs the flow saved on disk, so
  // this saves unsaved canvas edits first. Returning false aborts the run.
  onBeforeRun?: () => Promise<boolean>;
  // Receives each step's script console output once the run ends.
  onStepLogs?: (nodeId: string, logs: FlowLogEntry[]) => void;
  // Receives each debug node's sent request once the run ends.
  onStepDebug?: (nodeId: string, debug: FlowDebugRequest) => void;
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
  };
}

export function FlowToolbar({
  collection,
  flowName,
  environmentName,
  onPatchStatus,
  onPatchProgress,
  onRunStateChange,
  tabRunState,
  tabRunId,
  onBeforeRun,
  onStepLogs,
  onStepDebug,
}: FlowToolbarProps) {
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  // A run started by an earlier mount of this toolbar, still in progress.
  const resumedRunId =
    activeRunId === null && tabRunState === 'running' ? (tabRunId ?? null) : null;
  const liveRunId = activeRunId ?? resumedRunId;
  // The parent passes a new callback each render. A ref keeps the resumed
  // subscription below from resubscribing on every status patch.
  const onPatchStatusRef = useRef(onPatchStatus);
  onPatchStatusRef.current = onPatchStatus;
  const onPatchProgressRef = useRef(onPatchProgress);
  onPatchProgressRef.current = onPatchProgress;
  const unlistenRefs = useRef<UnlistenFn[]>([]);
  // A ref, not state: `activeRunId` is only set once the `flow-run-started`
  // event round-trips through the backend, so between a click and that
  // event the Run button's `disabled` prop alone does not prevent a second,
  // concurrent `handleRun` call (e.g. a fast double-click). Two concurrent
  // calls would each subscribe their own listener pair and overwrite
  // `unlistenRefs.current`, permanently orphaning whichever pair loses the
  // race — a real leaked-listener bug, not just a double `runFlow` call.
  // A ref guard, checked and set synchronously before any `await`, closes
  // that window regardless of render timing.
  const isStartingRef = useRef(false);

  const cleanupListeners = useCallback(() => {
    for (const unlisten of unlistenRefs.current) unlisten();
    unlistenRefs.current = [];
  }, []);

  // Unsubscribe when the tab closes mid-run.
  useEffect(() => cleanupListeners, [cleanupListeners]);

  // Keep streaming step results for a run this mount did not start. The
  // mount that started it still applies the final summary when it ends.
  useEffect(() => {
    if (!resumedRunId) return;
    let unlistenStep: UnlistenFn | undefined;
    let unlistenStarted: UnlistenFn | undefined;
    let unlistenProgress: UnlistenFn | undefined;
    let disposed = false;
    void onFlowStepStarted((event) => {
      if (event.run_id !== resumedRunId) return;
      onPatchStatusRef.current(event.node_id, 'running');
    }).then((fn) => {
      if (disposed) fn();
      else unlistenStarted = fn;
    });
    void onFlowStepCompleted((event) => {
      if (event.run_id !== resumedRunId) return;
      onPatchStatusRef.current(event.node_id, event.status, detailFromEvent(event));
    }).then((fn) => {
      if (disposed) fn();
      else unlistenStep = fn;
    });
    void onFlowStepProgress((event) => {
      if (event.run_id !== resumedRunId) return;
      onPatchProgressRef.current?.(event.node_id, event.message);
    }).then((fn) => {
      if (disposed) fn();
      else unlistenProgress = fn;
    });
    return () => {
      disposed = true;
      unlistenStarted?.();
      unlistenStep?.();
      unlistenProgress?.();
    };
  }, [resumedRunId]);

  const handleRun = async () => {
    if (isStartingRef.current || liveRunId !== null) return;
    isStartingRef.current = true;
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
    cleanupListeners();
    // Held in a local, not state, so the event handlers see it at once.
    let runId: string | null = null;

    // Subscribe first. run_flow only resolves when the run ends, so every
    // event is emitted while its promise is still pending.
    const unlistenStarted = await onFlowRunStarted((event) => {
      if (runId !== null) return;
      if (event.collection !== collection || event.flow_name !== flowName) return;
      runId = event.run_id;
      setActiveRunId(event.run_id);
      onRunStateChange('running', event.run_id);
    });
    const unlistenStepStarted = await onFlowStepStarted((event) => {
      if (runId === null || event.run_id !== runId) return;
      onPatchStatus(event.node_id, 'running');
    });
    const unlistenStep = await onFlowStepCompleted((event) => {
      if (runId === null || event.run_id !== runId) return;
      onPatchStatus(event.node_id, event.status, detailFromEvent(event));
    });
    const unlistenProgress = await onFlowStepProgress((event) => {
      if (runId === null || event.run_id !== runId) return;
      onPatchProgressRef.current?.(event.node_id, event.message);
    });
    unlistenRefs.current = [unlistenStarted, unlistenStepStarted, unlistenStep, unlistenProgress];

    try {
      // Read fresh at click-time, not from a prop snapshotted at an earlier
      // render — the active global environment can change (via the
      // environment switcher's Global tab) while this tab sits mounted but
      // idle, and a stale value here would silently resolve the run against
      // the wrong global environment. Mirrors how runner-execute.ts reads
      // this at execution time rather than caching it.
      const globalEnvName = getActiveGlobalEnvName();
      const summary = await runFlow(collection, flowName, environmentName, globalEnvName ?? null);
      // The summary is the authoritative final state. Event delivery is not
      // guaranteed to finish before the command response arrives.
      for (const step of summary.steps) {
        onPatchStatus(step.nodeId, step.status, detailFromStep(step));
        if (step.logs?.length) onStepLogs?.(step.nodeId, step.logs);
        if (step.debugRequest) onStepDebug?.(step.nodeId, step.debugRequest);
      }
      onRunStateChange('done', summary.runId);
    } catch (err) {
      // A run that cannot start rejects before any event is emitted.
      toast.error(`Could not run flow: ${String(err)}`);
      onRunStateChange('done');
    } finally {
      setActiveRunId(null);
      cleanupListeners();
      isStartingRef.current = false;
    }
  };

  const handleStop = () => {
    if (!liveRunId) return;
    // Cancelling a run that just finished is a no-op on the backend.
    cancelFlowRun(liveRunId).catch((err) => console.error('[FlowToolbar] cancel failed', err));
  };

  return (
    <div className='flex items-center gap-2'>
      <Button size='sm' onClick={() => void handleRun()} disabled={liveRunId !== null}>
        Run
      </Button>
      <Button size='sm' variant='outline' onClick={handleStop}>
        Stop
      </Button>
    </div>
  );
}
