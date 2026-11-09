import { RefreshCw, Unlink } from 'lucide-react';
import { Fragment } from 'react';
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
import { linkStatusText } from '@/lib/network-sync-utils';
import { peerName } from '@/lib/network-utils';
import type { NetworkLink } from '@/lib/tauri-api';

interface SyncMenuItemsProps {
  entries: CollectionSyncEntry[];
  onSync: (link: NetworkLink) => void;
  onUnlink: (link: NetworkLink) => void;
}

const LABEL_CLASS = 'text-2xs font-normal text-muted-foreground';
const ICON_CLASS = 'h-3.5 w-3.5 mr-2';

/** "Unlink" for one link, "Unlink from Ben" when several peers are linked. */
function unlinkLabel(count: number, name: string): string {
  return count === 1 ? 'Unlink' : `Unlink from ${name}`;
}

export function DropdownSyncItems({ entries, onSync, onUnlink }: SyncMenuItemsProps) {
  if (entries.length === 0) return null;
  return (
    <>
      <DropdownMenuSeparator />
      {entries.map(({ link, online }) => (
        <Fragment key={link.linkId}>
          <DropdownMenuLabel className={LABEL_CLASS}>
            {linkStatusText(link.state, link.displayName)}
          </DropdownMenuLabel>
          {online && (
            <DropdownMenuItem onClick={() => onSync(link)}>
              <RefreshCw className={ICON_CLASS} aria-hidden='true' />
              {`Sync with ${peerName(link.displayName)}`}
            </DropdownMenuItem>
          )}
          <DropdownMenuItem onClick={() => onUnlink(link)}>
            <Unlink className={ICON_CLASS} aria-hidden='true' />
            {unlinkLabel(entries.length, peerName(link.displayName))}
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
      {entries.map(({ link, online }) => (
        <Fragment key={link.linkId}>
          <ContextMenuLabel className={LABEL_CLASS}>
            {linkStatusText(link.state, link.displayName)}
          </ContextMenuLabel>
          {online && (
            <ContextMenuItem onClick={() => onSync(link)}>
              <RefreshCw className={ICON_CLASS} aria-hidden='true' />
              {`Sync with ${peerName(link.displayName)}`}
            </ContextMenuItem>
          )}
          <ContextMenuItem onClick={() => onUnlink(link)}>
            <Unlink className={ICON_CLASS} aria-hidden='true' />
            {unlinkLabel(entries.length, peerName(link.displayName))}
          </ContextMenuItem>
        </Fragment>
      ))}
    </>
  );
}
