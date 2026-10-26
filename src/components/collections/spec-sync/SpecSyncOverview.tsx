import { formatRelativeTime } from '@/lib/relative-time';
import type { OpenApiSyncPlan, OpenApiSyncStatus } from '@/lib/tauri-api';

interface SpecSyncOverviewProps {
  plan: OpenApiSyncPlan;
  status: OpenApiSyncStatus;
}

/** Whether a changed request carries a field the user edited themselves. */
function locallyEdited(item: OpenApiSyncPlan['items'][number]): boolean {
  return (
    item.kind === 'changed' &&
    item.changes.some((c) => c.outcome === 'keepYours' || c.outcome === 'conflict')
  );
}

function Stat({ id, label, value }: { id: string; label: string; value: number }) {
  return (
    <div className='flex flex-col'>
      <span data-testid={id} className='text-lg font-semibold leading-tight'>
        {value}
      </span>
      <span className='text-xs text-muted-foreground'>{label}</span>
    </div>
  );
}

/** A counts strip for the pending spec, with the last checked time. */
export function SpecSyncOverview({ plan, status }: SpecSyncOverviewProps) {
  return (
    <div className='flex flex-wrap items-end gap-x-8 gap-y-2 border-b border-border/70 px-4 py-3'>
      <Stat id='overview-in-sync' label='Endpoints in sync' value={plan.unchanged.length} />
      <Stat
        id='overview-edited'
        label='Changed in your collection'
        value={plan.items.filter(locallyEdited).length}
      />
      <Stat id='overview-pending' label='Spec updates pending' value={plan.items.length} />
      <span className='ml-auto text-xs text-muted-foreground'>
        {status.lastCheckedAt
          ? `Last checked ${formatRelativeTime(status.lastCheckedAt)}`
          : 'Not checked yet'}
      </span>
    </div>
  );
}
