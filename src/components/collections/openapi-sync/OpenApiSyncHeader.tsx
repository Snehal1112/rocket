import { Clock, FileText, Link2, Settings2, Timer } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { formatRelativeTime } from '@/lib/relative-time';
import type { OpenApiSyncStatus } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { CheckNowButtonView, useCheckRunner } from './CheckNowButton';
import { type ChipState, chipState } from './chipState';
import { OpenApiSyncConnectionDialog } from './OpenApiSyncConnectionDialog';
import { intervalLabel, isFileLocation, splitLocation } from './openApiSyncView';
import { SyncBadge, type SyncTone } from './SyncBadge';

interface OpenApiSyncHeaderProps {
  collection: string;
  status: OpenApiSyncStatus;
  /** Spec updates that wait, as counted from the plan. Null while the count is not known. */
  waiting: number | null;
  /** Opens the read-only spec viewer. */
  onViewSpec?: () => void;
}

const CHIP_TONE: Record<ChipState['kind'], SyncTone> = {
  'needs-approval': 'warning',
  failed: 'danger',
  updates: 'warning',
  off: 'neutral',
  connected: 'success',
};

/** The collection, its sync status chip, where the spec comes from and the sync actions. */
export function OpenApiSyncHeader({
  collection,
  status,
  waiting,
  onViewSpec,
}: OpenApiSyncHeaderProps) {
  const [showFull, setShowFull] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const runner = useCheckRunner(collection, status.consent);
  const chip = chipState(status, waiting);
  const openDialog = () => setDialogOpen(true);
  const { short, full } = splitLocation(status.location);
  const fileSource = isFileLocation(status.location);
  const expanded = Boolean(full && showFull);
  return (
    <div className='flex flex-col gap-1.5 border-b border-border px-4 py-3'>
      <div className='flex flex-wrap items-start justify-between gap-x-4 gap-y-2'>
        <div className='flex min-w-0 items-center gap-2 self-center'>
          <h2 className='truncate text-base font-semibold'>{collection}</h2>
          <SyncBadge tone={CHIP_TONE[chip.kind]} dot title={chip.detail}>
            {chip.label}
          </SyncBadge>
        </div>
        {/* Top aligned, so the check status under Check now does not push the other buttons. */}
        <div className='flex flex-wrap items-start gap-2'>
          {/* Disconnect is in this dialog too, so the header needs no menu. */}
          <Button variant='outline' size='sm' onClick={() => openDialog()}>
            <Settings2 aria-hidden='true' className='mr-1.5 h-3.5 w-3.5' />
            Connection settings
          </Button>
          {onViewSpec && (
            <Button variant='outline' size='sm' onClick={onViewSpec}>
              View spec
            </Button>
          )}
          <CheckNowButtonView runner={runner} />
        </div>
      </div>
      <div className='flex min-w-0 items-center gap-2 text-xs text-muted-foreground'>
        {fileSource ? (
          <FileText aria-hidden='true' className='h-3.5 w-3.5 shrink-0' />
        ) : (
          <Link2 aria-hidden='true' className='h-3.5 w-3.5 shrink-0' />
        )}
        {fileSource && <SyncBadge tone='info'>Local file</SyncBadge>}
        {/* The tooltip is the short form, so a link with credentials never shows them. */}
        <p className={cn('min-w-0 font-mono', expanded ? 'break-all' : 'truncate')} title={short}>
          {expanded ? full : short}
        </p>
        {full && (
          <Button
            variant='link'
            size='sm'
            className='h-auto shrink-0 p-0 text-xs'
            onClick={() => setShowFull((v) => !v)}
          >
            {showFull ? 'Hide full link' : 'Show full link'}
          </Button>
        )}
      </div>
      <div className='flex flex-wrap items-center gap-x-4 gap-y-0.5 text-xs text-muted-foreground'>
        <span className='inline-flex items-center gap-1.5'>
          <Clock aria-hidden='true' className='h-3.5 w-3.5' />
          <span>
            {status.lastCheckedAt
              ? `Last checked ${formatRelativeTime(status.lastCheckedAt)}`
              : 'Not checked yet'}
          </span>
        </span>
        <span className='inline-flex items-center gap-1.5'>
          <Timer aria-hidden='true' className='h-3.5 w-3.5' />
          <span>Check interval: {intervalLabel(status.interval)}</span>
        </span>
      </div>
      <OpenApiSyncConnectionDialog
        collection={collection}
        status={status}
        open={dialogOpen}
        onOpenChange={setDialogOpen}
      />
    </div>
  );
}
