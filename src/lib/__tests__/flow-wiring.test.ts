import { describe, expect, it } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import {
  buildEdgeFromConnection,
  defaultExpressionFor,
  parseCycleErrorMessage,
} from '../flow-wiring';

const requestSource: FlowNode = {
  id: 'node-a',
  kind: {
    kind: 'Request',
    label: 'Login',
    source: { type: 'Saved', requestPath: 'auth/login.yml' },
  },
  position: { x: 0, y: 0 },
};

const inputSource: FlowNode = {
  id: 'node-b',
  kind: { kind: 'Input', label: 'Username', value: 'alice' },
  position: { x: 0, y: 0 },
};

describe('defaultExpressionFor', () => {
  it('defaults to "response.body" for a Request source node', () => {
    expect(defaultExpressionFor(requestSource)).toBe('response.body');
  });

  it('defaults to "response.body" for an Input source node too', () => {
    // The backend exposes an Input node's value as response.body. A bare
    // `value` is not bound and would fail the run.
    expect(defaultExpressionFor(inputSource)).toBe('response.body');
  });
});

describe('buildEdgeFromConnection', () => {
  it('maps a React Flow connection into a FlowEdge with a generated id and default expression', () => {
    const edge = buildEdgeFromConnection(
      { source: 'node-a', sourceHandle: 'result', target: 'node-c', targetHandle: 'url' },
      requestSource,
    );
    expect(edge).toMatchObject({
      sourceNodeId: 'node-a',
      targetNodeId: 'node-c',
      targetField: 'url',
      expression: 'response.body',
    });
    expect(edge?.id).toBeTruthy();
  });

  it('does not block a connection that closes a cycle (Save rejects cycles, not the canvas)', () => {
    // b -> a, when a -> b already exists. Cycle rejection is save_flow's job.
    const edge = buildEdgeFromConnection(
      { source: 'b', target: 'a', sourceHandle: 'result', targetHandle: 'url' },
      {
        id: 'b',
        kind: { kind: 'Input', label: 'b', value: 'x' },
        position: { x: 0, y: 0 },
      },
    );
    expect(edge).toMatchObject({ sourceNodeId: 'b', targetNodeId: 'a', targetField: 'url' });
  });

  it('returns null when the connection is missing a target handle', () => {
    const edge = buildEdgeFromConnection(
      { source: 'node-a', sourceHandle: 'result', target: 'node-c', targetHandle: null },
      requestSource,
    );
    expect(edge).toBeNull();
  });
});

describe('parseCycleErrorMessage', () => {
  it('extracts node ids and edge ids from the backend cycle-rejection message', () => {
    const message = 'Invalid input: flow contains a cycle through node(s): a, b; edge(s): e1, e2';
    expect(parseCycleErrorMessage(message)).toEqual({
      nodeIds: ['a', 'b'],
      edgeIds: ['e1', 'e2'],
    });
  });

  it('returns an empty edge list when no edge segment is present', () => {
    const message = 'Invalid input: flow contains a cycle through node(s): a, b';
    expect(parseCycleErrorMessage(message)).toEqual({ nodeIds: ['a', 'b'], edgeIds: [] });
  });

  it('returns null for an unrelated error message', () => {
    expect(parseCycleErrorMessage('Invalid input: flow name is empty')).toBeNull();
  });
});
