import {
  AlertDialog,
  AlertDialogAction,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { useAsyncAction } from '@/hooks/useAsyncAction';
import { peerName } from '@/lib/network-utils';
import type { NetworkLink } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

interface UnlinkDialogProps {
  /** The link to remove. Null keeps the dialog closed. */
  link: NetworkLink | null;
  onClose: () => void;
}

// Asks before an unlink. The collection and its files stay as they are.
export function UnlinkDialog({ link, onClose }: UnlinkDialogProps) {
  // A failure is a toast, and the dialog stays open so the user can try again or cancel.
  const { busy, run } = useAsyncAction();

  async function unlink(target: NetworkLink) {
    const store = useNetworkStore.getState();
    // An unanswered review of this link ends as cancelled. It is the user's own doing, so the
    // bridge shows no toast for it.
    const pending = store.syncReviews.find((r) => r.linkId === target.linkId && !r.answered);
    if (pending) store.markSyncCancelledHere(pending.syncId);
    const ok = await run(() => useNetworkStore.getState().unlink(target.linkId));
    if (ok) onClose();
    else if (pending) useNetworkStore.getState().unmarkSyncCancelledHere(pending.syncId);
  }

  const name = link ? peerName(link.displayName) : '';

  return (
    <AlertDialog
      open={link !== null}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle className='break-words'>
            {link ? `Unlink ${link.collectionName} from ${name}?` : 'Unlink'}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {link
              ? `Your collection and its files stay as they are. ${name} (${link.shortId}) learns of it at the next sync. To sync again, send the collection again with Keep in sync.`
              : ''}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            disabled={busy}
            onClick={(e) => {
              // Radix would close the dialog on press. It stays open until the unlink worked.
              e.preventDefault();
              if (link) void unlink(link);
            }}
          >
            Unlink
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
