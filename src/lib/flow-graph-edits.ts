import { caseHandle } from '@/lib/flow-handles';
import type { FlowEdge, FlowNode, FlowNodeKind } from '@/lib/tauri-api';

export function replaceNodeKind(nodes: FlowNode[], nodeId: string, kind: FlowNodeKind): FlowNode[] {
  return nodes.map((n) => (n.id === nodeId ? { ...n, kind } : n));
}

// Removes the case and every wire leaving its exit, so no edge is left
// pointing at a handle that no longer exists.
export function removeSwitchCase(
  nodes: FlowNode[],
  edges: FlowEdge[],
  nodeId: string,
  caseId: string,
): { nodes: FlowNode[]; edges: FlowEdge[] } | null {
  const node = nodes.find((n) => n.id === nodeId);
  if (node?.kind.kind !== 'Switch') return null;
  const kind: FlowNodeKind = {
    ...node.kind,
    cases: node.kind.cases.filter((c) => c.id !== caseId),
  };
  const handle = caseHandle(caseId);
  return {
    nodes: replaceNodeKind(nodes, nodeId, kind),
    edges: edges.filter((e) => !(e.sourceNodeId === nodeId && e.sourceHandle === handle)),
  };
}
