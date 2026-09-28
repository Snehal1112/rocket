import {
  caseIdFromHandle,
  DEFAULT_HANDLE,
  FALSE_HANDLE,
  RESULT_HANDLE,
  TRUE_HANDLE,
} from '@/lib/flow-handles';
import type { FlowEdge, FlowNode, FlowNodeKind, FlowNodeStatus, SwitchCase } from '@/lib/tauri-api';

// Name shown for a Switch case. A blank label falls back to its position,
// so badges, edge labels and buttons never show an empty name.
export function caseDisplayLabel(c: SwitchCase, index: number): string {
  return c.label.trim() ? c.label : `Case ${index + 1}`;
}

// Display label of a routing node's exit. A case is looked up by id, so
// renaming a case relabels its edges and badge without rewiring anything.
export function exitLabel(kind: FlowNodeKind, handle: string): string | undefined {
  if (kind.kind === 'If') {
    if (handle === TRUE_HANDLE) return 'true';
    if (handle === FALSE_HANDLE) return 'false';
    return undefined;
  }
  if (kind.kind === 'Switch') {
    if (handle === DEFAULT_HANDLE) return 'default';
    const caseId = caseIdFromHandle(handle);
    if (!caseId) return undefined;
    const index = kind.cases.findIndex((c) => c.id === caseId);
    return index < 0 ? undefined : caseDisplayLabel(kind.cases[index], index);
  }
  return undefined;
}

export type EdgeRunState = 'taken' | 'not-taken' | 'neutral';

// Only a routing node that completed has a chosen exit. Every other case
// (running, failed, skipped, never run, plain node) renders neutral, so a
// new run never shows the previous run's branch.
export function edgeRunState(
  edge: FlowEdge,
  source: FlowNode | undefined,
  sourceStatus: FlowNodeStatus | undefined,
  sourceBranch: string | undefined,
): EdgeRunState {
  if (!source || (source.kind.kind !== 'If' && source.kind.kind !== 'Switch')) return 'neutral';
  if (sourceStatus !== 'success' || !sourceBranch) return 'neutral';
  return (edge.sourceHandle ?? RESULT_HANDLE) === sourceBranch ? 'taken' : 'not-taken';
}
