import { act, createEvent, fireEvent, render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
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
    listCollections: vi.fn().mockResolvedValue([]),
    listFlows: vi.fn().mockResolvedValue([]),
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

// jsdom reports every rect as 0,0,0,0 and userEvent clicks at 0,0, so the resize
// handle would count as hit by every click and steal focus from the fields.
// Park the handle away from the pointer.
const realGetRect = Element.prototype.getBoundingClientRect;
Element.prototype.getBoundingClientRect = function getRect() {
  if (this instanceof HTMLElement && this.dataset.slot === 'resizable-handle') {
    return new DOMRect(5000, 5000, 1, 100);
  }
  return realGetRect.call(this);
};

const baseTab: FlowTab = {
  id: 'flow-props-1',
  tabType: 'flow',
  title: 'Flow: props',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'props',
  nodes: [
    { id: 'in1', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'alice' } },
    { id: 'out1', position: { x: 300, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
    {
      id: 'req1',
      position: { x: 0, y: 200 },
      kind: {
        kind: 'Request',
        label: 'Fetch',
        source: { type: 'Inline', request: { method: 'GET', url: '', headers: [] } },
      },
    },
    {
      id: 'if1',
      position: { x: 300, y: 200 },
      kind: { kind: 'If', label: 'Check', condition: 'true' },
    },
    {
      id: 'sw1',
      position: { x: 600, y: 200 },
      kind: {
        kind: 'Switch',
        label: 'Route',
        value: 'x',
        cases: [{ id: 'c1', label: 'Case 1', matches: '' }],
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

// FlowPane receives the tab as a prop. Re-render it from the store after each
// store change, the way PaneRenderer does in the app.
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

describe('FlowPane node properties panel', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('is closed until a node is chosen', () => {
    render(<Harness />);
    expect(screen.queryByTestId('node-properties-panel')).not.toBeInTheDocument();
  });

  it('opens from a node ⋮ button and closes on ✕', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit User'));
    expect(screen.getByTestId('node-properties-panel')).toHaveTextContent('Input · User');
    await userEvent.click(screen.getByRole('button', { name: 'Close properties' }));
    expect(screen.queryByTestId('node-properties-panel')).not.toBeInTheDocument();
  });

  it('switches to the node whose ⋮ was clicked', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit User'));
    await userEvent.click(screen.getByLabelText('Edit Result'));
    expect(screen.getAllByTestId('node-properties-panel')).toHaveLength(1);
    expect(screen.getByTestId('node-properties-panel')).toHaveTextContent('Output · Result');
  });

  it('edits the label live and keeps the edges', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    await userEvent.type(screen.getByLabelText('Label'), '!');
    const tab = getFlowTab();
    expect(tab.nodes.find((n) => n.id === 'out1')?.kind.label).toBe('Result!');
    expect(tab.edges).toEqual(baseTab.edges);
    expect(tab.isDirty).toBe(true);
    expect(screen.getByTestId('output-node-card')).toHaveTextContent('Result!');
  });

  it('Backspace in the label field edits text and keeps the node', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    const field = screen.getByLabelText('Label');
    await userEvent.click(field);
    await userEvent.keyboard('{Backspace}');
    expect(document.activeElement).toBe(field);
    const tab = getFlowTab();
    expect(tab.nodes).toHaveLength(baseTab.nodes.length);
    expect(tab.nodes.find((n) => n.id === 'out1')?.kind.label).toBe('Resul');
  });

  it('closes when the node disappears', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    act(() => {
      usePaneStore.getState().updateFlowNodes(
        baseTab.id,
        getFlowTab().nodes.filter((n) => n.id !== 'out1'),
      );
    });
    expect(screen.queryByTestId('node-properties-panel')).not.toBeInTheDocument();
  });

  it('opens on a node added from the palette', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByRole('button', { name: 'Add node' }));
    await userEvent.click(await screen.findByRole('menuitem', { name: 'Output' }));
    expect(screen.getByTestId('node-properties-panel')).toHaveTextContent('Output · New Output');
  });

  it('closes when the empty canvas is clicked', async () => {
    const { container } = render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    const pane = container.querySelector('.react-flow__pane');
    if (!pane) throw new Error('Expected the React Flow pane');
    // With drag-select on, React Flow reports a pane click from pointer events.
    // jsdom pointer events lack isPrimary, which React Flow checks.
    const down = createEvent.pointerDown(pane, { button: 0, clientX: 5, clientY: 5 });
    Object.defineProperty(down, 'isPrimary', { value: true });
    fireEvent(pane, down);
    fireEvent.pointerUp(pane, { button: 0, clientX: 5, clientY: 5 });
    expect(screen.queryByTestId('node-properties-panel')).not.toBeInTheDocument();
  });

  it('keeps the node when Backspace is pressed on the resize handle', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    const handle = screen.getByRole('separator');
    act(() => handle.focus());
    await userEvent.keyboard('{Backspace}');
    expect(getFlowTab().nodes).toHaveLength(baseTab.nodes.length);
  });

  it('closes the panel when the pane switches to another flow tab', async () => {
    const otherTab: FlowTab = { ...baseTab, id: 'flow-props-2', flowName: 'other' };
    const groupId = usePaneStore.getState().activeGroupId;
    const { rerender } = render(<FlowPane tab={baseTab} groupId={groupId} />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    expect(screen.getByTestId('node-properties-panel')).toBeInTheDocument();
    rerender(<FlowPane tab={otherTab} groupId={groupId} />);
    expect(screen.queryByTestId('node-properties-panel')).not.toBeInTheDocument();
  });

  it('keeps the node when Backspace is pressed on a panel button', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    const close = screen.getByRole('button', { name: 'Close properties' });
    act(() => close.focus());
    await userEvent.keyboard('{Backspace}');
    expect(getFlowTab().nodes.map((n) => n.id)).toContain('out1');
  });

  it('keeps a palette-added node on Backspace and edits the selected label', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'Add node' }));
    await user.click(await screen.findByRole('menuitem', { name: 'Output' }));
    const field = screen.getByLabelText('Label');
    await waitFor(() => expect(document.activeElement).toBe(field));
    await user.keyboard('{Backspace}');
    expect(getFlowTab().nodes).toHaveLength(baseTab.nodes.length + 1);
    expect(document.activeElement).toBe(field);
    // The whole label was selected, so Backspace cleared it.
    expect(field).toHaveValue('');
  });

  it('does not steal focus when a node is opened from its menu later', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Result'));
    await new Promise((r) => setTimeout(r, 20));
    expect(document.activeElement).not.toBe(screen.getByLabelText('Label'));
  });

  it('opens the Request node panel from its menu', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Fetch'));
    await user.click(await screen.findByRole('menuitem', { name: 'Edit properties' }));
    expect(screen.getByTestId('node-properties-panel')).toHaveTextContent('Request · Fetch');
  });

  it('toggles debug mode from the Request menu, marks the tab dirty and keeps the node', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Fetch'));
    await screen.findByRole('menu');
    await user.keyboard('{Backspace}');
    expect(getFlowTab().nodes.map((n) => n.id)).toContain('req1');
    await user.click(screen.getByRole('menuitemcheckbox', { name: 'Debug mode' }));
    const tab = getFlowTab();
    const node = tab.nodes.find((n) => n.id === 'req1');
    expect(node?.kind).toMatchObject({ kind: 'Request', debug: true });
    expect(tab.isDirty).toBe(true);
  });

  it.each([
    ['Check', 'If · Check'],
    ['Route', 'Switch · Route'],
  ])('opens for the %s node from its menu button', async (label, header) => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText(`Edit ${label}`));
    expect(screen.getByTestId('node-properties-panel')).toHaveTextContent(header);
  });

  it('fades the wire of the exit not taken on the Wires tab', async () => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab({
      ...baseTab,
      edges: [
        {
          id: 'wt',
          sourceNodeId: 'if1',
          targetNodeId: 'out1',
          targetField: 'value',
          expression: '',
          sourceHandle: 'true',
        },
        {
          id: 'wf',
          sourceNodeId: 'if1',
          targetNodeId: 'in1',
          targetField: 'value',
          expression: '',
          sourceHandle: 'false',
        },
      ],
      nodeStatus: { if1: 'success' },
      nodeDetail: { if1: { branch: 'true' } },
    });
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Check'));
    await userEvent.click(screen.getByRole('tab', { name: 'Wires' }));
    const rows = screen.getAllByTestId('wire-row');
    expect(rows).toHaveLength(2);
    const faded = rows.filter((r) => r.className.includes('opacity-50'));
    expect(faded).toHaveLength(1);
    expect(faded[0]).toHaveTextContent('User');
    expect(faded[0]).toHaveTextContent('not taken');
  });

  it('selects the wired node from the Wires tab and stays on Wires', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    await userEvent.click(screen.getByRole('tab', { name: 'Wires' }));
    await userEvent.click(screen.getByRole('button', { name: 'Select node User' }));
    expect(screen.getByTestId('node-properties-panel')).toHaveTextContent('Input · User');
    expect(screen.getByRole('tab', { name: 'Wires' })).toHaveAttribute('aria-selected', 'true');
    expect(screen.getByTestId('wires-outgoing')).toBeInTheDocument();
  });

  it('keeps the newly selected node on Backspace after a Wires node link', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    await userEvent.click(screen.getByRole('tab', { name: 'Wires' }));
    await userEvent.click(screen.getByRole('button', { name: 'Select node User' }));
    expect(document.activeElement).toBe(screen.getByTestId('node-properties-panel'));
    await userEvent.keyboard('{Backspace}');
    expect(getFlowTab().nodes.map((n) => n.id)).toContain('in1');
  });

  it.each([
    ['the Copy button', 'Copy response body'],
    ['the Request as sent trigger', /Request as sent/],
  ])('keeps the node on Backspace with focus on %s in Last run', async (_, name) => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab({
      ...baseTab,
      nodeStatus: { req1: 'success' },
      nodeDetail: {
        req1: {
          statusCode: 200,
          exchange: {
            method: 'GET',
            url: 'https://x.test',
            headers: [],
            response: {
              status: 200,
              statusText: 'OK',
              durationMs: 1,
              sizeBytes: 2,
              headers: [],
              body: '{}',
            },
          },
        },
      },
    });
    const user = userEvent.setup();
    render(<Harness />);
    await user.click(screen.getByLabelText('Edit Fetch'));
    await user.click(await screen.findByRole('menuitem', { name: 'Edit properties' }));
    await user.click(screen.getByRole('tab', { name: 'Last run' }));
    const button = screen.getByRole('button', { name });
    act(() => button.focus());
    await user.keyboard('{Backspace}');
    expect(getFlowTab().nodes.map((n) => n.id)).toContain('req1');
  });

  it('opens the wire script dialog from the Wires tab pencil', async () => {
    render(<Harness />);
    await userEvent.click(screen.getByLabelText('Edit Result'));
    await userEvent.click(screen.getByRole('tab', { name: 'Wires' }));
    await userEvent.click(screen.getByRole('button', { name: /Edit wire into/i }));
    expect(await screen.findByText('Value from source')).toBeInTheDocument();
  });
});
