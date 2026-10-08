import { act, render, screen, waitFor } from '@testing-library/react';
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
  runFlow,
} from '@/lib/tauri-api';
import { findTabInTree } from '@/lib/pane-utils';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowLastRun, type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    lintFlow: vi.fn().mockResolvedValue([]),
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

// jsdom reports every rect as 0,0,0,0 and userEvent clicks at 0,0, so the resize
// handle would count as hit by every click. Park the handle away from the pointer.
const realGetRect = Element.prototype.getBoundingClientRect;
Element.prototype.getBoundingClientRect = function getRect() {
  if (this instanceof HTMLElement && this.dataset.slot === 'resizable-handle') {
    return new DOMRect(5000, 5000, 1, 100);
  }
  return realGetRect.call(this);
};

const tabId = 'flow-strip-1';

const baseTab: FlowTab = {
  id: tabId,
  tabType: 'flow',
  title: 'Flow: strip',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'strip',
  nodes: [
    {
      id: 'req1',
      position: { x: 0, y: 0 },
      kind: {
        kind: 'Request',
        label: 'Fetch',
        source: { type: 'Inline', request: { method: 'GET', url: '', headers: [] } },
      },
    },
    { id: 'out1', position: { x: 300, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
  ],
  edges: [],
  nodeStatus: {},
  runState: 'done',
};

const failedRun: FlowLastRun = {
  runId: 'run-1',
  stoppedReason: 'completed',
  totalMs: 2300,
  failedNodeId: 'req1',
  failedLabel: 'Fetch',
  failedCount: 1,
  skippedCount: 2,
};

function seed(over: Partial<FlowTab> = {}) {
  usePaneStore.getState().reset();
  usePaneStore.getState().openTab({ ...baseTab, ...over });
}

// FlowPane receives the tab as a prop. Re-render it from the store after each
// store change, the way PaneRenderer does in the app.
function Harness() {
  const tab = usePaneStore((s) => {
    if (s.root.type !== 'leaf') return null;
    const t = s.root.tabs.find((x) => x.id === tabId);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

// A tab that is running mounts a toolbar that subscribes to run events, so
// every listener needs a fake in both describes.
beforeEach(() => {
  vi.clearAllMocks();
  vi.mocked(listCollections).mockResolvedValue([]);
  vi.mocked(listFlows).mockResolvedValue([]);
  const unlisten = async () => () => undefined;
  vi.mocked(onFlowRunStarted).mockImplementation(unlisten);
  vi.mocked(onFlowRunFinished).mockImplementation(unlisten);
  vi.mocked(onFlowStepStarted).mockImplementation(unlisten);
  vi.mocked(onFlowStepCompleted).mockImplementation(unlisten);
  vi.mocked(onFlowStepProgress).mockImplementation(unlisten);
});

describe('FlowPane run-result strip', () => {
  it('shows no strip before any run', () => {
    seed();
    render(<Harness />);
    expect(screen.queryByTestId('run-result-strip')).not.toBeInTheDocument();
  });

  it('shows the outcome, time and counts of a failed run', () => {
    seed({ lastRun: failedRun });
    render(<Harness />);
    const strip = screen.getByTestId('run-result-strip');
    expect(strip).toHaveTextContent('Run finished with 1 failed in 2.3 s');
    expect(strip).toHaveTextContent('2 skipped');
    expect(screen.getByRole('button', { name: 'Select failed node Fetch' })).toBeEnabled();
  });

  it('shows a clean run without a failed-node chip', () => {
    seed({
      lastRun: { runId: 'run-2', stoppedReason: 'completed', totalMs: 850, failedCount: 0, skippedCount: 0 },
    });
    render(<Harness />);
    expect(screen.getByTestId('run-result-strip')).toHaveTextContent('Run completed in 850 ms');
    expect(screen.queryByRole('button', { name: /Select failed node/ })).not.toBeInTheDocument();
  });

  it('does not blame a node for a cancelled run', () => {
    seed({
      lastRun: { runId: 'run-3', stoppedReason: 'cancelled', totalMs: 1200, failedCount: 0, skippedCount: 0 },
    });
    render(<Harness />);
    expect(screen.getByTestId('run-result-strip')).toHaveTextContent('Run cancelled in 1.2 s');
    expect(screen.queryByRole('button', { name: /Select failed node/ })).not.toBeInTheDocument();
  });

  it('selecting the failed node opens its panel on the Last run tab', async () => {
    seed({ lastRun: failedRun });
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Select failed node Fetch' }));
    expect(await screen.findByTestId('node-properties-panel')).toHaveTextContent('Fetch');
    expect(screen.getByRole('tab', { name: 'Last run' })).toHaveAttribute('aria-selected', 'true');
  });

  it('keeps the captured label and disables the chip once the node is deleted', () => {
    seed({ lastRun: failedRun });
    render(<Harness />);
    act(() => {
      usePaneStore.getState().updateFlowNodes(
        tabId,
        baseTab.nodes.filter((n) => n.id !== 'req1'),
      );
    });
    expect(screen.getByRole('button', { name: 'Select failed node Fetch' })).toBeDisabled();
  });

  it('clears the strip when a new run starts', () => {
    seed({ lastRun: failedRun });
    render(<Harness />);
    act(() => {
      usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-4');
    });
    expect(screen.queryByTestId('run-result-strip')).not.toBeInTheDocument();
  });
});

describe('FlowPane run-result wiring', () => {
  it('shows the strip, with the failed node label, after Run finishes', async () => {
    vi.mocked(runFlow).mockResolvedValue({
      runId: 'run-5',
      stoppedReason: 'completed',
      steps: [
        {
          nodeId: 'out1',
          status: 'failed',
          statusCode: null,
          durationMs: null,
          error: 'boom',
          value: null,
        },
      ],
    });
    seed({ runState: 'idle' });
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() =>
      expect(screen.getByTestId('run-result-strip')).toHaveTextContent('Run finished with 1 failed'),
    );
    expect(screen.getByRole('button', { name: 'Select failed node Result' })).toBeEnabled();
  });

  it('writes the result to the tab that started the run, not the active one', async () => {
    vi.mocked(runFlow).mockImplementation(async () => {
      // Another tab opens and takes focus while the run is in flight.
      usePaneStore.getState().openTab({ ...baseTab, id: 'flow-strip-2', flowName: 'other' });
      return {
        runId: 'run-6',
        stoppedReason: 'completed',
        steps: [
          { nodeId: 'out1', status: 'failed', statusCode: null, durationMs: null, error: 'boom', value: null },
        ],
      };
    });
    seed({ runState: 'idle' });
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Run' }));
    await waitFor(() => expect(screen.getByTestId('run-result-strip')).toBeInTheDocument());
    const other = findTabInTree(usePaneStore.getState().root, 'flow-strip-2');
    expect(other && isFlowTab(other.tab) ? other.tab.lastRun : 'missing').toBeUndefined();
  });
});
