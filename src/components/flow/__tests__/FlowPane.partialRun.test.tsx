import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  listCollections,
  listFlows,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepProgress,
  onFlowStepStarted,
  runFlow,
  saveFlow,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    lintFlow: vi.fn().mockResolvedValue([]),
    listCollections: vi.fn(),
    listFlows: vi.fn(),
    saveFlow: vi.fn(),
    runFlow: vi.fn(),
    onFlowRunStarted: vi.fn(),
    onFlowStepStarted: vi.fn(),
    onFlowStepCompleted: vi.fn(),
    onFlowStepProgress: vi.fn(),
  };
});
vi.mock('@/lib/execute-request', () => ({ getActiveGlobalEnvName: vi.fn() }));
vi.mock('@/lib/flow-auth-preflight', () => ({ collectFlowAuthTokens: vi.fn(async () => ({})) }));
vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() } }));
vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: { value: string; 'aria-label'?: string }) => (
    <input aria-label={props['aria-label']} value={props.value} readOnly />
  ),
}));

// Radix menus call pointer-capture and scrollIntoView APIs that jsdom lacks.
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => undefined;
Element.prototype.scrollIntoView ??= () => undefined;
// jsdom has no DOMMatrixReadOnly, which React Flow reads when it re-measures nodes.
vi.stubGlobal(
  'DOMMatrixReadOnly',
  class {
    m22 = 1;
  },
);

const baseTab: FlowTab = {
  id: 'flow-partial-ui',
  tabType: 'flow',
  title: 'Flow: p',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'p',
  nodes: [
    { id: 'in1', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'alice' } },
    { id: 'out1', position: { x: 300, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
  ],
  edges: [
    {
      id: 'e1',
      sourceNodeId: 'in1',
      targetNodeId: 'out1',
      targetField: 'value',
      expression: 'response.body',
    },
  ],
  nodeStatus: { in1: 'success', out1: 'success' },
  nodeDetail: { in1: { value: 'alice' }, out1: { value: 'alice' } },
  runState: 'done',
  runId: 'run-0',
};

type StartedHandler = Parameters<typeof onFlowRunStarted>[0];
let startedHandler: StartedHandler | undefined;

function getFlowTab(): FlowTab {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
  const found = root.tabs.find((t) => t.id === baseTab.id);
  if (!found || !isFlowTab(found)) throw new Error('Expected the seeded flow tab');
  return found;
}

function Harness() {
  const tab = usePaneStore((s) => {
    if (s.root.type !== 'leaf') return null;
    const t = s.root.tabs.find((x) => x.id === baseTab.id);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

const user = () => userEvent.setup({ pointerEventsCheck: 0 });

async function runThisNode(label: string) {
  const u = user();
  await u.click(screen.getByLabelText(`Edit ${label}`));
  await u.click(await screen.findByRole('menuitem', { name: 'Run this node' }));
}

// The run id the toolbar chose, which the started event must echo.
function sentRunId(): string {
  const options = vi.mocked(runFlow).mock.calls[0][5];
  if (!options?.runId) throw new Error('Expected a client run id');
  return options.runId;
}

describe('FlowPane partial runs', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    startedHandler = undefined;
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    const unlisten = async () => () => undefined;
    vi.mocked(onFlowRunStarted).mockImplementation(async (h) => {
      startedHandler = h;
      return () => undefined;
    });
    vi.mocked(onFlowStepStarted).mockImplementation(unlisten);
    vi.mocked(onFlowStepCompleted).mockImplementation(unlisten);
    vi.mocked(onFlowStepProgress).mockImplementation(unlisten);
    // Keep the run pending so each test ends mid-run.
    vi.mocked(runFlow).mockImplementation(() => new Promise(() => undefined));
  });

  it('Run this node starts a partial run on the last run', async () => {
    render(<Harness />);
    await runThisNode('Result');
    await waitFor(() => expect(runFlow).toHaveBeenCalledTimes(1));
    expect(vi.mocked(runFlow).mock.calls[0][5]?.partial).toEqual({
      baseRunId: 'run-0',
      startNodeId: 'out1',
      mode: 'node',
    });
  });

  it('keeps results outside the run and marks them as earlier results', async () => {
    render(<Harness />);
    await runThisNode('Result');
    await waitFor(() => expect(startedHandler).toBeDefined());
    startedHandler?.({
      type: 'flowRunStarted',
      run_id: sentRunId(),
      flow_name: 'p',
      collection: 'demo',
      total_nodes: 1,
      partial: { baseRunId: 'run-0', startNodeId: 'out1', mode: 'node', nodeIds: ['out1'] },
    });
    await waitFor(() => expect(getFlowTab().runId).toBe(sentRunId()));
    expect(getFlowTab().nodeStatus).toEqual({ in1: 'success' });
    expect(getFlowTab().nodeDetail?.in1?.cached).toBe(true);
    expect(await screen.findByTestId('node-cached-caption')).toHaveTextContent(
      'Result from an earlier run',
    );
  });

  it('highlights the nodes a refused partial run names and keeps the base run', async () => {
    vi.mocked(runFlow).mockRejectedValue(
      "Invalid input: 'User' changed since the earlier run, or did not run in it. Run the full flow, or Run from the first changed node — node(s): in1; edge(s): ",
    );
    render(<Harness />);
    await runThisNode('Result');
    await waitFor(() =>
      expect(screen.getByTestId('input-node-card').className).toContain('ring-red-500'),
    );
    expect(screen.getByTestId('output-node-card').className).not.toContain('ring-red-500');
    expect(getFlowTab().runId).toBe('run-0');
    expect(getFlowTab().nodeDetail?.in1?.value).toBe('alice');
  });

  it('clears the highlight when the next run starts', async () => {
    vi.mocked(runFlow).mockRejectedValueOnce(
      "Invalid input: 'User' changed since the earlier run — node(s): in1; edge(s): ",
    );
    render(<Harness />);
    await runThisNode('Result');
    await waitFor(() =>
      expect(screen.getByTestId('input-node-card').className).toContain('ring-red-500'),
    );
    vi.mocked(runFlow).mockImplementation(() => new Promise(() => undefined));
    await user().click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(startedHandler).toBeDefined());
    startedHandler?.({
      type: 'flowRunStarted',
      run_id: vi.mocked(runFlow).mock.calls[1][5]?.runId ?? '',
      flow_name: 'p',
      collection: 'demo',
      total_nodes: 2,
    });
    await waitFor(() =>
      expect(screen.getByTestId('input-node-card').className).not.toContain('ring-red-500'),
    );
  });
});
