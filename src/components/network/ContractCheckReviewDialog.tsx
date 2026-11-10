import { useEffect, useState } from 'react';
import { toast } from 'sonner';
import { ContractCheckChangeRow } from '@/components/network/ContractCheckChangeRow';
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
import { contractCheckToastId } from '@/lib/network-contract-check-utils';
import {
  type ContractCheckReview,
  declineContractCheck,
  replyContractCheck,
  reviewContractCheck,
} from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

const store = () => useNetworkStore.getState();

export const CONTRACT_CHECK_EXPIRED_TEXT = 'This check expired.';

type LoadState =
  | { status: 'loading' }
  | { status: 'ready'; review: ContractCheckReview }
  | { status: 'error' };

/**
 * The review of a contract check another node offered. The local matches stay on this node; only
 * the verdicts go back when the user presses Send result. Mount once.
 */
export function ContractCheckReviewDialog() {
  const id = useNetworkStore((s) => s.dialogContractCheckId);
  const pending = useNetworkStore((s) =>
    s.contractChecksIncoming.some((c) => c.checkId === s.dialogContractCheckId),
  );
  const [state, setState] = useState<LoadState>({ status: 'loading' });
  const { busy, error, run, reset } = useAsyncAction('inline');

  useEffect(() => {
    if (!id) return;
    let cancelled = false;
    reset();
    setState({ status: 'loading' });
    reviewContractCheck(id)
      .then((review) => {
        if (!cancelled) setState({ status: 'ready', review });
      })
      .catch(() => {
        // The rejection text may name a local request, so it is not shown.
        if (!cancelled) setState({ status: 'error' });
      });
    return () => {
      cancelled = true;
    };
  }, [id, reset]);

  if (!id) return null;

  const checkId = id;
  const close = () => store().closeContractCheckDialog();
  const ready = state.status === 'ready' ? state.review : null;
  const canAnswer = pending && ready !== null && !busy;

  async function answer(action: (checkId: string) => Promise<void>) {
    const ok = await run(() => action(checkId));
    if (!ok) return;
    store().removeIncomingContractCheck(checkId);
    toast.dismiss(contractCheckToastId(checkId));
    if (store().dialogContractCheckId === checkId) close();
  }

  return (
    <Dialog
      open
      onOpenChange={(next) => {
        // Closing keeps the offer pending, and its toast stays.
        if (!next && !busy) close();
      }}
    >
      <DialogContent className='max-h-[90vh] sm:max-w-2xl'>
        <DialogHeader>
          <DialogTitle className='break-words'>
            {ready ? `Check a change to ${ready.contract}` : 'Check a contract change'}
          </DialogTitle>
          <DialogDescription>
            Your request names stay on this device. Only the results are sent back.
          </DialogDescription>
        </DialogHeader>
        <div className='max-h-[70vh] space-y-3 overflow-y-auto'>
          {state.status === 'loading' && (
            <StateBlock tone='loading'>Checking your requests...</StateBlock>
          )}
          {state.status === 'error' && (
            <StateBlock tone='error'>The check could not be loaded.</StateBlock>
          )}
          {ready && (
            <ul className='space-y-2'>
              {ready.changes.map((change) => (
                <ContractCheckChangeRow key={change.index} checkId={checkId} change={change} />
              ))}
            </ul>
          )}
          {!pending && (
            <p role='status' className='text-sm text-muted-foreground'>
              {CONTRACT_CHECK_EXPIRED_TEXT}
            </p>
          )}
          {error && <StateBlock tone='error'>{error}</StateBlock>}
        </div>
        <DialogFooter>
          <Button
            variant='outline'
            disabled={!canAnswer}
            onClick={() => void answer(declineContractCheck)}
          >
            Decline
          </Button>
          <Button disabled={!canAnswer} onClick={() => void answer(replyContractCheck)}>
            Send result
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
