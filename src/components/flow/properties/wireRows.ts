import { RESULT_HANDLE, TRIGGER_HANDLE } from '@/lib/flow-handles';
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

// The exit name of an edge's source, or null for the default exit.
function exitName(edge: FlowEdge, source: FlowNode | undefined): string | null {
  const handle = edge.sourceHandle ?? RESULT_HANDLE;
  if (handle === RESULT_HANDLE) return null;
  return (source && exitLabel(source.kind, handle)) ?? handle;
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
  return {
    edgeId: edge.id,
    field: fieldLabel(edge.targetField),
    otherNodeId: other?.id ?? '',
    otherLabel: other?.kind.label ?? null,
    exit: exitName(edge, source),
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
  const groups: OutgoingGroup[] = [];
  for (const e of edges.filter((edge) => edge.sourceNodeId === node.id)) {
    const r = row(e, byId.get(e.targetNodeId), node, nodeStatus, nodeDetail);
    const group = groups.find((g) => g.exit === r.exit);
    if (group) group.rows.push(r);
    else groups.push({ exit: r.exit, rows: [r] });
  }
  return groups.sort((a, b) => exitOrder(node, a.exit) - exitOrder(node, b.exit));
}

// Orders exits as the node draws them: result, true, false, cases in order, default.
function exitOrder(node: FlowNode, exit: string | null): number {
  if (exit === null) return 0;
  const kind = node.kind;
  if (kind.kind === 'If') return exit === 'true' ? 1 : 2;
  if (kind.kind === 'Switch') {
    const index = kind.cases.findIndex(
      (c, i) => exitLabel(kind, `case:${c.id}`) === exit || `Case ${i + 1}` === exit,
    );
    return index < 0 ? kind.cases.length + 1 : index + 1;
  }
  return 1;
}
