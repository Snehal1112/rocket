import { RefreshCw } from 'lucide-react';
import { useRef } from 'react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { useCheckNow } from '@/lib/queries/openapi-sync-queries';
import type { OpenApiCheckOutcome, OpenApiConsent } from '@/lib/tauri-api';
import { syncErrorMessage } from './syncErrors';

interface CheckNowButtonProps {
  collection: string;
  consent: OpenApiConsent;
}

function retryHint(secs?: number): string {
  return secs && secs > 0 ? `Try again in ${secs} seconds.` : 'Try again in a moment.';
}

function outcomeText(outcome: OpenApiCheckOutcome): string {
  if (outcome.state === 'unchanged') return 'No changes in the spec.';
  if (outcome.state === 'changed') return 'A new version of the spec was found.';
  const message = outcome.message ?? 'The check failed.';
  return `${message} ${retryHint(outcome.retryAfterSecs)}`;
}

/** Runs one check on demand. It needs the spec link to be approved on this machine. */
export function CheckNowButton({ collection, consent }: CheckNowButtonProps) {
  const check = useCheckNow(collection);
  // A ref closes the gap before the pending state renders on a fast double click.
  const busy = useRef(false);
  const granted = consent === 'granted';
  const run = () => {
    if (busy.current) return;
    busy.current = true;
    check.mutate(undefined, {
      onSettled: () => {
        busy.current = false;
      },
    });
  };
  let text: string | null = null;
  if (check.isError) text = `${syncErrorMessage(check.error)} ${retryHint()}`;
  else if (check.data) text = outcomeText(check.data);
  const failed = check.isError || check.data?.state === 'failed';
  return (
    <div className='flex flex-col items-start gap-1'>
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <span className='inline-flex'>
              <Button
                size='sm'
                variant='outline'
                onClick={run}
                disabled={check.isPending || !granted}
              >
                <RefreshCw
                  className={`mr-1 h-3.5 w-3.5 ${check.isPending ? 'animate-spin' : ''}`}
                  aria-hidden='true'
                />
                Check now
              </Button>
            </span>
          </TooltipTrigger>
          {!granted && (
            <TooltipContent>
              Approve the spec link first. Rocket does not contact it before.
            </TooltipContent>
          )}
        </Tooltip>
      </TooltipProvider>
      {text && (
        <p
          role='status'
          className={`text-xs ${failed ? 'text-destructive' : 'text-muted-foreground'}`}
        >
          {text}
        </p>
      )}
    </div>
  );
}
