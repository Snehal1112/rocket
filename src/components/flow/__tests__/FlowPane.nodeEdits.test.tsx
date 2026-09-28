import { act, fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';
import { isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

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

// jsdom has no DOMMatrixReadOnly, which React Flow reads when it re-measures a
// node after SwitchNode calls updateNodeInternals.
vi.stubGlobal(
  'DOMMatrixReadOnly',
  class {
    m22 = 1;
  },
);

const baseTab: FlowTab = {
  id: 'flow-edits-1',
  tabType: 'flow',
  title: 'Flow: routing',
  isDirty: false,
  collectionName: 'demo',
  flowName: 'routing',
  nodes: [
    {
      id: 'if1',
      position: { x: 0, y: 0 },
      kind: { kind: 'If', label: 'Logged in?', condition: 'response.status === 200' },
    },
    {
      id: 'sw1',
      position: { x: 400, y: 0 },
      kind: {
        kind: 'Switch',
        label: 'Plan router',
        value: 'response.body.plan',
        cases: [{ id: 'c1', label: 'Free', matches: 'free' }],
      },
    },
  ],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

// Reads the seeded flow tab back from the store.
function getFlowTab(): FlowTab {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
  const tab = root.tabs.find((t) => t.id === baseTab.id);
  if (!tab || !isFlowTab(tab)) throw new Error('Expected the seeded flow tab to be present');
  return tab;
}

describe('FlowPane inline node edits', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('keeps both edits when two nodes change in the same update', () => {
    render(<FlowPane tab={baseTab} groupId={usePaneStore.getState().activeGroupId} />);
    // Both edits run before React re-renders, so the second handler must not
    // work from the nodes it saw at the last render.
    act(() => {
      fireEvent.change(screen.getByLabelText('Condition'), {
        target: { value: 'response.status === 201' },
      });
      fireEvent.change(screen.getByLabelText('Switch value'), {
        target: { value: 'response.body.tier' },
      });
    });
    const [ifNode, switchNode] = getFlowTab().nodes;
    expect(ifNode.kind).toMatchObject({ condition: 'response.status === 201' });
    expect(switchNode.kind).toMatchObject({ value: 'response.body.tier' });
  });

  it('keeps an earlier edit when a case is removed in the same update', () => {
    render(<FlowPane tab={baseTab} groupId={usePaneStore.getState().activeGroupId} />);
    act(() => {
      fireEvent.change(screen.getByLabelText('Condition'), { target: { value: 'true' } });
      fireEvent.click(screen.getByLabelText('Remove case Free'));
    });
    const [ifNode, switchNode] = getFlowTab().nodes;
    expect(ifNode.kind).toMatchObject({ condition: 'true' });
    expect(switchNode.kind).toMatchObject({ cases: [] });
  });
});
