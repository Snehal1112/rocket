import { Inbox } from 'lucide-react';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { receivedFromText, receivedOnText } from '@/lib/received-from';
import type { ReceivedCollection } from '@/lib/tauri-api';

interface ReceivedFromNameProps {
  name: string;
  /** Set for a collection that came over the team network. */
  origin?: ReceivedCollection;
}

/**
 * The collection name in the sidebar. A received collection shows who sent it on hover,
 * with the short id and the date. The name itself is not changed.
 */
export function ReceivedFromName({ name, origin }: ReceivedFromNameProps) {
  const label = <span className='truncate text-foreground'>{name}</span>;
  if (!origin) return label;
  const date = receivedOnText(origin);
  return (
    <TooltipProvider delayDuration={400}>
      <Tooltip>
        <TooltipTrigger asChild>{label}</TooltipTrigger>
        <TooltipContent side='right' className='max-w-64'>
          <p className='font-medium'>{receivedFromText(origin)}</p>
          <p className='font-mono text-2xs opacity-80'>
            {date ? `${origin.senderShortId} · ${date}` : origin.senderShortId}
          </p>
        </TooltipContent>
      </Tooltip>
    </TooltipProvider>
  );
}

/** One quiet line under the collection title: who sent it and when. */
export function ReceivedFromLine({ origin }: { origin: ReceivedCollection }) {
  const date = receivedOnText(origin);
  return (
    <p className='mb-4 flex items-center gap-1.5 pl-8 text-sm text-muted-foreground'>
      <Inbox className='h-3.5 w-3.5 shrink-0' aria-hidden='true' />
      <span className='min-w-0 truncate'>
        {receivedFromText(origin)}
        {date ? ` on ${date}` : ''}
      </span>
      <span className='shrink-0 font-mono text-2xs'>{origin.senderShortId}</span>
    </p>
  );
}
