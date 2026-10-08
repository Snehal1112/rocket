import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { act, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findTabInTree } from '@/lib/pane-utils';
import {
  cancelFlowRun,
  type FlowRunStartedEvent,
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
import { EditorGroup } from '../EditorGroup';

vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listFlows: vi.fn(),
    runFlow: vi.fn(),
    cancelFlowRun: vi.fn(),
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

const flowTab = (id: string, flowName: string): FlowTab => ({
  id,
  tabType: 'flow',
  title: `Flow: ${flowName}`,
  isDirty: false,
  collectionName: 'demo',
  flowName,
  nodes: [{ id: 'out1', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } }],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
});

function Group() {
  const root = usePaneStore((s) => s.root);
  if (root.type !== 'leaf') return null;
  return <EditorGroup node={root} />;
}

function stored(id: string): FlowTab {
  const found = findTabInTree(usePaneStore.getState().root, id);
  if (!found || !isFlowTab(found.tab)) throw new Error(`Expected flow tab ${id}`);
  return found.tab;
}

const groupId = () => {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected leaf root');
  return root.groupId;
};

describe('switching between two flow tabs in one group', () => {
  let startedHandlers: Array<(e: FlowRunStartedEvent) => void> = [];

  beforeEach(() => {
    vi.clearAllMocks();
    startedHandlers = [];
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(flowTab('flow-a', 'alpha'));
    usePaneStore.getState().openTab(flowTab('flow-b', 'beta'));
    usePaneStore.getState().setActiveTab('flow-a', groupId());
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    vi.mocked(cancelFlowRun).mockResolvedValue(undefined);
    const quiet = async () => () => undefined;
    vi.mocked(onFlowRunStarted).mockImplementation(async (h) => {
      startedHandlers.push(h);
      return () => undefined;
    });
    vi.mocked(onFlowRunFinished).mockImplementation(quiet);
    vi.mocked(onFlowStepStarted).mockImplementation(quiet);
    vi.mocked(onFlowStepCompleted).mockImplementation(quiet);
    vi.mocked(onFlowStepProgress).mockImplementation(quiet);
    vi.mocked(runFlow).mockImplementation(() => new Promise(() => undefined));
  });

  it('keeps each tab with its own run and Stop', async () => {
    render(
      <QueryClientProvider client={new QueryClient()}>
        <Group />
      </QueryClientProvider>,
    );
    await userEvent.click(await screen.findByRole('button', { name: 'Run' }, { timeout: 5000 }));
    await waitFor(() => expect(runFlow).toHaveBeenCalledTimes(1));
    const idA = vi.mocked(runFlow).mock.calls[0][5]?.runId ?? '';
    act(() => {
      for (const h of startedHandlers) {
        h({
          type: 'flowRunStarted',
          run_id: idA,
          flow_name: 'alpha',
          collection: 'demo',
          total_nodes: 1,
        });
      }
    });
    expect(stored('flow-a').runState).toBe('running');

    act(() => usePaneStore.getState().setActiveTab('flow-b', groupId()));
    await waitFor(() => expect(screen.getByRole('button', { name: 'Run' })).toBeEnabled());
    expect(screen.getByRole('button', { name: 'Stop' })).toBeDisabled();
    expect(cancelFlowRun).not.toHaveBeenCalled();

    act(() => usePaneStore.getState().setActiveTab('flow-a', groupId()));
    const stop = await screen.findByRole('button', { name: 'Stop' });
    await waitFor(() => expect(stop).toBeEnabled());
    await userEvent.click(stop);
    expect(cancelFlowRun).toHaveBeenCalledWith(idA);
  });
});
