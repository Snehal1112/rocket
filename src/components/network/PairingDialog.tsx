import { useEffect } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { useAsyncAction } from '@/hooks/useAsyncAction';
import { formatPairingCode } from '@/lib/network-utils';
import { cancelNetworkPairing, respondNetworkPairing } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

const store = () => useNetworkStore.getState();

/**
 * The pairing dialog, for both roles. The node that started the pairing sees Accept and
 * Cancel. A node that received a request sees Accept, Decline and Block. Both compare the
 * same 6-digit code. Mount it once, outside the Network popover.
 */
export function PairingDialog() {
  const id = useNetworkStore((s) => s.dialogPairingId);
  const active = useNetworkStore((s) => s.activePairing);
  const request = useNetworkStore((s) => s.incoming.find((i) => i.pairingId === s.dialogPairingId));
  // A failed call shows its text in the dialog, under the code.
  const { busy, error, run, reset } = useAsyncAction('inline');

  // Another pairing starts with a clean error.
  // biome-ignore lint/correctness/useExhaustiveDependencies: id is the trigger, reset is stable.
  useEffect(() => {
    reset();
  }, [id, reset]);

  if (!id) return null;

  const started = active && active.pairingId === id ? active : null;
  // No view means the request is gone: it expired, or another event finished it.
  const view = started ?? request ?? null;
  const done = () => store().closePairingDialog();

  const acceptStarted = () =>
    run(async () => {
      await respondNetworkPairing(id, 'accept');
      // The pairing may have finished while the call was in flight. Then it stays gone.
      const current = store().activePairing;
      if (current?.pairingId === id) store().setActivePairing({ ...current, phase: 'done' });
    });

  const cancelStarted = () =>
    run(async () => {
      await cancelNetworkPairing(id);
      // A finish event may have ended this pairing already. Do not touch a newer one.
      if (store().activePairing?.pairingId === id) store().setActivePairing(null);
      if (store().dialogPairingId === id) store().closePairingDialog();
    });

  const acceptRequest = () =>
    run(async () => {
      await respondNetworkPairing(id, 'accept');
      store().markIncomingAccepted(id);
    });

  const answerRequest = (decision: 'decline' | 'block', name: string) =>
    run(async () => {
      await respondNetworkPairing(id, decision);
      store().removeIncoming(id);
      if (store().dialogPairingId === id) store().closePairingDialog();
      if (decision === 'block') toast.info(`${name} is blocked.`);
    });

  // Closing a dialog that still waits for the user's choice ends the pairing it started.
  function handleOpenChange(next: boolean) {
    if (next || busy) return;
    if (started && started.phase === 'waiting') {
      void cancelStarted();
      return;
    }
    done();
  }

  const name = view?.displayName ?? '';
  const waitingForPeer = started ? started.phase === 'done' : (request?.accepted ?? false);

  return (
    <Dialog open onOpenChange={handleOpenChange}>
      <DialogContent className='sm:max-w-sm'>
        <DialogHeader>
          <DialogTitle className='break-words'>
            {!view ? 'Pairing request' : started ? `Pair with ${name}` : `${name} wants to pair`}
          </DialogTitle>
          <DialogDescription>
            {view ? `Check that ${name} sees the same code, then accept.` : 'The request expired.'}
          </DialogDescription>
        </DialogHeader>
        {view && (
          // biome-ignore lint/a11y/useSemanticElements: A fieldset would be a raw form element.
          <div
            role='group'
            aria-label='Pairing code'
            className='flex flex-col items-center gap-1 py-2'
          >
            <p className='font-mono text-4xl font-semibold tracking-[0.2em]'>
              {formatPairingCode(view.code)}
            </p>
            <p className='font-mono text-xs text-muted-foreground'>{`Device ID ${view.shortId}`}</p>
          </div>
        )}
        {waitingForPeer && (
          <p role='status' className='text-center text-sm text-muted-foreground'>
            {`Waiting for ${name}...`}
          </p>
        )}
        {error && (
          <p role='alert' className='text-sm text-destructive'>
            {error}
          </p>
        )}
        <DialogFooter>
          {!view || waitingForPeer ? (
            <Button variant={view ? 'outline' : 'default'} onClick={done}>
              Done
            </Button>
          ) : started ? (
            <>
              <Button variant='outline' disabled={busy} onClick={() => void cancelStarted()}>
                Cancel
              </Button>
              <Button disabled={busy} onClick={() => void acceptStarted()}>
                Accept
              </Button>
            </>
          ) : (
            <>
              <Button
                variant='ghost'
                className='text-destructive hover:text-destructive'
                disabled={busy}
                onClick={() => void answerRequest('block', name)}
              >
                Block
              </Button>
              <Button
                variant='outline'
                disabled={busy}
                onClick={() => void answerRequest('decline', name)}
              >
                Decline
              </Button>
              <Button disabled={busy} onClick={() => void acceptRequest()}>
                Accept
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
