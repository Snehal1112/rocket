import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { getFlow, listCollections, listFlows, saveFlow } from '@/lib/tauri-api';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return {
    ...actual,
    listCollections: vi.fn(),
    listFlows: vi.fn(),
    saveFlow: vi.fn(),
    getFlow: vi.fn(),
  };
});

// `usePaneStore.setState({ openFlowTab: vi.fn(...) })` in the 'FlowPane
// picker' tests below replaces the store's `openFlowTab` action permanently
// (zustand `set` merges, and `reset()` does not restore actions), so later
// tests that call `usePaneStore.getState().openFlowTab` would silently run
// that stub instead of the real implementation. Capture the genuine action
// here, before any test can override it, so the save/reload test below
// exercises real store behavior regardless of test order.
const realOpenFlowTab = usePaneStore.getState().openFlowTab;

function pickerTab(collectionName: string | null): FlowTab {
  return {
    id: 'flow-picker-1',
    title: 'Flow',
    isDirty: false,
    tabType: 'flow',
    collectionName,
    flowName: null,
    nodes: [],
    edges: [],
    nodeStatus: {},
    runState: 'idle',
  };
}

describe('FlowPane picker', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
    vi.mocked(listCollections).mockResolvedValue([]);
    vi.mocked(listFlows).mockResolvedValue([]);
  });

  it('lists flows for the tab collection without a manual pick', async () => {
    render(<FlowPane tab={pickerTab('demo')} groupId='g1' />);
    await waitFor(() => expect(listFlows).toHaveBeenCalledWith('demo'));
  });

  it('creates an empty flow and opens it', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    const openFlowTab = vi.fn().mockResolvedValue(undefined);
    const closeTab = vi.fn();
    usePaneStore.setState({ openFlowTab, closeTab });

    render(<FlowPane tab={pickerTab('demo')} groupId='g1' />);
    await userEvent.type(screen.getByLabelText('New flow name'), 'Login flow');
    await userEvent.click(screen.getByRole('button', { name: 'Create flow' }));

    await waitFor(() => expect(openFlowTab).toHaveBeenCalledWith('demo', 'Login flow'));
    expect(saveFlow).toHaveBeenCalledWith('demo', { name: 'Login flow', nodes: [], edges: [] });
    expect(closeTab).toHaveBeenCalledWith('flow-picker-1', 'g1');
  });

  it('keeps the picker open when creating the flow fails', async () => {
    vi.mocked(saveFlow).mockRejectedValue('Invalid input: flow name is empty');
    const openFlowTab = vi.fn().mockResolvedValue(undefined);
    usePaneStore.setState({ openFlowTab });

    render(<FlowPane tab={pickerTab('demo')} groupId='g1' />);
    await userEvent.type(screen.getByLabelText('New flow name'), '!!!');
    await userEvent.click(screen.getByRole('button', { name: 'Create flow' }));

    await waitFor(() => expect(saveFlow).toHaveBeenCalled());
    expect(openFlowTab).not.toHaveBeenCalled();
  });
});

describe('FlowPane save', () => {
  const outputNode = (id: string) => ({
    id,
    kind: { kind: 'Output' as const, label: `Out ${id}` },
    position: { x: 0, y: 0 },
  });
  const flowTab: FlowTab = {
    id: 'flow-open-1',
    title: 'Flow: my-flow',
    isDirty: true,
    tabType: 'flow',
    collectionName: 'demo',
    flowName: 'my-flow',
    nodes: [outputNode('a'), outputNode('b'), outputNode('c')],
    edges: [],
    nodeStatus: {},
    runState: 'idle',
  };

  beforeEach(() => {
    usePaneStore.getState().reset();
    vi.clearAllMocks();
    usePaneStore.getState().openTab(flowTab);
  });

  it('flags the node ids named in a cycle rejection', async () => {
    vi.mocked(saveFlow).mockRejectedValue(
      'Invalid input: flow contains a cycle through node(s): a, b; edge(s): e1, e2',
    );
    render(<FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      const cards = screen.getAllByTestId('output-node-card');
      const flagged = cards.filter((c) => c.className.includes('ring-red-500'));
      expect(flagged.map((c) => c.textContent)).toEqual(['Out a—', 'Out b—']);
    });
  });

  it('flags the node named in a non-cycle validation error', async () => {
    vi.mocked(saveFlow).mockRejectedValue(
      "Invalid input: If node 'b' must have exactly one incoming edge — node(s): b; edge(s): ",
    );
    render(<FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => {
      const cards = screen.getAllByTestId('output-node-card');
      const flagged = cards.filter((c) => c.className.includes('ring-red-500'));
      expect(flagged.map((c) => c.textContent)).toEqual(['Out b—']);
    });
  });

  it('marks the tab clean after a successful save', async () => {
    vi.mocked(saveFlow).mockResolvedValue(undefined);
    render(<FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));

    await waitFor(() => expect(saveFlow).toHaveBeenCalled());
    const { root } = usePaneStore.getState();
    const tab = root.type === 'leaf' ? root.tabs.find((t) => t.id === flowTab.id) : undefined;
    expect(tab?.isDirty).toBe(false);
  });

  it('a saved flow reloads with the same nodes and edges when reopened', async () => {
    const store = new Map<string, { name: string; nodes: FlowNode[]; edges: FlowEdge[] }>();
    vi.mocked(saveFlow).mockImplementation(async (collection, flow) => {
      store.set(`${collection}/${flow.name}`, flow);
    });
    vi.mocked(getFlow).mockImplementation(async (collection, name) => {
      const saved = store.get(`${collection}/${name}`);
      if (!saved) throw new Error(`no such flow: ${collection}/${name}`);
      return saved;
    });

    render(<FlowPane tab={flowTab} groupId={usePaneStore.getState().activeGroupId} />);
    await userEvent.click(screen.getByRole('button', { name: 'Save' }));
    await waitFor(() => expect(saveFlow).toHaveBeenCalled());

    // The pre-existing flowTab already carries flowName 'my-flow', so a naive
    // find-by-flowName would match it instead of the newly reopened tab.
    // Track which tab ids exist before reopening and require the match to be new.
    const rootBefore = usePaneStore.getState().root;
    const idsBefore = new Set(rootBefore.type === 'leaf' ? rootBefore.tabs.map((t) => t.id) : []);

    await realOpenFlowTab('demo', 'my-flow');

    const { root } = usePaneStore.getState();
    const reopened =
      root.type === 'leaf'
        ? root.tabs.find(
            (t) => t.tabType === 'flow' && t.flowName === 'my-flow' && !idsBefore.has(t.id),
          )
        : undefined;
    expect(reopened).toBeDefined();
    expect(reopened && 'nodes' in reopened ? reopened.nodes : undefined).toEqual(flowTab.nodes);
    expect(reopened && 'edges' in reopened ? reopened.edges : undefined).toEqual(flowTab.edges);
  });
});
