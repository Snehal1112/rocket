import { beforeEach, describe, expect, it } from 'vitest';
import { DEFAULT_AUTH_NODE_AUTH } from '@/lib/flow-auth';
import { caseHandle } from '@/lib/flow-handles';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import {
  canPasteInto,
  clearFlowClipboard,
  copySelection,
  getFlowClipboard,
  instantiatePaste,
  nextPasteStep,
  PASTE_OFFSET,
  setFlowClipboard,
  uniqueCallbackName,
} from '../flow-clipboard';

const at = (id: string, kind: FlowNode['kind'], x = 0, y = 0): FlowNode => ({
  id,
  kind,
  position: { x, y },
});

const wire = (id: string, from: string, to: string, over: Partial<FlowEdge> = {}): FlowEdge => ({
  id,
  sourceNodeId: from,
  targetNodeId: to,
  targetField: 'value',
  expression: 'response.body',
  ...over,
});

const input = (id: string, x = 0, y = 0) =>
  at(id, { kind: 'Input', label: id, value: 'v' }, x, y);
const output = (id: string, x = 0, y = 0) => at(id, { kind: 'Output', label: id }, x, y);

describe('copySelection', () => {
  it('returns null for an empty selection', () => {
    expect(copySelection([input('a')], [], new Set(), 'demo')).toBeNull();
    expect(copySelection([input('a')], [], new Set(['missing']), 'demo')).toBeNull();
  });

  it('keeps only wires whose two ends are both selected', () => {
    const nodes = [input('a'), output('b'), output('c')];
    const edges = [wire('e1', 'a', 'b'), wire('e2', 'a', 'c')];
    const clip = copySelection(nodes, edges, new Set(['a', 'b']), 'demo');
    expect(clip?.nodes.map((n) => n.id)).toEqual(['a', 'b']);
    expect(clip?.edges.map((e) => e.id)).toEqual(['e1']);
    expect(clip?.collection).toBe('demo');
  });

  it('drops the auth wire when the Auth node is not copied', () => {
    const auth = at('auth', {
      kind: 'Auth',
      label: 'Auth',
      auth: DEFAULT_AUTH_NODE_AUTH,
      applyToInherit: false,
    });
    const req = at('req', {
      kind: 'Request',
      label: 'R',
      source: { type: 'Saved', requestPath: 'a.yml' },
    });
    const edges = [wire('e1', 'auth', 'req', { targetField: 'auth' })];
    const clip = copySelection([auth, req], edges, new Set(['req']), 'demo');
    expect(clip?.edges).toEqual([]);
  });

  it('makes a deep copy, so later edits to the graph do not reach the clip', () => {
    const nodes = [input('a')];
    const clip = copySelection(nodes, [], new Set(['a']), 'demo');
    nodes[0].position.x = 999;
    expect(clip?.nodes[0].position.x).toBe(0);
  });
});

