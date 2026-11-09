import { Loader2, Send } from 'lucide-react';
import { useEffect, useMemo, useRef, useState } from 'react';
import { toast } from 'sonner';
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
import { FormField } from '@/components/ui/form-field';
import { Label } from '@/components/ui/label';
import { RadioGroup, RadioGroupItem } from '@/components/ui/radio-group';
import { StateBlock } from '@/components/ui/state-block';
import { useAsyncAction } from '@/hooks/useAsyncAction';
import { toastError } from '@/lib/error-text';
import { peerName, transferWaitingToastId } from '@/lib/network-utils';
import { cancelNetworkTransfer, type SendStarted, sendNetworkCollection } from '@/lib/tauri-api';
import { onlinePairedNodes, useNetworkStore } from '@/stores/network-store';

interface SendCollectionDialogProps {
  open: boolean;
  collectionName: string;
  onClose: () => void;
}

// The toast stays until the transfer ends: the event bridge dismisses it on the finish event.
function showWaitingToast(transferId: string, name: string): void {
  toast.info(`Sent to ${name}. Waiting for them to accept.`, {
    id: transferWaitingToastId(transferId),
    duration: Number.POSITIVE_INFINITY,
    action: {
      label: 'Cancel',
      onClick: () => {
        // The finish that follows is the user's own doing, so the bridge shows no toast for it.
        useNetworkStore.getState().markCancelledHere(transferId);
        // A cancel after the last chunk is refused. The user sees why, and a later cancel by
        // the other node still shows its toast.
        cancelNetworkTransfer(transferId).catch((err) => {
          useNetworkStore.getState().unmarkCancelledHere(transferId);
          toastError(err);
        });
      },
    },
  });
}

// Notes can repeat, so a repeat gets its count in the key.
function keyedNotes(notes: string[]): { key: string; note: string }[] {
  const seen = new Map<string, number>();
  return notes.map((note) => {
    const count = (seen.get(note) ?? 0) + 1;
    seen.set(note, count);
    return { key: `${count}-${note}`, note };
  });
}

/**
 * Sends one collection to a paired node that is online. The other user is asked before
 * anything is saved. Environments are off by default. When the export dropped or blanked
 * something, the dialog lists those notes before it closes. Mount it as a sibling of the menu.
 */
