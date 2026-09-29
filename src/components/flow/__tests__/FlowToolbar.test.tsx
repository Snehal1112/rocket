import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getActiveGlobalEnvName } from '@/lib/execute-request';
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
    onFlowStepStarted: vi.fn(),
    onFlowStepCompleted: vi.fn(),
    onFlowStepProgress: vi.fn(),
  };
});

// FlowToolbar reads the active global environment itself, at click-time, so
// mock the query-cache read the same way FlowPane's own tests do.
vi.mock('@/lib/execute-request', () => ({
  getActiveGlobalEnvName: vi.fn(),
}));

const onPatchStatus = vi.fn();
const onRunStateChange = vi.fn();

type StartedHandler = Parameters<typeof tauriApi.onFlowRunStarted>[0];
type StepHandler = Parameters<typeof tauriApi.onFlowStepCompleted>[0];

let startedHandler: StartedHandler | undefined;
let stepHandler: StepHandler | undefined;
let progressHandler: Parameters<typeof tauriApi.onFlowStepProgress>[0] | undefined;
let startedStepHandler: Parameters<typeof tauriApi.onFlowStepStarted>[0] | undefined;
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

const renderToolbar = (extra: Partial<React.ComponentProps<typeof FlowToolbar>> = {}) =>
  render(
    <FlowToolbar
      collection='my-collection'
      flowName='my-flow'
      environmentName={null}
      onPatchStatus={onPatchStatus}
      onRunStateChange={onRunStateChange}
      {...extra}
    />,
  );

