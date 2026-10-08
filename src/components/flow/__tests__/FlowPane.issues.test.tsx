import { render, screen, waitFor, within } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { listCollections, listFlows, saveFlow } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, listCollections: vi.fn(), listFlows: vi.fn(), saveFlow: vi.fn() };
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

describe('FlowPane issues', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('counts the problems of the flow next to Run and badges the node', () => {
    render(<Harness />);
    // The If node has no input (error), both its exits are unwired and it leads to
    // no Output (two warnings).
    expect(screen.getByRole('button', { name: '1 error, 2 warnings' })).toBeInTheDocument();
    const card = screen.getByTestId('if-node-card');
    expect(card.className).toContain('ring-red-500');
    expect(within(card).getByTestId('node-issue-badge')).toHaveAttribute('data-severity', 'error');
  });

  it('opens the node panel when an issue is chosen from the list', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: '1 error, 2 warnings' }));
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
    expect(screen.getByTestId('output-node-card').className).toContain('ring-red-500');
    expect(screen.getByTestId('input-node-card').className).not.toContain('ring-red-500');
  });
});
