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
import type { OpenApiSyncItem } from '@/lib/tauri-api';

interface ApplyConfirmDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** The plan items that will be applied. */
  items: OpenApiSyncItem[];
  variableCount: number;
  onConfirm: () => void;
}

const plural = (n: number, one: string, many: string) => `${n} ${n === 1 ? one : many}`;

/** Shows what Apply is about to do, and which removed requests are deleted or kept. */
export function ApplyConfirmDialog({
  open,
  onOpenChange,
  items,
  variableCount,
  onConfirm,
}: ApplyConfirmDialogProps) {
  const added = items.filter((i) => i.kind === 'new').length;
  const changed = items.filter((i) => i.kind === 'changed').length;
  const removed = items.filter((i) => i.kind === 'removed');
  // Only a request known to be unedited is deleted. Unknown counts as edited.
  const deleted = removed.filter((i) => i.kind === 'removed' && i.edited === false).length;
  const kept = removed.length - deleted;
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Apply these changes?</AlertDialogTitle>
          <AlertDialogDescription>This changes the files of the collection.</AlertDialogDescription>
        </AlertDialogHeader>
        <ul className='flex flex-col gap-1 text-sm'>
          {added > 0 && <li>{`${plural(added, 'new request', 'new requests')} will be added.`}</li>}
          {changed > 0 && (
            <li>{`${plural(changed, 'changed request', 'changed requests')} will be updated.`}</li>
          )}
          {deleted > 0 && (
            <li>{`${plural(deleted, 'removed request', 'removed requests')} will be deleted if Rocket can rewrite ${deleted === 1 ? 'it' : 'them'}, you did not edit ${deleted === 1 ? 'it' : 'them'}.`}</li>
          )}
          {kept > 0 && (
            <li>{`${plural(kept, 'removed request', 'removed requests')} ${kept === 1 ? 'stays' : 'stay'} in your collection but ${kept === 1 ? 'is' : 'are'} no longer tracked, you edited ${kept === 1 ? 'it' : 'them'} or it cannot be known.`}</li>
          )}
          {variableCount > 0 && (
            <li>{`${plural(variableCount, 'variable change', 'variable changes')} will be applied.`}</li>
          )}
        </ul>
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction
            onClick={onConfirm}
            disabled={items.length === 0 && variableCount === 0}
          >
            Apply now
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
