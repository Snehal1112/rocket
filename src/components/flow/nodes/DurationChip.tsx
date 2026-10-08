import { Timer } from 'lucide-react';
import { Badge } from '@/components/ui/badge';
import { msToSecondsLabel } from '@/lib/flow-repeat';

// Milliseconds under a second, seconds from a second, like the Request card.
export function formatDuration(ms: number): string {
  return ms < 1000 ? `${ms}ms` : msToSecondsLabel(ms);
}

// How long a node's last run took. Renders nothing before a run.
export function DurationChip({ durationMs }: { durationMs?: number }) {
  if (durationMs === undefined) return null;
  const text = formatDuration(durationMs);
  return (
    <div className='px-2 pt-1'>
      <Badge
        variant='outline'
        data-testid='duration-chip'
        className='gap-0.5 px-1 py-0 text-[10px] font-normal text-muted-foreground'
      >
        <Timer className='h-2.5 w-2.5' aria-hidden='true' />
        {text}
      </Badge>
    </div>
  );
}
