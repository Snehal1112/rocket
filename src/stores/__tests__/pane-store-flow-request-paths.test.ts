import { beforeEach, describe, expect, it, vi } from 'vitest';
import { isAtSaved, snapOf } from '@/lib/flow-history';
import { collectAllTabs } from '@/lib/pane-utils';
import type { FlowNode } from '@/lib/tauri-api';
import { type FlowTab, isFlowTab } from '@/types/pane-types';
import { usePaneStore } from '../pane-store';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn() }));
vi.mock('@/lib/tauri-api', async () => {
  const actual = await vi.importActual<typeof import('@/lib/tauri-api')>('@/lib/tauri-api');
  return { ...actual, getFlow: vi.fn(), endAgentSession: vi.fn() };
});

const requestNode = (id: string, requestPath: string): FlowNode =>
  ({
    id,
    kind: { kind: 'Request', source: { type: 'Saved', requestPath } },
    position: { x: 0, y: 0 },
  }) as unknown as FlowNode;

const pathOf = (node: FlowNode | undefined): string | null => {
  const kind = node?.kind;
  return kind && kind.kind === 'Request' && kind.source.type === 'Saved'
    ? kind.source.requestPath
    : null;
};

const flowTab = (id: string, patch: Partial<FlowTab> = {}): FlowTab => ({
  id,
  title: 'Flow: F',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'c',
  flowName: 'F',
  nodes: [],
  edges: [],
  callbackHost: null,
  nodeStatus: {},
  runState: 'idle',
  ...patch,
});

const liveTab = (id: string) =>
  collectAllTabs(usePaneStore.getState().root)
    .filter(isFlowTab)
    .find((t) => t.id === id);

describe('rewriteFlowRequestPaths', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
  });

  it('rewriteFlowRequestPaths patches nodes and history snapshots', () => {
    const nodes = [requestNode('a', 'auth/login.yml')];
    const snap = { nodes, edges: [], callbackHost: null };
    usePaneStore
      .getState()
      .openTab(flowTab('t1', { nodes, history: { past: [snap], future: [], saved: snap } }));

    usePaneStore.getState().rewriteFlowRequestPaths('c', 'auth', 'identity');

    const tab = liveTab('t1');
    expect(pathOf(tab?.nodes[0])).toBe('identity/login.yml');
    expect(pathOf(tab?.history?.past[0]?.nodes[0])).toBe('identity/login.yml');
    expect(pathOf(tab?.history?.saved?.nodes[0])).toBe('identity/login.yml');
  });

  it('rewriteFlowRequestPaths keeps a clean tab clean and a dirty tab dirty', () => {
    const nodes = [requestNode('a', 'auth/login.yml')];
    const edges: FlowTab['edges'] = [];
    const snap = { nodes, edges, callbackHost: null };
    const history = { past: [], future: [], saved: snap };
    usePaneStore.getState().openTab(flowTab('clean', { nodes, edges, history }));
    usePaneStore.getState().openTab(flowTab('dirty', { nodes, edges, history, isDirty: true }));

    usePaneStore.getState().rewriteFlowRequestPaths('c', 'auth', 'identity');

    const clean = liveTab('clean');
    expect(clean?.isDirty).toBe(false);
    expect(clean?.history?.saved).toBeDefined();
    if (clean?.history) {
      expect(isAtSaved(snapOf(clean), clean.history.saved)).toBe(true);
    }
    expect(liveTab('dirty')?.isDirty).toBe(true);
  });

  it('rewriteFlowRequestPaths patches parked collection tabs', () => {
    const tab = flowTab('p1', { nodes: [requestNode('a', 'auth/login.yml')] });
    usePaneStore.setState({ collectionTabState: { c: { tabs: [tab], activeTabId: tab.id } } });

    usePaneStore.getState().rewriteFlowRequestPaths('c', 'auth', 'identity');

    const parked = usePaneStore.getState().collectionTabState.c?.tabs[0];
    expect(parked && isFlowTab(parked) ? pathOf(parked.nodes[0]) : null).toBe('identity/login.yml');
  });

  it('rewriteFlowRequestPaths leaves other collections and unrelated paths alone', () => {
    const other = flowTab('o1', {
      collectionName: 'other',
      nodes: [requestNode('a', 'auth/login.yml')],
    });
    const unrelated = requestNode('b', 'author/x.yml');
    usePaneStore.getState().openTab(other);
    usePaneStore
      .getState()
      .openTab(flowTab('t1', { nodes: [requestNode('a', 'auth/login.yml'), unrelated] }));

    usePaneStore.getState().rewriteFlowRequestPaths('c', 'auth', 'identity');

    expect(liveTab('o1')).toBe(other);
    expect(liveTab('t1')?.nodes[1]).toBe(unrelated);
    expect(pathOf(liveTab('t1')?.nodes[0])).toBe('identity/login.yml');
  });

  it('rewriteFlowRequestPaths ignores tabs without history or collection', () => {
    const noHistory = flowTab('t1', { nodes: [requestNode('a', 'auth/login.yml')] });
    const noCollection = flowTab('t2', {
      collectionName: null,
      nodes: [requestNode('a', 'auth/login.yml')],
    });
    usePaneStore.getState().openTab(noHistory);
    usePaneStore.getState().openTab(noCollection);

    expect(() => usePaneStore.getState().rewriteFlowRequestPaths('c', 'auth', 'identity')).not.toThrow();

    expect(pathOf(liveTab('t1')?.nodes[0])).toBe('identity/login.yml');
    expect(liveTab('t2')).toBe(noCollection);
  });

  it('rewriteFlowRequestPaths patches a tab that is both live and parked once', () => {
    const tab = flowTab('t1', { nodes: [requestNode('a', 'auth/login.yml')] });
    usePaneStore.getState().openTab(tab);
    usePaneStore.setState({ collectionTabState: { c: { tabs: [tab], activeTabId: tab.id } } });

    usePaneStore.getState().rewriteFlowRequestPaths('c', 'auth', 'identity');

    expect(pathOf(liveTab('t1')?.nodes[0])).toBe('identity/login.yml');
    const parked = usePaneStore.getState().collectionTabState.c?.tabs[0];
    expect(parked && isFlowTab(parked) ? pathOf(parked.nodes[0]) : null).toBe('identity/login.yml');
  });
});
