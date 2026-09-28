import type { Connection } from '@xyflow/react';
import {
  caseIdFromHandle,
  DEFAULT_HANDLE,
  FALSE_HANDLE,
  INPUT_HANDLE,
  isRoutingKind,
  RESULT_HANDLE,
  TRIGGER_HANDLE,
  TRUE_HANDLE,
} from '@/lib/flow-handles';
import type { FlowEdge, FlowNode } from '@/lib/tauri-api';

// Every source kind is evaluated as a response-shaped object. An Input
// node's value is its `response.body` (see resolve_flow_wire_expression).
// The node argument is kept so a later per-kind default is a local change.
export function defaultExpressionFor(_sourceNode: FlowNode): string {
  return 'response.body';
}

// An If/Switch `input` and a "Run when" `trigger` carry no wired value, so
// their edges have no expression to evaluate or edit.
export function isDataLessTarget(targetHandle: string): boolean {
  return targetHandle === INPUT_HANDLE || targetHandle === TRIGGER_HANDLE;
}

export function shouldPromptForExpression(edge: FlowEdge): boolean {
  return !isDataLessTarget(edge.targetField);
}

export function buildEdgeFromConnection(
  connection: Connection,
  sourceNode: FlowNode,
): FlowEdge | null {
  if (!connection.source || !connection.target || !connection.targetHandle) return null;
  const edge: FlowEdge = {
    id: crypto.randomUUID(),
    sourceNodeId: connection.source,
    targetNodeId: connection.target,
    targetField: connection.targetHandle,
    expression: isDataLessTarget(connection.targetHandle) ? '' : defaultExpressionFor(sourceNode),
  };
  // Absent or `result` means the default exit. New plain wires leave the key
  // out to match what the backend writes.
  if (connection.sourceHandle && connection.sourceHandle !== RESULT_HANDLE) {
    edge.sourceHandle = connection.sourceHandle;
  }
  return edge;
}

// React Flow passes either a Connection or an Edge to isValidConnection.
export type ConnectionLike = {
  source: string | null;
  target: string | null;
  sourceHandle?: string | null;
  targetHandle?: string | null;
};

function sourceHandleExists(node: FlowNode, handle: string): boolean {
  switch (node.kind.kind) {
    case 'Request':
    case 'Input':
      return handle === RESULT_HANDLE;
    case 'If':
      return handle === TRUE_HANDLE || handle === FALSE_HANDLE;
    case 'Switch': {
      if (handle === DEFAULT_HANDLE) return true;
      const caseId = caseIdFromHandle(handle);
      return caseId !== null && node.kind.cases.some((c) => c.id === caseId);
    }
    case 'Output':
      return false;
  }
}

const REQUEST_TARGETS = ['url', 'headers', 'body', TRIGGER_HANDLE];
const OUTPUT_TARGETS = ['value', TRIGGER_HANDLE];

function targetAccepts(node: FlowNode, handle: string): boolean {
  switch (node.kind.kind) {
    case 'If':
    case 'Switch':
      return handle === INPUT_HANDLE;
    case 'Request':
      return REQUEST_TARGETS.includes(handle);
    case 'Output':
      return OUTPUT_TARGETS.includes(handle);
    case 'Input':
      return false;
  }
}

// Client-side copy of rocket_flow::validate rules V1–V5, so obviously
// invalid wires cannot be drawn. Save still runs the real validation, and
// cycles are left to it.
export function isValidFlowConnection(
  connection: ConnectionLike,
  nodes: FlowNode[],
  edges: FlowEdge[],
): boolean {
  const { source, target, targetHandle } = connection;
  if (!source || !target || !targetHandle) return false;
  const sourceNode = nodes.find((n) => n.id === source);
  const targetNode = nodes.find((n) => n.id === target);
  if (!sourceNode || !targetNode) return false;
  if (!sourceHandleExists(sourceNode, connection.sourceHandle ?? RESULT_HANDLE)) return false;
  if (!targetAccepts(targetNode, targetHandle)) return false;
  // A routing node evaluates exactly one input.
  if (isRoutingKind(targetNode.kind) && edges.some((e) => e.targetNodeId === target)) return false;
  // An Output has one `value` input. Run-when triggers stay unlimited.
  if (
    targetNode.kind.kind === 'Output' &&
    targetHandle === 'value' &&
    edges.some((e) => e.targetNodeId === target && e.targetField === 'value')
  ) {
    return false;
  }
  return true;
}

export interface GraphErrorIds {
  nodeIds: string[];
  edgeIds: string[];
}

const splitIds = (list: string | undefined): string[] =>
  (list ?? '')
    .split(',')
    .map((s) => s.trim())
    .filter(Boolean);

// Every save_flow validation error ends with "node(s): a, b; edge(s): e1".
// That covers a cycle and rocket_flow::validate's InvalidNode/InvalidEdge.
// Either list may be empty. Older cycle messages have no edge segment.
export function parseGraphErrorMessage(message: string): GraphErrorIds | null {
  const match = message.match(/node\(s\): ([^;]*)(?:; edge\(s\): (.*))?$/);
  if (!match) return null;
  return { nodeIds: splitIds(match[1]), edgeIds: splitIds(match[2]) };
}
