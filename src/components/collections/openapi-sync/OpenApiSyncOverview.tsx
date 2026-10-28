import { Activity, Link2 } from 'lucide-react';
import { type ReactNode, useState } from 'react';
import { Button } from '@/components/ui/button';
import { StateBlock } from '@/components/ui/state-block';
import { useOpenApiSyncOverview } from '@/lib/queries/openapi-sync-queries';
import { formatRelativeTime } from '@/lib/relative-time';
import type { OpenApiSyncStatus } from '@/lib/tauri-api';
import { OpenApiSyncConnectionDialog } from './OpenApiSyncConnectionDialog';
import { intervalLabel, isFileLocation, splitLocation } from './openApiSyncView';
import { SyncBadge } from './SyncBadge';
import { syncErrorMessage } from './syncErrors';

const SUCCESS = 'bg-[hsl(var(--success))]';

interface OpenApiSyncOverviewProps {
  collection: string;
  status: OpenApiSyncStatus;
  /** Opens the Spec updates tab. */
  onReview: () => void;
}

function Legend({
  label,
  value,
  total,
  swatch,
}: {
  label: string;
  value: number | null;
  total: number;
  swatch: string;
}) {
  const share = value !== null && total > 0 ? Math.round((value / total) * 100) : null;
  return (
    <div className='flex items-center gap-2 text-sm'>
      <span aria-hidden='true' className={`h-2 w-2 shrink-0 rounded-full ${swatch}`} />
      <span className='text-muted-foreground'>{label}</span>
      <span className='font-semibold'>{value === null ? '–' : value}</span>
      {share !== null && <span className='text-xs text-muted-foreground'>{share}%</span>}
    </div>
  );
}

/** How the endpoints split between the spec and the user's own changes. */
function SyncBar({
  inSync,
  changed,
  total,
}: {
  inSync: number | null;
  changed: number | null;
  total: number;
}) {
  const known = inSync !== null && changed !== null && total > 0;
  return (
    <div
      role='img'
      aria-label={known ? `${inSync} in sync, ${changed} changed by you` : 'Counts are unknown'}
      className='flex h-2 w-full overflow-hidden rounded-full bg-muted'
    >
      {known && (
        <>
          <div className={SUCCESS} style={{ width: `${(inSync / total) * 100}%` }} />
          <div className='bg-primary' style={{ width: `${(changed / total) * 100}%` }} />
        </>
      )}
    </div>
  );
}

function Fact({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className='grid grid-cols-[8rem_1fr] gap-3 py-1.5 text-sm'>
      <dt className='text-muted-foreground'>{label}</dt>
      <dd className='min-w-0 break-all'>{children}</dd>
    </div>
  );
}

function Panel({ title, icon, children }: { title: string; icon: ReactNode; children: ReactNode }) {
  return (
    <section className='rounded-sm border border-border px-3 py-2'>
      <h3 className='flex items-center gap-2 text-sm font-semibold'>
        {icon}
        {title}
      </h3>
      <dl className='mt-1 divide-y divide-border'>{children}</dl>
    </section>
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
        <div className='flex flex-wrap items-center justify-between gap-2 rounded-sm border border-border bg-muted/40 px-3 py-2 text-sm'>
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
      <div className='grid gap-3 md:grid-cols-[1fr_15rem]'>
        <div className='flex flex-col gap-3 rounded-sm border border-border px-3 py-2'>
          <div className='flex flex-col'>
            <span className='text-xs text-muted-foreground'>Endpoints</span>
            <span className='text-2xl font-semibold leading-tight'>
              {o.endpoints === null ? '–' : o.endpoints}
            </span>
          </div>
          <SyncBar inSync={o.inSync} changed={o.changedByYou} total={o.endpoints} />
          <div className='flex flex-wrap gap-x-6 gap-y-1'>
            <Legend
              label='In sync with spec'
              value={o.inSync}
              total={o.endpoints}
              swatch={SUCCESS}
            />
            <Legend
              label='Changed by you'
              value={o.changedByYou}
              total={o.endpoints}
              swatch='bg-primary'
            />
          </div>
        </div>
        <div
          className={`flex flex-col justify-center rounded-sm border px-3 py-2 ${o.updatesWaiting > 0 ? 'border-warning/60 bg-warning/10' : 'border-border'}`}
        >
          <span className='text-xs text-muted-foreground'>Spec updates waiting</span>
          <span className='text-2xl font-semibold leading-tight'>{o.updatesWaiting}</span>
        </div>
      </div>
      {unknown && (
        <p className='text-xs text-muted-foreground'>
          Sync data is from an older version, so these counts are unknown.
        </p>
      )}
      <div className='grid gap-3 md:grid-cols-2'>
        <Panel
          title='Source'
          icon={<Link2 aria-hidden='true' className='h-4 w-4 text-muted-foreground' />}
        >
          <Fact label='Spec link'>
            {isFileLocation(status.location) && (
              <SyncBadge tone='info' className='mr-2'>
                Local file
              </SyncBadge>
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
        </Panel>
        <Panel
          title='Activity'
          icon={<Activity aria-hidden='true' className='h-4 w-4 text-muted-foreground' />}
        >
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
        </Panel>
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
