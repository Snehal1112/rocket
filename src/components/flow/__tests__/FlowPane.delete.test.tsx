import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { deleteRequest, saveFlow, saveRequest } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/hooks/useCollectionVariableContext', () => ({
  useCollectionVariableContext: () => ({ variableContext: new Map() }),
}));

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
    saveRequest: vi.fn(),
    deleteRequest: vi.fn(),
  };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }));

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

// Park the resize handle away from the pointer, as the properties test does.
const realGetRect = Element.prototype.getBoundingClientRect;
Element.prototype.getBoundingClientRect = function getRect() {
  if (this instanceof HTMLElement && this.dataset.slot === 'resizable-handle') {
    return new DOMRect(5000, 5000, 1, 100);
  }
  return realGetRect.call(this);
};

const baseTab: FlowTab = {
  id: 'flow-del-1',
  tabType: 'flow',
  title: 'Flow: del',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'del',
  nodes: [
    { id: 'in1', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'alice' } },
    { id: 'out1', position: { x: 300, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
    { id: 'out2', position: { x: 300, y: 200 }, kind: { kind: 'Output', label: 'Other' } },
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

const nodeIds = () => getFlowTab().nodes.map((n) => n.id);

describe('FlowPane node deletion', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });
  const setup = () => userEvent.setup({ pointerEventsCheck: 0 });

  it('deletes the selected node with the Delete key', async () => {
    const user = setup();
    render(<Harness />);
    // User-event mouse events have a null view, which d3-drag rejects on a node body.
    fireEvent.click(screen.getAllByTestId('output-node-card')[0]);
    await user.keyboard('{Delete}');
    await waitFor(() => expect(nodeIds()).not.toContain('out1'));
    expect(nodeIds()).toContain('out2');
  });

  it('deletes a wired node and its edges with the panel button', async () => {
    const user = setup();
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Result'));
    await user.click(screen.getByRole('button', { name: 'Delete node' }));
    const tab = getFlowTab();
    expect(tab.nodes.map((n) => n.id)).toEqual(['in1', 'out2', 'req1']);
    expect(tab.edges.map((e) => e.id)).toEqual(['e2']);
    expect(screen.queryByTestId('node-properties-panel')).not.toBeInTheDocument();
    expect(document.activeElement).not.toBe(document.body);
    expect(saveFlow).not.toHaveBeenCalled();
  });

  it('keeps saved requests untouched when a Saved Request node is deleted', async () => {
    const user = setup();
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Login'));
    await user.click(await screen.findByRole('menuitem', { name: 'Edit properties' }));
    await user.click(screen.getByRole('button', { name: 'Delete node' }));
    expect(nodeIds()).not.toContain('req1');
    await user.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(saveFlow).toHaveBeenCalledTimes(1));
    const saved = vi.mocked(saveFlow).mock.calls[0][1];
    expect(saved.nodes.map((n) => n.id)).not.toContain('req1');
    expect(deleteRequest).not.toHaveBeenCalled();
    expect(saveRequest).not.toHaveBeenCalled();
  });

  it('says the saved request is not deleted in the tooltip', async () => {
    const user = setup();
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Login'));
    await user.click(await screen.findByRole('menuitem', { name: 'Edit properties' }));
    expect(screen.getByRole('button', { name: 'Delete node' })).toHaveAttribute(
      'title',
      'Removes this node from the flow. The saved request is not deleted.',
    );
  });

  it('moves focus into the panel after the node menu button', async () => {
    const user = setup();
    render(<Harness />);
    const menu = screen.getByLabelText('Edit Result');
    await user.click(menu);
    const panel = screen.getByTestId('node-properties-panel');
    await waitFor(() => expect(panel.contains(document.activeElement)).toBe(true));
    expect(document.activeElement).not.toBe(menu);
  });
});
