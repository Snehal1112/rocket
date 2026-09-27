import type { UnlistenFn } from '@tauri-apps/api/event';
import { useCallback, useEffect, useRef, useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { cancelFlowRun, onFlowRunStarted, onFlowStepCompleted, runFlow } from '@/lib/tauri-api';

type NodeDetail = { statusCode?: number; durationMs?: number; error?: string };

interface FlowToolbarProps {
  collection: string;
  flowName: string;
  environmentName: string | null;
  onPatchStatus: (nodeId: string, status: string, detail?: NodeDetail) => void;
  onRunStateChange: (state: 'running' | 'done', runId?: string) => void;
}

export function FlowToolbar({
  collection,
  flowName,
  environmentName,
  onPatchStatus,
  onRunStateChange,
}: FlowToolbarProps) {
  const [activeRunId, setActiveRunId] = useState<string | null>(null);
  const unlistenRefs = useRef<UnlistenFn[]>([]);

  const cleanupListeners = useCallback(() => {
    for (const unlisten of unlistenRefs.current) unlisten();
    unlistenRefs.current = [];
  }, []);

  // Unsubscribe when the tab closes mid-run.
  useEffect(() => cleanupListeners, [cleanupListeners]);

  const handleRun = async () => {
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
    const unlistenStep = await onFlowStepCompleted((event) => {
      if (runId === null || event.run_id !== runId) return;
      onPatchStatus(event.node_id, event.status, {
        statusCode: event.status_code ?? undefined,
        durationMs: event.duration_ms ?? undefined,
        error: event.error ?? undefined,
      });
    });
    unlistenRefs.current = [unlistenStarted, unlistenStep];

    try {
      const summary = await runFlow(collection, flowName, environmentName);
      // The summary is the authoritative final state. Event delivery is not
      // guaranteed to finish before the command response arrives.
      for (const step of summary.steps) {
        onPatchStatus(step.nodeId, step.status, {
          statusCode: step.statusCode ?? undefined,
          durationMs: step.durationMs ?? undefined,
          error: step.error ?? undefined,
        });
      }
      onRunStateChange('done', summary.runId);
    } catch (err) {
      // A run that cannot start rejects before any event is emitted.
      toast.error(`Could not run flow: ${String(err)}`);
      onRunStateChange('done');
    } finally {
      setActiveRunId(null);
      cleanupListeners();
    }
  };

  const handleStop = () => {
    if (!activeRunId) return;
    void cancelFlowRun(activeRunId);
  };

  return (
    <div className='flex items-center gap-2'>
      <Button size='sm' onClick={() => void handleRun()} disabled={activeRunId !== null}>
        Run
      </Button>
      <Button size='sm' variant='outline' onClick={handleStop}>
        Stop
      </Button>
    </div>
  );
}
