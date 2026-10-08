import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
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
  id: 'flow-undo-1',
  tabType: 'flow',
  title: 'Flow: undo',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'undo',
  nodes: [
    { id: 'in1', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'alice' } },
    { id: 'out1', position: { x: 300, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
    { id: 'out2', position: { x: 300, y: 200 }, kind: { kind: 'Output', label: 'Other' } },
  ],
  edges: [
    {
      id: 'e1',
      sourceNodeId: 'in1',
      targetNodeId: 'out1',
      targetField: 'value',
      expression: 'response.body',
    },
    {
      id: 'e2',
      sourceNodeId: 'in1',
      targetNodeId: 'out2',
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
const nodeIds = () => getFlowTab().nodes.map((n) => n.id);

describe('FlowPane undo and redo', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });
  const setup = () => userEvent.setup({ pointerEventsCheck: 0 });

  it('recovers a Ctrl+A then Backspace wipe with one Ctrl+Z (issue #45)', async () => {
    const user = setup();
    render(<Harness />);
    // User-event mouse events have a null view, which d3-drag rejects on a node body.
    fireEvent.click(screen.getAllByTestId('output-node-card')[0]);
    fireEvent.keyDown(canvas(), { key: 'a', ctrlKey: true });
    // React Flow tracks held keys, so release A before the delete key.
    fireEvent.keyUp(canvas(), { key: 'a', ctrlKey: true });
    await user.keyboard('{Backspace}');
    await waitFor(() => expect(nodeIds()).toEqual([]));
    expect(getFlowTab().edges).toEqual([]);
    expect(getFlowTab().isDirty).toBe(true);

    fireEvent.keyDown(canvas(), { key: 'z', ctrlKey: true });
    await waitFor(() => expect(nodeIds()).toEqual(['in1', 'out1', 'out2']));
    expect(getFlowTab().nodes).toEqual(baseTab.nodes);
    expect(getFlowTab().edges).toEqual(baseTab.edges);
    expect(getFlowTab().isDirty).toBe(false);

    fireEvent.keyDown(canvas(), { key: 'Z', ctrlKey: true, shiftKey: true });
    await waitFor(() => expect(nodeIds()).toEqual([]));
    expect(getFlowTab().edges).toEqual([]);
  });

  it('shows disabled buttons at first and restores a deleted node from the button', async () => {
    const user = setup();
    render(<Harness />);
    expect(screen.getByRole('button', { name: 'Undo' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Redo' })).toBeDisabled();

    fireEvent.click(screen.getAllByTestId('output-node-card')[0]);
    await user.keyboard('{Delete}');
    await waitFor(() => expect(nodeIds()).not.toContain('out1'));
    expect(screen.getByRole('button', { name: 'Undo' })).toBeEnabled();

    await user.click(screen.getByRole('button', { name: 'Undo' }));
    expect(nodeIds()).toContain('out1');
    expect(getFlowTab().edges.map((e) => e.id)).toEqual(['e1', 'e2']);
    expect(screen.getByRole('button', { name: 'Undo' })).toBeDisabled();
    expect(screen.getByRole('button', { name: 'Redo' })).toBeEnabled();
  });

  it('does not keep a removed node selected, so a redo does not bring it back selected', async () => {
    render(<Harness />);
    const extra: FlowNode = {
      id: 'extra',
      position: { x: 600, y: 0 },
      kind: { kind: 'Output', label: 'Extra' },
    };
    act(() => {
      usePaneStore.getState().updateFlowNodes(baseTab.id, [...baseTab.nodes, extra]);
    });
    fireEvent.click(await screen.findByText('Extra'));
    await waitFor(() =>
      expect(document.querySelector('.react-flow__node[data-id="extra"]')).toHaveClass('selected'),
    );

    fireEvent.keyDown(canvas(), { key: 'z', ctrlKey: true });
    await waitFor(() => expect(nodeIds()).not.toContain('extra'));
    fireEvent.keyDown(canvas(), { key: 'y', ctrlKey: true });
    await waitFor(() => expect(nodeIds()).toContain('extra'));
    expect(document.querySelector('.react-flow__node[data-id="extra"]')).not.toHaveClass(
      'selected',
    );
  });

  it('does not undo while typing in a field inside the canvas', async () => {
    render(<Harness />);
    act(() => {
      usePaneStore.getState().updateFlowNodes(baseTab.id, baseTab.nodes.slice(0, 2));
    });
    const input = document.createElement('input');
    canvas().appendChild(input);
    fireEvent.keyDown(input, { key: 'z', ctrlKey: true });
    expect(nodeIds()).toEqual(['in1', 'out1']);
  });
});
