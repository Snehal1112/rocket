import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { formatBytes, peerName } from '@/lib/network-utils';
import { useNetworkStore } from '@/stores/network-store';

/** One fixed toast id per offer, so a repeated event updates the toast in place. */
export const incomingTransferToastId = (transferId: string): string =>
  `network-transfer-offer-${transferId}`;

interface IncomingTransferToastProps {
  transferId: string;
  displayName: string;
  shortId: string;
  collectionName: string;
  sizeBytes: number;
}

// Nothing is saved before the user presses Accept in the dialog this toast opens.
export function IncomingTransferToast({
  transferId,
  displayName,
  shortId,
  collectionName,
  sizeBytes,
}: IncomingTransferToastProps) {
  const dismiss = () => toast.dismiss(incomingTransferToastId(transferId));

  return (
    <div className='flex w-[356px] max-w-full flex-col gap-2 rounded-md border bg-popover p-4 text-popover-foreground shadow-lg'>
      <p className='break-words text-sm font-medium'>
        {`${peerName(displayName)} wants to send you a collection`}
      </p>
      <p className='break-words text-xs text-muted-foreground'>
        {`${collectionName} (${formatBytes(sizeBytes)})`}
      </p>
      <p className='font-mono text-xs text-muted-foreground'>{shortId}</p>
      <div className='flex flex-wrap items-center gap-2'>
        <Button
          size='sm'
          onClick={() => {
            useNetworkStore.getState().openTransferDialog(transferId);
            dismiss();
          }}
        >
          Review
        </Button>
      </div>
    </div>
  );
}
