import { CheckCircle2, TriangleAlert } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { useOpenApiSyncChanges } from '@/lib/queries/openapi-sync-queries';
import { formatRelativeTime } from '@/lib/relative-time';
import type { OpenApiSyncStatus } from '@/lib/tauri-api';
import { useCheckRunner } from './CheckNowButton';
import { intervalLabel } from './openApiSyncView';

interface NoUpdatesStateProps {
  collection: string;
  status: OpenApiSyncStatus;
  /** Opens the Your changes tab. */
  onOpenChanges?: () => void;
}

/** The Spec updates empty state: the spec is in step, when it was checked, and where to go next. */
export function NoUpdatesState({ collection, status, onOpenChanges }: NoUpdatesStateProps) {
  const runner = useCheckRunner(collection, status.consent);
  const changes = useOpenApiSyncChanges(collection);
  // While the last check failed the banner above says why, and a calm line would contradict it.
  const failed = status.lastStatus?.state === 'failed' && status.consent === 'granted';
  const lastChecked = status.lastCheckedAt
    ? `Last checked ${formatRelativeTime(status.lastCheckedAt)}.`
    : 'Not checked yet.';
  const changeCount = changes.data
    ? changes.data.modified.length +
      changes.data.added.length +
      changes.data.deleted.length +
      changes.data.backups.length
    : 0;
  const Icon = failed ? TriangleAlert : CheckCircle2;
  return (
    <div className='p-4'>
      <section className='flex max-w-xl flex-col gap-4 rounded-sm border border-border p-4'>
        <div className='flex items-start gap-3'>
          <Icon
            aria-hidden='true'
            className={`mt-0.5 h-5 w-5 shrink-0 ${failed ? 'text-warning' : 'text-[hsl(var(--success))]'}`}
          />
          <div className='flex min-w-0 flex-col gap-1'>
            <h3 className='text-sm font-semibold'>
              {failed ? 'Updates could not be checked' : 'No spec updates waiting'}
            </h3>
            <p className='text-sm text-muted-foreground'>
              {failed
                ? 'Unable to check for updates. Fix the connection issue above and check again.'
                : 'When the spec changes, the changes show up here for you to review.'}
            </p>
          </div>
        </div>
        {!failed && (
          <div className='flex flex-col gap-1 border-t border-border pt-3 text-xs text-muted-foreground'>
            <span>{lastChecked}</span>
            <span>{`Automatic checks: ${intervalLabel(status.interval)}.`}</span>
          </div>
        )}
        {runner.text && (
          <p role='status' className='text-xs text-muted-foreground'>
            {runner.text}
          </p>
        )}
        <div className='flex flex-wrap items-center gap-2'>
          {runner.granted && (
            <Button size='sm' onClick={runner.run} disabled={runner.pending}>
              {runner.pending ? 'Checking...' : 'Check now'}
            </Button>
          )}
          {onOpenChanges && changeCount > 0 && (
            <Button variant='outline' size='sm' onClick={onOpenChanges}>
              {`Review your changes (${changeCount})`}
            </Button>
          )}
        </div>
      </section>
    </div>
  );
}
