import { beforeEach, describe, expect, it, vi } from 'vitest';
import { findTabInTree } from '@/lib/pane-utils';
import { type FlowLastRun, type FlowTab, isFlowTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), getFlow: vi.fn(), endAgentSession: vi.fn() };
});

const tabId = 'flow-run-tab';

const flowTab: FlowTab = {
  id: tabId,
  title: 'Flow: my-flow',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [{ id: 'a', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } }],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
};

const run = (over: Partial<FlowLastRun> = {}): FlowLastRun => ({
  runId: 'run-1',
  stoppedReason: 'completed',
  totalMs: 1000,
  failedCount: 0,
  skippedCount: 0,
  ...over,
});

function stored(): FlowTab {
  const found = findTabInTree(usePaneStore.getState().root, tabId);
  if (!found || !isFlowTab(found.tab)) throw new Error('Expected the flow tab');
  return found.tab;
}

describe('pane-store flow run result', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(flowTab);
  });

  it('setFlowRunResult stores the result on the tab', () => {
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-1');
    usePaneStore.getState().setFlowRunResult(tabId, run());
    expect(stored().lastRun).toEqual(run());
  });

  it('setFlowRunResult with undefined clears it', () => {
    usePaneStore.getState().setFlowRunResult(tabId, run());
    usePaneStore.getState().setFlowRunResult(tabId, undefined);
    expect(stored().lastRun).toBeUndefined();
  });

  it('a new run clears the previous result together with the node results', () => {
    usePaneStore.getState().setFlowRunResult(tabId, run());
    usePaneStore.getState().setFlowRunState(tabId, 'done', 'run-1');
    expect(stored().lastRun).toBeDefined();
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-2');
    expect(stored().lastRun).toBeUndefined();
  });

  it('finishing a run keeps the result', () => {
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-1');
    usePaneStore.getState().setFlowRunResult(tabId, run());
    usePaneStore.getState().setFlowRunState(tabId, 'done', 'run-1');
    expect(stored().lastRun?.runId).toBe('run-1');
  });

  it('ignores a result from an older run while a newer run is active', () => {
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-2');
    usePaneStore.getState().setFlowRunResult(tabId, run({ runId: 'run-1' }));
    expect(stored().lastRun).toBeUndefined();
  });

  it('keeps a timed result when a counts-only result for the same run arrives later', () => {
    usePaneStore.getState().setFlowRunResult(tabId, run({ failedNodeId: 'a', failedLabel: 'Out' }));
    usePaneStore.getState().setFlowRunResult(tabId, run({ totalMs: null }));
    expect(stored().lastRun?.totalMs).toBe(1000);
    expect(stored().lastRun?.failedLabel).toBe('Out');
  });

  it('is a no-op for an unknown tab id', () => {
    usePaneStore.getState().setFlowRunResult('missing', run());
    expect(stored().lastRun).toBeUndefined();
  });
});

describe('pane-store pending flow run', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore
      .getState()
      .openTab({ ...flowTab, nodeStatus: { a: 'success' }, runState: 'done', runId: 'run-0' });
  });

  it('stores the pending id and keeps the last run and its results', () => {
    usePaneStore.getState().setFlowPendingRun(tabId, 'run-1');
    expect(stored().pendingRunId).toBe('run-1');
    expect(stored().runId).toBe('run-0');
    expect(stored().runState).toBe('done');
    expect(stored().nodeStatus).toEqual({ a: 'success' });
  });

  it('every run state change clears the pending id', () => {
    usePaneStore.getState().setFlowPendingRun(tabId, 'run-1');
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-1');
    expect(stored().pendingRunId).toBeUndefined();
    usePaneStore.getState().setFlowPendingRun(tabId, 'run-2');
    usePaneStore.getState().setFlowRunState(tabId, 'done');
    expect(stored().pendingRunId).toBeUndefined();
  });

  it('reaches a tab parked in a collection snapshot', () => {
    usePaneStore.setState({
      collectionTabState: { other: { tabs: [{ ...flowTab, id: 'parked' }], activeTabId: 'parked' } },
    });
    usePaneStore.getState().setFlowPendingRun('parked', 'run-3');
    const parked = usePaneStore.getState().collectionTabState.other?.tabs[0];
    expect(parked && isFlowTab(parked) ? parked.pendingRunId : null).toBe('run-3');
  });
});
