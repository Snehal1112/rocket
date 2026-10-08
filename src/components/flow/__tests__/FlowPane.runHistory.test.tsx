import { act, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  listCollections,
  listFlows,
  onFlowRunFinished,
  onFlowRunStarted,
  onFlowStepCompleted,
  onFlowStepProgress,
  onFlowStepStarted,
  saveFlow,
} from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowRunRecord, type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listFlows: vi.fn(),
    saveFlow: vi.fn(),
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
vi.mock('@/components/editor', () => ({
  SingleLineEditor: (props: {
    value: string;
    onChange: (v: string) => void;
    'aria-label'?: string;
  }) => (
    <input
      aria-label={props['aria-label']}
      value={props.value}
      onChange={(e) => props.onChange(e.target.value)}
    />
  ),
}));

// Radix Select calls pointer-capture and scrollIntoView APIs that jsdom lacks.
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

const tabId = 'flow-hist-ui-1';
const at = (h: number, m: number, s: number) => new Date(2026, 9, 8, h, m, s).getTime();

const newest: FlowRunRecord = {
  runId: 'run-2',
  finishedAt: at(14, 5, 0),
  environmentName: 'dev',
  result: { runId: 'run-2', stoppedReason: 'completed', totalMs: 900, failedCount: 0, skippedCount: 0 },
  nodeStatus: { out1: 'success' },
  nodeDetail: { out1: { value: '"ok"' } },
};

const older: FlowRunRecord = {
  runId: 'run-1',
  finishedAt: at(14, 1, 0),
  environmentName: null,
  result: {
    runId: 'run-1',
    stoppedReason: 'completed',
    totalMs: 1500,
    failedNodeId: 'out1',
    failedLabel: 'Result',
    failedCount: 1,
    skippedCount: 0,
  },
  // `ghost` is a node that was deleted after the run.
  nodeStatus: { out1: 'failed', ghost: 'failed' },
  nodeDetail: { out1: { error: 'boom' }, ghost: { error: 'gone' } },
};

const baseTab: FlowTab = {
  id: tabId,
  tabType: 'flow',
  title: 'Flow: hist',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'hist',
  nodes: [{ id: 'out1', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } }],
  edges: [],
  nodeStatus: newest.nodeStatus,
  nodeDetail: newest.nodeDetail,
  runState: 'done',
  runId: 'run-2',
  lastRun: newest.result,
  runHistory: [newest, older],
};

function seed(over: Partial<FlowTab> = {}) {
  usePaneStore.getState().reset();
  usePaneStore.getState().openTab({ ...baseTab, ...over });
}

function storedTab(): FlowTab {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected a leaf');
  const tab = root.tabs.find((t) => t.id === tabId);
  if (!tab || !isFlowTab(tab)) throw new Error('Expected the flow tab');
  return tab;
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

const cardStatus = () => screen.getByTestId('output-node-card').getAttribute('data-status');

async function choose(optionName: RegExp) {
  await userEvent.click(screen.getByRole('combobox', { name: 'Run history' }));
  await userEvent.click(await screen.findByRole('option', { name: optionName }));
}

describe('FlowPane run history', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    const unlisten = async () => () => undefined;
    vi.mocked(onFlowRunStarted).mockImplementation(unlisten);
    vi.mocked(onFlowRunFinished).mockImplementation(unlisten);
    vi.mocked(onFlowStepStarted).mockImplementation(unlisten);
    vi.mocked(onFlowStepCompleted).mockImplementation(unlisten);
    vi.mocked(onFlowStepProgress).mockImplementation(unlisten);
  });

  it('shows no selector until there are two recorded runs', () => {
    seed({ runHistory: [newest] });
    render(<Harness />);
    expect(screen.queryByRole('combobox', { name: 'Run history' })).not.toBeInTheDocument();
  });

  it('lists the runs and shows the latest one live', async () => {
    seed();
    render(<Harness />);
    expect(cardStatus()).toBe('success');
    await userEvent.click(screen.getByRole('combobox', { name: 'Run history' }));
    expect(await screen.findByRole('option', { name: /Latest/ })).toBeInTheDocument();
    expect(screen.getByRole('option', { name: /1 failed/ })).toBeInTheDocument();
  });

  it('switches the canvas, panel result and strip to a past run and back', async () => {
    seed();
    render(<Harness />);
    await choose(/1 failed/);
    expect(cardStatus()).toBe('failed');
    expect(screen.getByTestId('viewed-run-banner')).toHaveTextContent(
      /Viewing the run from 14:01:00\. The graph may have changed since\./,
    );
    expect(screen.getByTestId('run-result-strip')).toHaveTextContent(
      'Run finished with 1 failed in 1.5 s',
    );
    expect(storedTab().isDirty).toBe(false);

    await choose(/Latest/);
    expect(cardStatus()).toBe('success');
    expect(screen.queryByTestId('viewed-run-banner')).not.toBeInTheDocument();
  });

  it('Back to latest returns to the live results', async () => {
    seed({ viewedRunId: 'run-1' });
    render(<Harness />);
    expect(cardStatus()).toBe('failed');
    await userEvent.click(screen.getByRole('button', { name: 'Back to latest' }));
    expect(cardStatus()).toBe('success');
    expect(storedTab().viewedRunId).toBeNull();
  });

  it('a new run returns to live and disables the selector', () => {
    seed({ viewedRunId: 'run-1' });
    render(<Harness />);
    expect(screen.getByTestId('viewed-run-banner')).toBeInTheDocument();
    act(() => {
      usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-3');
    });
    expect(screen.queryByTestId('viewed-run-banner')).not.toBeInTheDocument();
    expect(screen.getByRole('combobox', { name: 'Run history' })).toBeDisabled();
  });

  it('treats a viewed run that is not in the history as the live run', () => {
    seed({ viewedRunId: 'nope' });
    render(<Harness />);
    expect(screen.queryByTestId('viewed-run-banner')).not.toBeInTheDocument();
    expect(cardStatus()).toBe('success');
  });

  it('ignores results of nodes that no longer exist and disables the failed-node chip', () => {
    seed({ viewedRunId: 'run-1' });
    render(<Harness />);
    expect(screen.getByRole('button', { name: 'Select failed node Result' })).toBeEnabled();
    act(() => {
      usePaneStore.getState().updateFlowNodes(tabId, []);
    });
    expect(screen.getByTestId('viewed-run-banner')).toBeInTheDocument();
    expect(screen.getByRole('button', { name: 'Select failed node Result' })).toBeDisabled();
  });

  it('Save writes only the graph while a past run is shown', async () => {
    seed({ viewedRunId: 'run-1' });
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    expect(saveFlow).toHaveBeenCalledWith('demo', {
      name: 'hist',
      nodes: baseTab.nodes,
      edges: [],
    });
  });
});
