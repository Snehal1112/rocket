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

export interface CycleError {
  nodeIds: string[];
  edgeIds: string[];
}

// Backend's FlowService::save rejects a cyclic flow with the plain string
// "Invalid input: flow contains a cycle through node(s): a, b; edge(s): e1, e2"
// (ids joined by ", ", node/edge segments joined by "; " — see
// flow_service.rs's save()). The edge segment is optional so an older-format
// message without it still parses.
export function parseCycleErrorMessage(message: string): CycleError | null {
  const match = message.match(
    /flow contains a cycle through node\(s\): ([^;]*)(?:; edge\(s\): (.*))?$/,
  );
  if (!match) return null;
  return {
    nodeIds: match[1].split(', ').map((s) => s.trim()),
    edgeIds: match[2] ? match[2].split(', ').map((s) => s.trim()) : [],
  };
}
