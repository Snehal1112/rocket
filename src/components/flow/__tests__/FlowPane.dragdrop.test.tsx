import { fireEvent, render, screen } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { encodeFlowRequestDragPayload, FLOW_REQUEST_DRAG_MIME } from '@/lib/flow-drag';
import { usePaneStore } from '@/stores/pane-store';
import type { FlowTab } from '@/types/pane-types';
import { isFlowTab } from '@/types/pane-types';
import { FlowPane } from '../FlowPane';

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
