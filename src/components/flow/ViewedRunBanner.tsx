import { History } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { formatClock } from '@/lib/flow-run-history';
import type { FlowRunRecord } from '@/types/pane-types';

interface ViewedRunBannerProps {
  record: FlowRunRecord;
  onBack: () => void;
}

// Shown while a past run's results replace the live ones on the canvas.
export function ViewedRunBanner({ record, onBack }: ViewedRunBannerProps) {
  return (
    <div
      role='note'
      data-testid='viewed-run-banner'
      className='nokey flex max-w-full items-center gap-2 rounded-md border border-amber-500/50 bg-amber-500/10 px-2.5 py-1 text-xs'
    >
      <History className='h-3.5 w-3.5 shrink-0 text-amber-600' aria-hidden='true' />
      <span className='truncate'>
        Viewing the run from {formatClock(record.finishedAt)}. The graph may have changed since.
      </span>
      <Button
        type='button'
        size='sm'
        variant='outline'
        className='h-6 px-2 text-xs'
        onClick={onBack}
      >
        Back to latest
      </Button>
    </div>
  );
}
