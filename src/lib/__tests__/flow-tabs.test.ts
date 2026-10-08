import { describe, expect, it } from 'vitest';
import type { FlowTab, LeafNode } from '@/types/pane-types';
import { findFlowTabs, hasDirtyFlow, isFlowRunning } from '../flow-tabs';

const flowTab = (id: string, flowName: string, patch: Partial<FlowTab> = {}): FlowTab => ({
  id,
  title: `Flow: ${flowName}`,
  isDirty: false,
  tabType: 'flow',
  collectionName: 'col',
  flowName,
  nodes: [],
  edges: [],
  nodeStatus: {},
  runState: 'idle',
  ...patch,
});

const leaf = (tabs: FlowTab[]): LeafNode =>
  ({ type: 'leaf', groupId: 'g1', tabs, activeTabId: tabs[0]?.id ?? null }) as LeafNode;

describe('flow-tabs', () => {
  it('finds tabs of one flow in the tree and in snapshots, once each', () => {
    const shared = flowTab('t1', 'Login');
    const root = leaf([shared, flowTab('t2', 'Other')]);
    const snapshots = {
      other: { tabs: [shared, flowTab('t3', 'Login', { collectionName: 'col' })] },
    };
    expect(findFlowTabs(root, snapshots, 'col', 'Login').map((t) => t.id)).toEqual(['t1', 't3']);
  });

  it('matches the collection and the exact flow name', () => {
    const root = leaf([
      flowTab('t1', 'Login'),
      flowTab('t2', 'login'),
      flowTab('t3', 'Login', { collectionName: 'x' }),
    ]);
    expect(findFlowTabs(root, {}, 'col', 'Login').map((t) => t.id)).toEqual(['t1']);
  });

  it('reports a running tab, also one parked in a snapshot', () => {
    const root = leaf([flowTab('t1', 'Login')]);
    expect(isFlowRunning(root, {}, 'col', 'Login')).toBe(false);
    const parked = { c: { tabs: [flowTab('t2', 'Login', { runState: 'running' })] } };
    expect(isFlowRunning(root, parked, 'col', 'Login')).toBe(true);
  });

  it('reports a dirty tab', () => {
    const root = leaf([flowTab('t1', 'Login', { isDirty: true })]);
    expect(hasDirtyFlow(root, {}, 'col', 'Login')).toBe(true);
    expect(hasDirtyFlow(root, {}, 'col', 'Other')).toBe(false);
  });
});
