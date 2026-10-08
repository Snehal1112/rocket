import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findTabInTree } from '@/lib/pane-utils';
import {
  type FlowRunSummary,
  listCollections,
  listFlows,
  onFlowRunFinished,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepProgress,
  onFlowStepStarted,
  runFlow,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listFlows: vi.fn(),
    runFlow: vi.fn(),
    onFlowRunStarted: vi.fn(),
    onFlowRunFinished: vi.fn(),
    onFlowStepStarted: vi.fn(),
    onFlowStepCompleted: vi.fn(),
    onFlowStepProgress: vi.fn(),
  };
});
vi.mock('@/lib/execute-request', () => ({ getActiveGlobalEnvName: vi.fn() }));
vi.mock('@/lib/flow-auth-preflight', () => ({ collectFlowAuthTokens: vi.fn(async () => ({})) }));
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));

const tabId = 'flow-record-1';

const baseTab: FlowTab = {
  id: tabId,
  tabType: 'flow',
  title: 'Flow: rec',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'rec',
  nodes: [{ id: 'out1', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } }],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

function storedTab(): FlowTab {
  const found = findTabInTree(usePaneStore.getState().root, tabId);
  if (!found || !isFlowTab(found.tab)) throw new Error('Expected the flow tab');
  return found.tab;
}

function Harness() {
  const tab = usePaneStore((s) => {
    if (s.root.type !== 'leaf') return null;
    const t = s.root.tabs.find((x) => x.id === tabId);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

const summaryFor = (runId: string, status: 'success' | 'failed'): FlowRunSummary => ({
  runId,
  stoppedReason: 'completed',
  steps: [
    {
      nodeId: 'out1',
      status,
      statusCode: null,
      durationMs: 4,
      error: status === 'failed' ? 'boom' : null,
      value: status === 'success' ? '"ok"' : null,
    },
  ],
});

let startedHandler: Parameters<typeof onFlowRunStarted>[0] | undefined;
let resolveRun: (summary: FlowRunSummary) => void = () => undefined;

// Clicks Run, lets the toolbar subscribe, fires the run-started event, then ends the run.
async function runOnce(runId: string, status: 'success' | 'failed') {
  vi.mocked(runFlow).mockImplementationOnce(
    () =>
      new Promise((resolve) => {
        resolveRun = resolve;
      }),
  );
  startedHandler = undefined;
  await waitFor(() => expect(screen.getByRole('button', { name: 'Run' })).toBeEnabled());
  await userEvent.click(screen.getByRole('button', { name: 'Run' }));
  await waitFor(() => expect(startedHandler).toBeDefined());
  act(() => {
    startedHandler?.({
      type: 'flowRunStarted',
      run_id: runId,
      flow_name: 'rec',
      collection: 'demo',
      total_nodes: 1,
    });
  });
  await act(async () => {
    resolveRun(summaryFor(runId, status));
  });
}

describe('FlowPane run history recording', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    const unlisten = async () => () => undefined;
    vi.mocked(onFlowRunStarted).mockImplementation(async (handler) => {
      startedHandler = handler;
      return () => undefined;
    });
    vi.mocked(onFlowRunFinished).mockImplementation(unlisten);
    vi.mocked(onFlowStepStarted).mockImplementation(unlisten);
    vi.mocked(onFlowStepCompleted).mockImplementation(unlisten);
    vi.mocked(onFlowStepProgress).mockImplementation(unlisten);
  });

  it('records a snapshot of each finished run, newest first', async () => {
    render(<Harness />);
    await runOnce('run-1', 'failed');
    await waitFor(() => expect(storedTab().runHistory).toHaveLength(1));
    await runOnce('run-2', 'success');
    await waitFor(() => expect(storedTab().runHistory).toHaveLength(2));

    const [newest, older] = storedTab().runHistory ?? [];
    expect(newest.runId).toBe('run-2');
    expect(newest.nodeStatus).toEqual({ out1: 'success' });
    expect(older.runId).toBe('run-1');
    expect(older.nodeStatus).toEqual({ out1: 'failed' });
    expect(older.nodeDetail.out1.error).toBe('boom');
    expect(older.result.failedCount).toBe(1);
  });

  it('does not record a run that was rejected before it started', async () => {
    vi.mocked(runFlow).mockRejectedValueOnce('Invalid input: bad graph');
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(runFlow).toHaveBeenCalled());
    await waitFor(() => expect(storedTab().runState).toBe('done'));
    expect(storedTab().runHistory ?? []).toHaveLength(0);
  });
});
