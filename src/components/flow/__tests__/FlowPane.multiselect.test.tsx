import { act, render, screen } from '@testing-library/react';
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
    listCollections: vi.fn().mockResolvedValue([]),
    listFlows: vi.fn().mockResolvedValue([]),
  };
});

vi.mock('sonner', () => ({ toast: { error: vi.fn(), success: vi.fn() } }));

vi.mock('@/components/editor', () => ({ SingleLineEditor: () => null }));

// jsdom cannot drive React Flow's multi-select gesture, so the canvas is a
// stand-in that reports a selection through the same callback.
let reportSelection: (ids: string[]) => void = () => undefined;
let openProperties: (id: string) => void = () => undefined;
vi.mock('../FlowCanvas', () => ({
  FlowCanvas: (props: {
    onSelectedNodeIdsChange?: (ids: ReadonlySet<string>) => void;
    onOpenProperties?: (id: string) => void;
  }) => {
    reportSelection = (ids) => props.onSelectedNodeIdsChange?.(new Set(ids));
    openProperties = (id) => props.onOpenProperties?.(id);
    return null;
  },
}));

const tab: FlowTab = {
  id: 'flow-multi-1',
  tabType: 'flow',
  title: 'Flow: multi',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'multi',
  nodes: [
    { id: 'a', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'A' } },
    { id: 'b', position: { x: 100, y: 0 }, kind: { kind: 'Output', label: 'B' } },
  ],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

describe('FlowPane multi-selection', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(tab);
  });

  it('shows the panel for the opened node and closes it when two are selected', () => {
    const { root } = usePaneStore.getState();
    if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
    const stored = root.tabs.find((t) => t.id === tab.id);
    if (!stored || !isFlowTab(stored)) throw new Error('Expected the seeded flow tab');
    render(<FlowPane tab={stored} groupId={usePaneStore.getState().activeGroupId} />);
    act(() => {
      reportSelection(['a']);
      openProperties('a');
    });
    expect(screen.getByTestId('node-properties-panel')).toHaveTextContent('Output · A');
    act(() => reportSelection(['a', 'b']));
    expect(screen.queryByTestId('node-properties-panel')).not.toBeInTheDocument();
  });

  it('does not open the panel for a selection alone', () => {
    const { root } = usePaneStore.getState();
    if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
    const stored = root.tabs.find((t) => t.id === tab.id);
    if (!stored || !isFlowTab(stored)) throw new Error('Expected the seeded flow tab');
    render(<FlowPane tab={stored} groupId={usePaneStore.getState().activeGroupId} />);
    act(() => reportSelection(['a']));
    expect(screen.queryByTestId('node-properties-panel')).not.toBeInTheDocument();
  });
});
