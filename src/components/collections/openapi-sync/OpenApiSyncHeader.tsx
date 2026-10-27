import { MoreHorizontal } from 'lucide-react';
import { useState } from 'react';
import { Alert } from '@/components/ui/alert';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { formatRelativeTime } from '@/lib/relative-time';
import type { OpenApiSyncStatus } from '@/lib/tauri-api';
import { CheckNowButtonView, useCheckRunner } from './CheckNowButton';
import { type ChipState, chipState } from './chipState';
import { DisconnectSyncDialog } from './DisconnectSyncDialog';
import { OpenApiSyncConnectionDialog } from './OpenApiSyncConnectionDialog';
import { intervalLabel, splitLocation } from './openApiSyncView';

interface OpenApiSyncHeaderProps {
  collection: string;
  status: OpenApiSyncStatus;
  /** Spec updates that wait, as counted from the plan. Null while the count is not known. */
  waiting: number | null;
}

const CHIP_VARIANT: Record<ChipState['kind'], 'warning' | 'destructive' | 'secondary' | 'outline'> =
  {
    'needs-approval': 'warning',
    failed: 'destructive',
    updates: 'warning',
    off: 'secondary',
    connected: 'outline',
  };

/** The collection, its sync status chip, where the spec comes from and Check now. */
export function OpenApiSyncHeader({ collection, status, waiting }: OpenApiSyncHeaderProps) {
  const [showFull, setShowFull] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [disconnectOpen, setDisconnectOpen] = useState(false);
  const runner = useCheckRunner(collection, status.consent);
  const chip = chipState(status, waiting);
  const failed = status.lastStatus?.state === 'failed';
  const { short, full } = splitLocation(status.location);
  return (
    <div className='flex flex-col gap-2 border-b border-border/70 px-4 py-3'>
      <div className='flex flex-wrap items-start justify-between gap-3'>
        <div className='flex min-w-0 flex-col gap-1'>
          <div className='flex items-center gap-1'>
            <h2 className='truncate text-sm font-semibold'>{collection}</h2>
            <Badge variant={CHIP_VARIANT[chip.kind]} title={chip.detail}>
              {chip.label}
            </Badge>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button
                  variant='ghost'
                  size='icon'
                  className='h-6 w-6'
                  aria-label='OpenAPI sync actions'
                >
                  <MoreHorizontal aria-hidden='true' className='h-4 w-4' />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align='start'>
                <DropdownMenuItem onSelect={() => setDialogOpen(true)}>
                  Edit connection settings
                </DropdownMenuItem>
                <DropdownMenuItem onSelect={() => setDisconnectOpen(true)}>
                  Disconnect
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
        <CheckNowButtonView runner={runner} />
      </div>
      {failed && (
        <Alert variant='destructive'>
          <p>
            {status.lastMessage ?? 'The last check failed.'}
            {status.lastStatus?.code ? ` (${status.lastStatus.code})` : ''}
          </p>
        </Alert>
      )}
      <DisconnectSyncDialog
        collection={collection}
        open={disconnectOpen}
        onOpenChange={setDisconnectOpen}
      />
      <OpenApiSyncConnectionDialog
        collection={collection}
        status={status}
        open={dialogOpen}
        onOpenChange={setDialogOpen}
      />
    </div>
  );
}
