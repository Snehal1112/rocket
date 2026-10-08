import { describe, expect, it } from 'vitest';
import type { FlowEdge, FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import {
  flowEdgeAriaLabel,
  flowNodeAriaLabel,
  flowNodeName,
  shortError,
} from '../flowA11y';

const request: FlowNodeKind = {
  kind: 'Request',
  label: 'Fetch',
  source: { type: 'Saved', requestPath: 'a.yml' },
};

describe('flowNodeName', () => {
  it('uses the label, and the kind name when the label is blank', () => {
    expect(flowNodeName(request)).toBe('Fetch');
    expect(flowNodeName({ ...request, label: '  ' })).toBe('request');
    expect(flowNodeName({ kind: 'WaitForCallback', label: '', name: 'cb', timeoutMs: 1000 })).toBe(
      'wait for callback',
    );
  });
});

describe('shortError', () => {
  it('keeps the first line only', () => {
    expect(shortError('first line\nsecond line')).toBe('first line');
  });

  it('cuts a long error at 80 characters with an ellipsis', () => {
    const cut = shortError('x'.repeat(200));
    expect(cut).toHaveLength(80);
    expect(cut.endsWith('…')).toBe(true);
  });

  it('leaves a short error alone', () => {
    expect(shortError('boom')).toBe('boom');
  });
});

describe('flowNodeAriaLabel', () => {
  it('names label, kind and status', () => {
    expect(flowNodeAriaLabel(request, 'idle')).toBe('Fetch, request node, not run');
    expect(flowNodeAriaLabel(request, 'success')).toBe('Fetch, request node, succeeded');
  });

  it('adds the short error of a failed node', () => {
    expect(flowNodeAriaLabel(request, 'failed', { error: 'boom\nstack' })).toBe(
      'Fetch, request node, failed: boom',
    );
    expect(flowNodeAriaLabel(request, 'failed')).toBe('Fetch, request node, failed');
  });

  it('says why a node was skipped', () => {
    expect(flowNodeAriaLabel(request, 'skipped', { skipReason: 'branch_not_taken' })).toBe(
      'Fetch, request node, skipped, branch not taken',
    );
  });

  it('never includes the progress text, the value or an error of a node that did not fail', () => {
    const label = flowNodeAriaLabel(request, 'running', {
      progress: 'attempt 3/30',
      value: 'secret-value',
      error: 'old error',
    });
    expect(label).toBe('Fetch, request node, running');
  });
});

describe('flowEdgeAriaLabel', () => {
  const nodes: FlowNode[] = [
    { id: 'in', position: { x: 0, y: 0 }, kind: { kind: 'Input', label: 'User', value: 'a' } },
    { id: 'if', position: { x: 0, y: 0 }, kind: { kind: 'If', label: 'Check', condition: 'x' } },
    { id: 'out', position: { x: 0, y: 0 }, kind: { kind: 'Output', label: 'Result' } },
    {
      id: 'sw',
      position: { x: 0, y: 0 },
      kind: {
        kind: 'Switch',
        label: 'Route',
        value: 'x',
        cases: [{ id: 'c1', label: 'Pro plan', matches: 'pro' }],
      },
    },
  ];
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const edge = (over: Partial<FlowEdge>): FlowEdge => ({
    id: 'e1',
    sourceNodeId: 'in',
    targetNodeId: 'out',
    targetField: 'value',
    expression: '',
    ...over,
  });

  it('names both ends and the target field', () => {
    expect(flowEdgeAriaLabel(edge({}), byId, 'neutral')).toBe('Wire from User to Result, value');
  });

  it('names a routing exit and whether the run took it', () => {
    const fromIf = edge({ sourceNodeId: 'if', sourceHandle: 'true' });
    expect(flowEdgeAriaLabel(fromIf, byId, 'taken')).toBe(
      'Wire from Check (true exit) to Result, value, taken',
    );
    expect(flowEdgeAriaLabel(fromIf, byId, 'not-taken')).toBe(
      'Wire from Check (true exit) to Result, value, not taken',
    );
    const fromSwitch = edge({ sourceNodeId: 'sw', sourceHandle: 'case:c1' });
    expect(flowEdgeAriaLabel(fromSwitch, byId, 'neutral')).toBe(
      'Wire from Route (Pro plan exit) to Result, value',
    );
  });

  it('calls a trigger wire "run when"', () => {
    expect(flowEdgeAriaLabel(edge({ targetField: 'trigger' }), byId, 'neutral')).toBe(
      'Wire from User to Result, run when',
    );
  });

  it('falls back to the ids when an end is missing', () => {
    expect(flowEdgeAriaLabel(edge({ sourceNodeId: 'gone' }), byId, 'neutral')).toBe(
      'Wire from gone to Result, value',
    );
  });
});
