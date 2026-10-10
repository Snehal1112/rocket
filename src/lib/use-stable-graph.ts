import { useRef } from 'react';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';

export interface StableGraph {
  nodes: FlowNode[];
  edges: FlowEdge[];
}

// Positions are not compared. Issues read ids, kinds and wire ends, never positions.
function sameNodes(a: FlowNode[], b: FlowNode[]): boolean {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  return a.every((n, i) => n.id === b[i].id && n.kind === b[i].kind);
}

function sameEdges(a: FlowEdge[], b: FlowEdge[]): boolean {
  if (a === b) return true;
  if (a.length !== b.length) return false;
  return a.every((e, i) => {
    const o = b[i];
    return (
      e.id === o.id &&
      e.sourceNodeId === o.sourceNodeId &&
      e.targetNodeId === o.targetNodeId &&
      e.targetField === o.targetField &&
      e.expression === o.expression
    );
  });
}

// Returns the previous arrays while nothing issues depend on has changed, so a
// memo keyed on them does not re-run for every drag frame.
export function useStableGraph(nodes: FlowNode[], edges: FlowEdge[]): StableGraph {
  const last = useRef<StableGraph | null>(null);
  const prev = last.current;
  const stableNodes = prev && sameNodes(prev.nodes, nodes) ? prev.nodes : nodes;
  const stableEdges = prev && sameEdges(prev.edges, edges) ? prev.edges : edges;
  if (prev && prev.nodes === stableNodes && prev.edges === stableEdges) return prev;
  const next = { nodes: stableNodes, edges: stableEdges };
  last.current = next;
  return next;
}
