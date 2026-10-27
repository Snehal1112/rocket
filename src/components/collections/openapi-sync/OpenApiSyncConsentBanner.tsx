import { ShieldQuestion } from 'lucide-react';
import { useRef, useState } from 'react';
import { Alert } from '@/components/ui/alert';
import { Button } from '@/components/ui/button';
import { useGrantConsent } from '@/lib/queries/openapi-sync-queries';
import type { OpenApiConsent } from '@/lib/tauri-api';
import { isGithubLocation } from './openApiSyncView';
import { syncErrorMessage } from './syncErrors';

interface OpenApiSyncConsentBannerProps {
  collection: string;
  /** The exact location text from the status. It is what gets approved. */
  location: string;
  consent: Exclude<OpenApiConsent, 'granted'>;
}

const WHY: Record<OpenApiSyncConsentBannerProps['consent'], string> = {
  missing:
    'This collection came from another place or was moved. Rocket does not contact the link until you approve it on this machine.',
  locationChanged:
    'The spec link in this collection changed. Approve the new link before Rocket contacts it.',
};

/** Asks the user to trust the spec link on this machine before Rocket contacts it. */
export function OpenApiSyncConsentBanner({
  collection,
  location,
  consent,
}: OpenApiSyncConsentBannerProps) {
  const grant = useGrantConsent(collection);
  // The text the user was shown. A refetch must not swap it before the click.
  const [seen, setSeen] = useState(location);
  const changed = seen !== location;
  const busy = useRef(false);
  const approve = () => {
    if (busy.current) return;
    busy.current = true;
    grant.mutate(seen, {
      onSettled: () => {
        busy.current = false;
      },
    });
  };
  const error = grant.isError ? syncErrorMessage(grant.error) : null;
  const ambiguous = (error?.includes('ambiguous') ?? false) && isGithubLocation(seen);
  return (
    <Alert className='mx-4 mt-3 w-auto'>
      <ShieldQuestion className='mt-0.5 h-4 w-4 shrink-0' aria-hidden='true' />
      <div className='flex min-w-0 flex-1 flex-col gap-2'>
        <p className='font-semibold'>Trust this spec link</p>
        {/* The full exact text is shown on purpose, so the user knows what they approve. */}
        <p className='break-all rounded bg-muted px-2 py-1 font-mono'>
          {changed ? location : seen}
        </p>
        {changed && <p className='font-medium'>The link changed. Review it.</p>}
        <p className='text-muted-foreground'>{WHY[consent]}</p>
        {error && (
          <p role='alert' className='text-destructive'>
            {error}
            {ambiguous &&
              ' A GitHub link with a branch name with a slash cannot be approved here. Import the spec again to approve it.'}
          </p>
        )}
        <div>
          {changed ? (
            <Button size='sm' variant='outline' onClick={() => setSeen(location)}>
              Review the new link
            </Button>
          ) : (
            <Button size='sm' onClick={approve} disabled={grant.isPending}>
              Approve
            </Button>
          )}
        </div>
      </div>
    </Alert>
  );
}
