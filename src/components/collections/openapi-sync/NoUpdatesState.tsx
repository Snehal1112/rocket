import { EmptyState } from '@/components/ui/empty-state';
import { formatRelativeTime } from '@/lib/relative-time';
import type { OpenApiSyncStatus } from '@/lib/tauri-api';
import { useCheckRunner } from './CheckNowButton';

/** The Spec updates empty state, with when the spec was last checked and a way to check. */
export function NoUpdatesState({
  collection,
  status,
}: {
  collection: string;
  status: OpenApiSyncStatus;
}) {
  const runner = useCheckRunner(collection, status.consent);
  const lastChecked = status.lastCheckedAt
    ? `Last checked ${formatRelativeTime(status.lastCheckedAt)}.`
    : 'Not checked yet.';
  return (
    <EmptyState
      cause='none-yet'
      size='compact'
      title='No spec updates waiting'
      reason={
        <>
          When the spec changes, the changes show up here for you to review.
          <span className='block'>{lastChecked}</span>
          {runner.text && (
            <span role='status' className='block'>
              {runner.text}
            </span>
          )}
        </>
      }
      action={
        runner.granted
          ? { label: runner.pending ? 'Checking...' : 'Check now', onClick: runner.run }
          : undefined
      }
    />
  );
}
