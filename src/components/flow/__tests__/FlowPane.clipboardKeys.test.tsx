import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { clearFlowClipboard } from '@/lib/flow-clipboard';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    lintFlow: vi.fn().mockResolvedValue([]),
    listCollections: vi.fn().mockResolvedValue([]),
    listFlows: vi.fn().mockResolvedValue([]),
    saveFlow: vi.fn().mockResolvedValue(undefined),
  };
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

// Park the resize handle away from the pointer, as the delete test does.
const realGetRect = Element.prototype.getBoundingClientRect;
Element.prototype.getBoundingClientRect = function getRect() {
  if (this instanceof HTMLElement && this.dataset.slot === 'resizable-handle') {
    return new DOMRect(5000, 5000, 1, 100);
  }
  return realGetRect.call(this);
};

const baseTab: FlowTab = {
  id: 'flow-clipkeys-1',
  tabType: 'flow',
  title: 'Flow: keys',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'keys',
  nodes: [
    { id: 'in1', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'alice' } },
    { id: 'out1', position: { x: 300, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
    {
      id: 'req1',
      position: { x: 0, y: 200 },
      kind: {
        kind: 'Request',
        label: 'Login',
        source: { type: 'Saved', requestPath: 'login.yml' },
      },
    },
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

function getFlowTab(): FlowTab {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
  const tab = root.tabs.find((t) => t.id === baseTab.id);
  if (!tab || !isFlowTab(tab)) throw new Error('Expected the seeded flow tab');
  return tab;
}

function Harness() {
  const tab = usePaneStore((s) => {
    const root = s.root;
    if (root.type !== 'leaf') return null;
    const t = root.tabs.find((x) => x.id === baseTab.id);
    return t && isFlowTab(t) ? t : null;
  });
  if (!tab) return null;
  return <FlowPane tab={tab} groupId={usePaneStore.getState().activeGroupId} />;
}

const canvas = () => screen.getByTestId('flow-canvas');
const count = () => getFlowTab().nodes.length;

describe('FlowPane clipboard keys on the real canvas', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
    clearFlowClipboard();
  });

  it('copies with Ctrl+C and pastes with Ctrl+V, selecting the new node', async () => {
    render(<Harness />);
    // User-event mouse events have a null view, which d3-drag rejects on a node body.
    fireEvent.click(screen.getAllByTestId('output-node-card')[0]);
    fireEvent.keyDown(canvas(), { key: 'c', ctrlKey: true });
    fireEvent.keyDown(canvas(), { key: 'v', ctrlKey: true });
    await waitFor(() => expect(count()).toBe(4));
    const pasted = getFlowTab().nodes[3];
    expect(pasted.position).toEqual({ x: 340, y: 40 });
    await waitFor(() =>
      expect(document.querySelector(`.react-flow__node[data-id="${pasted.id}"]`)).toHaveClass(
        'selected',
      ),
    );
    expect(document.querySelector('.react-flow__node[data-id="out1"]')).not.toHaveClass('selected');
  });

  it('undoes a paste in one Ctrl+Z', async () => {
    render(<Harness />);
    fireEvent.click(screen.getAllByTestId('output-node-card')[0]);
    fireEvent.keyDown(canvas(), { key: 'c', ctrlKey: true });
    fireEvent.keyDown(canvas(), { key: 'v', ctrlKey: true });
    fireEvent.keyDown(canvas(), { key: 'v', ctrlKey: true });
    await waitFor(() => expect(count()).toBe(5));
    fireEvent.keyDown(canvas(), { key: 'z', ctrlKey: true });
    await waitFor(() => expect(count()).toBe(4));
    fireEvent.keyDown(canvas(), { key: 'z', ctrlKey: true });
    await waitFor(() => expect(count()).toBe(3));
  });

  it('duplicates the wired pair with Ctrl+D', async () => {
    render(<Harness />);
    fireEvent.click(screen.getAllByTestId('output-node-card')[0]);
    fireEvent.keyDown(canvas(), { key: 'a', ctrlKey: true });
    fireEvent.keyDown(canvas(), { key: 'd', ctrlKey: true });
    await waitFor(() => expect(count()).toBe(6));
    const tab = getFlowTab();
    expect(tab.edges).toHaveLength(2);
    const ids = new Set(tab.nodes.map((n) => n.id));
    expect(ids.size).toBe(6);
    for (const e of tab.edges) {
      expect(ids.has(e.sourceNodeId)).toBe(true);
      expect(ids.has(e.targetNodeId)).toBe(true);
    }
    expect(tab.edges[1].sourceNodeId).not.toBe('in1');
    expect(tab.edges[1].targetNodeId).not.toBe('out1');
  });

  it('leaves Ctrl+V to a field inside the canvas', () => {
    render(<Harness />);
    fireEvent.click(screen.getAllByTestId('output-node-card')[0]);
    fireEvent.keyDown(canvas(), { key: 'c', ctrlKey: true });
    const input = document.createElement('input');
    canvas().appendChild(input);
    fireEvent.keyDown(input, { key: 'v', ctrlKey: true });
    expect(count()).toBe(3);
  });

  it('duplicates a request from its node menu', async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Login'));
    await user.click(await screen.findByRole('menuitem', { name: 'Duplicate' }));
    await waitFor(() => expect(count()).toBe(4));
    const copy = getFlowTab().nodes[3].kind;
    if (copy.kind !== 'Request') throw new Error('Expected a Request node');
    expect(copy.source).toEqual({ type: 'Saved', requestPath: 'login.yml' });
  });
});
