import { Badge } from '@/components/ui/badge';
import { msToSecondsLabel } from '@/lib/flow-repeat';
import type { FlowNode, FlowNodeStatus } from '@/lib/tauri-api';
import type { FlowNodeDetail } from '@/types/pane-types';

interface LastRunTabProps {
  node: FlowNode;
  status: FlowNodeStatus;
  detail?: FlowNodeDetail;
}

// The badge text for a status. A branch that was not taken is a skip too,
// but people read it as its own outcome.
function statusLabel(status: FlowNodeStatus, detail?: FlowNodeDetail): string {
  switch (status) {
    case 'success':
      return 'Success';
    case 'failed':
      return 'Failed';
    case 'running':
      return 'Running';
    case 'skipped':
      return detail?.skipReason === 'branch_not_taken' ? 'Not taken' : 'Skipped';
    case 'idle':
      return 'Not run';
  }
}

const badgeClass: Record<FlowNodeStatus, string> = {
  idle: '',
  running: 'bg-blue-500/15 text-blue-600',
  success: 'bg-green-500/15 text-green-600',
  failed: 'bg-red-500/15 text-red-600',
  skipped: 'bg-muted text-muted-foreground',
};

function attemptsLabel(n: number): string {
  return n === 1 ? '1 attempt' : `${n} attempts`;
}

// Duration reads as milliseconds for a single send and seconds for a poll,
// matching the Request card.
function timingParts(detail?: FlowNodeDetail): string[] {
  if (!detail) return [];
  const parts: string[] = [];
  if (detail.statusCode !== undefined) parts.push(String(detail.statusCode));
  if (detail.attempts !== undefined) {
    parts.push(attemptsLabel(detail.attempts));
    if (detail.durationMs !== undefined) parts.push(msToSecondsLabel(detail.durationMs));
  } else if (detail.durationMs !== undefined) {
    parts.push(`${detail.durationMs}ms`);
  }
  return parts;
}

function skipText(detail?: FlowNodeDetail): string {
  return detail?.skipReason === 'branch_not_taken'
    ? 'Its branch was not taken.'
    : 'An earlier node failed.';
}

export function LastRunTab({ node, status, detail }: LastRunTabProps) {
  if (status === 'idle') {
    return (
      <p className='text-xs text-muted-foreground'>
        Not run yet. Run the flow to see results here.
      </p>
    );
  }

  const parts = status === 'running' ? [] : timingParts(detail);

  return (
    <div data-node-id={node.id} className='space-y-3 text-xs'>
      <div data-testid='last-run-status' className='flex flex-wrap items-center gap-1.5'>
        <Badge variant='secondary' className={badgeClass[status]}>
          {statusLabel(status, detail)}
        </Badge>
        {status === 'running' && detail?.progress && (
          <span className='text-muted-foreground'>{detail.progress}</span>
        )}
        {parts.length > 0 && <span className='text-muted-foreground'>{parts.join(' · ')}</span>}
      </div>
      {status === 'failed' && detail?.error && (
        <div
          data-testid='last-run-error'
          className='select-text whitespace-pre-wrap break-words rounded-md border border-red-500/40 bg-red-500/5 p-2 text-red-600'
        >
          {detail.error}
        </div>
      )}
      {status === 'skipped' && <p className='text-muted-foreground'>{skipText(detail)}</p>}
    </div>
  );
}
