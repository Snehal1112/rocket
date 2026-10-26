import { type ReactNode, useState } from 'react';
import { EmptyState } from '@/components/ui/empty-state';
import { StateBlock } from '@/components/ui/state-block';
import {
  useOpenApiSyncEvents,
  useOpenApiSyncPlan,
  useOpenApiSyncStatus,
} from '@/lib/queries/openapi-sync-queries';
import { formatRelativeTime } from '@/lib/relative-time';
import { SpecSyncDetail } from './SpecSyncDetail';
import { SpecSyncHeader } from './SpecSyncHeader';
import { SpecSyncList } from './SpecSyncList';
import { findEntry, firstEntryId, isPendingChangedError, syncErrorMessage } from './specSyncView';

interface SpecSyncTabProps {
  collection: string;
}

/** A read-only view of what the pending spec would change. It never runs a check. */
export function SpecSyncTab({ collection }: SpecSyncTabProps) {
  useOpenApiSyncEvents();
  const status = useOpenApiSyncStatus(collection);
  const planEnabled = !!status.data?.hasPending;
  const plan = useOpenApiSyncPlan(collection, planEnabled);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  // The first entry is selected by default, and again when the chosen one is gone.
  const planData = plan.data;
  const activeId =
    planData && findEntry(planData, selectedId)
      ? selectedId
      : planData
        ? firstEntryId(planData)
        : null;

  if (status.isError) {
    return (
      <div className='p-4'>
        <StateBlock tone='error'>
          The sync state file cannot be read: {syncErrorMessage(status.error)}
        </StateBlock>
      </div>
    );
  }
  if (status.isPending) {
    return (
      <div className='p-4'>
        <StateBlock tone='loading'>Loading spec sync...</StateBlock>
      </div>
    );
  }
  const s = status.data;
  if (!s) return null;

  const noChanges = (
    <EmptyState
      cause='none-yet'
      size='compact'
      title='No changes found'
      reason={
        s.lastCheckedAt
          ? `Last checked ${formatRelativeTime(s.lastCheckedAt)}.`
          : 'The spec has not been checked yet.'
      }
    />
  );

  let body: ReactNode;
  if (!planEnabled) {
    body = noChanges;
  } else if (plan.isError) {
    body = (
      <div className='p-4'>
        <StateBlock tone='error'>
          {isPendingChangedError(plan.error)
            ? 'The pending spec changed, check again'
            : `The plan cannot be built: ${syncErrorMessage(plan.error)}`}
        </StateBlock>
      </div>
    );
  } else if (!planData) {
    body = (
      <div className='p-4'>
        <StateBlock tone='loading'>Loading the plan...</StateBlock>
      </div>
    );
  } else if (!planData.pending) {
    // The plan is the truth when it disagrees with the status flag.
    body = noChanges;
  } else {
    body = (
      <div className='flex min-h-0 flex-1'>
        <div className='w-72 shrink-0 border-r border-border'>
          <SpecSyncList plan={planData} selectedId={activeId} onSelect={setSelectedId} />
        </div>
        <div className='min-w-0 flex-1'>
          <SpecSyncDetail plan={planData} selectedId={activeId} />
        </div>
      </div>
    );
  }

  return (
    <div className='flex h-full flex-col overflow-hidden'>
      <SpecSyncHeader status={s} />
      <div className='flex min-h-0 flex-1 flex-col'>{body}</div>
    </div>
  );
}
