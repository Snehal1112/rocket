import { renderHook } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { FlowEdge, FlowNode, FlowNodeKind } from '@/lib/tauri-api';
import { useStableGraph } from '../use-stable-graph';

const outputKind = (label: string): FlowNodeKind => ({ kind: 'Output', label });

const node = (id: string, kind: FlowNodeKind, x = 0): FlowNode => ({
  id,
  kind,
  position: { x, y: 0 },
});

const wire = (id: string, source: string, target: string, expression = ''): FlowEdge => ({
  id,
  sourceNodeId: source,
  targetNodeId: target,
  targetField: 'value',
  expression,
});

const kindA = outputKind('A');
const kindB = outputKind('B');

describe('useStableGraph', () => {
  it('returns the first arrays unchanged', () => {
    const nodes = [node('a', kindA)];
    const edges = [wire('e1', 'a', 'b')];
    const { result } = renderHook(() => useStableGraph(nodes, edges));
    expect(result.current.nodes).toBe(nodes);
    expect(result.current.edges).toBe(edges);
  });

  it('keeps both arrays when only positions change', () => {
    const first = { nodes: [node('a', kindA), node('b', kindB)], edges: [wire('e1', 'a', 'b')] };
    const { result, rerender } = renderHook((p) => useStableGraph(p.nodes, p.edges), {
      initialProps: first,
    });
    const before = result.current;
    rerender({
      nodes: [node('a', kindA, 50), node('b', kindB, 90)],
      edges: [{ ...first.edges[0] }],
    });
    expect(result.current).toBe(before);
    expect(result.current.nodes).toBe(first.nodes);
    expect(result.current.edges).toBe(first.edges);
  });

  it('returns new nodes when a node kind reference changes, and keeps the edges', () => {
    const first = { nodes: [node('a', kindA)], edges: [wire('e1', 'a', 'a')] };
    const { result, rerender } = renderHook((p) => useStableGraph(p.nodes, p.edges), {
      initialProps: first,
    });
    const nextNodes = [node('a', outputKind('A'))];
    rerender({ nodes: nextNodes, edges: first.edges });
    expect(result.current.nodes).toBe(nextNodes);
    expect(result.current.edges).toBe(first.edges);
  });

  it('returns new edges when an edge expression or target changes, and keeps the nodes', () => {
    const first = { nodes: [node('a', kindA)], edges: [wire('e1', 'a', 'b', 'x')] };
    const { result, rerender } = renderHook((p) => useStableGraph(p.nodes, p.edges), {
      initialProps: first,
    });
    const changed = [wire('e1', 'a', 'b', 'y')];
    rerender({ nodes: first.nodes, edges: changed });
    expect(result.current.edges).toBe(changed);
    expect(result.current.nodes).toBe(first.nodes);

    const retargeted = [wire('e1', 'a', 'c', 'y')];
    rerender({ nodes: first.nodes, edges: retargeted });
    expect(result.current.edges).toBe(retargeted);
  });

  it('returns new arrays when an item is added or an id changes', () => {
    const first = { nodes: [node('a', kindA)], edges: [] as FlowEdge[] };
    const { result, rerender } = renderHook((p) => useStableGraph(p.nodes, p.edges), {
      initialProps: first,
    });
    const added = [node('a', kindA), node('b', kindB)];
    rerender({ nodes: added, edges: first.edges });
    expect(result.current.nodes).toBe(added);

    const renamed = [node('z', kindA), node('b', kindB)];
    rerender({ nodes: renamed, edges: first.edges });
    expect(result.current.nodes).toBe(renamed);
  });
});
