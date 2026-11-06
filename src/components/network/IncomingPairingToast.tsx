import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { useNetworkStore } from '@/stores/network-store';

/** One fixed toast id per pairing, so a repeated event updates the toast in place. */
export const incomingToastId = (pairingId: string): string => `network-pairing-${pairingId}`;

interface IncomingPairingToastProps {
  pairingId: string;
  displayName: string;
  shortId: string;
}

// The code is not in the toast. It shows in the dialog the user opens.
export function IncomingPairingToast({
  pairingId,
  displayName,
  shortId,
}: IncomingPairingToastProps) {
  const dismiss = () => toast.dismiss(incomingToastId(pairingId));

  return (
    <div className='flex w-[356px] max-w-full flex-col gap-2 rounded-md border bg-popover p-4 text-popover-foreground shadow-lg'>
      <p className='break-words text-sm font-medium'>{`${displayName} wants to pair`}</p>
      <p className='font-mono text-xs text-muted-foreground'>{shortId}</p>
      <div className='flex flex-wrap items-center gap-2'>
        <Button
          size='sm'
          onClick={() => {
            useNetworkStore.getState().openPairingDialog(pairingId);
            dismiss();
          }}
        >
          Review
        </Button>
        <Button size='sm' variant='ghost' onClick={dismiss}>
          Later
        </Button>
      </div>
    </div>
  );
}
