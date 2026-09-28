import { ExternalLink } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import type { SavedRequestEntry } from '@/lib/flow-node-edits';
import { openSavedRequestTab } from './openSavedRequest';
import { RequestPicker } from './RequestPicker';

// A saved request's content is edited in its own tab, never from the flow.
// Here the node can be repointed, opened, or copied into an inline request.
export function SavedSourceEditor({
  requestPath,
  collection,
  onPick,
  onConvertToInline,
  converting,
}: {
  requestPath: string;
  collection: string;
  onPick: (entry: SavedRequestEntry) => void;
  onConvertToInline: () => void;
  converting: boolean;
}) {
  const [openError, setOpenError] = useState<string | null>(null);
  const [otherCollection, setOtherCollection] = useState(false);

  const open = () => {
    setOpenError(null);
    setOtherCollection(false);
    openSavedRequestTab(collection, requestPath)
      .then((result) => setOtherCollection(result === 'other-collection'))
      .catch((err) => setOpenError(String(err)));
  };

  return (
    <div className='space-y-2'>
      <div className='space-y-1'>
        <span className='text-xs font-medium'>Saved request</span>
        <p
          data-testid='saved-request-path'
          className='truncate rounded border bg-muted px-2 py-1 font-mono text-xs'
        >
          {requestPath}
        </p>
      </div>
      <div className='flex flex-wrap gap-2'>
        <RequestPicker collection={collection} triggerLabel='Choose request…' onPick={onPick} />
        <Button
          type='button'
          variant='outline'
          size='sm'
          className='h-7 gap-1 text-xs'
          onClick={open}
        >
          <ExternalLink className='h-3 w-3' aria-hidden='true' />
          Open request
        </Button>
        <Button
          type='button'
          variant='outline'
          size='sm'
          className='h-7 text-xs'
          disabled={converting}
          onClick={onConvertToInline}
        >
          Convert to inline
        </Button>
      </div>
      {otherCollection && (
        <p role='status' className='text-xs text-muted-foreground'>
          This request is in collection "{collection}". Switch to that collection to open it.
        </p>
      )}
      {openError && (
        <p role='alert' className='text-xs text-red-600'>
          Could not open the request: {openError}
        </p>
      )}
    </div>
  );
}
