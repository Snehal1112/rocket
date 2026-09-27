import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import * as tauriApi from '@/lib/tauri-api';
import { FlowToolbar } from '../FlowToolbar';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof tauriApi>('@/lib/tauri-api');
  return {
    ...actual,
    runFlow: vi.fn(),
    cancelFlowRun: vi.fn(),
    saveFlow: vi.fn(),
    onFlowRunStarted: vi.fn(),
    onFlowStepCompleted: vi.fn(),
  };
});

const onPatchStatus = vi.fn();
const onRunStateChange = vi.fn();

type StartedHandler = Parameters<typeof tauriApi.onFlowRunStarted>[0];
type StepHandler = Parameters<typeof tauriApi.onFlowStepCompleted>[0];

let startedHandler: StartedHandler | undefined;
let stepHandler: StepHandler | undefined;
let resolveRun: (summary: tauriApi.FlowRunSummary) => void = () => {
  // Reassigned by beforeEach's mock implementation before use.
};

const started = (runId: string, flowName = 'my-flow') =>
  startedHandler?.({
    type: 'flowRunStarted',
    run_id: runId,
    flow_name: flowName,
    collection: 'my-collection',
    total_nodes: 1,
  });

const renderToolbar = () =>
  render(
    <FlowToolbar
      collection='my-collection'
      flowName='my-flow'
      environmentName={null}
      onPatchStatus={onPatchStatus}
      onRunStateChange={onRunStateChange}
    />,
  );

describe('FlowToolbar', () => {
  beforeEach(() => {
    startedHandler = undefined;
    stepHandler = undefined;
    vi.mocked(tauriApi.onFlowRunStarted).mockImplementation(async (h) => {
      startedHandler = h;
      return () => {
        // Fake unlisten — no real Tauri listener to tear down in tests.
      };
    });
    vi.mocked(tauriApi.onFlowStepCompleted).mockImplementation(async (h) => {
      stepHandler = h;
      return () => {
        // Fake unlisten — no real Tauri listener to tear down in tests.
      };
    });
    // run_flow stays pending until the test resolves it, like the real backend.
    vi.mocked(tauriApi.runFlow).mockImplementation(
      () =>
        new Promise((resolve) => {
          resolveRun = resolve;
        }),
    );
    vi.mocked(tauriApi.saveFlow).mockResolvedValue(undefined);
    vi.mocked(tauriApi.cancelFlowRun).mockResolvedValue(undefined);
    onPatchStatus.mockClear();
    onRunStateChange.mockClear();
    vi.mocked(tauriApi.cancelFlowRun).mockClear();
    vi.mocked(tauriApi.onFlowRunStarted).mockClear();
    vi.mocked(tauriApi.onFlowStepCompleted).mockClear();
    vi.mocked(tauriApi.runFlow).mockClear();
  });

  it('subscribes before running, takes the run id from flow-run-started, and finishes on resolve', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() =>
      expect(tauriApi.runFlow).toHaveBeenCalledWith('my-collection', 'my-flow', null),
    );
    expect(startedHandler).toBeDefined();
    expect(stepHandler).toBeDefined();

    started('run-123');
    expect(onRunStateChange).toHaveBeenCalledWith('running', 'run-123');

    resolveRun({ runId: 'run-123', steps: [], stoppedReason: 'completed' });
    await waitFor(() => expect(onRunStateChange).toHaveBeenCalledWith('done', 'run-123'));
  });

  it('ignores a flow-run-started event for a different flow', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    started('run-other', 'some-other-flow');
    expect(onRunStateChange).not.toHaveBeenCalled();
  });

  it('a step-completed event for a different run id is ignored', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(stepHandler).toBeDefined());
    started('run-123');
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'some-other-run',
      node_id: 'node-a',
      status: 'success',
      status_code: 200,
      duration_ms: 5,
      error: null,
    });
    expect(onPatchStatus).not.toHaveBeenCalled();
  });

  it('applies the returned summary steps as the final state', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    started('run-123');
    resolveRun({
      runId: 'run-123',
      steps: [{ nodeId: 'n1', status: 'success', statusCode: 200, durationMs: 184, error: null }],
      stoppedReason: 'completed',
    });
    await waitFor(() =>
      expect(onPatchStatus).toHaveBeenCalledWith('n1', 'success', {
        statusCode: 200,
        durationMs: 184,
        error: undefined,
      }),
    );
  });

  it('Stop calls cancelFlowRun with the active run id', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    started('run-123');
    await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(tauriApi.cancelFlowRun).toHaveBeenCalledWith('run-123');
  });

  it('Stop is a no-op when no run is active', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(tauriApi.cancelFlowRun).not.toHaveBeenCalled();
  });

  it('a rapid double-click on Run only starts one run and does not orphan a listener pair', async () => {
    renderToolbar();
    const runButton = screen.getByRole('button', { name: 'Run' });
    // `activeRunId` (and thus the button's `disabled` prop) is only set once
    // the flow-run-started event round-trips through the backend, so a
    // second click before that event arrives must still be a no-op — both
    // for the backend call and for listener subscription (a second
    // subscribe pass would overwrite unlistenRefs and orphan the first
    // pair, since only the ref's current contents get unlistened later).
    await userEvent.click(runButton);
    await userEvent.click(runButton);
    await waitFor(() => expect(tauriApi.onFlowRunStarted).toHaveBeenCalledTimes(1));
    expect(tauriApi.runFlow).toHaveBeenCalledTimes(1);
  });
});
