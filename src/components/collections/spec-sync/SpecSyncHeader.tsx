import { Badge } from '@/components/ui/badge';
import { formatRelativeTime } from '@/lib/relative-time';
import type { OpenApiSyncStatus } from '@/lib/tauri-api';
import { intervalLabel } from './specSyncView';

interface SpecSyncHeaderProps {
  status: OpenApiSyncStatus;
}

/** Where the spec comes from, when it was last checked and how often. */
export function SpecSyncHeader({ status }: SpecSyncHeaderProps) {
  const failed = status.lastStatus?.state === 'failed';
  return (
    <div className='flex flex-col gap-1 border-b border-border/70 px-4 py-3'>
      <p className='break-all text-sm font-medium'>{status.location}</p>
      <div className='flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground'>
        <span>
          {status.lastCheckedAt
            ? `Last checked ${formatRelativeTime(status.lastCheckedAt)}`
            : 'Not checked yet'}
        </span>
        <span>Check interval: {intervalLabel(status.interval)}</span>
        {failed && (
          <Badge variant='destructive'>
            Last check failed{status.lastStatus?.code ? `: ${status.lastStatus.code}` : ''}
          </Badge>
        )}
      </div>
    </div>
  );
}
