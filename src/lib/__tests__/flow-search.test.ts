import { describe, expect, it } from 'vitest';
import type { FlowNode } from '@/lib/tauri-api';
import { searchFlowNodes } from '../flow-search';

const at = (id: string, kind: FlowNode['kind']): FlowNode => ({
  id,
  kind,
  position: { x: 0, y: 0 },
});

const nodes: FlowNode[] = [
  at('n1', {
    kind: 'Request',
    label: 'Fetch Users',
    source: { type: 'Saved', requestPath: 'users/list.yml' },
  }),
  at('n2', {
    kind: 'Request',
    label: 'Ping',
    source: {
      type: 'Inline',
      request: { method: 'GET', url: 'https://api.example.test/health', headers: [] },
    },
  }),
  at('n3', {
    kind: 'Switch',
    label: 'Route',
    value: '{{status}}',
    cases: [{ id: 'c1', label: 'Ok', matches: '200' }],
  }),
  at('n4', { kind: 'Output', label: 'Result' }),
  at('n5', { kind: 'Input', label: 'Token', value: 'secret-value' }),
  at('n6', { kind: 'WaitForCallback', label: 'Wait', name: 'pay', timeoutMs: 1000 }),
];

describe('searchFlowNodes', () => {
  it('returns nothing for an empty or blank query', () => {
    expect(searchFlowNodes(nodes, '')).toEqual([]);
    expect(searchFlowNodes(nodes, '   ')).toEqual([]);
  });

  it('matches the label without regard to case', () => {
    expect(searchFlowNodes(nodes, 'fetch')).toEqual(['n1']);
    expect(searchFlowNodes(nodes, 'FETCH USERS')).toEqual(['n1']);
  });

  it('matches the kind name', () => {
    expect(searchFlowNodes(nodes, 'switch')).toEqual(['n3']);
    expect(searchFlowNodes(nodes, 'waitforcallback')).toEqual(['n6']);
  });

  it('matches a saved request path', () => {
    expect(searchFlowNodes(nodes, 'users/list')).toEqual(['n1']);
  });

  it('matches an inline request url', () => {
    expect(searchFlowNodes(nodes, 'example.test/health')).toEqual(['n2']);
  });

  it('matches a Switch value', () => {
    expect(searchFlowNodes(nodes, '{{status}}')).toEqual(['n3']);
  });

  it('lists a node once, in node order, even when several fields match', () => {
    // "request" is the kind of n1 and n2. "users" is in the label and the path of n1.
    expect(searchFlowNodes(nodes, 'request')).toEqual(['n1', 'n2']);
    expect(searchFlowNodes(nodes, 'users')).toEqual(['n1']);
  });

  it('does not search an Input value', () => {
    expect(searchFlowNodes(nodes, 'secret-value')).toEqual([]);
  });

  it('returns nothing when no node matches', () => {
    expect(searchFlowNodes(nodes, 'zzz')).toEqual([]);
  });
});
