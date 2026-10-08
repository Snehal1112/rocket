import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
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

vi.mock('@/components/editor', () => ({ SingleLineEditor: () => null }));

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
  id: 'flow-tidy-1',
  tabType: 'flow',
  title: 'Flow: tidy',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'tidy',
  nodes: [
    { id: 'in1', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'alice' } },
    { id: 'out1', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
    { id: 'out2', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Other' } },
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

describe('FlowPane tidy', () => {
  beforeEach(() => {
    vi.clearAllMocks();
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('moves the nodes apart in one undo step and keeps the graph otherwise', async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'Tidy layout' }));
    await waitFor(() => expect(getFlowTab().isDirty).toBe(true));
    const tidy = getFlowTab();
    expect(new Set(tidy.nodes.map((n) => `${n.position.x},${n.position.y}`)).size).toBe(3);
    expect(tidy.nodes.map((n) => n.id)).toEqual(['in1', 'out1', 'out2']);
    expect(tidy.edges).toBe(baseTab.edges);
    expect(tidy.history?.past).toHaveLength(1);

    await user.click(screen.getByRole('button', { name: 'Undo' }));
    expect(getFlowTab().nodes).toBe(baseTab.nodes);
    expect(getFlowTab().isDirty).toBe(false);
  });

  it('adds no undo step when pressed again on a tidy graph', async () => {
    const user = userEvent.setup({ pointerEventsCheck: 0 });
    render(<Harness />);
    await user.click(screen.getByRole('button', { name: 'Tidy layout' }));
    await waitFor(() => expect(getFlowTab().history?.past).toHaveLength(1));
    await user.click(screen.getByRole('button', { name: 'Tidy layout' }));
    expect(getFlowTab().history?.past).toHaveLength(1);
  });
});
