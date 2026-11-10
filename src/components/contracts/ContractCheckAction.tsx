import { Loader2, UsersRound } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { EmptyState } from '@/components/ui/empty-state';
import { Label } from '@/components/ui/label';
import { StateBlock } from '@/components/ui/state-block';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { useAsyncAction } from '@/hooks/useAsyncAction';
import { peerName } from '@/lib/network-utils';
import { startContractCheck } from '@/lib/tauri-api';
import { contractCheckRunKey, useContractCheckRunsStore } from '@/stores/contract-check-runs-store';
import { onlinePairedNodes, useNetworkStore } from '@/stores/network-store';

interface ContractCheckActionProps {
  /** The collection root path. */
  collectionId: string;
  contractId: string;
  /** True when the contract has changes since its snapshot. */
  hasChanges: boolean;
}

export const TOO_MANY_CHANGES_TEXT = 'Check fewer changes at a time.';
export const START_FAILED_TEXT = 'Could not start the check.';

// Only two backend texts are shown. Anything else could carry local detail.
export function startErrorText(err: unknown): string {
  const raw = err instanceof Error ? err.message : typeof err === 'string' ? err : '';
  return raw.trim() === TOO_MANY_CHANGES_TEXT ? TOO_MANY_CHANGES_TEXT : START_FAILED_TEXT;
}

function disabledReason(hasChanges: boolean, networkOn: boolean): string | null {
  if (!networkOn) return 'Turn on the team network to check with the team.';
  if (!hasChanges) return 'This contract has no changes since its snapshot.';
  return null;
}

/**
 * The "Check with the team" button and its node picker. Only online paired nodes are listed.
 * Starting a check stores the rows for the results view and changes nothing else.
 */
export function ContractCheckAction({
  collectionId,
  contractId,
  hasChanges,
}: ContractCheckActionProps) {
  // The list is filtered here, because a selector must not build a new array.
  const nodes = useNetworkStore((s) => s.nodes);
  const networkOn = useNetworkStore((s) => s.status?.state === 'running');
  const online = useMemo(() => onlinePairedNodes(nodes), [nodes]);
  const [open, setOpen] = useState(false);
  const [picked, setPicked] = useState<string[]>([]);
  const { busy, error, run, reset } = useAsyncAction('inline');
  const sending = useRef(false);

  // A fresh open starts clean.
  useEffect(() => {
    if (open) return;
    setPicked([]);
    reset();
  }, [open, reset]);

  const chosen = online.filter((n) => picked.includes(n.deviceId));
  const reason = disabledReason(hasChanges, networkOn === true);
  const toggle = (deviceId: string, on: boolean) =>
    setPicked((cur) => (on ? [...cur, deviceId] : cur.filter((d) => d !== deviceId)));

  async function send() {
    if (sending.current || chosen.length === 0) return;
    sending.current = true;
    let started: Awaited<ReturnType<typeof startContractCheck>> | null = null;
    try {
      const ok = await run(async () => {
        started = await startContractCheck(
          collectionId,
          contractId,
          chosen.map((n) => n.deviceId),
        );
      });
      if (!ok || !started) return;
    } finally {
      sending.current = false;
    }
    const result = started as Awaited<ReturnType<typeof startContractCheck>>;
    const track = useNetworkStore.getState().trackContractCheck;
    for (const c of result.checks) track(c.checkId, c.deviceId);
    useContractCheckRunsStore.getState().setRun(contractCheckRunKey(collectionId, contractId), {
      changes: result.changes,
      targets: result.checks.map((c) => ({
        checkId: c.checkId,
        deviceId: c.deviceId,
        name: peerName(nodes.find((n) => n.deviceId === c.deviceId)?.displayName ?? ''),
      })),
    });
    setOpen(false);
  }

  // The failure text is fixed, so the dialog never echoes the backend message.
  const failure = error === null ? null : startErrorText(error);

  return (
    <>
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            {/* A disabled button gets no pointer events, so the span carries the tooltip. */}
            <span tabIndex={reason ? 0 : undefined}>
              <Button
                size='sm'
                variant='outline'
                disabled={reason !== null}
                onClick={() => setOpen(true)}
              >
                <UsersRound className='mr-1 h-3.5 w-3.5' aria-hidden='true' />
                Check with the team
              </Button>
            </span>
          </TooltipTrigger>
          {reason && <TooltipContent>{reason}</TooltipContent>}
        </Tooltip>
      </TooltipProvider>
      <Dialog open={open} onOpenChange={(next) => !next && !busy && setOpen(false)}>
        <DialogContent className='sm:max-w-md'>
          <DialogHeader>
            <DialogTitle>Check with the team</DialogTitle>
            <DialogDescription>
              Each teammate sees the changed fields and checks them against their own requests.
              Secret values are hidden. Nothing is saved on either side.
            </DialogDescription>
          </DialogHeader>
          {online.length === 0 ? (
            <EmptyState
              cause='needs-setup'
              size='compact'
              title='No paired node is online'
              reason='Start Rocket on the other computer, or pair it in the Network panel.'
            />
          ) : (
            <ul className='space-y-2' aria-label='Teammates'>
              {online.map((node) => (
                <li key={node.deviceId} className='flex items-center gap-2'>
                  <Checkbox
                    id={`check-node-${node.deviceId}`}
                    checked={picked.includes(node.deviceId)}
                    disabled={busy}
                    onCheckedChange={(v) => toggle(node.deviceId, v === true)}
                  />
                  <Label htmlFor={`check-node-${node.deviceId}`} className='text-sm font-normal'>
                    {node.displayName.trim() ? `${node.displayName} ` : ''}
                    <span className='font-mono text-xs text-muted-foreground'>{node.shortId}</span>
                  </Label>
                </li>
              ))}
            </ul>
          )}
          {failure && <StateBlock tone='error'>{failure}</StateBlock>}
          <DialogFooter>
            <Button variant='outline' disabled={busy} onClick={() => setOpen(false)}>
              Cancel
            </Button>
            <Button disabled={busy || chosen.length === 0} onClick={() => void send()}>
              {busy && <Loader2 className='mr-1 h-3.5 w-3.5 animate-spin' aria-hidden='true' />}
              Send
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </>
  );
}
