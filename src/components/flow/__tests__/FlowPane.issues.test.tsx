import { act, render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { toast } from 'sonner';
import { computeFlowIssues } from '@/lib/flow-issues';
import { lintFlow, listCollections, listFlows, saveFlow } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual,
    lintFlow: vi.fn().mockResolvedValue([]), listCollections: vi.fn(), listFlows: vi.fn(), saveFlow: vi.fn() };
});
// Wraps the real function, so a test can count how often the pane recomputes issues.
vi.mock('@/lib/flow-issues', async (importOriginal) => {
  const actual = await importOriginal<typeof import('@/lib/flow-issues')>();
  return { ...actual, computeFlowIssues: vi.fn(actual.computeFlowIssues) };
});
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

// Radix menus and popovers call APIs that jsdom lacks.
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

const tabId = 'flow-issues-1';

const baseTab: FlowTab = {
  id: tabId,
  tabType: 'flow',
  title: 'Flow: issues',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'issues',
  nodes: [
    { id: 'in1', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'a' } },
    { id: 'out1', position: { x: 300, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
    { id: 'if1', position: { x: 0, y: 200 }, kind: { kind: 'If', label: 'Check', condition: 'true' } },
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
  nodeStatus: {},
  runState: 'idle',
};

function Harness() {
  const tab = usePaneStore((s) => {
    if (s.root.type !== 'leaf') return null;
    const t = s.root.tabs.find((x) => x.id === tabId);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

function storedNodes() {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected a leaf');
  const tab = root.tabs.find((t) => t.id === tabId);
  if (!tab || !isFlowTab(tab)) throw new Error('Expected the flow tab');
  return tab.nodes;
}

const movedBy = (dx: number) => () =>
  storedNodes().map((n) => ({ ...n, position: { x: n.position.x + dx, y: n.position.y } }));

describe('FlowPane issues', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    // The exit and output warnings come from the backend lint feed now.
    // The backend also reports the unwired input as its first graph error.
    vi.mocked(lintFlow).mockResolvedValue([
      {
        code: 'invalid_graph',
        severity: 'error',
        nodeId: 'if1',
        message: "'Check': needs an input wire",
      },
      { code: 'exit_without_edge', severity: 'warning', nodeId: 'if1', message: 'Nothing is wired to the true exit.' },
      { code: 'no_path_to_output', severity: 'warning', nodeId: 'if1', message: "'Check' does not lead to an Output." },
    ]);
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('counts the problems of the flow next to Run and badges the node', async () => {
    render(<Harness />);
    // The If node has no input (a client error). The backend adds two warnings.
    expect(await screen.findByRole('button', { name: '1 error, 2 warnings' })).toBeInTheDocument();
    const card = screen.getByTestId('if-node-card');
    expect(card.className).toContain('ring-destructive');
    expect(within(card).getByTestId('node-issue-badge')).toHaveAttribute('data-severity', 'error');
  });

  it('opens the node panel when an issue is chosen from the list', async () => {
    render(<Harness />);
    await userEvent.click(await screen.findByRole('button', { name: '1 error, 2 warnings' }));
    const list = await screen.findByRole('list', { name: 'Flow issues' });
    await userEvent.click(within(list).getAllByRole('button')[0]);
    expect(await screen.findByTestId('node-properties-panel')).toHaveTextContent('Check');
    expect(screen.getByRole('tab', { name: 'Settings' })).toHaveAttribute('aria-selected', 'true');
  });

  it('does not block a run or a save because of an issue', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(saveFlow).toHaveBeenCalled());
    expect(screen.getByRole('button', { name: 'Run' })).toBeEnabled();
  });

  it('adds a rejected save to the count and keeps the named node red', async () => {
    vi.mocked(saveFlow).mockRejectedValue(
      'Invalid input: flow is invalid: the Output has a problem — node(s): out1; edge(s): ',
    );
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await screen.findByRole('button', { name: '2 errors, 2 warnings' });
    expect(screen.getByTestId('output-node-card').className).toContain('ring-destructive');
    expect(screen.getByTestId('input-node-card').className).not.toContain('ring-destructive');
  });

  const REJECTED_SAVE =
    'Invalid input: flow is invalid: the Output has a problem — node(s): out1; edge(s): ';

  it('hides a quoted case value in the toast and the panel text', async () => {
    vi.mocked(saveFlow).mockRejectedValue(
      "Invalid input: flow is invalid: more than one case matches 'secret-token' — node(s): out1; edge(s): ",
    );
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(toast.error).toHaveBeenCalled());
    expect(String(vi.mocked(toast.error).mock.calls[0][0])).not.toContain('secret-token');
    expect(screen.getByTestId('node-properties-panel').textContent).toContain('value hidden');
    expect(document.body.textContent).not.toContain('secret-token');
  });

  it('clears the failed-save highlight when a wire is removed', async () => {
    vi.mocked(saveFlow).mockRejectedValue(REJECTED_SAVE);
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await screen.findByRole('button', { name: '2 errors, 2 warnings' });
    expect(screen.getByTestId('output-node-card').className).toContain('ring-destructive');

    act(() => {
      usePaneStore.getState().updateFlowEdges(tabId, []);
    });
    await waitFor(() =>
      expect(screen.getByTestId('output-node-card').className).not.toContain('ring-destructive'),
    );
    // Only the If node's own error is left.
    expect(screen.getByRole('button', { name: /^1 error,/ })).toBeInTheDocument();
  });

  it('keeps the failed-save highlight when only a node position changes', async () => {
    vi.mocked(saveFlow).mockRejectedValue(REJECTED_SAVE);
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await screen.findByRole('button', { name: '2 errors, 2 warnings' });

    act(() => {
      usePaneStore.getState().updateFlowNodes(tabId, movedBy(40)());
    });
    expect(screen.getByTestId('output-node-card').className).toContain('ring-destructive');
    expect(screen.getByRole('button', { name: '2 errors, 2 warnings' })).toBeInTheDocument();
  });

  it('does not recompute client issues for a position-only change, but does for a kind change', async () => {
    render(<Harness />);
    await screen.findByRole('button', { name: '1 error, 2 warnings' });
    const calls = () => vi.mocked(computeFlowIssues).mock.calls.length;
    const settled = calls();
    expect(settled).toBeGreaterThan(0);

    act(() => {
      usePaneStore.getState().updateFlowNodes(tabId, movedBy(25)());
    });
    expect(calls()).toBe(settled);

    act(() => {
      usePaneStore.getState().updateFlowNodes(
        tabId,
        storedNodes().map((n) =>
          n.id === 'if1' ? { ...n, kind: { kind: 'If', label: 'Check', condition: '' } } : n,
        ),
      );
    });
    expect(calls()).toBeGreaterThan(settled);
  });
});