describe('FlowToolbar', () => {
  beforeEach(() => {
    startedHandler = undefined;
    stepHandler = undefined;
    startedStepHandler = undefined;
    progressHandler = undefined;
    vi.mocked(tauriApi.onFlowStepProgress).mockImplementation(async (h) => {
      progressHandler = h;
      return () => {
        // Fake unlisten — no real Tauri listener to tear down in tests.
      };
    });
    vi.mocked(tauriApi.onFlowRunStarted).mockImplementation(async (h) => {
      startedHandler = h;
      return () => {
        // Fake unlisten — no real Tauri listener to tear down in tests.
      };
    });
    vi.mocked(tauriApi.onFlowStepStarted).mockImplementation(async (h) => {
      startedStepHandler = h;
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
    vi.mocked(getActiveGlobalEnvName).mockReturnValue(undefined);
    onPatchStatus.mockClear();
    onRunStateChange.mockClear();
    vi.mocked(tauriApi.cancelFlowRun).mockClear();
    vi.mocked(tauriApi.onFlowRunStarted).mockClear();
    vi.mocked(tauriApi.onFlowStepStarted).mockClear();
    vi.mocked(tauriApi.onFlowStepCompleted).mockClear();
    vi.mocked(tauriApi.runFlow).mockClear();
  });

  it('hands each summary step with logs to onStepLogs, once, in order', async () => {
    const onStepLogs = vi.fn();
    renderToolbar({ onStepLogs });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
    const base = { statusCode: null, durationMs: null, error: null, value: null };
    resolveRun({
      runId: 'r1',
      stoppedReason: 'completed',
      steps: [
        { ...base, nodeId: 'a', status: 'success', logs: [{ level: 'log', message: 'one' }] },
        { ...base, nodeId: 'b', status: 'success' },
        {
          ...base,
          nodeId: 'c',
          status: 'failed',
          error: 'x',
          logs: [{ level: 'error', message: 'two' }],
        },
      ],
    });
    await waitFor(() => expect(onStepLogs).toHaveBeenCalledTimes(2));
    expect(onStepLogs).toHaveBeenNthCalledWith(1, 'a', [{ level: 'log', message: 'one' }]);
    expect(onStepLogs).toHaveBeenNthCalledWith(2, 'c', [{ level: 'error', message: 'two' }]);
  });

  it('hands each summary step with a debug request to onStepDebug, once, in order', async () => {
    const debugA = { method: 'POST', url: 'https://x.test/a', headers: [], body: '{}' };
    const onStepDebug = vi.fn();
    renderToolbar({ onStepDebug });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
    const base = { statusCode: 200, durationMs: 5, error: null, value: null };
    resolveRun({
      runId: 'r1',
      stoppedReason: 'completed',
      steps: [
        { ...base, nodeId: 'a', status: 'success', debugRequest: debugA },
        { ...base, nodeId: 'b', status: 'success' },
      ],
    });
    await waitFor(() => expect(onStepDebug).toHaveBeenCalledTimes(1));
    expect(onStepDebug).toHaveBeenCalledWith('a', debugA);
  });

  it('subscribes before running, takes the run id from flow-run-started, and finishes on resolve', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() =>
      expect(tauriApi.runFlow).toHaveBeenCalledWith('my-collection', 'my-flow', null, null),
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
      value: null,
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
      steps: [
        {
          nodeId: 'n1',
          status: 'success',
          statusCode: 200,
          durationMs: 184,
          error: null,
          value: null,
        },
      ],
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

  it('forwards skip_reason and branch from flow-step-completed', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(stepHandler).toBeDefined());
    started('run-123');
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'run-123',
      node_id: 'if1',
      status: 'success',
      status_code: null,
      duration_ms: null,
      error: null,
      value: null,
      branch: 'false',
    });
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'run-123',
      node_id: 'n2',
      status: 'skipped',
      status_code: null,
      duration_ms: null,
      error: null,
      value: null,
      skip_reason: 'branch_not_taken',
    });
    expect(onPatchStatus).toHaveBeenCalledWith(
      'if1',
      'success',
      expect.objectContaining({ branch: 'false', skipReason: undefined }),
    );
    expect(onPatchStatus).toHaveBeenCalledWith(
      'n2',
      'skipped',
      expect.objectContaining({ skipReason: 'branch_not_taken', branch: undefined }),
    );
  });

  it('applies skipReason and branch from the returned summary', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    started('run-123');
    resolveRun({
      runId: 'run-123',
      steps: [
        {
          nodeId: 'sw1',
          status: 'success',
          statusCode: null,
          durationMs: null,
          error: null,
          value: null,
          branch: 'case:c1',
        },
        {
          nodeId: 'n3',
          status: 'skipped',
          statusCode: null,
          durationMs: null,
          error: null,
          value: null,
          skipReason: 'upstream_failed',
        },
      ],
      stoppedReason: 'completed',
    });
    await waitFor(() =>
      expect(onPatchStatus).toHaveBeenCalledWith(
        'n3',
        'skipped',
        expect.objectContaining({ skipReason: 'upstream_failed' }),
      ),
    );
    expect(onPatchStatus).toHaveBeenCalledWith(
      'sw1',
      'success',
      expect.objectContaining({ branch: 'case:c1' }),
    );
  });

  it('forwards the active global environment name to runFlow when set', async () => {
    vi.mocked(getActiveGlobalEnvName).mockReturnValue('shared-global');
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() =>
      expect(tauriApi.runFlow).toHaveBeenCalledWith(
        'my-collection',
        'my-flow',
        null,
        'shared-global',
      ),
    );
  });

  it('reads the global environment name fresh at click-time, not from an earlier render', async () => {
    // Simulates the user switching the active global environment (via the
    // environment switcher) while this toolbar sits mounted but idle, before
    // ever clicking Run — the exact staleness gap a snapshot-in-props would
    // have missed.
    vi.mocked(getActiveGlobalEnvName).mockReturnValue('stale-global');
    renderToolbar();
    vi.mocked(getActiveGlobalEnvName).mockReturnValue('fresh-global');
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() =>
      expect(tauriApi.runFlow).toHaveBeenCalledWith(
        'my-collection',
        'my-flow',
        null,
        'fresh-global',
      ),
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

  it('does not start a run when onBeforeRun reports a failed save', async () => {
    const onBeforeRun = vi.fn().mockResolvedValue(false);
    render(
      <FlowToolbar
        collection='my-collection'
        flowName='my-flow'
        environmentName={null}
        onPatchStatus={onPatchStatus}
        onRunStateChange={onRunStateChange}
        onBeforeRun={onBeforeRun}
      />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(onBeforeRun).toHaveBeenCalledTimes(1));
    expect(tauriApi.onFlowRunStarted).not.toHaveBeenCalled();
    expect(tauriApi.runFlow).not.toHaveBeenCalled();
  });

  it('a remounted toolbar picks up a run still in progress from the tab state', async () => {
    render(
      <FlowToolbar
        collection='my-collection'
        flowName='my-flow'
        environmentName={null}
        onPatchStatus={onPatchStatus}
        onRunStateChange={onRunStateChange}
        tabRunState='running'
        tabRunId='run-9'
      />,
    );
    expect(screen.getByRole('button', { name: 'Run' })).toBeDisabled();
    await waitFor(() => expect(stepHandler).toBeDefined());
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'run-9',
      node_id: 'node-a',
      status: 'success',
      status_code: 200,
      duration_ms: 5,
      error: null,
      value: null,
    });
    expect(onPatchStatus).toHaveBeenCalledWith('node-a', 'success', {
      statusCode: 200,
      durationMs: 5,
      error: undefined,
    });
    await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(tauriApi.cancelFlowRun).toHaveBeenCalledWith('run-9');
  });

  it('shows a node as running when flow-step-started fires', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedStepHandler).toBeDefined());
    started('run-123');
    startedStepHandler?.({ type: 'flowStepStarted', run_id: 'run-123', node_id: 'node-a' });
    expect(onPatchStatus).toHaveBeenCalledWith('node-a', 'running');
  });

  it('forwards flow-step-progress messages for the active run', async () => {
    const onPatchProgress = vi.fn();
    renderToolbar({ onPatchProgress });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(progressHandler).toBeDefined());
    started('run-123');
    progressHandler?.({
      type: 'flowStepProgress',
      run_id: 'run-123',
      node_id: 'node-a',
      attempt: 3,
      max_attempts: 30,
      message: 'attempt 3/30',
    });
    expect(onPatchProgress).toHaveBeenCalledWith('node-a', 'attempt 3/30');
  });

  it('ignores flow-step-progress for another run', async () => {
    const onPatchProgress = vi.fn();
    renderToolbar({ onPatchProgress });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(progressHandler).toBeDefined());
    started('run-123');
    progressHandler?.({
      type: 'flowStepProgress',
      run_id: 'other-run',
      node_id: 'node-a',
      attempt: null,
      max_attempts: null,
      message: 'waiting',
    });
    expect(onPatchProgress).not.toHaveBeenCalled();
  });

  it('a remounted toolbar forwards progress for the resumed run', async () => {
    const onPatchProgress = vi.fn();
    render(
      <FlowToolbar
        collection='my-collection'
        flowName='my-flow'
        environmentName={null}
        onPatchStatus={onPatchStatus}
        onPatchProgress={onPatchProgress}
        onRunStateChange={onRunStateChange}
        tabRunState='running'
        tabRunId='run-9'
      />,
    );
    await waitFor(() => expect(progressHandler).toBeDefined());
    progressHandler?.({
      type: 'flowStepProgress',
      run_id: 'run-9',
      node_id: 'node-a',
      attempt: 1,
      max_attempts: 5,
      message: 'attempt 1/5',
    });
    expect(onPatchProgress).toHaveBeenCalledWith('node-a', 'attempt 1/5');
  });
});
