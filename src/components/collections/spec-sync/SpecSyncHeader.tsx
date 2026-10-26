import { MoreHorizontal } from 'lucide-react';
import { useState } from 'react';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useCheckNow } from '@/lib/queries/openapi-sync-queries';
import { formatRelativeTime } from '@/lib/relative-time';
import type { OpenApiSyncStatus } from '@/lib/tauri-api';
import { CheckNowButton } from './CheckNowButton';
import { SpecSyncConnectionDialog } from './SpecSyncConnectionDialog';
import { intervalLabel, splitLocation } from './specSyncView';

interface SpecSyncHeaderProps {
  collection: string;
  status: OpenApiSyncStatus;
}

/** Where the spec comes from, when it was last checked and how often, with Check now. */
export function SpecSyncHeader({ collection, status }: SpecSyncHeaderProps) {
  const [showFull, setShowFull] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const check = useCheckNow(collection);
  const failed = status.lastStatus?.state === 'failed';
  const { short, full } = splitLocation(status.location);
  return (
    <div className='flex flex-col gap-2 border-b border-border/70 px-4 py-3'>
      <div className='flex flex-wrap items-start justify-between gap-3'>
        <div className='flex min-w-0 flex-col gap-1'>
          <div className='flex items-center gap-1'>
            <h2 className='text-sm font-semibold'>Spec sync</h2>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant='ghost'
                  size='icon'
                  className='h-6 w-6'
                  aria-label='Spec sync actions'
                >
                  <MoreHorizontal aria-hidden='true' className='h-4 w-4' />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align='start'>
                <DropdownMenuItem onSelect={() => setDialogOpen(true)}>
                  Edit connection settings
                </DropdownMenuItem>
                <DropdownMenuItem
                  disabled={status.consent !== 'granted' || check.isPending}
                  onSelect={() => check.mutate()}
                >
                  Check now
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
          <p className='break-all text-sm font-medium'>{full && showFull ? full : short}</p>
          {full && (
            <div>
              <Button
                variant='link'
                size='sm'
                className='h-auto p-0 text-xs'
                onClick={() => setShowFull((v) => !v)}
              >
                {showFull ? 'Hide full link' : 'Show full link'}
              </Button>
            </div>
          )}
          <div className='flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground'>
            <span>
              {status.lastCheckedAt
                ? `Last checked ${formatRelativeTime(status.lastCheckedAt)}`
                : 'Not checked yet'}
            </span>
            <span>Check interval: {intervalLabel(status.interval)}</span>
          </div>
        </div>
        <CheckNowButton collection={collection} consent={status.consent} />
      </div>
      {failed && (
        <Alert variant='destructive'>
          <p>
            {status.lastMessage ?? 'The last check failed.'}
            {status.lastStatus?.code ? ` (${status.lastStatus.code})` : ''}
          </p>
        </Alert>
      )}
      <SpecSyncConnectionDialog
        collection={collection}
        status={status}
        open={dialogOpen}
        onOpenChange={setDialogOpen}
      />
    </div>
  );
}
