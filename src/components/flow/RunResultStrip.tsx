import { Ban, CheckCircle2, Clock, type LucideIcon, XCircle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { formatRunDuration } from '@/lib/flow-run-result';
import { cn } from '@/lib/utils';
import type { FlowLastRun } from '@/types/pane-types';

interface RunResultStripProps {
  result: FlowLastRun;
  // False when the failed node is no longer on the canvas.
  canSelectFailed: boolean;
  onSelectFailed: (nodeId: string) => void;
}

function describeRun(result: FlowLastRun): { text: string; Icon: LucideIcon; tone: string } {
  const took = result.totalMs === null ? '' : ` in ${formatRunDuration(result.totalMs)}`;
  switch (result.stoppedReason) {
    case 'cancelled':
      return { text: `Run cancelled${took}`, Icon: Ban, tone: 'text-muted-foreground' };
    case 'completed':
      return result.failedCount > 0
        ? {
            text: `Run finished with ${result.failedCount} failed${took}`,
            Icon: XCircle,
            tone: 'text-red-600',
          }
        : { text: `Run completed${took}`, Icon: CheckCircle2, tone: 'text-green-600' };
    case 'error':
      return { text: `Run ended with an error${took}`, Icon: XCircle, tone: 'text-red-600' };
    default:
      return {
        text: `Run stopped (${result.stoppedReason})${took}`,
        Icon: Clock,
        tone: 'text-amber-600',
      };
  }
}

// One line under the toolbar: how the last run ended, and which node failed first.
// It is a plain group, not a live region. The run announcer owns spoken updates.
export function RunResultStrip({ result, canSelectFailed, onSelectFailed }: RunResultStripProps) {
  const { text, Icon, tone } = describeRun(result);
  const skipped = result.skippedCount > 0 ? ` · ${result.skippedCount} skipped` : '';
  const failedNodeId = result.failedNodeId;
  const failedLabel = result.failedLabel || failedNodeId;
  return (
    <section
      aria-label='Last run result'
      data-testid='run-result-strip'
      className='nokey flex max-w-full items-center gap-2 rounded-md border bg-card px-2.5 py-1 text-xs shadow-sm'
    >
      <Icon className={cn('h-3.5 w-3.5 shrink-0', tone)} aria-hidden='true' />
      <span className='truncate'>
        {text}
        {skipped}
      </span>
      {failedNodeId && (
        <Button
          type='button'
          variant='link'
          size='sm'
          className='h-auto min-w-0 max-w-44 p-0 text-xs'
          disabled={!canSelectFailed}
          aria-label={`Select failed node ${failedLabel}`}
          onClick={() => onSelectFailed(failedNodeId)}
        >
          <span className='truncate'>Failed at {failedLabel}</span>
        </Button>
      )}
    </section>
  );
}
