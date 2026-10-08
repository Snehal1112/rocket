import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
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
    lintFlow: vi.fn().mockResolvedValue([]),
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
  id: 'flow-dbl-1',
  tabType: 'flow',
  title: 'Flow: dbl',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'dbl',
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

const panel = () => screen.queryByTestId('node-properties-panel');

// Waits until focus sits inside the properties panel.
async function expectFocusInPanel() {
  await waitFor(() =>
    expect(screen.getByTestId('node-properties-panel').contains(document.activeElement)).toBe(true),
  );
}

describe('FlowPane opens the properties panel on double-click', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('does not open the panel on a single click', () => {
    render(<Harness />);
    fireEvent.click(screen.getByTestId('output-node-card'));
    expect(panel()).not.toBeInTheDocument();
  });

  it('opens the panel for the node and moves focus into it', async () => {
    render(<Harness />);
    fireEvent.doubleClick(screen.getByTestId('output-node-card'));
    expect(panel()).toHaveTextContent('Output · Result');
    await expectFocusInPanel();
  });

  it('ignores a double-click inside an inline editor', () => {
    render(<Harness />);
    fireEvent.doubleClick(screen.getByLabelText('Condition'));
    expect(panel()).not.toBeInTheDocument();
  });

  it('ignores a double-click on the menu button', () => {
    render(<Harness />);
    fireEvent.doubleClick(screen.getByLabelText('Edit Result'));
    expect(panel()).not.toBeInTheDocument();
  });

  it('closes when another node is clicked', () => {
    render(<Harness />);
    fireEvent.doubleClick(screen.getByTestId('output-node-card'));
    expect(panel()).toBeInTheDocument();
    // User-event mouse events have a null view, which d3-drag rejects on a node body.
    fireEvent.click(screen.getByTestId('input-node-card'));
    expect(panel()).not.toBeInTheDocument();
  });

  it('closes when all nodes are selected with Ctrl+A', async () => {
    const user = userEvent.setup();
    render(<Harness />);
    fireEvent.doubleClick(screen.getByTestId('output-node-card'));
    expect(panel()).toBeInTheDocument();
    act(() => screen.getByTestId('flow-canvas').focus());
    await user.keyboard('{Control>}a{/Control}');
    expect(panel()).not.toBeInTheDocument();
  });
});
