import { describe, expect, it } from 'vitest';
import { caseHandle } from '@/lib/flow-handles';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';
import { DEFAULT_NODE_SIZE, layoutFlow, NODE_SEP, type NodeSize, RANK_SEP } from '../flow-layout';

const out = (id: string, x = 0, y = 0): FlowNode => ({
  id,
  kind: { kind: 'Output', label: id },
  position: { x, y },
});

const wire = (id: string, from: string, to: string, over: Partial<FlowEdge> = {}): FlowEdge => ({
  id,
  sourceNodeId: from,
  targetNodeId: to,
  targetField: 'trigger',
  expression: 'response.body',
  ...over,
});

const noSizes = new Map<string, NodeSize>();

const rect = (n: FlowNode, sizes: ReadonlyMap<string, NodeSize>) => {
  const s = sizes.get(n.id) ?? DEFAULT_NODE_SIZE;
  return { x: n.position.x, y: n.position.y, w: s.width, h: s.height };
};

function overlaps(nodes: FlowNode[], sizes: ReadonlyMap<string, NodeSize>) {
  for (let i = 0; i < nodes.length; i += 1) {
    for (let j = i + 1; j < nodes.length; j += 1) {
      const a = rect(nodes[i], sizes);
      const b = rect(nodes[j], sizes);
      if (a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h) return true;
    }
  }
  return false;
}

