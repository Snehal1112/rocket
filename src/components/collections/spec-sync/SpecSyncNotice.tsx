import { RefreshCw } from 'lucide-react';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { useOpenApiSyncEvents, useOpenApiSyncStatus } from '@/lib/queries/openapi-sync-queries';
import { usePaneStore } from '@/stores/pane-store';

interface SpecSyncNoticeProps {
  collection: string;
}

/** Tells the user that the spec changed, and opens the Spec sync tab. */
export function SpecSyncNotice({ collection }: SpecSyncNoticeProps) {
  useOpenApiSyncEvents();
  const { data: status } = useOpenApiSyncStatus(collection);
  const openCollectionTab = usePaneStore((s) => s.openCollectionTab);
  if (!status || status.lastStatus?.state !== 'changed' || !status.hasPending) return null;
  return (
    <Alert>
      <RefreshCw className='mt-0.5 h-4 w-4 shrink-0' aria-hidden='true' />
      <div className='flex flex-1 flex-col gap-1'>
        <p className='font-semibold'>The API spec has changed.</p>
        <p className='text-muted-foreground'>Review what would change in this collection.</p>
        <div>
          <Button
            size='sm'
            variant='outline'
            onClick={() => openCollectionTab(collection, 'spec-sync')}
          >
            Review changes
          </Button>
        </div>
      </div>
    </Alert>
  );
}
