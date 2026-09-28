import { describe, expect, it } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { edgeRunState, exitLabel } from '../flowExits';

const ifKind = { kind: 'If' as const, label: 'Ok?', condition: 'response.status === 200' };
const switchKind = {
  kind: 'Switch' as const,
  label: 'Plan',
  value: 'response.body.plan',
  cases: [{ id: 'c1', label: 'Pro plan', matches: 'pro' }],
};

describe('exitLabel', () => {
  it('labels If exits', () => {
    expect(exitLabel(ifKind, 'true')).toBe('true');
    expect(exitLabel(ifKind, 'false')).toBe('false');
    expect(exitLabel(ifKind, 'result')).toBeUndefined();
  });

  it('labels Switch exits by the current case label', () => {
    expect(exitLabel(switchKind, 'case:c1')).toBe('Pro plan');
    expect(exitLabel(switchKind, 'default')).toBe('default');
    expect(exitLabel(switchKind, 'case:gone')).toBeUndefined();
  });

  it('has no label for plain node exits', () => {
    expect(exitLabel({ kind: 'Output', label: 'Out' }, 'result')).toBeUndefined();
  });
});

describe('edgeRunState', () => {
  const ifNode: FlowNode = { id: 'if1', position: { x: 0, y: 0 }, kind: ifKind };
  const plain: FlowNode = {
    id: 'r1',
    position: { x: 0, y: 0 },
    kind: { kind: 'Output', label: 'O' },
  };
  const trueEdge: FlowEdge = {
    id: 'e1',
    sourceNodeId: 'if1',
    sourceHandle: 'true',
    targetNodeId: 'x',
    targetField: 'trigger',
    expression: '',
  };
  const falseEdge: FlowEdge = { ...trueEdge, id: 'e2', sourceHandle: 'false' };

  it('marks the chosen exit taken and the other not-taken', () => {
    expect(edgeRunState(trueEdge, ifNode, 'success', 'true')).toBe('taken');
    expect(edgeRunState(falseEdge, ifNode, 'success', 'true')).toBe('not-taken');
  });

  it('is neutral while the routing node is running or has no branch', () => {
    expect(edgeRunState(trueEdge, ifNode, 'running', 'true')).toBe('neutral');
    expect(edgeRunState(trueEdge, ifNode, 'success', undefined)).toBe('neutral');
    expect(edgeRunState(trueEdge, ifNode, undefined, undefined)).toBe('neutral');
  });

  it('is neutral when the routing node failed or was skipped', () => {
    expect(edgeRunState(trueEdge, ifNode, 'failed', undefined)).toBe('neutral');
    expect(edgeRunState(trueEdge, ifNode, 'skipped', undefined)).toBe('neutral');
  });

  it('is neutral for plain nodes and a missing source', () => {
    const plainEdge: FlowEdge = { ...trueEdge, sourceNodeId: 'r1', sourceHandle: undefined };
    expect(edgeRunState(plainEdge, plain, 'success', undefined)).toBe('neutral');
    expect(edgeRunState(trueEdge, undefined, 'success', 'true')).toBe('neutral');
  });
});
