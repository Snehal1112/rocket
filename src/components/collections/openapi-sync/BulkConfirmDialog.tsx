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

interface BulkConfirmDialogProps {
  kind: 'accept' | 'skip';
  /** Requests the action covers. */
  count: number;
  /** Edits of the user that Accept all would replace with the spec. */
  overwritten?: number;
  onConfirm: () => void;
  onCancel: () => void;
}

const requests = (n: number) => `${n} ${n === 1 ? 'request' : 'requests'}`;

/** Asks before Accept all overwrites edits, and before Skip all hides every update. */
export function BulkConfirmDialog({
  kind,
  count,
  overwritten = 0,
  onConfirm,
  onCancel,
}: BulkConfirmDialogProps) {
  const accept = kind === 'accept';
  return (
    <AlertDialog open onOpenChange={(open) => !open && onCancel()}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>
            {accept ? 'Accept all updates?' : 'Skip all updates?'}
          </AlertDialogTitle>
          <AlertDialogDescription>
            {accept
              ? `This selects ${requests(count)} from the spec. Nothing changes until you confirm and sync.`
              : `This skips ${requests(count)}. They stay hidden until the spec changes.`}
          </AlertDialogDescription>
        </AlertDialogHeader>
        {accept && (
          <ul className='flex flex-col gap-1 text-sm'>
            {overwritten > 0 && <li>{`${overwritten} of your edits will be overwritten.`}</li>}
            <li>Removed requests you edited are kept, never deleted.</li>
          </ul>
        )}
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          <AlertDialogAction onClick={onConfirm}>
            {accept ? 'Accept all' : 'Skip all'}
          </AlertDialogAction>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
