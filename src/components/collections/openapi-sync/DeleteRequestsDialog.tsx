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

/** One request the dialog asks about. `label` is what the user knows it by. */
export interface DeleteRequestRow {
  uid: string;
  requestPath: string;
  label: string;
}

interface DeleteRequestsDialogProps {
  requests: DeleteRequestRow[];
  open: boolean;
  onConfirm: () => void;
  onCancel: () => void;
  busy: boolean;
}

/** Asks before request files are deleted from the collection. */
export function DeleteRequestsDialog({
  requests,
  open,
  onConfirm,
  onCancel,
  busy,
}: DeleteRequestsDialogProps) {
  const title =
    requests.length === 1
      ? `Delete "${requests[0].label}"?`
      : `Delete ${requests.length} requests?`;
  return (
    <AlertDialog open={open} onOpenChange={(next) => !next && onCancel()}>
      <AlertDialogContent>
        <AlertDialogHeader className='min-w-0'>
          <AlertDialogTitle className='[overflow-wrap:anywhere]'>{title}</AlertDialogTitle>
          <AlertDialogDescription>
            This removes the file from the collection. The sync page cannot undo it, git can.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction disabled={busy} onClick={onConfirm}>
            Delete
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
