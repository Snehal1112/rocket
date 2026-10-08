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

// First non-empty line of an error, cut so that one failure cannot flood a reader.
// The cut counts code points, so it never splits a surrogate pair.
export function shortError(error: string): string {
  const first =
    error
      .split('\n')
      .map((line) => line.trim())
      .find((line) => line !== '') ?? '';
  const chars = Array.from(first);
  return chars.length > MAX_ERROR_CHARS
    ? `${chars.slice(0, MAX_ERROR_CHARS - 1).join('')}…`
    : first;
}

// The ": text" suffix for a failed node, or nothing when the error has no text.
export function errorSuffix(error: string | undefined): string {
  const text = error ? shortError(error) : '';
  return text ? `: ${text}` : '';
}

// Label, kind and status only. Progress text, values and exchanges change too
// often or are too long, and the node's own content is read separately.
export function flowNodeAriaLabel(
  kind: FlowNodeKind,
  status: FlowNodeStatus,
  detail?: FlowNodeDetail,
): string {
  const base = `${flowNodeName(kind)}, ${KIND_NAMES[kind.kind]} node, ${nodeStatusLabel(status, detail)}`;
  return status === 'failed' ? `${base}${errorSuffix(detail?.error)}` : base;
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
