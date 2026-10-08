import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { getActiveGlobalEnvName } from '@/lib/execute-request';
import { newFlowRunId } from '@/lib/flow-run-id';
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
    onFlowRunFinished: vi.fn(),
  };
});

// FlowToolbar reads the active global environment itself, at click-time, so
// mock the query-cache read the same way FlowPane's own tests do.
vi.mock('@/lib/execute-request', () => ({
  getActiveGlobalEnvName: vi.fn(),
}));

vi.mock('sonner', () => ({ toast: { error: vi.fn(), info: vi.fn() } }));
// The toolbar's run id. Tests send events with this id.
vi.mock('@/lib/flow-run-id', () => ({ newFlowRunId: vi.fn() }));

const onPatchStatus = vi.fn();
const onRunStateChange = vi.fn();

type StartedHandler = Parameters<typeof tauriApi.onFlowRunStarted>[0];
type StepHandler = Parameters<typeof tauriApi.onFlowStepCompleted>[0];

let startedHandler: StartedHandler | undefined;
let stepHandler: StepHandler | undefined;
let progressHandler: Parameters<typeof tauriApi.onFlowStepProgress>[0] | undefined;
let startedStepHandler: Parameters<typeof tauriApi.onFlowStepStarted>[0] | undefined;
let finishedHandler: Parameters<typeof tauriApi.onFlowRunFinished>[0] | undefined;
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
    vi.mocked(newFlowRunId).mockReset();
    vi.mocked(newFlowRunId).mockReturnValue('run-123');
    startedHandler = undefined;
    stepHandler = undefined;
    startedStepHandler = undefined;
    progressHandler = undefined;
    finishedHandler = undefined;
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
    vi.mocked(tauriApi.onFlowRunFinished).mockImplementation(async (h) => {
      finishedHandler = h;
      return () => {
        // Fake unlisten — no real Tauri listener to tear down in tests.
      };
    });
    vi.mocked(tauriApi.onFlowRunFinished).mockClear();
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
    vi.mocked(toast.error).mockClear();
    vi.mocked(toast.info).mockClear();
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

  it('subscribes before running, sends its own run id, and finishes on resolve', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() =>
      expect(tauriApi.runFlow).toHaveBeenCalledWith(
        'my-collection',
        'my-flow',
        null,
        null,
        undefined,
        { runId: 'run-123' },
      ),
    );
    expect(startedHandler).toBeDefined();
    expect(stepHandler).toBeDefined();

    started('run-123');
    expect(onRunStateChange).toHaveBeenCalledWith('running', 'run-123');

    resolveRun({ runId: 'run-123', steps: [], stoppedReason: 'completed' });
    await waitFor(() => expect(onRunStateChange).toHaveBeenCalledWith('done', 'run-123'));
  });

  it('ignores a flow-run-started event for another run of the same flow', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
    // Same collection and flow name, another tab's run.
    started('run-other');
    expect(onRunStateChange).not.toHaveBeenCalled();
    started('run-123');
    expect(onRunStateChange).toHaveBeenCalledWith('running', 'run-123');
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

  it('forwards attempts from the step event and the summary', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(stepHandler).toBeDefined());
    started('run-123');
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'run-123',
      node_id: 'job',
      status: 'success',
      status_code: 200,
      duration_ms: 14200,
      error: null,
      value: null,
      attempts: 7,
    });
    expect(onPatchStatus).toHaveBeenCalledWith(
      'job',
      'success',
      expect.objectContaining({ attempts: 7 }),
    );

    resolveRun({
      runId: 'run-123',
      steps: [
        {
          nodeId: 'job',
          status: 'success',
          statusCode: 200,
          durationMs: 14200,
          error: null,
          value: null,
          attempts: 7,
        },
      ],
      stoppedReason: 'completed',
    });
    await waitFor(() =>
      expect(onPatchStatus).toHaveBeenLastCalledWith(
        'job',
        'success',
        expect.objectContaining({ attempts: 7 }),
      ),
    );
  });

  it('stores the exchange and logs from the step event and the summary', async () => {
    const exchange = {
      method: 'GET',
      url: 'https://x.test',
      headers: [],
      response: {
        status: 200,
        statusText: 'OK',
        durationMs: 5,
        sizeBytes: 2,
        headers: [],
        body: '{}',
      },
    };
    const logs = [{ level: 'log' as const, message: 'hi' }];
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(stepHandler).toBeDefined());
    started('run-123');
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'run-123',
      node_id: 'r',
      status: 'success',
      status_code: 200,
      duration_ms: 5,
      error: null,
      value: null,
      logs,
      exchange,
    });
    expect(onPatchStatus).toHaveBeenCalledWith(
      'r',
      'success',
      expect.objectContaining({ exchange, logs }),
    );

    resolveRun({
      runId: 'run-123',
      steps: [
        {
          nodeId: 'r',
          status: 'success',
          statusCode: 200,
          durationMs: 5,
          error: null,
          value: null,
          logs,
          exchange,
        },
      ],
      stoppedReason: 'completed',
    });
    await waitFor(() =>
      expect(onPatchStatus).toHaveBeenLastCalledWith(
        'r',
        'success',
        expect.objectContaining({ exchange, logs }),
      ),
    );
  });

  it('stores the trace from the step event and the summary', async () => {
    const trace: tauriApi.FlowStepTrace = {
      wires: [{ edgeId: 'e1', sourceNodeId: 'in', targetField: 'value', value: '••••••' }],
      route: { kind: 'if', value: 'true' },
      failedEdgeId: 'e1',
    };
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(stepHandler).toBeDefined());
    started('run-123');
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'run-123',
      node_id: 'n',
      status: 'success',
      status_code: null,
      duration_ms: 4,
      error: null,
      value: null,
      trace,
    });
    expect(onPatchStatus).toHaveBeenCalledWith(
      'n',
      'success',
      expect.objectContaining({ trace, durationMs: 4 }),
    );

    resolveRun({
      runId: 'run-123',
      stoppedReason: 'completed',
      steps: [
        {
          nodeId: 'n',
          status: 'success',
          statusCode: null,
          durationMs: 4,
          error: null,
          value: null,
          trace,
        },
      ],
    });
    await waitFor(() =>
      expect(onPatchStatus).toHaveBeenLastCalledWith(
        'n',
        'success',
        expect.objectContaining({ trace }),
      ),
    );
  });

  it('leaves the trace unset for a step without one', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(stepHandler).toBeDefined());
    started('run-123');
    stepHandler?.({
      type: 'flowStepCompleted',
      run_id: 'run-123',
      node_id: 'n',
      status: 'success',
      status_code: 200,
      duration_ms: 5,
      error: null,
      value: null,
    });
    const detail = onPatchStatus.mock.calls[onPatchStatus.mock.calls.length - 1]?.[2];
    expect(detail).toBeDefined();
    expect(detail?.trace).toBeUndefined();
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
        undefined,
        { runId: 'run-123' },
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
        undefined,
        { runId: 'run-123' },
      ),
    );
  });

  it('passes the tokens from onPrepareAuth to runFlow', async () => {
    const onPrepareAuth = vi.fn().mockResolvedValue({ a: { accessToken: 'tok-123456' } });
    renderToolbar({ onPrepareAuth });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() =>
      expect(tauriApi.runFlow).toHaveBeenCalledWith(
        'my-collection',
        'my-flow',
        null,
        null,
        { a: { accessToken: 'tok-123456' } },
        { runId: 'run-123' },
      ),
    );
  });

  it('sends no tokens when there are none', async () => {
    const onPrepareAuth = vi.fn().mockResolvedValue({});
    renderToolbar({ onPrepareAuth });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() =>
      expect(tauriApi.runFlow).toHaveBeenCalledWith(
        'my-collection',
        'my-flow',
        null,
        null,
        undefined,
        { runId: 'run-123' },
      ),
    );
  });

  it('does not start the run when the sign-in step fails, and says why', async () => {
    const onPrepareAuth = vi
      .fn()
      .mockRejectedValue(new Error('Sign-in for Auth node "SSO" failed: window closed'));
    renderToolbar({ onPrepareAuth });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        expect.stringContaining('Sign-in for Auth node "SSO" failed: window closed'),
      ),
    );
    expect(tauriApi.runFlow).not.toHaveBeenCalled();
    // The Run button works again.
    expect(screen.getByRole('button', { name: 'Run' })).toBeEnabled();
  });

  it('does not start the run when onPrepareAuth returns null', async () => {
    const onPrepareAuth = vi.fn().mockResolvedValue(null);
    renderToolbar({ onPrepareAuth });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(onPrepareAuth).toHaveBeenCalled());
    expect(tauriApi.runFlow).not.toHaveBeenCalled();
  });

  it('runs onPrepareAuth after onBeforeRun saves', async () => {
    const order: string[] = [];
    const onBeforeRun = vi.fn(async () => {
      order.push('save');
      return true;
    });
    const onPrepareAuth = vi.fn(async () => {
      order.push('auth');
      return {};
    });
    renderToolbar({ onBeforeRun, onPrepareAuth });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
    expect(order).toEqual(['save', 'auth']);
  });

  it('Stop calls cancelFlowRun with the active run id', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    started('run-123');
    await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
    expect(tauriApi.cancelFlowRun).toHaveBeenCalledWith('run-123');
  });

  it('Stop is disabled when no run is active', () => {
    renderToolbar();
    expect(screen.getByRole('button', { name: 'Stop' })).toBeDisabled();
  });

  it('Stop is enabled once a run is active', async () => {
    renderToolbar();
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    started('run-123');
    await waitFor(() => expect(screen.getByRole('button', { name: 'Stop' })).toBeEnabled());
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

  it('passes live progress as a third argument only when present', async () => {
    const onPatchProgress = vi.fn();
    renderToolbar({ onPatchProgress });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(progressHandler).toBeDefined());
    started('run-123');
    const live = { lastStatusCode: 202, conditionMet: false, remainingMs: 12000 };
    progressHandler?.({
      type: 'flowStepProgress',
      run_id: 'run-123',
      node_id: 'node-a',
      attempt: 2,
      max_attempts: 5,
      message: 'attempt 2/5 · condition false',
      live,
    });
    expect(onPatchProgress).toHaveBeenLastCalledWith(
      'node-a',
      'attempt 2/5 · condition false',
      live,
    );
    progressHandler?.({
      type: 'flowStepProgress',
      run_id: 'run-123',
      node_id: 'node-a',
      attempt: 3,
      max_attempts: 5,
      message: 'attempt 3/5',
    });
    expect(onPatchProgress.mock.calls[onPatchProgress.mock.calls.length - 1]).toEqual(['node-a', 'attempt 3/5']);
  });

  it('a remounted toolbar forwards live progress for the resumed run', async () => {
    const onPatchProgress = vi.fn();
    renderToolbar({ onPatchProgress, tabRunState: 'running', tabRunId: 'run-9' });
    await waitFor(() => expect(progressHandler).toBeDefined());
    const live = { ignored: 2, remainingMs: 5000 };
    progressHandler?.({
      type: 'flowStepProgress',
      run_id: 'run-9',
      node_id: 'w',
      attempt: null,
      max_attempts: null,
      message: 'waiting… 5s left · 2 ignored call(s)',
      live,
    });
    expect(onPatchProgress).toHaveBeenCalledWith('w', 'waiting… 5s left · 2 ignored call(s)', live);
  });

  it('hands the callback URLs from flow-run-started to onCallbackUrls', async () => {
    const onCallbackUrls = vi.fn();
    renderToolbar({ onCallbackUrls });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    startedHandler?.({
      type: 'flowRunStarted',
      run_id: 'run-123',
      flow_name: 'my-flow',
      collection: 'my-collection',
      total_nodes: 2,
      callbacks: [{ nodeId: 'w', name: 'payment', url: 'http://10.0.0.5:4000/cb/tok' }],
    });
    expect(onRunStateChange).toHaveBeenCalledWith('running', 'run-123');
    expect(onCallbackUrls).toHaveBeenCalledWith({ w: 'http://10.0.0.5:4000/cb/tok' });
    // The run state is set first, because a new run drops older URLs.
    expect(onRunStateChange.mock.invocationCallOrder[0]).toBeLessThan(
      onCallbackUrls.mock.invocationCallOrder[0],
    );
  });

  it('does not call onCallbackUrls for a run without callbacks', async () => {
    const onCallbackUrls = vi.fn();
    renderToolbar({ onCallbackUrls });
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    started('run-123');
    expect(onCallbackUrls).not.toHaveBeenCalled();
  });

  describe('pending sign-in', () => {
    type Tokens = Record<string, tauriApi.FlowAuthToken> | null;
    const pendingAuth = () => {
      let resolve: (v: Tokens) => void = () => {
        // Reassigned by the promise executor below.
      };
      let reject: (e: unknown) => void = () => {
        // Reassigned by the promise executor below.
      };
      const promise = new Promise<Tokens>((res, rej) => {
        resolve = res;
        reject = rej;
      });
      return { promise, resolve, reject };
    };

    it('disables Run with a "Signing in…" label and keeps Stop enabled while pending', async () => {
      const auth = pendingAuth();
      renderToolbar({ onPrepareAuth: () => auth.promise });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      const busy = await screen.findByRole('button', { name: /Signing in/ });
      expect(busy).toBeDisabled();
      expect(screen.getByRole('button', { name: 'Stop' })).toBeEnabled();
    });

    it('Stop abandons the wait, resets the guard, and a new Run works', async () => {
      const first = pendingAuth();
      const second = pendingAuth();
      const onPrepareAuth = vi
        .fn()
        .mockReturnValueOnce(first.promise)
        .mockReturnValueOnce(second.promise);
      renderToolbar({ onPrepareAuth });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await screen.findByRole('button', { name: /Signing in/ });
      await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
      expect(await screen.findByRole('button', { name: 'Run' })).toBeEnabled();
      expect(tauriApi.runFlow).not.toHaveBeenCalled();
      expect(toast.info).toHaveBeenCalledWith('Sign-in cancelled');
      expect(tauriApi.cancelFlowRun).not.toHaveBeenCalled();

      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await screen.findByRole('button', { name: /Signing in/ });
      expect(onPrepareAuth).toHaveBeenCalledTimes(2);
      second.resolve({ a: { accessToken: 'tok-123456' } });
      await waitFor(() =>
        expect(tauriApi.runFlow).toHaveBeenCalledWith(
          'my-collection',
          'my-flow',
          null,
          null,
          { a: { accessToken: 'tok-123456' } },
          { runId: 'run-123' },
        ),
      );
    });

    it('ignores an abandoned sign-in that resolves later', async () => {
      const auth = pendingAuth();
      renderToolbar({ onPrepareAuth: () => auth.promise });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await screen.findByRole('button', { name: /Signing in/ });
      await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
      await screen.findByRole('button', { name: 'Run' });
      auth.resolve({ a: { accessToken: 'tok-123456' } });
      await new Promise((r) => setTimeout(r, 20));
      expect(tauriApi.runFlow).not.toHaveBeenCalled();
      expect(screen.getByRole('button', { name: 'Run' })).toBeEnabled();
    });

    it('ignores an abandoned sign-in that rejects later, without an error toast', async () => {
      // Detects only rejections Node reports as unhandled; the race itself
      // already handles the late rejection.
      const unhandled = vi.fn();
      process.on('unhandledRejection', unhandled);
      try {
        const auth = pendingAuth();
        renderToolbar({ onPrepareAuth: () => auth.promise });
        await userEvent.click(screen.getByRole('button', { name: 'Run' }));
        await screen.findByRole('button', { name: /Signing in/ });
        await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
        await screen.findByRole('button', { name: 'Run' });
        auth.reject(new Error('window closed'));
        await new Promise((r) => setTimeout(r, 20));
        expect(toast.error).not.toHaveBeenCalled();
        expect(unhandled).not.toHaveBeenCalled();
      } finally {
        process.off('unhandledRejection', unhandled);
      }
    });

    it('recovers and shows an error when the sign-in rejects without a cancel', async () => {
      const auth = pendingAuth();
      renderToolbar({ onPrepareAuth: () => auth.promise });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await screen.findByRole('button', { name: /Signing in/ });
      auth.reject(new Error('boom'));
      await waitFor(() => expect(toast.error).toHaveBeenCalledWith('boom'));
      expect(await screen.findByRole('button', { name: 'Run' })).toBeEnabled();
    });

    it('does not start a run when unmounted while signing in', async () => {
      const auth = pendingAuth();
      const { unmount } = renderToolbar({ onPrepareAuth: () => auth.promise });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await screen.findByRole('button', { name: /Signing in/ });
      unmount();
      auth.resolve({ a: { accessToken: 'tok-123456' } });
      await new Promise((r) => setTimeout(r, 20));
      expect(tauriApi.runFlow).not.toHaveBeenCalled();
      expect(tauriApi.onFlowRunStarted).not.toHaveBeenCalled();
    });

    it('recovers when onPrepareAuth throws synchronously', async () => {
      const second = pendingAuth();
      const onPrepareAuth = vi
        .fn()
        .mockImplementationOnce(() => {
          throw new Error('boom');
        })
        .mockReturnValueOnce(second.promise);
      renderToolbar({ onPrepareAuth });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(toast.error).toHaveBeenCalledWith('boom'));
      expect(await screen.findByRole('button', { name: 'Run' })).toBeEnabled();
      expect(tauriApi.runFlow).not.toHaveBeenCalled();
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await screen.findByRole('button', { name: /Signing in/ });
      expect(onPrepareAuth).toHaveBeenCalledTimes(2);
    });

    it('an abandoned attempt resolving after a newer attempt started does not run', async () => {
      const first = pendingAuth();
      const second = pendingAuth();
      const onPrepareAuth = vi
        .fn()
        .mockReturnValueOnce(first.promise)
        .mockReturnValueOnce(second.promise);
      renderToolbar({ onPrepareAuth });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await screen.findByRole('button', { name: /Signing in/ });
      await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
      await userEvent.click(await screen.findByRole('button', { name: 'Run' }));
      await screen.findByRole('button', { name: /Signing in/ });
      first.resolve({ a: { accessToken: 'tok-123456' } });
      await new Promise((r) => setTimeout(r, 20));
      expect(tauriApi.runFlow).not.toHaveBeenCalled();
      expect(onPrepareAuth).toHaveBeenCalledTimes(2);
      second.resolve({});
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalledTimes(1));
    });
  });

  describe('rocket:flow-run shortcut', () => {
    const fire = (tabId: string) =>
      act(() => {
        window.dispatchEvent(new CustomEvent('rocket:flow-run', { detail: { tabId } }));
      });

    it('starts a run for its own tab', async () => {
      renderToolbar({ tabId: 'tab-1' });
      fire('tab-1');
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalledTimes(1));
    });

    it('ignores an event for another tab', async () => {
      renderToolbar({ tabId: 'tab-1' });
      fire('tab-2');
      await act(async () => {});
      expect(tauriApi.runFlow).not.toHaveBeenCalled();
    });

    it('starts only one run when pressed twice, and none while running', async () => {
      renderToolbar({ tabId: 'tab-1' });
      fire('tab-1');
      fire('tab-1');
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalledTimes(1));
      await waitFor(() => expect(startedHandler).toBeDefined());
      started('run-123');
      fire('tab-1');
      await act(async () => {});
      expect(tauriApi.runFlow).toHaveBeenCalledTimes(1);
    });

    it('does not start a run while sign-in is pending', async () => {
      const onPrepareAuth = vi.fn(() => new Promise<null>(() => undefined));
      renderToolbar({ tabId: 'tab-1', onPrepareAuth });
      fire('tab-1');
      await waitFor(() => expect(onPrepareAuth).toHaveBeenCalledTimes(1));
      fire('tab-1');
      await act(async () => {});
      expect(onPrepareAuth).toHaveBeenCalledTimes(1);
      expect(tauriApi.runFlow).not.toHaveBeenCalled();
    });

    it('does nothing without a tab id, and stops listening after unmount', async () => {
      const { unmount } = renderToolbar();
      fire('tab-1');
      await act(async () => {});
      expect(tauriApi.runFlow).not.toHaveBeenCalled();
      unmount();
      renderToolbar({ tabId: 'tab-1' }).unmount();
      fire('tab-1');
      await act(async () => {});
      expect(tauriApi.runFlow).not.toHaveBeenCalled();
    });
  });

  describe('run result', () => {
    beforeEach(() => {
      vi.mocked(newFlowRunId).mockReturnValue('run-1');
    });

    const base = { statusCode: null, durationMs: null, error: null, value: null };

    const finishedEvent = (
      over: Partial<tauriApi.FlowRunFinishedEvent> = {},
    ): tauriApi.FlowRunFinishedEvent => ({
      type: 'flowRunFinished',
      run_id: 'run-9',
      stopped_reason: 'completed',
      node_count: 3,
      failed_count: 1,
      skipped_count: 2,
      not_taken_count: 1,
      ...over,
    });

    it('reports the environment the run was started with, even if it changes mid-run', async () => {
      const onRunResult = vi.fn();
      const element = (environmentName: string | null) => (
        <FlowToolbar
          collection='my-collection'
          flowName='my-flow'
          environmentName={environmentName}
          onPatchStatus={onPatchStatus}
          onRunStateChange={onRunStateChange}
          onRunResult={onRunResult}
        />
      );
      const view = render(element('staging'));
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(startedHandler).toBeDefined());
      started('run-1');
      view.rerender(element('prod'));
      resolveRun({ runId: 'run-1', stoppedReason: 'completed', steps: [] });
      await waitFor(() => expect(onRunResult).toHaveBeenCalledTimes(1));
      expect(onRunResult).toHaveBeenCalledWith(
        expect.objectContaining({ runId: 'run-1', environmentName: 'staging' }),
      );
    });

    it('reports an error result when a run that had started is rejected', async () => {
      const onRunResult = vi.fn();
      let rejectRun: (reason: unknown) => void = () => undefined;
      vi.mocked(tauriApi.runFlow).mockImplementationOnce(
        () =>
          new Promise((_, reject) => {
            rejectRun = reject;
          }),
      );
      renderToolbar({ onRunResult });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(startedHandler).toBeDefined());
      started('run-1');
      rejectRun('socket closed');
      await waitFor(() => expect(onRunResult).toHaveBeenCalledTimes(1));
      expect(onRunResult).toHaveBeenCalledWith(
        expect.objectContaining({
          runId: 'run-1',
          stoppedReason: 'error',
          failedCount: 0,
          skippedCount: 0,
          totalMs: expect.any(Number),
        }),
      );
      expect(toast.error).toHaveBeenCalled();
    });

    it('reports the result after the final summary and before the run is marked done', async () => {
      const onRunResult = vi.fn();
      renderToolbar({ onRunResult });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(startedHandler).toBeDefined());
      started('run-1');
      resolveRun({
        runId: 'run-1',
        stoppedReason: 'completed',
        steps: [
          { ...base, nodeId: 'a', status: 'success' },
          { ...base, nodeId: 'b', status: 'failed', error: 'boom' },
          { ...base, nodeId: 'c', status: 'skipped', skipReason: 'upstream_failed' },
          { ...base, nodeId: 'd', status: 'skipped', skipReason: 'branch_not_taken' },
        ],
      });
      await waitFor(() => expect(onRunResult).toHaveBeenCalledTimes(1));
      expect(onRunResult).toHaveBeenCalledWith(
        expect.objectContaining({
          runId: 'run-1',
          stoppedReason: 'completed',
          failedNodeId: 'b',
          failedCount: 1,
          skippedCount: 1,
          totalMs: expect.any(Number),
        }),
      );
      expect(onRunResult.mock.calls[0][0].totalMs).toBeGreaterThanOrEqual(0);
      const doneIndex = onRunStateChange.mock.calls.findIndex((c) => c[0] === 'done');
      expect(onRunResult.mock.invocationCallOrder[0]).toBeLessThan(
        onRunStateChange.mock.invocationCallOrder[doneIndex],
      );
    });

    it('names no failed node for a cancelled run', async () => {
      const onRunResult = vi.fn();
      renderToolbar({ onRunResult });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(startedHandler).toBeDefined());
      started('run-1');
      resolveRun({
        runId: 'run-1',
        stoppedReason: 'cancelled',
        steps: [{ ...base, nodeId: 'b', status: 'failed', error: 'cancelled' }],
      });
      await waitFor(() => expect(onRunResult).toHaveBeenCalledTimes(1));
      const result = onRunResult.mock.calls[0][0];
      expect(result.stoppedReason).toBe('cancelled');
      expect(result.failedCount).toBe(0);
      expect(result).not.toHaveProperty('failedNodeId');
    });

    it('reports nothing when the run is rejected before it starts', async () => {
      const onRunResult = vi.fn();
      vi.mocked(tauriApi.runFlow).mockRejectedValue('Invalid input: bad graph');
      renderToolbar({ onRunResult });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(toast.error).toHaveBeenCalled());
      expect(onRunResult).not.toHaveBeenCalled();
    });

    it('a remounted toolbar reports the result from the finished event of its run', async () => {
      const onRunResult = vi.fn();
      renderToolbar({ onRunResult, tabRunState: 'running', tabRunId: 'run-9' });
      await waitFor(() => expect(finishedHandler).toBeDefined());
      finishedHandler?.(finishedEvent());
      expect(onRunResult).toHaveBeenCalledWith({
        runId: 'run-9',
        stoppedReason: 'completed',
        totalMs: null,
        failedCount: 1,
        skippedCount: 1,
      });
      expect(onRunStateChange).toHaveBeenCalledWith('done', 'run-9');
    });

    it('a remounted toolbar ignores the finished event of another run', async () => {
      const onRunResult = vi.fn();
      renderToolbar({ onRunResult, tabRunState: 'running', tabRunId: 'run-9' });
      await waitFor(() => expect(finishedHandler).toBeDefined());
      finishedHandler?.(finishedEvent({ run_id: 'run-other' }));
      expect(onRunResult).not.toHaveBeenCalled();
      expect(onRunStateChange).not.toHaveBeenCalled();
    });

    it('ignores a late finished event after the resumed toolbar unmounts', async () => {
      const onRunResult = vi.fn();
      const { unmount } = renderToolbar({
        onRunResult,
        tabRunState: 'running',
        tabRunId: 'run-9',
      });
      await waitFor(() => expect(finishedHandler).toBeDefined());
      unmount();
      finishedHandler?.(finishedEvent());
      expect(onRunResult).not.toHaveBeenCalled();
      expect(onRunStateChange).not.toHaveBeenCalled();
    });

    it('does not subscribe to the finished event when no run is being resumed', async () => {
      renderToolbar({ onRunResult: vi.fn() });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
      expect(tauriApi.onFlowRunFinished).not.toHaveBeenCalled();
    });
  });

  describe('client run id', () => {
    const lateStep = (runId: string) =>
      stepHandler?.({
        type: 'flowStepCompleted',
        run_id: runId,
        node_id: 'node-a',
        status: 'failed',
        status_code: null,
        duration_ms: null,
        error: 'late',
        value: null,
      });

    it('stores its run id on the tab before the run is sent', async () => {
      const onRunRequested = vi.fn();
      renderToolbar({ onRunRequested });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
      expect(onRunRequested).toHaveBeenCalledWith('run-123');
      expect(onRunRequested.mock.invocationCallOrder[0]).toBeLessThan(
        vi.mocked(tauriApi.runFlow).mock.invocationCallOrder[0],
      );
    });

    it('Stop cancels its own run before flow-run-started arrives', async () => {
      renderToolbar();
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
      await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
      expect(tauriApi.cancelFlowRun).toHaveBeenCalledWith('run-123');
    });

    it('ignores late events of its run once run_flow settled', async () => {
      renderToolbar();
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
      started('run-123');
      resolveRun({ runId: 'run-123', steps: [], stoppedReason: 'completed' });
      await waitFor(() => expect(onRunStateChange).toHaveBeenCalledWith('done', 'run-123'));
      onPatchStatus.mockClear();
      // The fake unlisten keeps the handler, like an event already queued.
      lateStep('run-123');
      expect(onPatchStatus).not.toHaveBeenCalled();
    });

    it('uses a new id for every run', async () => {
      vi.mocked(newFlowRunId).mockReturnValueOnce('run-a').mockReturnValueOnce('run-b');
      renderToolbar();
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalledTimes(1));
      started('run-a');
      resolveRun({ runId: 'run-a', steps: [], stoppedReason: 'completed' });
      await waitFor(() => expect(screen.getByRole('button', { name: 'Run' })).toBeEnabled());
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalledTimes(2));
      expect(vi.mocked(tauriApi.runFlow).mock.calls[0][5]).toEqual({ runId: 'run-a' });
      expect(vi.mocked(tauriApi.runFlow).mock.calls[1][5]).toEqual({ runId: 'run-b' });
    });

    it('frees listeners that finish subscribing after the toolbar unmounted, once the run ends', async () => {
      const unlisten = vi.fn();
      let release: () => void = () => undefined;
      const gate = new Promise<void>((resolve) => {
        release = resolve;
      });
      vi.mocked(tauriApi.onFlowRunStarted).mockImplementation(async () => {
        await gate;
        return unlisten;
      });
      vi.mocked(tauriApi.onFlowStepStarted).mockImplementation(async () => unlisten);
      vi.mocked(tauriApi.onFlowStepCompleted).mockImplementation(async () => unlisten);
      vi.mocked(tauriApi.onFlowStepProgress).mockImplementation(async () => unlisten);
      const view = renderToolbar();
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(tauriApi.onFlowRunStarted).toHaveBeenCalled());
      view.unmount();
      release();
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
      expect(unlisten).not.toHaveBeenCalled();
      resolveRun({ runId: 'run-123', steps: [], stoppedReason: 'completed' });
      await waitFor(() => expect(unlisten).toHaveBeenCalledTimes(4));
    });

    it('a run still learns it started after its toolbar unmounted, without double handling', async () => {
      const unlisten = vi.fn();
      // Like Tauri, every started event reaches every subscriber.
      const subscribers: Array<NonNullable<typeof startedHandler>> = [];
      const emit = (event: Parameters<NonNullable<typeof startedHandler>>[0]) => {
        for (const h of subscribers) h(event);
      };
      vi.mocked(tauriApi.onFlowRunStarted).mockImplementation(async (h) => {
        subscribers.push(h);
        return unlisten;
      });
      const onCallbackUrls = vi.fn();
      const view = renderToolbar({ onCallbackUrls });
      await userEvent.click(screen.getByRole('button', { name: 'Run' }));
      await waitFor(() => expect(tauriApi.runFlow).toHaveBeenCalled());
      view.unmount();
      // The run is announced while no toolbar is mounted.
      emit({
        type: 'flowRunStarted',
        run_id: 'run-123',
        flow_name: 'my-flow',
        collection: 'my-collection',
        total_nodes: 1,
        callbacks: [{ nodeId: 'w', name: 'payment', url: 'http://h:1/cb/tok' }],
      });
      expect(onRunStateChange).toHaveBeenCalledWith('running', 'run-123');
      expect(onCallbackUrls).toHaveBeenCalledWith({ w: 'http://h:1/cb/tok' });
      // A toolbar mounted later follows the run without handling the event twice.
      onRunStateChange.mockClear();
      renderToolbar({ tabRunState: 'running', tabRunId: 'run-123', onCallbackUrls });
      await act(async () => {});
      emit({
        type: 'flowRunStarted',
        run_id: 'run-123',
        flow_name: 'my-flow',
        collection: 'my-collection',
        total_nodes: 1,
      });
      expect(onRunStateChange).toHaveBeenCalledTimes(0);
      resolveRun({ runId: 'run-123', steps: [], stoppedReason: 'completed' });
      await waitFor(() => expect(onRunStateChange).toHaveBeenCalledWith('done', 'run-123'));
      await waitFor(() => expect(unlisten).toHaveBeenCalled());
    });

    it('a remounted toolbar follows a run that has not announced itself yet', async () => {
      const onCallbackUrls = vi.fn();
      renderToolbar({
        tabRunState: 'done',
        tabRunId: 'run-0',
        tabPendingRunId: 'run-7',
        onCallbackUrls,
      });
      expect(screen.getByRole('button', { name: 'Run' })).toBeDisabled();
      await waitFor(() => expect(startedHandler).toBeDefined());
      started('run-8');
      expect(onRunStateChange).not.toHaveBeenCalled();
      startedHandler?.({
        type: 'flowRunStarted',
        run_id: 'run-7',
        flow_name: 'my-flow',
        collection: 'my-collection',
        total_nodes: 1,
        callbacks: [{ nodeId: 'w', name: 'payment', url: 'http://h:1/cb/tok' }],
      });
      expect(onRunStateChange).toHaveBeenCalledWith('running', 'run-7');
      expect(onCallbackUrls).toHaveBeenCalledWith({ w: 'http://h:1/cb/tok' });
      await userEvent.click(screen.getByRole('button', { name: 'Stop' }));
      expect(tauriApi.cancelFlowRun).toHaveBeenCalledWith('run-7');
    });
  });
});
