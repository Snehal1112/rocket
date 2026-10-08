import { beforeEach, describe, expect, it, vi } from 'vitest';
import { buildRunRecord, MAX_RUN_HISTORY } from '@/lib/flow-run-history';
import { findTabInTree } from '@/lib/pane-utils';
import { type FlowRunRecord, type FlowTab, isFlowTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getCollection: vi.fn(), getFlow: vi.fn(), endAgentSession: vi.fn() };
});

const tabId = 'flow-hist-tab';

const flowTab = (id = tabId): FlowTab => ({
  id,
  title: 'Flow: my-flow',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'demo',
  flowName: 'my-flow',
  nodes: [{ id: 'a', kind: { kind: 'Output', label: 'Out' }, position: { x: 0, y: 0 } }],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
});

const record = (runId: string): FlowRunRecord => ({
  runId,
  finishedAt: 1000,
  environmentName: null,
  result: { runId, stoppedReason: 'completed', totalMs: 10, failedCount: 0, skippedCount: 0 },
  nodeStatus: { a: 'success' },
  nodeDetail: { a: { durationMs: 3 } },
});

function stored(id = tabId): FlowTab {
  const found = findTabInTree(usePaneStore.getState().root, id);
  if (!found || !isFlowTab(found.tab)) throw new Error('Expected the flow tab');
  return found.tab;
}

describe('pane-store flow run history', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    usePaneStore.getState().openTab(flowTab());
  });

  it('recordFlowRun keeps the newest run first', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().recordFlowRun(tabId, record('r2'));
    expect(stored().runHistory?.map((r) => r.runId)).toEqual(['r2', 'r1']);
  });

  it('recordFlowRun caps the history', () => {
    for (let i = 1; i <= MAX_RUN_HISTORY + 1; i += 1) {
      usePaneStore.getState().recordFlowRun(tabId, record(`r${i}`));
    }
    expect(stored().runHistory).toHaveLength(MAX_RUN_HISTORY);
  });

  it('recording the same run twice keeps one entry', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().recordFlowRun(tabId, { ...record('r1'), finishedAt: 2000 });
    expect(stored().runHistory).toHaveLength(1);
    expect(stored().runHistory?.[0].finishedAt).toBe(2000);
  });

  it('recording does not mark the tab dirty', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    expect(stored().isDirty).toBe(false);
  });

  it('a record keeps its results when the next run wipes the live ones', () => {
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-1');
    usePaneStore.getState().patchFlowNodeStatus(tabId, 'a', 'success', { durationMs: 3 });
    usePaneStore.getState().recordFlowRun(tabId, record('run-1'));
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-2');
    expect(stored().nodeStatus).toEqual({});
    expect(stored().runHistory?.[0].nodeStatus).toEqual({ a: 'success' });
    expect(stored().runHistory?.[0].nodeDetail).toEqual({ a: { durationMs: 3 } });
  });

  it('history belongs to one tab', () => {
    usePaneStore.getState().openTab(flowTab('flow-hist-other'));
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    expect(stored('flow-hist-other').runHistory).toBeUndefined();
  });

  it('recordFlowRun for an unknown tab does nothing', () => {
    usePaneStore.getState().recordFlowRun('missing', record('r1'));
    expect(stored().runHistory).toBeUndefined();
  });

  it('setViewedFlowRun shows a recorded run and null returns to live', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().setViewedFlowRun(tabId, 'r1');
    expect(stored().viewedRunId).toBe('r1');
    usePaneStore.getState().setViewedFlowRun(tabId, null);
    expect(stored().viewedRunId).toBeNull();
  });

  it('setViewedFlowRun ignores a run that is not in the history', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().setViewedFlowRun(tabId, 'nope');
    expect(stored().viewedRunId ?? null).toBeNull();
  });

  it('setViewedFlowRun does not switch the view while a run is active', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-2');
    usePaneStore.getState().setViewedFlowRun(tabId, 'r1');
    expect(stored().viewedRunId ?? null).toBeNull();
  });

  it('starting a run returns the view to live and keeps the history', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().setViewedFlowRun(tabId, 'r1');
    usePaneStore.getState().setFlowRunState(tabId, 'running', 'run-2');
    expect(stored().viewedRunId).toBeNull();
    expect(stored().runHistory).toHaveLength(1);
  });

  it('viewing a run does not mark the tab dirty', () => {
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().setViewedFlowRun(tabId, 'r1');
    expect(stored().isDirty).toBe(false);
  });

  it('records and views a run on a tab parked by a collection switch', () => {
    usePaneStore.setState({ activeCollection: 'demo' });
    usePaneStore.getState().switchCollection('other');
    usePaneStore.getState().recordFlowRun(tabId, record('r1'));
    usePaneStore.getState().setViewedFlowRun(tabId, 'r1');
    usePaneStore.getState().switchCollection('demo');
    expect(stored().runHistory?.map((r) => r.runId)).toEqual(['r1']);
    expect(stored().viewedRunId).toBe('r1');
  });

  it('keeps the callback URLs of a run out of its history record', () => {
    const api = usePaneStore.getState();
    api.setFlowRunState(tabId, 'running', 'run-1');
    api.setFlowCallbackUrls(tabId, { w: 'http://h/cb/SECRET' });
    api.setFlowRunResult(tabId, {
      runId: 'run-1',
      stoppedReason: 'completed',
      totalMs: 5,
      failedCount: 0,
      skippedCount: 0,
    });
    expect(JSON.stringify(stored().callbackUrls)).toContain('SECRET');
    const built = buildRunRecord(stored(), 'run-1', 1);
    if (!built) throw new Error('Expected a record');
    usePaneStore.getState().recordFlowRun(tabId, built);
    expect(stored().runHistory).toHaveLength(1);
    expect(JSON.stringify(stored().runHistory)).not.toContain('SECRET');
  });
});
