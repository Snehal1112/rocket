import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { describe, expect, it, vi } from 'vitest';

vi.mock('@tauri-apps/api/core', () => ({ invoke: vi.fn() }));
vi.mock('@tauri-apps/api/event', () => ({ listen: vi.fn() }));

describe('flow tauri-api bindings', () => {
  it('listFlows invokes list_flows with the collection name', async () => {
    vi.mocked(invoke).mockResolvedValue(['My Flow']);
    const { listFlows } = await import('@/lib/tauri-api');
    const result = await listFlows('my-collection');
    expect(invoke).toHaveBeenCalledWith('list_flows', { collection: 'my-collection' });
    expect(result).toEqual(['My Flow']);
  });

  it('getFlow invokes get_flow with collection and name', async () => {
    const sample = { name: 'My Flow', nodes: [], edges: [] };
    vi.mocked(invoke).mockResolvedValue(sample);
    const { getFlow } = await import('@/lib/tauri-api');
    const result = await getFlow('my-collection', 'My Flow');
    expect(invoke).toHaveBeenCalledWith('get_flow', {
      collection: 'my-collection',
      name: 'My Flow',
    });
    expect(result).toEqual(sample);
  });

  it('saveFlow invokes save_flow with collection and flow', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { saveFlow } = await import('@/lib/tauri-api');
    const flow = { name: 'My Flow', nodes: [], edges: [] };
    await saveFlow('my-collection', flow);
    expect(invoke).toHaveBeenCalledWith('save_flow', { collection: 'my-collection', flow });
  });

  it('deleteFlow invokes delete_flow with collection and name', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { deleteFlow } = await import('@/lib/tauri-api');
    await deleteFlow('my-collection', 'My Flow');
    expect(invoke).toHaveBeenCalledWith('delete_flow', {
      collection: 'my-collection',
      name: 'My Flow',
    });
  });

  it('runFlow invokes run_flow with a nested input object and resolves with the summary', async () => {
    const summary = { runId: 'run-1', steps: [], stoppedReason: 'completed' };
    vi.mocked(invoke).mockResolvedValue(summary);
    const { runFlow } = await import('@/lib/tauri-api');
    const result = await runFlow('my-collection', 'My Flow', 'staging');
    expect(invoke).toHaveBeenCalledWith('run_flow', {
      input: {
        collection: 'my-collection',
        flowName: 'My Flow',
        environmentName: 'staging',
      },
    });
    expect(result).toEqual(summary);
  });

  it('runFlow sends a null environmentName when none is given', async () => {
    vi.mocked(invoke).mockResolvedValue({ runId: 'run-1', steps: [], stoppedReason: 'completed' });
    const { runFlow } = await import('@/lib/tauri-api');
    await runFlow('my-collection', 'My Flow');
    expect(invoke).toHaveBeenCalledWith('run_flow', {
      input: { collection: 'my-collection', flowName: 'My Flow', environmentName: null },
    });
  });

  it('cancelFlowRun invokes cancel_flow_run with the run id', async () => {
    vi.mocked(invoke).mockResolvedValue(undefined);
    const { cancelFlowRun } = await import('@/lib/tauri-api');
    await cancelFlowRun('run-1');
    expect(invoke).toHaveBeenCalledWith('cancel_flow_run', { runId: 'run-1' });
  });

  it('onFlowRunStarted subscribes to the flow-run-started event and unwraps the payload', async () => {
    const payload = {
      type: 'flowRunStarted',
      run_id: 'run-1',
      flow_name: 'My Flow',
      collection: 'my-collection',
      total_nodes: 2,
    };
    vi.mocked(listen).mockImplementation(((
      _event: string,
      cb: (e: { payload: unknown }) => void,
    ) => {
      cb({ payload });
      return Promise.resolve(() => undefined);
    }) as typeof listen);
    const { onFlowRunStarted } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onFlowRunStarted(handler);
    expect(listen).toHaveBeenCalledWith('flow-run-started', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });

  it('onFlowStepCompleted subscribes to the flow-step-completed event and unwraps the payload', async () => {
    const payload = {
      type: 'flowStepCompleted',
      run_id: 'run-1',
      node_id: 'n1',
      status: 'success' as const,
      status_code: 200,
      duration_ms: 12,
      error: null,
    };
    vi.mocked(listen).mockImplementation(((
      _event: string,
      cb: (e: { payload: unknown }) => void,
    ) => {
      cb({ payload });
      return Promise.resolve(() => undefined);
    }) as typeof listen);
    const { onFlowStepCompleted } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onFlowStepCompleted(handler);
    expect(listen).toHaveBeenCalledWith('flow-step-completed', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });

  it('onFlowRunFinished subscribes to the flow-run-finished event and unwraps the payload', async () => {
    const payload = {
      type: 'flowRunFinished',
      run_id: 'run-1',
      stopped_reason: 'completed',
      node_count: 3,
      failed_count: 0,
      skipped_count: 0,
    };
    vi.mocked(listen).mockImplementation(((
      _event: string,
      cb: (e: { payload: unknown }) => void,
    ) => {
      cb({ payload });
      return Promise.resolve(() => undefined);
    }) as typeof listen);
    const { onFlowRunFinished } = await import('@/lib/tauri-api');
    const handler = vi.fn();
    await onFlowRunFinished(handler);
    expect(listen).toHaveBeenCalledWith('flow-run-finished', expect.any(Function));
    expect(handler).toHaveBeenCalledWith(payload);
  });
});
