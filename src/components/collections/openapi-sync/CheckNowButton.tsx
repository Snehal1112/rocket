import { useIsMutating, useQueryClient } from '@tanstack/react-query';
import { RefreshCw } from 'lucide-react';
import { useEffect, useRef } from 'react';
import { Button } from '@/components/ui/button';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { checkNowKey, useCheckNow } from '@/lib/queries/openapi-sync-queries';
import type { OpenApiCheckOutcome, OpenApiConsent } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
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

export interface CheckRunner {
  run: () => void;
  pending: boolean;
  granted: boolean;
  text: string | null;
  failed: boolean;
}

/** One check state for every Check now trigger of a collection, so they share it. */
export function useCheckRunner(collection: string, consent: OpenApiConsent): CheckRunner {
  const check = useCheckNow(collection);
  const queryClient = useQueryClient();
  // Another trigger of the same collection may be checking already.
  const running = useIsMutating({ mutationKey: checkNowKey(collection) }) > 0;
  // A ref closes the gap before the pending state renders on a fast double click.
  const busy = useRef(false);
  const granted = consent === 'granted';
  const { reset } = check;
  // An outcome belongs to an approved link, so it goes when the approval does.
  useEffect(() => {
    if (consent !== 'granted') reset();
  }, [consent, reset]);
  const run = () => {
    if (busy.current || !granted) return;
    if (queryClient.isMutating({ mutationKey: checkNowKey(collection) }) > 0) return;
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
  return { run, pending: check.isPending || running, granted, text, failed };
}

/** The Check now button for a runner shared with other triggers. */
export function CheckNowButtonView({ runner }: { runner: CheckRunner }) {
  const { run, pending, granted, text, failed } = runner;
  return (
    <div className='flex flex-col items-start gap-1'>
      <TooltipProvider>
        <Tooltip>
          <TooltipTrigger asChild>
            <span className='inline-flex' tabIndex={granted ? undefined : 0}>
              <Button size='sm' variant='outline' onClick={run} disabled={pending || !granted}>
                <RefreshCw
                  className={cn('mr-1 h-3.5 w-3.5', pending && 'animate-spin')}
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
          className={cn('text-xs', failed ? 'text-destructive' : 'text-muted-foreground')}
        >
          {text}
        </p>
      )}
    </div>
  );
}

/** Runs one check on demand. It needs the spec link to be approved on this machine. */
export function CheckNowButton({ collection, consent }: CheckNowButtonProps) {
  return <CheckNowButtonView runner={useCheckRunner(collection, consent)} />;
}
