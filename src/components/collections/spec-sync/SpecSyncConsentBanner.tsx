import { ShieldQuestion } from 'lucide-react';
import { useRef } from 'react';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { useGrantConsent } from '@/lib/queries/openapi-sync-queries';
import type { OpenApiConsent } from '@/lib/tauri-api';
import { syncErrorMessage } from './syncErrors';

interface SpecSyncConsentBannerProps {
  collection: string;
  /** The exact location text from the status. It is what gets approved. */
  location: string;
  consent: Exclude<OpenApiConsent, 'granted'>;
}

const WHY: Record<SpecSyncConsentBannerProps['consent'], string> = {
  missing:
    'This collection came from another place or was moved. Rocket does not contact the link until you approve it on this machine.',
  locationChanged:
    'The spec link in this collection changed. Approve the new link before Rocket contacts it.',
};

/** Asks the user to trust the spec link on this machine before Rocket contacts it. */
export function SpecSyncConsentBanner({
  collection,
  location,
  consent,
}: SpecSyncConsentBannerProps) {
  const grant = useGrantConsent(collection);
  const busy = useRef(false);
  const approve = () => {
    if (busy.current) return;
    busy.current = true;
    grant.mutate(location, {
      onSettled: () => {
        busy.current = false;
      },
    });
  };
  const error = grant.isError ? syncErrorMessage(grant.error) : null;
  const ambiguous = error?.includes('ambiguous') ?? false;
  return (
    <Alert className='mx-4 mt-3 w-auto'>
      <ShieldQuestion className='mt-0.5 h-4 w-4 shrink-0' aria-hidden='true' />
      <div className='flex min-w-0 flex-1 flex-col gap-2'>
        <p className='font-semibold'>Trust this spec link</p>
        <p className='break-all rounded bg-muted px-2 py-1 font-mono'>{location}</p>
        <p className='text-muted-foreground'>{WHY[consent]}</p>
        {error && (
          <p role='status' className='text-destructive'>
            {error}
            {ambiguous &&
              ' A GitHub link with a branch name with a slash cannot be approved here. Import the spec again to approve it.'}
          </p>
        )}
        <div>
          <Button size='sm' onClick={approve} disabled={grant.isPending}>
            Approve
          </Button>
        </div>
      </div>
    </Alert>
  );
}
