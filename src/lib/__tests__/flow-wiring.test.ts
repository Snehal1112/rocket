import { describe, expect, it } from 'vitest';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import {
  buildEdgeFromConnection,
  defaultExpressionFor,
  isDataLessTarget,
  isValidFlowConnection,
  parseGraphErrorMessage,
  shouldPromptForExpression,
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

const node = (id: string, kind: FlowNode['kind']): FlowNode => ({
  id,
  kind,
  position: { x: 0, y: 0 },
});

const req = node('req', {
  kind: 'Request',
  label: 'Login',
  source: { type: 'Saved', requestPath: 'auth/login.yml' },
});
const inp = node('inp', { kind: 'Input', label: 'User', value: 'alice' });
const out = node('out', { kind: 'Output', label: 'Out' });
const iff = node('iff', { kind: 'If', label: 'Ok?', condition: 'response.status === 200' });
const sw = node('sw', {
  kind: 'Switch',
  label: 'Plan',
  value: 'response.body.plan',
  cases: [{ id: 'c1', label: 'Pro', matches: 'pro' }],
});
const nodes = [req, inp, out, iff, sw];

describe('buildEdgeFromConnection with exits', () => {
  it('omits sourceHandle for the default result exit', () => {
    const edge = buildEdgeFromConnection(
      { source: 'req', sourceHandle: 'result', target: 'out', targetHandle: 'value' },
      req,
    );
    expect(edge).not.toBeNull();
    expect(edge && 'sourceHandle' in edge).toBe(false);
  });

  it('omits sourceHandle when React Flow reports a null source handle', () => {
    const edge = buildEdgeFromConnection(
      { source: 'req', sourceHandle: null, target: 'out', targetHandle: 'value' },
      req,
    );
    expect(edge && 'sourceHandle' in edge).toBe(false);
  });

  it('keeps a routing exit as sourceHandle', () => {
    const edge = buildEdgeFromConnection(
      { source: 'iff', sourceHandle: 'true', target: 'req', targetHandle: 'url' },
      iff,
    );
    expect(edge).toMatchObject({ sourceHandle: 'true', targetField: 'url' });
    expect(edge?.expression).toBe('response.body');
  });

  it('gives input and trigger wires an empty expression', () => {
    const intoIf = buildEdgeFromConnection(
      { source: 'req', sourceHandle: 'result', target: 'iff', targetHandle: 'input' },
      req,
    );
    const trigger = buildEdgeFromConnection(
      { source: 'iff', sourceHandle: 'false', target: 'req', targetHandle: 'trigger' },
      iff,
    );
    expect(intoIf?.expression).toBe('');
    expect(trigger).toMatchObject({
      expression: '',
      sourceHandle: 'false',
      targetField: 'trigger',
    });
  });
});

describe('isDataLessTarget / shouldPromptForExpression', () => {
  it('treats only input and trigger as data-less', () => {
    expect(['input', 'trigger', 'url', 'headers', 'body', 'value'].map(isDataLessTarget)).toEqual([
      true,
      true,
      false,
      false,
      false,
      false,
    ]);
  });

  it('prompts for an expression only on data wires', () => {
    const base: FlowEdge = {
      id: 'e',
      sourceNodeId: 'a',
      targetNodeId: 'b',
      targetField: 'url',
      expression: 'response.body',
    };
    expect(shouldPromptForExpression(base)).toBe(true);
    expect(shouldPromptForExpression({ ...base, targetField: 'trigger', expression: '' })).toBe(
      false,
    );
    expect(shouldPromptForExpression({ ...base, targetField: 'input', expression: '' })).toBe(
      false,
    );
  });
});

describe('isValidFlowConnection', () => {
  const conn = (
    source: string,
    sourceHandle: string | null,
    target: string,
    targetHandle: string,
  ) => ({ source, sourceHandle, target, targetHandle });

  it('accepts plain data wires, including a null source handle from a Request', () => {
    expect(isValidFlowConnection(conn('req', null, 'out', 'value'), nodes, [])).toBe(true);
    expect(isValidFlowConnection(conn('inp', 'result', 'req', 'url'), nodes, [])).toBe(true);
  });

  it('accepts routing exits into data fields and Run when inputs', () => {
    expect(isValidFlowConnection(conn('iff', 'true', 'req', 'headers'), nodes, [])).toBe(true);
    expect(isValidFlowConnection(conn('iff', 'false', 'out', 'trigger'), nodes, [])).toBe(true);
    expect(isValidFlowConnection(conn('sw', 'case:c1', 'req', 'trigger'), nodes, [])).toBe(true);
    expect(isValidFlowConnection(conn('sw', 'default', 'out', 'value'), nodes, [])).toBe(true);
  });

  it('rejects a source handle that does not exist on the source node', () => {
    expect(isValidFlowConnection(conn('iff', null, 'req', 'url'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('iff', 'result', 'req', 'url'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('req', 'true', 'out', 'value'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('sw', 'case:deleted', 'req', 'url'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('sw', 'case:', 'req', 'url'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('out', 'result', 'req', 'url'), nodes, [])).toBe(false);
  });

  it('rejects wires into an Input node and misplaced input/trigger handles', () => {
    expect(isValidFlowConnection(conn('req', 'result', 'inp', 'value'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('req', 'result', 'req', 'input'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('req', 'result', 'iff', 'trigger'), nodes, [])).toBe(false);
    expect(isValidFlowConnection(conn('req', 'result', 'sw', 'url'), nodes, [])).toBe(false);
  });

  it('allows only one wire into an If or Switch input', () => {
    const existing: FlowEdge[] = [
      { id: 'e1', sourceNodeId: 'inp', targetNodeId: 'iff', targetField: 'input', expression: '' },
    ];
    expect(isValidFlowConnection(conn('req', 'result', 'iff', 'input'), nodes, [])).toBe(true);
    expect(isValidFlowConnection(conn('req', 'result', 'iff', 'input'), nodes, existing)).toBe(
      false,
    );
  });

  it('rejects a connection with a missing endpoint or unknown node', () => {
    expect(
      isValidFlowConnection(
        { source: null, sourceHandle: null, target: 'out', targetHandle: 'value' },
        nodes,
        [],
      ),
    ).toBe(false);
    expect(isValidFlowConnection(conn('ghost', 'result', 'out', 'value'), nodes, [])).toBe(false);
  });
});

describe('parseGraphErrorMessage', () => {
  it('extracts node and edge ids from a cycle rejection', () => {
    const message = 'Invalid input: flow contains a cycle through node(s): a, b; edge(s): e1, e2';
    expect(parseGraphErrorMessage(message)).toEqual({ nodeIds: ['a', 'b'], edgeIds: ['e1', 'e2'] });
  });

  it('still parses the older cycle message without an edge segment', () => {
    const message = 'Invalid input: flow contains a cycle through node(s): a, b';
    expect(parseGraphErrorMessage(message)).toEqual({ nodeIds: ['a', 'b'], edgeIds: [] });
  });

  it('parses a node validation error whose edge list is empty', () => {
    const message =
      "Invalid input: If node 'b' must have exactly one incoming edge — node(s): b; edge(s): ";
    expect(parseGraphErrorMessage(message)).toEqual({ nodeIds: ['b'], edgeIds: [] });
  });

  it('parses an edge validation error whose node list is empty', () => {
    const message =
      "Invalid input: edge 'e3' leaves from unknown exit 'case:gone' — node(s): ; edge(s): e3";
    expect(parseGraphErrorMessage(message)).toEqual({ nodeIds: [], edgeIds: ['e3'] });
  });

  it('returns null for an unrelated error message', () => {
    expect(parseGraphErrorMessage('Invalid input: flow name is empty')).toBeNull();
  });
});
