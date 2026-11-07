import { ShieldAlert } from 'lucide-react';
import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { StateBlock } from '@/components/ui/state-block';
import { useAsyncAction } from '@/hooks/useAsyncAction';
import { formatBytes, peerName } from '@/lib/network-utils';
import { cancelNetworkTransfer, respondNetworkTransfer } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

// A transfer that ends faster than this never flashes a progress line.
const PROGRESS_DELAY_MS = 300;

// There is no script gate: a received collection has no trust record, so the trust gate keeps
// the dangerous capabilities off, but its scripts run in the Safe sandbox (spec decision 6).
// The offer has no script count, so the warning has none.
const SCRIPT_WARNING =
  "This collection's scripts will run in the Safe sandbox when you send its requests. Developer mode, host environment and vault access stay off until you allow them.";

const store = () => useNetworkStore.getState();

/**
 * The dialog for an offered collection. It shows the sender, the short device id, the
 * collection name, the size and the script warning, with Accept and Decline. After Accept it
 * shows that the collection is arriving, with Cancel. When the offer is gone it shows why
 * (expired, or the end reason the bridge stored). Mount it once.
 */
export function IncomingTransferDialog() {
  const id = useNetworkStore((s) => s.dialogTransferId);
  const offer = useNetworkStore((s) =>
    s.incomingTransfers.find((t) => t.transferId === s.dialogTransferId),
  );
  const active = useNetworkStore((s) =>
    s.activeTransfers.find((t) => t.transferId === s.dialogTransferId),
  );
  const endedText = useNetworkStore((s) => s.dialogEndedText);
  // A failed call shows its text in a StateBlock, above the buttons.
  const { busy, error, run, reset } = useAsyncAction('inline');
  const [showProgress, setShowProgress] = useState(false);
  const receiving = active !== undefined;

  // Another offer starts with a clean error.
  // biome-ignore lint/correctness/useExhaustiveDependencies: id is the trigger, reset is stable.
  useEffect(() => {
    reset();
  }, [id, reset]);

  // The progress line waits 300 ms after the first bytes arrive.
  // biome-ignore lint/correctness/useExhaustiveDependencies: id restarts the delay for another transfer.
  useEffect(() => {
    setShowProgress(false);
    if (!receiving) return undefined;
    const timer = setTimeout(() => setShowProgress(true), PROGRESS_DELAY_MS);
    return () => clearTimeout(timer);
  }, [id, receiving]);

  if (!id) return null;

  const done = () => store().closeTransferDialog();

  const accept = () =>
    run(async () => {
      await respondNetworkTransfer(id, true);
      // The offer may have finished while the call was in flight. Then it stays gone.
      store().markTransferAccepted(id);
    });

  const decline = () =>
    run(async () => {
      await respondNetworkTransfer(id, false);
      store().removeIncomingTransfer(id);
      // The user may have opened another offer meanwhile. Do not close that one.
      if (store().dialogTransferId === id) store().closeTransferDialog();
    });

  const cancel = () =>
    run(async () => {
      // The finish event that follows is the user's own doing, so it shows no toast.
      store().markCancelledHere(id);
      try {
        await cancelNetworkTransfer(id);
      } catch (err) {
        // A refused cancel changes nothing, so a cancel by the other node still shows its toast.
        store().unmarkCancelledHere(id);
        throw err;
      }
    });

  const name = offer ? peerName(offer.displayName) : '';
  const waiting = offer?.accepted ?? false;
  const progressLine =
    showProgress && active
      ? `${formatBytes(active.doneBytes)} of ${formatBytes(active.totalBytes)}`
      : null;

  const title = !offer
    ? 'Collection offer'
    : waiting
      ? `Receiving from ${name}`
      : `${name} wants to send you a collection`;
  const description = !offer
    ? (endedText ?? 'The request expired.')
    : waiting
      ? 'You can close this window. The transfer goes on.'
      : 'Nothing is saved until you accept.';

  return (
    <Dialog
      open
      onOpenChange={(next) => {
        if (!next && !busy) done();
      }}
    >
      <DialogContent className='sm:max-w-sm'>
        <DialogHeader>
          <DialogTitle className='break-words'>{title}</DialogTitle>
          <DialogDescription>{description}</DialogDescription>
        </DialogHeader>
        {offer && (
          <div className='flex flex-col gap-1 py-2'>
            <p className='break-words text-sm font-medium'>{offer.collectionName}</p>
            <p className='text-xs text-muted-foreground'>{formatBytes(offer.sizeBytes)}</p>
            <p className='font-mono text-xs text-muted-foreground'>{`Device ID ${offer.shortId}`}</p>
          </div>
        )}
        {offer && !waiting && (
          <p
            role='note'
            className='flex items-start gap-2 rounded-md border p-2 text-xs text-muted-foreground'
          >
            <ShieldAlert className='mt-0.5 h-3.5 w-3.5 shrink-0' aria-hidden />
            <span>{SCRIPT_WARNING}</span>
          </p>
        )}
        {waiting && (
          <p role='status' className='text-sm text-muted-foreground'>
            {`Receiving from ${name}...`}
          </p>
        )}
        {progressLine && (
          <p role='status' className='text-xs text-muted-foreground'>
            {progressLine}
          </p>
        )}
        {error && <StateBlock tone='error'>{error}</StateBlock>}
        <DialogFooter>
          {!offer ? (
            <Button onClick={done}>Done</Button>
          ) : waiting ? (
            <>
              <Button variant='outline' disabled={busy} onClick={() => void cancel()}>
                Cancel
              </Button>
              <Button onClick={done}>Done</Button>
            </>
          ) : (
            <>
              <Button variant='outline' disabled={busy} onClick={() => void decline()}>
                Decline
              </Button>
              <Button disabled={busy} onClick={() => void accept()}>
                Accept
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
