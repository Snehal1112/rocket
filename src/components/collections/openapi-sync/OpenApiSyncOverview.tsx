import { type ReactNode, useState } from 'react';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { StateBlock } from '@/components/ui/state-block';
import { useOpenApiSyncOverview } from '@/lib/queries/openapi-sync-queries';
import { formatRelativeTime } from '@/lib/relative-time';
import type { OpenApiSyncStatus } from '@/lib/tauri-api';
import { OpenApiSyncConnectionDialog } from './OpenApiSyncConnectionDialog';
import { intervalLabel, isFileLocation, splitLocation } from './openApiSyncView';
import { syncErrorMessage } from './syncErrors';

interface OpenApiSyncOverviewProps {
  collection: string;
  status: OpenApiSyncStatus;
  /** Opens the Spec updates tab. */
  onReview: () => void;
}

function StatCard({ label, value }: { label: string; value: number | null }) {
  return (
    <div className='flex flex-col rounded-md border border-border/70 px-3 py-2'>
      <span className='text-xs text-muted-foreground'>{label}</span>
      <span className='text-lg font-semibold leading-tight'>{value === null ? '–' : value}</span>
    </div>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className='grid grid-cols-[10rem_1fr] gap-3 py-2 text-sm'>
      <dt className='text-muted-foreground'>{label}</dt>
      <dd className='min-w-0 break-all'>{children}</dd>
    </div>
  );
}

/** The counts, the waiting-updates banner and the connection facts of a synced collection. */
export function OpenApiSyncOverview({ collection, status, onReview }: OpenApiSyncOverviewProps) {
  const overview = useOpenApiSyncOverview(collection);
  const [showFull, setShowFull] = useState(false);
  const [dialogOpen, setDialogOpen] = useState(false);
  const { short, full } = splitLocation(status.location);

  if (overview.isError) {
    return <StateBlock tone='error'>{syncErrorMessage(overview.error)}</StateBlock>;
  }
  const o = overview.data;
  if (!o) return <StateBlock tone='loading'>Loading the overview...</StateBlock>;

  const unknown = o.inSync === null || o.changedByYou === null;
  const failed = status.lastStatus?.state === 'failed';
  return (
    <div className='flex flex-col gap-4'>
      {o.updatesWaiting > 0 && (
        <div className='flex flex-wrap items-center justify-between gap-2 rounded-md border border-border bg-muted/40 px-3 py-2 text-sm'>
          <span>
            {o.updatesWaiting === 1
              ? '1 update is waiting.'
              : `${o.updatesWaiting} updates are waiting.`}
          </span>
          <Button size='sm' onClick={onReview}>
            Review updates
          </Button>
        </div>
      )}
      <div className='grid grid-cols-2 gap-3 md:grid-cols-4'>
        <StatCard label='Endpoints' value={o.endpoints} />
        <StatCard label='In sync with spec' value={o.inSync} />
        <StatCard label='Changed by you' value={o.changedByYou} />
        <StatCard label='Spec updates waiting' value={o.updatesWaiting} />
      </div>
      {unknown && (
        <p className='text-xs text-muted-foreground'>
          Sync data is from an older version, so these counts are unknown.
        </p>
      )}
      <dl className='divide-y divide-border/70'>
        <Fact label='Spec link'>
          {isFileLocation(status.location) && (
            <Badge variant='secondary' className='mr-2'>
              Local file
            </Badge>
          )}
          <span>{full && showFull ? full : short}</span>
          {full && (
            <Button
              variant='link'
              size='sm'
              className='ml-2 h-auto p-0 text-xs'
              onClick={() => setShowFull((v) => !v)}
            >
              {showFull ? 'Hide full link' : 'Show full link'}
            </Button>
          )}
        </Fact>
        <Fact label='Spec version'>{o.specVersion ?? '–'}</Fact>
        <Fact label='Last checked'>
          {status.lastCheckedAt
            ? `${formatRelativeTime(status.lastCheckedAt)}.${failed ? '' : ' No problems.'}`
            : 'Not checked yet'}
        </Fact>
        <Fact label='Automatic checks'>
          <span>{intervalLabel(status.interval)}</span>
          <Button
            variant='link'
            size='sm'
            className='ml-2 h-auto p-0 text-xs'
            onClick={() => setDialogOpen(true)}
          >
            Connection settings
          </Button>
        </Fact>
        <Fact label='Last synced'>
          {o.lastSyncedAt
            ? `${formatRelativeTime(o.lastSyncedAt)}, ${o.lastSyncedUpdated} ${o.lastSyncedUpdated === 1 ? 'request' : 'requests'} updated`
            : 'Never'}
        </Fact>
      </dl>
      <OpenApiSyncConnectionDialog
        collection={collection}
        status={status}
        open={dialogOpen}
        onOpenChange={setDialogOpen}
      />
    </div>
  );
}
