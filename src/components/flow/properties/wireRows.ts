import {
  caseIdFromHandle,
  DEFAULT_HANDLE,
  RESULT_HANDLE,
  TRIGGER_HANDLE,
} from '@/lib/flow-handles';
import type { FlowEdge, FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';
import { edgeRunState, exitLabel } from '../flowExits';

export interface WireRow {
  edgeId: string;
  field: string;
  otherNodeId: string;
  otherLabel: string | null;
  exit: string | null;
  preview: string | null;
  editable: boolean;
  notTaken: boolean;
}

export interface OutgoingGroup {
  handle: string | null;
  exit: string | null;
  rows: WireRow[];
}

const FIELD_LABELS: Record<string, string> = {
  url: 'URL',
  body: 'Body',
  headers: 'Headers',
  [TRIGGER_HANDLE]: 'Run when',
  value: 'Value',
  input: 'Input',
};

const PREVIEW_MAX = 60;

// A header wire names its header; other fields use the label the node shows.
export function fieldLabel(targetField: string): string {
  const header = /^headers\[(.+)\]\.value$/.exec(targetField);
  if (header) return header[1];
  return FIELD_LABELS[targetField] ?? targetField;
}

// The first non-empty line of a script, cut to fit one row.
export function scriptPreview(expression: string): string | null {
  const firstLine = expression
    .split('\n')
    .map((line) => line.trim())
    .find((line) => line !== '');
  if (!firstLine) return null;
  return firstLine.length > PREVIEW_MAX ? `${firstLine.slice(0, PREVIEW_MAX)}…` : firstLine;
}

// The exit handle of an edge, or null for the default exit.
function exitHandle(edge: FlowEdge): string | null {
  const handle = edge.sourceHandle ?? RESULT_HANDLE;
  return handle === RESULT_HANDLE ? null : handle;
}

// Display label for an exit handle, handling deleted cases.
export function exitDisplayLabel(source: FlowNode | undefined, handle: string): string {
  if (!source) return handle;
  const label = exitLabel(source.kind, handle);
  if (label) return label;
  // Handle is not recognized. Check if it's a deleted case.
  if (handle.startsWith('case:')) return '(deleted case)';
  return handle;
}

function isNotTaken(
  edge: FlowEdge,
  source: FlowNode | undefined,
  nodeStatus?: Record<string, FlowNodeStatus>,
  nodeDetail?: Record<string, FlowNodeDetail>,
): boolean {
  return (
    edgeRunState(
      edge,
      source,
      nodeStatus?.[edge.sourceNodeId],
      nodeDetail?.[edge.sourceNodeId]?.branch,
    ) === 'not-taken'
  );
}

function row(
  edge: FlowEdge,
  other: FlowNode | undefined,
  source: FlowNode | undefined,
  nodeStatus?: Record<string, FlowNodeStatus>,
  nodeDetail?: Record<string, FlowNodeDetail>,
): WireRow {
  const isTrigger = edge.targetField === TRIGGER_HANDLE;
  const preview = isTrigger ? null : scriptPreview(edge.expression);
  const handle = exitHandle(edge);
  const exitLabel = handle === null ? null : exitDisplayLabel(source, handle);
  return {
    edgeId: edge.id,
    field: fieldLabel(edge.targetField),
    otherNodeId: other?.id ?? '',
    otherLabel: other?.kind.label ?? null,
    exit: exitLabel,
    preview,
    // A Run when wire has no script, and a wire to a missing node cannot be edited.
    editable: !isTrigger && other !== undefined,
    notTaken: isNotTaken(edge, source, nodeStatus, nodeDetail),
  };
}

export function incomingRows(
  node: FlowNode,
  nodes: FlowNode[],
  edges: FlowEdge[],
  nodeStatus?: Record<string, FlowNodeStatus>,
  nodeDetail?: Record<string, FlowNodeDetail>,
): WireRow[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  return edges
    .filter((e) => e.targetNodeId === node.id)
    .map((e) => {
      const source = byId.get(e.sourceNodeId);
      return row(e, source, source, nodeStatus, nodeDetail);
    });
}

export function outgoingGroups(
  node: FlowNode,
  nodes: FlowNode[],
  edges: FlowEdge[],
  nodeStatus?: Record<string, FlowNodeStatus>,
  nodeDetail?: Record<string, FlowNodeDetail>,
): OutgoingGroup[] {
  const byId = new Map(nodes.map((n) => [n.id, n]));
  const groups: Map<string | null, OutgoingGroup> = new Map();
  for (const e of edges.filter((edge) => edge.sourceNodeId === node.id)) {
    const r = row(e, byId.get(e.targetNodeId), node, nodeStatus, nodeDetail);
    const handle = exitHandle(e);
    const groupKey = handle === null ? '__result__' : handle;
    if (!groups.has(groupKey)) {
      groups.set(groupKey, {
        handle,
        exit: r.exit,
        rows: [],
      });
    }
    const group = groups.get(groupKey);
    if (group) {
      group.rows.push(r);
    }
  }
  return Array.from(groups.values()).sort(
    (a, b) => exitOrder(node, a.handle) - exitOrder(node, b.handle),
  );
}

// Orders exits as the node draws them: result, true, false, cases in order, deleted cases, default.
function exitOrder(node: FlowNode, handle: string | null): number {
  if (handle === null) return 0;
  const kind = node.kind;
  if (kind.kind === 'If') {
    if (handle === 'true') return 1;
    if (handle === 'false') return 2;
    return 999;
  }
  if (kind.kind === 'Switch') {
    if (handle === DEFAULT_HANDLE) return kind.cases.length + 2;
    const caseId = caseIdFromHandle(handle);
    if (!caseId) return 999;
    const index = kind.cases.findIndex((c) => c.id === caseId);
    if (index < 0) return kind.cases.length + 1; // Deleted case.
    return index + 1;
  }
  return 1;
}
