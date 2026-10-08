import * as dagre from '@dagrejs/dagre';
import { caseIdFromHandle } from '@/lib/flow-handles';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';

export interface NodeSize {
  width: number;
  height: number;
}

// Used for a node React Flow has not measured yet.
export const DEFAULT_NODE_SIZE: NodeSize = { width: 260, height: 120 };
// Gap between columns and between nodes in a column.
export const RANK_SEP = 80;
export const NODE_SEP = 40;

// Position of a wire's exit on its source: a Switch case index, true before false, else 0.
// Dagre stacks siblings in reverse insertion order, so the wires are inserted
// from the last exit to the first to show the exits in the order the node does.
// This ordering was observed on @dagrejs/dagre 3.1.1.
function exitIndex(edge: FlowEdge, source: FlowNode | undefined): number {
  const handle = edge.sourceHandle;
  if (!handle || !source) return 0;
  const kind = source.kind;
  if (kind.kind === 'Switch') {
    const caseId = caseIdFromHandle(handle);
    if (caseId === null) return kind.cases.length;
    const i = kind.cases.findIndex((c) => c.id === caseId);
    return i === -1 ? kind.cases.length : i;
  }
  if (kind.kind === 'If') return handle === 'true' ? 0 : 1;
  return 0;
}

/**
 * Lays the nodes out left to right and returns them with new positions. With a
 * non-empty `only`, just those nodes move. The top-left corner of what is laid
 * out stays where it was, so nothing jumps across the canvas. The same `nodes`
 * array comes back when no position changes.
 */
export function layoutFlow(
  nodes: FlowNode[],
  edges: FlowEdge[],
  sizes: ReadonlyMap<string, NodeSize>,
  only?: ReadonlySet<string>,
): FlowNode[] {
  const target = nodes.filter((n) => !only || only.size === 0 || only.has(n.id));
  if (target.length === 0) return nodes;
  const ids = new Set(target.map((n) => n.id));
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const order = new Map(target.map((n, i) => [n.id, i]));
  const sizeOf = (id: string) => sizes.get(id) ?? DEFAULT_NODE_SIZE;

  const graph = new dagre.graphlib.Graph();
  graph.setGraph({ rankdir: 'LR', ranksep: RANK_SEP, nodesep: NODE_SEP });
  graph.setDefaultEdgeLabel(() => ({}));
  for (const n of target) {
    const { width, height } = sizeOf(n.id);
    graph.setNode(n.id, { width, height });
  }
  const usable = edges
    .filter(
      (e) =>
        ids.has(e.sourceNodeId) && ids.has(e.targetNodeId) && e.sourceNodeId !== e.targetNodeId,
    )
    .sort(
      (a, b) =>
        (order.get(a.sourceNodeId) ?? 0) - (order.get(b.sourceNodeId) ?? 0) ||
        exitIndex(b, byId.get(b.sourceNodeId)) - exitIndex(a, byId.get(a.sourceNodeId)),
    );
  for (const e of usable) graph.setEdge(e.sourceNodeId, e.targetNodeId);
  dagre.layout(graph);

  // Dagre reports node centres. Convert to top-left corners.
  const corners = new Map<string, { x: number; y: number }>();
  for (const n of target) {
    const placed = graph.node(n.id);
    const { width, height } = sizeOf(n.id);
    corners.set(n.id, { x: placed.x - width / 2, y: placed.y - height / 2 });
  }
  const laidMinX = Math.min(...[...corners.values()].map((c) => c.x));
  const laidMinY = Math.min(...[...corners.values()].map((c) => c.y));
  const oldMinX = Math.min(...target.map((n) => n.position.x));
  const oldMinY = Math.min(...target.map((n) => n.position.y));

  let changed = false;
  const next = nodes.map((n) => {
    const corner = corners.get(n.id);
    if (!corner) return n;
    const x = Math.round(corner.x - laidMinX + oldMinX);
    const y = Math.round(corner.y - laidMinY + oldMinY);
    if (x === n.position.x && y === n.position.y) return n;
    changed = true;
    return { ...n, position: { x, y } };
  });
  return changed ? next : nodes;
}
