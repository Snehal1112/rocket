import { Button } from '@/components/ui/button';
import { useOpenApiSyncSkipped, useUnskip } from '@/lib/queries/openapi-sync-queries';
import { syncErrorMessage } from './syncErrors';

const LOAD_FAILED = 'The skipped items could not be loaded.';

/** The items the user skipped, each with a way to take the skip back. */
export function OpenApiSkippedList({ collection }: { collection: string }) {
  const skipped = useOpenApiSyncSkipped(collection, true);
  const unskip = useUnskip(collection);
  if (skipped.isError) {
    return <p className='px-1 text-xs text-destructive'>{LOAD_FAILED}</p>;
  }
  const rows = skipped.data;
  if (!rows) return <p className='px-1 text-xs text-muted-foreground'>Loading...</p>;
  if (rows.length === 0) {
    return <p className='px-1 text-xs text-muted-foreground'>Nothing is skipped.</p>;
  }
  return (
    <div className='flex flex-col gap-1 px-1'>
      <ul className='flex flex-col gap-1'>
        {rows.map((row) => (
          <li key={row.key} className='flex items-center justify-between gap-2 text-xs'>
            <span className='flex min-w-0 flex-col'>
              <span className='truncate font-medium'>{row.name}</span>
              {(row.method || row.path) && (
                <span className='flex gap-1 truncate text-muted-foreground'>
                  <span>{row.method}</span>
                  <span className='truncate'>{row.path}</span>
                </span>
              )}
            </span>
            <Button
              variant='outline'
              size='sm'
              className='h-6 shrink-0 px-2 text-xs'
              disabled={unskip.isPending}
              onClick={() => unskip.mutate([row.key])}
            >
              Un-skip
            </Button>
          </li>
        ))}
      </ul>
      <Button
        variant='outline'
        size='sm'
        className='h-6 self-start px-2 text-xs'
        disabled={unskip.isPending}
        onClick={() => unskip.mutate(rows.map((r) => r.key))}
      >
        {`Un-skip all (${rows.length})`}
      </Button>
      {unskip.isError && (
        <p role='alert' className='text-xs text-destructive'>
          {syncErrorMessage(unskip.error)}
        </p>
      )}
    </div>
  );
}