describe('instantiatePaste', () => {
  it('gives every node and wire a fresh id and remaps the wire ends', () => {
    const clip = copySelection(
      [input('a', 10, 20), output('b', 110, 20)],
      [wire('e1', 'a', 'b', { expression: 'response.body.id' })],
      new Set(['a', 'b']),
      'demo',
    );
    if (!clip) throw new Error('Expected a clip');
    const result = instantiatePaste(clip, [input('a'), output('b')]);
    expect(result.nodes).toHaveLength(2);
    const ids = result.nodes.map((n) => n.id);
    expect(new Set(ids).size).toBe(2);
    expect(ids).not.toContain('a');
    expect(ids).not.toContain('b');
    expect(result.edges).toHaveLength(1);
    const [edge] = result.edges;
    expect(edge.id).not.toBe('e1');
    expect(edge.sourceNodeId).toBe(ids[0]);
    expect(edge.targetNodeId).toBe(ids[1]);
    expect(edge.targetField).toBe('value');
    expect(edge.expression).toBe('response.body.id');
  });

  it('offsets positions by PASTE_OFFSET times the step', () => {
    const clip = copySelection([input('a', 10, 20)], [], new Set(['a']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    expect(instantiatePaste(clip, []).nodes[0].position).toEqual({
      x: 10 + PASTE_OFFSET,
      y: 20 + PASTE_OFFSET,
    });
    expect(instantiatePaste(clip, [], 3).nodes[0].position).toEqual({
      x: 10 + 3 * PASTE_OFFSET,
      y: 20 + 3 * PASTE_OFFSET,
    });
  });

  it('does not touch the clip or the existing nodes', () => {
    const clip = copySelection([input('a')], [], new Set(['a']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const snapshot = JSON.stringify(clip);
    const existing = [input('a')];
    instantiatePaste(clip, existing);
    expect(JSON.stringify(clip)).toBe(snapshot);
    expect(existing[0].id).toBe('a');
  });

  it('pastes an Auth node with applyToInherit off and says so', () => {
    const auth = at('auth', {
      kind: 'Auth',
      label: 'Auth',
      auth: { authType: 'bearer', token: '{{token}}' },
      applyToInherit: true,
    });
    const clip = copySelection([auth], [], new Set(['auth']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const result = instantiatePaste(clip, [auth]);
    const pasted = result.nodes[0].kind;
    if (pasted.kind !== 'Auth') throw new Error('Expected an Auth node');
    expect(pasted.applyToInherit).toBe(false);
    expect(pasted.auth).toEqual({ authType: 'bearer', token: '{{token}}' });
    expect(result.notices).toHaveLength(1);
    expect(result.notices[0]).toMatch(/inherited auth/i);
    // The original keeps its setting and the clip is unchanged.
    expect(auth.kind.kind === 'Auth' && auth.kind.applyToInherit).toBe(true);
    expect(clip.nodes[0].kind.kind === 'Auth' && clip.nodes[0].kind.applyToInherit).toBe(true);
  });

  it('gives a pasted Auth node its own copy of the auth config', () => {
    const auth = at('auth', {
      kind: 'Auth',
      label: 'Auth',
      auth: { authType: 'basic', username: 'u', password: 'p' },
      applyToInherit: false,
    });
    const clip = copySelection([auth], [], new Set(['auth']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const result = instantiatePaste(clip, [auth]);
    expect(result.notices).toEqual([]);
    const pasted = result.nodes[0].kind;
    if (pasted.kind !== 'Auth' || auth.kind.kind !== 'Auth') throw new Error('Expected Auth nodes');
    expect(pasted.auth).not.toBe(auth.kind.auth);
    expect(pasted.auth).toEqual(auth.kind.auth);
  });

  it('regenerates Switch case ids and follows them on the wires', () => {
    const sw = at('sw', {
      kind: 'Switch',
      label: 'Route',
      value: 'x',
      cases: [
        { id: 'c1', label: 'One', matches: '1' },
        { id: 'c2', label: 'Two', matches: '2' },
      ],
    });
    const edges = [
      wire('e1', 'sw', 'o1', { sourceHandle: caseHandle('c1'), targetField: 'trigger' }),
      wire('e2', 'sw', 'o2', { sourceHandle: caseHandle('c2'), targetField: 'trigger' }),
      wire('e3', 'sw', 'o3', { sourceHandle: 'default', targetField: 'trigger' }),
    ];
    const nodes = [sw, output('o1'), output('o2'), output('o3')];
    const clip = copySelection(nodes, edges, new Set(['sw', 'o1', 'o2', 'o3']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const result = instantiatePaste(clip, nodes);
    const pasted = result.nodes[0].kind;
    if (pasted.kind !== 'Switch') throw new Error('Expected a Switch node');
    const newIds = pasted.cases.map((c) => c.id);
    expect(newIds).toHaveLength(2);
    expect(newIds).not.toContain('c1');
    expect(newIds).not.toContain('c2');
    expect(pasted.cases.map((c) => c.label)).toEqual(['One', 'Two']);
    const handles = result.edges.map((e) => e.sourceHandle);
    expect(handles).toEqual([caseHandle(newIds[0]), caseHandle(newIds[1]), 'default']);
  });

  it('drops a wire that names a Switch case that is not on the node', () => {
    const sw = at('sw', {
      kind: 'Switch',
      label: 'Route',
      value: 'x',
      cases: [{ id: 'c1', label: 'One', matches: '1' }],
    });
    const nodes = [sw, output('o1')];
    const edges = [
      wire('e1', 'sw', 'o1', { sourceHandle: caseHandle('gone'), targetField: 'trigger' }),
    ];
    const clip = copySelection(nodes, edges, new Set(['sw', 'o1']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    expect(instantiatePaste(clip, nodes).edges).toEqual([]);
  });

  it('keeps the source handle of an If wire', () => {
    const iff = at('if', { kind: 'If', label: 'If', condition: 'true' });
    const nodes = [iff, output('o1')];
    const edges = [wire('e1', 'if', 'o1', { sourceHandle: 'true', targetField: 'trigger' })];
    const clip = copySelection(nodes, edges, new Set(['if', 'o1']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    expect(instantiatePaste(clip, nodes).edges[0].sourceHandle).toBe('true');
  });

  it('renames a pasted Wait for callback to a free name and warns about the old variable', () => {
    const wait = at('w', {
      kind: 'WaitForCallback',
      label: 'Wait',
      name: 'pay',
      timeoutMs: 60000,
    });
    const clip = copySelection([wait], [], new Set(['w']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const first = instantiatePaste(clip, [wait]);
    const firstKind = first.nodes[0].kind;
    if (firstKind.kind !== 'WaitForCallback') throw new Error('Expected a Wait node');
    expect(firstKind.name).toBe('pay_2');
    expect(first.notices[0]).toContain('{{callback.pay}}');
    expect(first.notices[0]).toContain('pay_2');
    // A second paste sees the first one and moves on.
    const second = instantiatePaste(clip, [wait, ...first.nodes]);
    const secondKind = second.nodes[0].kind;
    if (secondKind.kind !== 'WaitForCallback') throw new Error('Expected a Wait node');
    expect(secondKind.name).toBe('pay_3');
    expect(secondKind.timeoutMs).toBe(60000);
  });

  it('keeps a Wait name that is free', () => {
    const wait = at('w', {
      kind: 'WaitForCallback',
      label: 'Wait',
      name: 'pay',
      timeoutMs: 60000,
    });
    const clip = copySelection([wait], [], new Set(['w']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const result = instantiatePaste(clip, []);
    const kind = result.nodes[0].kind;
    if (kind.kind !== 'WaitForCallback') throw new Error('Expected a Wait node');
    expect(kind.name).toBe('pay');
    expect(result.notices).toEqual([]);
  });

  it('never gives two pasted Wait nodes the same name', () => {
    const w1 = at('w1', { kind: 'WaitForCallback', label: 'A', name: 'a', timeoutMs: 1000 });
    const w2 = at('w2', { kind: 'WaitForCallback', label: 'B', name: 'a_2', timeoutMs: 1000 });
    const clip = copySelection([w1, w2], [], new Set(['w1', 'w2']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const names = instantiatePaste(clip, [w1, w2]).nodes.map((n) =>
      n.kind.kind === 'WaitForCallback' ? n.kind.name : '',
    );
    expect(new Set(names).size).toBe(2);
    expect(names).not.toContain('a');
    expect(names).not.toContain('a_2');
  });

  it('keeps a saved request path and gives an inline request its own copy', () => {
    const saved = at('s', {
      kind: 'Request',
      label: 'Saved',
      source: { type: 'Saved', requestPath: 'users/get.yml' },
    });
    const inline = at('i', {
      kind: 'Request',
      label: 'Inline',
      source: {
        type: 'Inline',
        request: {
          method: 'GET',
          url: 'https://x.test',
          headers: [{ name: 'a', value: 'b' }],
        },
      },
    });
    const clip = copySelection([saved, inline], [], new Set(['s', 'i']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    const [pastedSaved, pastedInline] = instantiatePaste(clip, [saved, inline]).nodes;
    if (pastedSaved.kind.kind !== 'Request' || pastedInline.kind.kind !== 'Request') {
      throw new Error('Expected Request nodes');
    }
    expect(pastedSaved.kind.source).toEqual({ type: 'Saved', requestPath: 'users/get.yml' });
    if (pastedInline.kind.source.type !== 'Inline' || inline.kind.kind !== 'Request') {
      throw new Error('Expected an inline source');
    }
    if (inline.kind.source.type !== 'Inline') throw new Error('Expected an inline source');
    pastedInline.kind.source.request.headers.push({ name: 'x', value: 'y' });
    expect(inline.kind.source.request.headers).toHaveLength(1);
  });
});

describe('uniqueCallbackName', () => {
  it('returns the name when it is free and adds _2, _3 when it is not', () => {
    expect(uniqueCallbackName('pay', new Set())).toBe('pay');
    expect(uniqueCallbackName('pay', new Set(['pay']))).toBe('pay_2');
    expect(uniqueCallbackName('pay', new Set(['pay', 'pay_2']))).toBe('pay_3');
  });
});

describe('canPasteInto', () => {
  const savedNode = at('s', {
    kind: 'Request',
    label: 'S',
    source: { type: 'Saved', requestPath: 'a.yml' },
  });

  it('refuses saved requests from another collection', () => {
    const clip = copySelection([savedNode], [], new Set(['s']), 'one');
    if (!clip) throw new Error('Expected a clip');
    expect(canPasteInto(clip, 'two')).toMatch(/one/);
    expect(canPasteInto(clip, 'one')).toBeNull();
  });

  it('allows other nodes anywhere', () => {
    const clip = copySelection([input('a')], [], new Set(['a']), 'one');
    if (!clip) throw new Error('Expected a clip');
    expect(canPasteInto(clip, 'two')).toBeNull();
  });
});

describe('in-memory clipboard', () => {
  beforeEach(() => clearFlowClipboard());

  it('starts empty and keeps what was set', () => {
    expect(getFlowClipboard()).toBeNull();
    const clip = copySelection([input('a')], [], new Set(['a']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    setFlowClipboard(clip);
    expect(getFlowClipboard()).toBe(clip);
  });

  it('counts pastes and restarts the count on a new copy', () => {
    const clip = copySelection([input('a')], [], new Set(['a']), 'demo');
    if (!clip) throw new Error('Expected a clip');
    setFlowClipboard(clip);
    expect(nextPasteStep()).toBe(1);
    expect(nextPasteStep()).toBe(2);
    setFlowClipboard(clip);
    expect(nextPasteStep()).toBe(1);
  });
});