describe('layoutFlow', () => {
  it('lays a chain out left to right with the rank gap between nodes', () => {
    const nodes = [out('a'), out('b'), out('c')];
    const edges = [wire('e1', 'a', 'b'), wire('e2', 'b', 'c')];
    const laid = layoutFlow(nodes, edges, noSizes);
    const x = (id: string) => laid.find((n) => n.id === id)?.position.x ?? Number.NaN;
    expect(x('b') - x('a')).toBe(DEFAULT_NODE_SIZE.width + RANK_SEP);
    expect(x('c') - x('b')).toBe(DEFAULT_NODE_SIZE.width + RANK_SEP);
  });

  it('separates nodes that start on top of each other', () => {
    const nodes = [out('a'), out('b'), out('c'), out('d')];
    const edges = [wire('e1', 'a', 'b'), wire('e2', 'a', 'c'), wire('e3', 'a', 'd')];
    const laid = layoutFlow(nodes, edges, noSizes);
    expect(overlaps(laid, noSizes)).toBe(false);
    const column = laid.filter((n) => n.id !== 'a').map((n) => n.position.y);
    const sorted = [...column].sort((p, q) => p - q);
    expect(sorted[1] - sorted[0]).toBeGreaterThanOrEqual(DEFAULT_NODE_SIZE.height + NODE_SEP);
  });

  it('uses the measured sizes', () => {
    const sizes = new Map<string, NodeSize>([
      ['a', { width: 400, height: 300 }],
      ['b', { width: 100, height: 50 }],
    ]);
    const laid = layoutFlow([out('a'), out('b')], [wire('e1', 'a', 'b')], sizes);
    expect(laid[1].position.x - laid[0].position.x).toBe(400 + RANK_SEP);
    expect(overlaps(laid, sizes)).toBe(false);
  });

  it('keeps the top-left corner of the graph where it was', () => {
    const nodes = [out('a', 500, 700), out('b', 900, 700)];
    const laid = layoutFlow(nodes, [wire('e1', 'a', 'b')], noSizes);
    expect(Math.min(...laid.map((n) => n.position.x))).toBe(500);
    expect(Math.min(...laid.map((n) => n.position.y))).toBe(700);
  });

  it('is deterministic', () => {
    const nodes = [out('a'), out('b'), out('c')];
    const edges = [wire('e1', 'a', 'b'), wire('e2', 'a', 'c')];
    expect(layoutFlow(nodes, edges, noSizes)).toEqual(layoutFlow(nodes, edges, noSizes));
  });

  it('returns the same array when the graph is already tidy', () => {
    const nodes = [out('a'), out('b'), out('c')];
    const edges = [wire('e1', 'a', 'b'), wire('e2', 'a', 'c')];
    const once = layoutFlow(nodes, edges, noSizes);
    expect(once).not.toBe(nodes);
    expect(layoutFlow(once, edges, noSizes)).toBe(once);
  });

  it('returns the input for an empty graph', () => {
    const nodes: FlowNode[] = [];
    expect(layoutFlow(nodes, [], noSizes)).toBe(nodes);
  });

  it('does not throw on a cycle or a self loop, and returns finite positions', () => {
    const nodes = [out('a'), out('b'), out('c')];
    const edges = [wire('e1', 'a', 'b'), wire('e2', 'b', 'a'), wire('e3', 'c', 'c')];
    const laid = layoutFlow(nodes, edges, noSizes);
    for (const n of laid) {
      expect(Number.isFinite(n.position.x)).toBe(true);
      expect(Number.isFinite(n.position.y)).toBe(true);
    }
  });

  it('ignores wires that point at missing nodes', () => {
    const nodes = [out('a'), out('b')];
    const laid = layoutFlow(nodes, [wire('e1', 'a', 'ghost'), wire('e2', 'a', 'b')], noSizes);
    expect(laid).toHaveLength(2);
    expect(overlaps(laid, noSizes)).toBe(false);
  });

  it('lays out only the selection and leaves the others as they are', () => {
    const a = out('a', 100, 100);
    const b = out('b', 100, 100);
    const far = out('far', 5000, 5000);
    const laid = layoutFlow([a, b, far], [wire('e1', 'a', 'b')], noSizes, new Set(['a', 'b']));
    expect(laid.find((n) => n.id === 'far')).toBe(far);
    const [la, lb] = laid;
    expect(la.position).toEqual({ x: 100, y: 100 });
    expect(lb.position.x - la.position.x).toBe(DEFAULT_NODE_SIZE.width + RANK_SEP);
  });

  it('treats an empty selection as the whole graph', () => {
    const nodes = [out('a'), out('b')];
    const laid = layoutFlow(nodes, [wire('e1', 'a', 'b')], noSizes, new Set());
    expect(laid[1].position.x).toBeGreaterThan(laid[0].position.x);
  });

  it('keeps the exits of a Switch in case order', () => {
    const sw: FlowNode = {
      id: 'sw',
      kind: {
        kind: 'Switch',
        label: 'Route',
        value: 'x',
        cases: [
          { id: 'c1', label: 'One', matches: '1' },
          { id: 'c2', label: 'Two', matches: '2' },
          { id: 'c3', label: 'Three', matches: '3' },
        ],
      },
      position: { x: 0, y: 0 },
    };
    const nodes = [sw, out('o1'), out('o2'), out('o3')];
    // The wires are listed in a different order from the cases on purpose.
    const edges = [
      wire('e3', 'sw', 'o3', { sourceHandle: caseHandle('c3') }),
      wire('e1', 'sw', 'o1', { sourceHandle: caseHandle('c1') }),
      wire('e2', 'sw', 'o2', { sourceHandle: caseHandle('c2') }),
    ];
    const laid = layoutFlow(nodes, edges, noSizes);
    const y = (id: string) => laid.find((n) => n.id === id)?.position.y ?? Number.NaN;
    expect(y('o1')).toBeLessThan(y('o2'));
    expect(y('o2')).toBeLessThan(y('o3'));
  });

  it('keeps the true exit of an If above the false exit', () => {
    const cond: FlowNode = {
      id: 'if',
      kind: { kind: 'If', label: 'Check', condition: 'x' },
      position: { x: 0, y: 0 },
    };
    const nodes = [cond, out('t'), out('f')];
    // The false wire is listed first on purpose.
    const edges = [
      wire('e2', 'if', 'f', { sourceHandle: 'false' }),
      wire('e1', 'if', 't', { sourceHandle: 'true' }),
    ];
    const laid = layoutFlow(nodes, edges, noSizes);
    const y = (id: string) => laid.find((n) => n.id === id)?.position.y ?? Number.NaN;
    expect(y('t')).toBeLessThan(y('f'));
  });
});
