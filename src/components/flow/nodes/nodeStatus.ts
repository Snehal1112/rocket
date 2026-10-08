import { type FlowIssue, worstSeverity } from '@/lib/flow-issues';
import type { FlowNodeStatus, FlowSkipReason } from '@/lib/tauri-api';

const statusStyles: Record<FlowNodeStatus, string> = {
  idle: 'border-border',
  running: 'border-blue-400 shadow-[0_0_0_1px_rgba(96,165,250,0.5)] animate-pulse',
  success: 'border-green-500 shadow-[0_0_0_1px_rgba(34,197,94,0.5)]',
  failed: 'border-red-500 shadow-[0_0_0_1px_rgba(239,68,68,0.5)]',
  skipped: 'border-muted-foreground/40 opacity-60',
};

const notTakenStyle = 'border-dashed border-muted-foreground/40 opacity-50';

export function nodeStatusClassName(status: FlowNodeStatus, skipReason?: FlowSkipReason): string {
  if (status === 'skipped' && skipReason === 'branch_not_taken') return notTakenStyle;
  return statusStyles[status];
}

// A skip with no reason predates Phase 2 (it can only mean an upstream failure).
export function nodeStatusCaption(
  status: FlowNodeStatus,
  detail?: { skipReason?: FlowSkipReason },
): string | null {
  if (status !== 'skipped') return null;
  return detail?.skipReason === 'branch_not_taken' ? 'Not taken' : 'Skipped — upstream failed';
}

// Ring around a node card for the worst problem it has. Plain colours only.
export function issueRingClassName(issues?: FlowIssue[]): string | undefined {
  const worst = worstSeverity(issues ?? []);
  if (worst === 'error') return 'ring-2 ring-red-500';
  if (worst === 'warning') return 'ring-1 ring-amber-500';
  return undefined;
}
