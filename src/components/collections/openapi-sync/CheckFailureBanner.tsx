import { useState } from 'react';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import type { OpenApiSyncStatus } from '@/lib/tauri-api';
import { useCheckRunner } from './CheckNowButton';
import { nextStep } from './nextStep';
import { OpenApiSyncConnectionDialog } from './OpenApiSyncConnectionDialog';

/**
 * The failed last check with the fixed sentence the backend stored and the one action that fits
 * it. It shows only when the link is approved, because the consent banner owns that state.
 */
export function CheckFailureBanner({
  collection,
  status,
}: {
  collection: string;
  status: OpenApiSyncStatus;
}) {
  const runner = useCheckRunner(collection, status.consent);
  const [open, setOpen] = useState(false);
  const [focus, setFocus] = useState<'token' | 'link' | undefined>();
  const shown = status.lastStatus?.state === 'failed' && status.consent === 'granted';
  const step = nextStep(status);
  const openSettings = (next?: 'token' | 'link') => {
    setFocus(next);
    setOpen(true);
  };
  let action: { label: string; onClick: () => void; disabled?: boolean };
  switch (step.kind) {
    case 'add_token':
      action = { label: step.label, onClick: () => openSettings('token') };
      break;
    case 'change_link':
      action = { label: step.label, onClick: () => openSettings('link') };
      break;
    case 'check_now':
      action = { label: step.label, onClick: runner.run, disabled: runner.pending };
      break;
    default:
      action = { label: 'Connection settings', onClick: () => openSettings() };
  }
  // The dialog stays mounted when the failure clears, so a fix made in it does not close it.
  return (
    <>
      {shown && (
        <Alert variant='destructive'>
          <p className='font-semibold'>Failed to check for spec updates</p>
          <p>{status.lastMessage ?? 'The last check failed.'}</p>
          <Button
            variant='outline'
            size='sm'
            className='mt-2'
            disabled={action.disabled}
            onClick={action.onClick}
          >
            {runner.pending && step.kind === 'check_now' ? 'Checking...' : action.label}
          </Button>
          {runner.text && (
            <p role='status' className='mt-2 text-sm'>
              {runner.text}
            </p>
          )}
        </Alert>
      )}
      <OpenApiSyncConnectionDialog
        collection={collection}
        status={status}
        open={open}
        onOpenChange={setOpen}
        focus={focus}
      />
    </>
  );
}
