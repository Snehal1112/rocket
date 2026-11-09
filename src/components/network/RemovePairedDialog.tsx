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
import type { NetworkNode } from '@/lib/tauri-api';
import { useNetworkStore } from '@/stores/network-store';

interface RemovePairedDialogProps {
  /** The node to remove. Null keeps the dialog closed. */
  node: NetworkNode | null;
  onClose: () => void;
}

// Asks before an unpair. The peer is told when it is online.
export function RemovePairedDialog({ node, onClose }: RemovePairedDialogProps) {
  // A failure is a toast. The store action reloads the list either way.
  const { run } = useAsyncAction();

  return (
    <AlertDialog
      open={node !== null}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>{node ? `Remove ${node.displayName}?` : 'Remove'}</AlertDialogTitle>
          <AlertDialogDescription>
            {node
              ? `${node.displayName} (${node.shortId}) will no longer be paired with you. Links to collections on that node are removed too. You can pair again later with a new code.`
              : ''}
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            onClick={() => {
              if (node) void run(() => useNetworkStore.getState().removePaired(node.deviceId));
            }}
          >
            Remove
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
