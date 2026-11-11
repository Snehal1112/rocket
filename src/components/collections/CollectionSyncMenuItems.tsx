import { RefreshCw, Unlink } from 'lucide-react';
import { Fragment } from 'react';
import { linkDotClass } from '@/components/network/network-helpers';
import {
  ContextMenuItem,
  ContextMenuLabel,
  ContextMenuSeparator,
} from '@/components/ui/context-menu';
import {
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
} from '@/components/ui/dropdown-menu';
import type { CollectionSyncEntry } from '@/hooks/useCollectionSyncLinks';
import { linkStateLabel, linkStatusText } from '@/lib/network-sync-utils';
import { peerName } from '@/lib/network-utils';
import type { LinkState, NetworkLink } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';

interface SyncMenuItemsProps {
  entries: CollectionSyncEntry[];
  onSync: (link: NetworkLink) => void;
  onUnlink: (link: NetworkLink) => void;
}

const ICON_CLASS = 'h-3.5 w-3.5 mr-2';

/** Advice for the states that need the person to act. */
function hintText(state: LinkState): string | null {
  if (state === 'out_of_date') return 'Sync again to update.';
  if (state === 'broken') return 'Unlink, then send the collection again.';
  return null;
}

/**
 * The header of one linked peer: a state dot, the peer name and the short state. The full
 * sentence stays in the DOM for screen readers, so the visible parts are hidden from them.
 */
function PeerHeader({ link, online }: CollectionSyncEntry) {
  const hint = hintText(link.state);
  return (
    <div className='flex flex-col gap-0.5 py-0.5'>
      <span className='sr-only'>{linkStatusText(link.state, link.displayName)}</span>
      <div aria-hidden='true' className='flex items-center gap-2'>
        <span
          className={cn(
            'h-2 w-2 shrink-0 rounded-full',
            linkDotClass(link.state),
            // A faded dot marks a peer that is offline now.
            !online && 'opacity-40',
          )}
        />
        <span className='min-w-0 flex-1 truncate text-xs font-medium text-foreground'>
          {peerName(link.displayName)}
        </span>
        <span className='shrink-0 text-2xs font-normal text-muted-foreground'>
          {linkStateLabel(link.state)}
        </span>
      </div>
      {hint && (
        <span aria-hidden='true' className='pl-4 text-2xs font-normal text-muted-foreground'>
          {hint}
        </span>
      )}
    </div>
  );
}

/** "Unlink from Ben" for screen readers when several peers are linked. */
function unlinkAriaLabel(count: number, name: string): string {
  return count === 1 ? 'Unlink' : `Unlink from ${name}`;
}

export function DropdownSyncItems({ entries, onSync, onUnlink }: SyncMenuItemsProps) {
  if (entries.length === 0) return null;
  return (
    <>
      <DropdownMenuSeparator />
      {entries.map((entry) => (
        <Fragment key={entry.link.linkId}>
          <DropdownMenuLabel className='font-normal'>
            <PeerHeader {...entry} />
          </DropdownMenuLabel>
          {entry.online && (
            <DropdownMenuItem
              aria-label={`Sync with ${peerName(entry.link.displayName)}`}
              onClick={() => onSync(entry.link)}
            >
              <RefreshCw className={ICON_CLASS} aria-hidden='true' />
              Sync now
            </DropdownMenuItem>
          )}
          <DropdownMenuItem
            aria-label={unlinkAriaLabel(entries.length, peerName(entry.link.displayName))}
            onClick={() => onUnlink(entry.link)}
          >
            <Unlink className={ICON_CLASS} aria-hidden='true' />
            Unlink
          </DropdownMenuItem>
        </Fragment>
      ))}
    </>
  );
}

export function ContextSyncItems({ entries, onSync, onUnlink }: SyncMenuItemsProps) {
  if (entries.length === 0) return null;
  return (
    <>
      <ContextMenuSeparator />
      {entries.map((entry) => (
        <Fragment key={entry.link.linkId}>
          <ContextMenuLabel className='font-normal'>
            <PeerHeader {...entry} />
          </ContextMenuLabel>
          {entry.online && (
            <ContextMenuItem
              aria-label={`Sync with ${peerName(entry.link.displayName)}`}
              onClick={() => onSync(entry.link)}
            >
              <RefreshCw className={ICON_CLASS} aria-hidden='true' />
              Sync now
            </ContextMenuItem>
          )}
          <ContextMenuItem
            aria-label={unlinkAriaLabel(entries.length, peerName(entry.link.displayName))}
            onClick={() => onUnlink(entry.link)}
          >
            <Unlink className={ICON_CLASS} aria-hidden='true' />
            Unlink
          </ContextMenuItem>
        </Fragment>
      ))}
    </>
  );
}
