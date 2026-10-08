import { RESULT_HANDLE } from '@/lib/flow-handles';
import type { FlowEdge, FlowNode, FlowNodeKind, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import { type EdgeRunState, exitLabel } from './flowExits';
import { nodeStatusLabel } from './nodes/nodeStatus';

const KIND_NAMES: Record<FlowNodeKind['kind'], string> = {
  Request: 'request',
  Input: 'input',
  Output: 'output',
  If: 'if',
  Switch: 'switch',
  WaitForCallback: 'wait for callback',
  Transform: 'transform',
  Auth: 'auth',
};

const MAX_ERROR_CHARS = 80;

// What a screen reader calls a node: its label, or its kind when the label is blank.
export function flowNodeName(kind: FlowNodeKind): string {
  return kind.label.trim() || KIND_NAMES[kind.kind];
}

// First line of an error, cut so that one failure cannot flood a reader.
export function shortError(error: string): string {
  const first = (error.split('\n')[0] ?? '').trim();
  return first.length > MAX_ERROR_CHARS ? `${first.slice(0, MAX_ERROR_CHARS - 1)}…` : first;
}

// Label, kind and status only. Progress text, values and exchanges change too
// often or are too long, and the node's own content is read separately.
export function flowNodeAriaLabel(
  kind: FlowNodeKind,
  status: FlowNodeStatus,
  detail?: FlowNodeDetail,
): string {
  const base = `${flowNodeName(kind)}, ${KIND_NAMES[kind.kind]} node, ${nodeStatusLabel(status, detail)}`;
  return status === 'failed' && detail?.error ? `${base}: ${shortError(detail.error)}` : base;
}

const targetFieldLabel = (field: string) => (field === 'trigger' ? 'run when' : field);

export function flowEdgeAriaLabel(
  edge: FlowEdge,
  byId: ReadonlyMap<string, FlowNode>,
  run: EdgeRunState,
): string {
  const source = byId.get(edge.sourceNodeId);
  const target = byId.get(edge.targetNodeId);
  const exit = source ? exitLabel(source.kind, edge.sourceHandle ?? RESULT_HANDLE) : undefined;
  const from = source ? flowNodeName(source.kind) : edge.sourceNodeId;
  const to = target ? flowNodeName(target.kind) : edge.targetNodeId;
  const state = run === 'taken' ? ', taken' : run === 'not-taken' ? ', not taken' : '';
  return `Wire from ${from}${exit ? ` (${exit} exit)` : ''} to ${to}, ${targetFieldLabel(edge.targetField)}${state}`;
}
