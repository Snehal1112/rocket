import { describe, expect, it } from 'vitest';
import { rewriteTabPaths } from '@/lib/flow-request-paths';
import { type GraphSnap, isAtSaved, snapOf } from '@/lib/flow-history';
import type { FlowNode } from '@/lib/tauri-api';
import type { FlowTab } from '@/types/pane-types';

const requestNode = (id: string, path: string): FlowNode => ({
  id,
  kind: { kind: 'Request', label: id, source: { type: 'Saved', requestPath: path } },
  position: { x: 0, y: 0 },
});

const flowTab = (nodes: FlowNode[], patch: Partial<FlowTab> = {}): FlowTab => ({
  id: 't1',
  title: 'Flow: f',
  isDirty: false,
  tabType: 'flow',
  collectionName: 'c',
  flowName: 'f',
  nodes,
  edges: [],
  callbackHost: null,
  nodeStatus: {},
  runState: 'idle',
  ...patch,
});

const pathOf = (n: FlowNode): string | undefined =>
  n.kind.kind === 'Request' && n.kind.source.type === 'Saved'
    ? n.kind.source.requestPath
    : undefined;

describe('rewriteTabPaths', () => {
  it('replaces a folder prefix in nodes and every history snapshot', () => {
    const nodes = [requestNode('a', 'auth/login.yml'), requestNode('b', 'auth/sub/me.yml')];
    const snap: GraphSnap = { nodes, edges: [] };
    const tab = flowTab(nodes, { history: { past: [snap], future: [snap], saved: snap } });
    const out = rewriteTabPaths(tab, 'auth', 'identity');
    const h = out.history;
    for (const ns of [out.nodes, h?.past[0].nodes, h?.future[0].nodes, h?.saved?.nodes]) {
      expect(ns?.map(pathOf)).toEqual(['identity/login.yml', 'identity/sub/me.yml']);
    }
  });

  it('keeps nodes and saved nodes as one array', () => {
    const tab = flowTab([requestNode('a', 'auth/login.yml')]);
    tab.history = { past: [], future: [], saved: snapOf(tab) };
    const out = rewriteTabPaths(tab, 'auth', 'identity');
    expect(out.nodes).toBe(out.history?.saved?.nodes);
    expect(isAtSaved(snapOf(out), out.history?.saved)).toBe(true);
  });

  it('returns the same tab when nothing matches', () => {
    const other = requestNode('o', 'other/x.yml');
    const tab = flowTab([other]);
    expect(rewriteTabPaths(tab, 'auth', 'identity')).toBe(tab);
    const keep = requestNode('k', 'misc/y.yml');
    const mixed = flowTab([keep, requestNode('a', 'auth/login.yml')]);
    const out = rewriteTabPaths(mixed, 'auth', 'identity');
    expect(out.nodes[0]).toBe(keep);
  });

  it('never matches an empty from', () => {
    const tab = flowTab([requestNode('a', 'auth/login.yml')]);
    expect(rewriteTabPaths(tab, '', 'x')).toBe(tab);
    expect(rewriteTabPaths(tab, '/', 'x')).toBe(tab);
  });

  it('does not touch author for auth', () => {
    const tab = flowTab([requestNode('a', 'author/x.yml')]);
    expect(rewriteTabPaths(tab, 'auth', 'identity')).toBe(tab);
  });

  it('survives a tab without history', () => {
    const tab = flowTab([requestNode('a', 'auth/login.yml')]);
    const out = rewriteTabPaths(tab, 'auth', 'identity');
    expect(out.nodes.map(pathOf)).toEqual(['identity/login.yml']);
  });
});
