import type { Connection } from '@xyflow/react';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';

// Every source kind is evaluated as a response-shaped object. An Input
// node's value is its `response.body` (see resolve_flow_wire_expression).
// The node argument is kept so a later per-kind default is a local change.
export function defaultExpressionFor(_sourceNode: FlowNode): string {
  return 'response.body';
}

export function buildEdgeFromConnection(
  connection: Connection,
  sourceNode: FlowNode,
): FlowEdge | null {
  if (!connection.source || !connection.target || !connection.targetHandle) return null;
  return {
    id: crypto.randomUUID(),
    sourceNodeId: connection.source,
    targetNodeId: connection.target,
    targetField: connection.targetHandle,
    expression: defaultExpressionFor(sourceNode),
  };
}
