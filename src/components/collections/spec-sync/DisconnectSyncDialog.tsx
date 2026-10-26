import { useEffect, useRef, useState } from 'react';
import {
  AlertDialog,
  AlertDialogCancel,
  AlertDialogContent,
  AlertDialogDescription,
  AlertDialogFooter,
  AlertDialogHeader,
  AlertDialogTitle,
} from '@/components/ui/alert-dialog';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { Label } from '@/components/ui/label';
import { useDisconnectSync, useIsApplying } from '@/lib/queries/openapi-sync-queries';
import { syncErrorMessage } from './syncErrors';

interface DisconnectSyncDialogProps {
  collection: string;
  open: boolean;
  onOpenChange: (open: boolean) => void;
  /** Called once after the sync state is gone. */
  onDisconnected?: () => void;
}

/** Confirms the end of spec sync. The requests stay as they are. */
export function DisconnectSyncDialog({
  collection,
  open,
  onOpenChange,
  onDisconnected,
}: DisconnectSyncDialogProps) {
  const disconnect = useDisconnectSync(collection);
  const [removeToken, setRemoveToken] = useState(false);
  const applying = useIsApplying(collection);
  const { reset } = disconnect;
  // A reopened dialog starts clean: the box unticked and no old error.
  useEffect(() => {
    if (open) {
      setRemoveToken(false);
      reset();
    }
  }, [open, reset]);
  // A ref closes the gap before the pending state renders on a fast double click.
  const busy = useRef(false);
  const run = () => {
    if (busy.current) return;
    busy.current = true;
    disconnect.mutate(removeToken, {
      onSuccess: () => {
        onOpenChange(false);
        onDisconnected?.();
      },
      onSettled: () => {
        busy.current = false;
      },
    });
  };
  const error = applying
    ? 'An apply is running on this collection. Try again when it finishes.'
    : disconnect.isError
      ? syncErrorMessage(disconnect.error)
      : null;
  return (
    <AlertDialog open={open} onOpenChange={onOpenChange}>
      <AlertDialogContent>
        <AlertDialogHeader>
          <AlertDialogTitle>Disconnect from the spec?</AlertDialogTitle>
          <AlertDialogDescription>
            Rocket stops checking this spec. Your requests are not changed.
          </AlertDialogDescription>
        </AlertDialogHeader>
        <div className='flex items-start gap-2'>
          <Checkbox
            id='spec-sync-remove-token'
            checked={removeToken}
            onCheckedChange={(v) => setRemoveToken(v === true)}
            className='mt-0.5'
          />
          <Label htmlFor='spec-sync-remove-token' className='text-sm font-normal'>
            Also remove the stored token (this also removes it for any copy of this collection on
            this machine that uses the same link)
          </Label>
        </div>
        {error && (
          <p role='alert' className='text-xs text-destructive'>
            {error}
          </p>
        )}
        <AlertDialogFooter>
          <AlertDialogCancel>Cancel</AlertDialogCancel>
          {/* A plain button keeps the dialog open until the call succeeds. */}
          <Button variant='destructive' onClick={run} disabled={disconnect.isPending || applying}>
            Disconnect
          </Button>
        </AlertDialogFooter>
      </AlertDialogContent>
    </AlertDialog>
  );
}
