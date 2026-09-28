import { describe, expect, it } from 'vitest';
import { removeSwitchCase, replaceNodeKind } from '@/lib/flow-graph-edits';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';

const switchNode: FlowNode = {
  id: 'sw1',
  position: { x: 0, y: 0 },
  kind: {
    kind: 'Switch',
    label: 'Plan',
    value: 'response.body.plan',
    cases: [
      { id: 'c1', label: 'Free', matches: 'free' },
      { id: 'c2', label: 'Pro', matches: 'pro' },
    ],
  },
};
const other: FlowNode = {
  id: 'o1',
  kind: { kind: 'Output', label: 'Out' },
  position: { x: 1, y: 1 },
};

const edges: FlowEdge[] = [
  {
    id: 'toFree',
    sourceNodeId: 'sw1',
    sourceHandle: 'case:c1',
    targetNodeId: 'o1',
    targetField: 'trigger',
    expression: '',
  },
  {
    id: 'toPro',
    sourceNodeId: 'sw1',
    sourceHandle: 'case:c2',
    targetNodeId: 'o1',
    targetField: 'trigger',
    expression: '',
  },
  {
    id: 'unrelated',
    sourceNodeId: 'o1',
    targetNodeId: 'sw1',
    targetField: 'input',
    expression: '',
  },
];

describe('replaceNodeKind', () => {
  it('swaps only the target node kind and keeps position and id', () => {
    const next = replaceNodeKind([switchNode, other], 'o1', { kind: 'Output', label: 'Renamed' });
    expect(next[0]).toBe(switchNode);
    expect(next[1]).toEqual({ ...other, kind: { kind: 'Output', label: 'Renamed' } });
  });
});

describe('removeSwitchCase', () => {
  it('drops the case and exactly the edges leaving its handle', () => {
    const result = removeSwitchCase([switchNode, other], edges, 'sw1', 'c1');
    expect(result).not.toBeNull();
    const sw = result?.nodes.find((n) => n.id === 'sw1');
    expect(sw?.kind.kind === 'Switch' && sw.kind.cases.map((c) => c.id)).toEqual(['c2']);
    expect(result?.edges.map((e) => e.id)).toEqual(['toPro', 'unrelated']);
  });

  it('returns null when the node is missing or is not a Switch', () => {
    expect(removeSwitchCase([other], edges, 'sw1', 'c1')).toBeNull();
    expect(removeSwitchCase([other], edges, 'o1', 'c1')).toBeNull();
  });
});
