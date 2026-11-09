import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { peerName } from '@/lib/network-utils';
import { useNetworkStore } from '@/stores/network-store';

/** One fixed toast id per sync, so a repeated event updates the toast in place. */
export const incomingSyncToastId = (syncId: string): string => `network-sync-offer-${syncId}`;

interface IncomingSyncToastProps {
  syncId: string;
  kind: 'review' | 'confirm';
  displayName: string;
  shortId: string;
  collectionName: string;
}

// Nothing is saved before the user answers in the dialog this toast opens.
export function IncomingSyncToast({
  syncId,
  kind,
  displayName,
  shortId,
  collectionName,
}: IncomingSyncToastProps) {
  const name = peerName(displayName);
  const title =
    kind === 'review' ? `${name} wants to sync a collection` : `${name} has changes ready to apply`;

  return (
    <div className='flex w-[356px] max-w-full flex-col gap-2 rounded-md border bg-popover p-4 text-popover-foreground shadow-lg'>
      <p className='break-words text-sm font-medium'>{title}</p>
      <p className='break-words text-xs text-muted-foreground'>{collectionName}</p>
      <p className='font-mono text-xs text-muted-foreground'>{shortId}</p>
      <div className='flex flex-wrap items-center gap-2'>
        <Button
          size='sm'
          onClick={() => {
            useNetworkStore.getState().openSyncDialog(syncId);
            toast.dismiss(incomingSyncToastId(syncId));
          }}
        >
          Review
        </Button>
      </div>
    </div>
  );
}