export function SendCollectionDialog({ open, collectionName, onClose }: SendCollectionDialogProps) {
  // The list is filtered here, because a selector must not build a new array.
  const nodes = useNetworkStore((s) => s.nodes);
  const online = useMemo(() => onlinePairedNodes(nodes), [nodes]);
  const [picked, setPicked] = useState<string | null>(null);
  const [includeEnvironments, setIncludeEnvironments] = useState(false);
  const [keepInSync, setKeepInSync] = useState(false);
  // The export notes of a started send. Null while the form shows.
  const [notes, setNotes] = useState<string[] | null>(null);
  // A failed send shows its text in the dialog.
  const { busy, error, run, reset } = useAsyncAction('inline');

  // A second send call while one is in flight is ignored, even if the button is not disabled yet.
  const sending = useRef(false);

  // The dialog stays mounted while closed, so it resets on close. A reopen then starts clean
  // from the first frame.
  useEffect(() => {
    if (open) return;
    setPicked(null);
    setIncludeEnvironments(false);
    setKeepInSync(false);
    setNotes(null);
    reset();
  }, [open, reset]);

  // The only online node is chosen once. After that the choice is frozen, so if that node leaves
  // and another arrives, nothing is chosen and the user must pick. Setting state while rendering
  // is the React way to adjust state to a change.
  if (open && picked === null && online.length === 1) setPicked(online[0].deviceId);

  const pickedNode = picked === null ? undefined : online.find((n) => n.deviceId === picked);
  const lost = picked !== null && pickedNode === undefined;
  // If the chosen node left, nothing is chosen: another node must never receive the collection
  // by surprise.
  const chosen = pickedNode;

  async function send() {
    if (!chosen || sending.current) return;
    sending.current = true;
    let result = null as SendStarted | null;
    let ok = false;
    try {
      ok = await run(async () => {
        result = await sendNetworkCollection(
          chosen.deviceId,
          collectionName,
          includeEnvironments,
          keepInSync,
        );
      });
    } finally {
      sending.current = false;
    }
    if (!ok || !result) return;
    const sent: SendStarted = result;
    // The bridge updates the waiting toast of a send it knows (index R19).
    // False means the transfer finished before this call returned: no toast would ever close.
    if (useNetworkStore.getState().trackOutgoing(sent.transferId, chosen.displayName)) {
      showWaitingToast(sent.transferId, peerName(chosen.displayName));
    }
    if (sent.notes.length === 0) onClose();
    else setNotes(sent.notes);
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => {
        if (!next && !busy) onClose();
      }}
    >
      <DialogContent className='sm:max-w-md'>
        <DialogHeader>
          <DialogTitle className='break-words'>{`Send ${collectionName}`}</DialogTitle>
          <DialogDescription>
            The other user is asked before anything is saved. Credentials and secret values are
            blanked, as in an export.
          </DialogDescription>
        </DialogHeader>
        {notes !== null ? (
          <div className='space-y-2'>
            <p className='text-sm font-medium'>Not sent with the collection</p>
            <ul className='list-disc space-y-1 pl-5 text-xs text-muted-foreground'>
              {keyedNotes(notes).map(({ key, note }) => (
                <li key={key} className='break-words'>
                  {note}
                </li>
              ))}
            </ul>
          </div>
        ) : online.length === 0 ? (
          <EmptyState
            cause='needs-setup'
            size='compact'
            title='No paired node is online'
            reason='Start Rocket on the other computer, or pair it in the Network panel.'
          />
        ) : (
          <div className='space-y-4'>
            <FormField
              id='send-node'
              group
              label='Send to'
              help='Only paired nodes that are online are listed.'
              error={lost ? 'That node went offline. Choose another one.' : null}
            >
              {() => (
                <RadioGroup
                  aria-label='Send to'
                  value={chosen?.deviceId ?? ''}
                  onValueChange={setPicked}
                  disabled={busy}
                  className='gap-2'
                >
                  {online.map((node) => (
                    <div key={node.deviceId} className='flex items-center gap-2'>
                      <RadioGroupItem value={node.deviceId} id={`send-node-${node.deviceId}`} />
                      <Label htmlFor={`send-node-${node.deviceId}`} className='text-sm font-normal'>
                        {node.displayName.trim() ? `${node.displayName} ` : ''}
                        <span className='font-mono text-xs text-muted-foreground'>
                          {node.shortId}
                        </span>
                      </Label>
                    </div>
                  ))}
                </RadioGroup>
              )}
            </FormField>
            <FormField
              id='send-environments'
              group
              label='Environments'
              help='Off by default. Environment variables are sent with credentials and secret values blanked.'
            >
              {(a11y) => (
                <div className='flex items-center gap-2'>
                  <Checkbox
                    id='send-environments-box'
                    aria-describedby={a11y['aria-describedby']}
                    checked={includeEnvironments}
                    disabled={busy}
                    onCheckedChange={(v) => setIncludeEnvironments(v === true)}
                  />
                  <Label htmlFor='send-environments-box' className='text-sm font-normal'>
                    Include environments
                  </Label>
                </div>
              )}
            </FormField>
            <FormField
              id='send-sync'
              group
              label='Sync'
              help='Off by default. Lets you and the other user sync later changes, each one reviewed. Only a collection you send now can be linked.'
            >
              {(a11y) => (
                <div className='flex items-center gap-2'>
                  <Checkbox
                    id='send-sync-box'
                    aria-describedby={a11y['aria-describedby']}
                    checked={keepInSync}
                    disabled={busy}
                    onCheckedChange={(v) => setKeepInSync(v === true)}
                  />
                  <Label htmlFor='send-sync-box' className='text-sm font-normal'>
                    Keep in sync
                  </Label>
                </div>
              )}
            </FormField>
            {error && <StateBlock tone='error'>{error}</StateBlock>}
          </div>
        )}
        <DialogFooter>
          {notes !== null ? (
            <Button onClick={onClose}>Done</Button>
          ) : (
            <>
              <Button variant='outline' disabled={busy} onClick={onClose}>
                Cancel
              </Button>
              <Button disabled={busy || !chosen} onClick={() => void send()}>
                {busy ? (
                  <Loader2 className='mr-2 h-4 w-4 animate-spin' />
                ) : (
                  <Send className='mr-2 h-4 w-4' />
                )}
                Send
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
