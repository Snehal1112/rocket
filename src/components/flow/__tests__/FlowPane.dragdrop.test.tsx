import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import {
  encodeFlowRequestDragPayload,
  FLOW_REQUEST_DRAG_MIME,
  FLOW_REQUEST_DRAG_TEXT_PREFIX,
} from '@/lib/flow-drag';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';
import { isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

vi.mock('@/hooks/useCollectionVariableContext', () => ({
  useCollectionVariableContext: () => ({ variableContext: new Map() }),
}));

// The Request editor pulls in Monaco, which jsdom cannot load.
vi.mock('@/components/editor/MonacoWrapper', () => ({ MonacoWrapper: () => null }));

vi.mock('sonner', () => ({ toast: { error: vi.fn() } }));

const baseTab: FlowTab = {
  id: 'flow-1',
  tabType: 'flow',
  title: 'Untitled Flow',
  isDirty: false,
  collectionName: 'my-collection',
  flowName: 'my-flow',
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

// Reads the seeded flow tab back from the store. After reset() + openTab(),
// the pane tree is a single leaf, so the tab lives directly in root.tabs.
function getFlowTab(): FlowTab {
  const { root } = usePaneStore.getState();
  if (root.type !== 'leaf') throw new Error('Expected root to be a leaf');
  const tab = root.tabs.find((t) => t.id === baseTab.id);
  if (!tab || !isFlowTab(tab)) throw new Error('Expected the seeded flow tab to be present');
  return tab;
}

describe('FlowPane drag-and-drop', () => {
  beforeEach(() => {
    // The store only updates tabs that are in its tree, so seed baseTab.
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(baseTab);
  });

  it('adds a Saved Request node at the drop position when a sidebar request is dropped', () => {
    render(<FlowPane tab={baseTab} groupId={usePaneStore.getState().activeGroupId} />);
    const canvas = screen.getByTestId('flow-canvas'); // Plan 09 must expose this test id on the ReactFlow wrapper

    const dataTransfer = {
      getData: (type: string) =>
        type === FLOW_REQUEST_DRAG_MIME
          ? encodeFlowRequestDragPayload({
              collection: 'my-collection',
              path: 'auth/login.yml',
              name: 'Login',
              method: 'POST',
            })
          : '',
    };

    fireEvent.dragOver(canvas, { dataTransfer });
    fireEvent.drop(canvas, { dataTransfer, clientX: 200, clientY: 150 });

    const updatedTab = getFlowTab();
    expect(updatedTab.nodes).toHaveLength(1);
    expect(updatedTab.nodes[0].kind).toMatchObject({
      kind: 'Request',
      label: 'Login',
      source: { type: 'Saved', requestPath: 'auth/login.yml' },
    });
  });

  it('dropping the same request twice creates two independent nodes', () => {
    const { rerender } = render(
      <FlowPane tab={baseTab} groupId={usePaneStore.getState().activeGroupId} />,
    );
    const dataTransfer = {
      getData: (type: string) =>
        type === FLOW_REQUEST_DRAG_MIME
          ? encodeFlowRequestDragPayload({
              collection: 'my-collection',
              path: 'auth/refresh.yml',
              name: 'Refresh',
              method: 'POST',
            })
          : '',
    };
    fireEvent.drop(screen.getByTestId('flow-canvas'), { dataTransfer, clientX: 10, clientY: 10 });
    // FlowPane reads nodes from its tab prop, so pass the updated tab back.
    rerender(<FlowPane tab={getFlowTab()} groupId={usePaneStore.getState().activeGroupId} />);
    fireEvent.drop(screen.getByTestId('flow-canvas'), { dataTransfer, clientX: 300, clientY: 10 });

    const { nodes } = getFlowTab();
    expect(nodes).toHaveLength(2);
    expect(nodes[0].id).not.toBe(nodes[1].id);
    for (const node of nodes) {
      expect(node.kind).toMatchObject({
        source: { type: 'Saved', requestPath: 'auth/refresh.yml' },
      });
    }
  });

  it('adds a Saved Request node from the text fallback payload', () => {
    render(<FlowPane tab={baseTab} groupId={usePaneStore.getState().activeGroupId} />);
    const payload = encodeFlowRequestDragPayload({
      collection: 'my-collection',
      path: 'auth/login.yml',
      name: 'Login',
      method: 'POST',
    });
    const dataTransfer = {
      getData: (type: string) =>
        type === 'text/plain' ? `${FLOW_REQUEST_DRAG_TEXT_PREFIX}${payload}` : '',
    };

    fireEvent.drop(screen.getByTestId('flow-canvas'), { dataTransfer, clientX: 200, clientY: 150 });

    expect(getFlowTab().nodes).toHaveLength(1);
  });

  it('ignores a drop whose dataTransfer carries no flow-request payload', () => {
    render(<FlowPane tab={baseTab} groupId={usePaneStore.getState().activeGroupId} />);
    const canvas = screen.getByTestId('flow-canvas');
    fireEvent.drop(canvas, { dataTransfer: { getData: () => '' } });
    const updatedTab = getFlowTab();
    expect(updatedTab.nodes).toHaveLength(0);
  });

  it('rejects a drop whose payload belongs to a different collection than the flow', () => {
    render(<FlowPane tab={baseTab} groupId={usePaneStore.getState().activeGroupId} />);
    const canvas = screen.getByTestId('flow-canvas');

    const dataTransfer = {
      getData: (type: string) =>
        type === FLOW_REQUEST_DRAG_MIME
          ? encodeFlowRequestDragPayload({
              collection: 'a-different-collection',
              path: 'auth/login.yml',
              name: 'Login',
              method: 'POST',
            })
          : '',
    };

    fireEvent.drop(canvas, { dataTransfer, clientX: 200, clientY: 150 });

    const updatedTab = getFlowTab();
    expect(updatedTab.nodes).toHaveLength(0);
  });
});
