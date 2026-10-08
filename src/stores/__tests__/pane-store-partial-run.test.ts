import { beforeEach, describe, expect, it } from 'vitest';
import { findTabInTree } from '@/lib/pane-utils';
import { usePaneStore } from '@/stores/pane-store';
import { type FlowTab, isFlowTab } from '@/types/pane-types';

const output = (id: string) => ({
  id,
  position: { x: 0, y: 0 },
  kind: { kind: 'Output' as const, label: id },
});

const tab: FlowTab = {
  id: 'flow-partial-1',
  title: 'Flow: f',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'f',
  nodes: [output('a'), output('b'), output('c')],
  edges: [],
  nodeStatus: { a: 'success', b: 'success', c: 'failed' },
  nodeDetail: { a: { value: 'one' }, b: { value: 'two' }, c: { error: 'boom' } },
  runState: 'done',
  runId: 'run-0',
};

function stored(): FlowTab {
  const found = findTabInTree(usePaneStore.getState().root, tab.id);
  if (!found || !isFlowTab(found.tab)) throw new Error('Expected the flow tab');
  return found.tab;
}

describe('startPartialFlowRun', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(tab);
  });

  it('clears only the nodes the run executes and marks the rest as earlier results', () => {
    usePaneStore.getState().startPartialFlowRun(tab.id, 'run-1', ['b']);
    const next = stored();
    expect(next.runState).toBe('running');
    expect(next.runId).toBe('run-1');
    expect(next.nodeStatus).toEqual({ a: 'success', c: 'failed' });
    expect(next.nodeDetail?.b).toBeUndefined();
    expect(next.nodeDetail?.a).toEqual({ value: 'one', cached: true });
    expect(next.nodeDetail?.c).toEqual({ error: 'boom', cached: true });
  });

  it('a new result for a node replaces its earlier-run mark', () => {
    usePaneStore.getState().startPartialFlowRun(tab.id, 'run-1', ['b']);
    usePaneStore.getState().patchFlowNodeStatus(tab.id, 'b', 'success', { value: 'new' });
    expect(stored().nodeDetail?.b).toEqual({ value: 'new' });
  });

  it('a full run clears every earlier-run mark', () => {
    usePaneStore.getState().startPartialFlowRun(tab.id, 'run-1', ['b']);
    usePaneStore.getState().setFlowRunState(tab.id, 'running', 'run-2');
    expect(stored().nodeStatus).toEqual({});
    expect(stored().nodeDetail).toEqual({});
  });

  it('drops the marks but keeps the results when the run ends', () => {
    usePaneStore.getState().startPartialFlowRun(tab.id, 'run-1', ['b']);
    usePaneStore.getState().setFlowRunState(tab.id, 'done', 'run-1');
    expect(stored().nodeStatus).toEqual({ a: 'success', c: 'failed' });
    expect(stored().nodeDetail?.a).toEqual({ value: 'one' });
  });

  it('keeps the base run and its results when a refused run ends without an id', () => {
    usePaneStore.getState().setFlowRunState(tab.id, 'done');
    expect(stored().runId).toBe('run-0');
    expect(stored().nodeDetail?.a).toEqual({ value: 'one' });
  });

  it('marks results of a tab parked by a collection switch', () => {
    usePaneStore.setState({
      collectionTabState: {
        other: { tabs: [tab], activeTabId: tab.id },
      } as never,
    });
    usePaneStore.getState().startPartialFlowRun(tab.id, 'run-1', ['b']);
    const parked = usePaneStore.getState().collectionTabState.other?.tabs[0];
    expect(parked && isFlowTab(parked) ? parked.nodeDetail?.a?.cached : null).toBe(true);
  });
});
