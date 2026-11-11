import { Unlink } from 'lucide-react';
import { ListRow } from '@/components/network/ListRow';
import { linkDotClass } from '@/components/network/network-helpers';
import { Button } from '@/components/ui/button';
import { linkStateLabel } from '@/lib/network-sync-utils';
import { peerName } from '@/lib/network-utils';
import type { NetworkLink } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';

interface LinkListProps {
  links: NetworkLink[];
  /** Names of the collections in the active workspace. Null while unknown: every link counts as here. */
  workspaceCollections?: readonly string[] | null;
  onUnlink: (link: NetworkLink) => void;
}

/**
 * Rows of linked collections. Line one: state dot, collection name and Unlink. Line two: the
 * state in words, then the node name and its short id.
 */
export function LinkList({ links, workspaceCollections = null, onUnlink }: LinkListProps) {
  if (links.length === 0) return null;
  return (
    <ul aria-label='Linked collections'>
      {links.map((link) => {
        const name = peerName(link.displayName);
        const inWorkspace = workspaceCollections?.includes(link.collectionName) ?? true;
        return (
          <ListRow key={link.linkId} className='items-start gap-2.5 px-3 py-2'>
            <span
              aria-hidden='true'
              className={cn(
                'mt-1.5 h-2 w-2 shrink-0 rounded-full',
                // A collection from another workspace has no live state here.
                inWorkspace ? linkDotClass(link.state) : 'bg-muted-foreground/40',
              )}
            />
            <div className='min-w-0 flex-1'>
              <p className='break-words text-sm leading-snug'>{link.collectionName}</p>
              <p className='mt-0.5 flex min-w-0 flex-wrap items-baseline gap-x-1.5 text-2xs text-muted-foreground'>
                <span className='font-medium text-foreground/80'>
                  {linkStateLabel(link.state, inWorkspace)}
                </span>
                <span className='truncate'>{name}</span>
                <span className='font-mono'>{link.shortId}</span>
              </p>
            </div>
            <Button
              size='sm'
              variant='ghost'
              className='-mr-1 h-6 shrink-0 gap-1 px-2 text-xs text-muted-foreground hover:text-foreground'
              aria-label={`Unlink ${link.collectionName} from ${name} (${link.shortId})`}
              onClick={() => onUnlink(link)}
            >
              <Unlink className='h-3 w-3' aria-hidden='true' />
              Unlink
            </Button>
          </ListRow>
        );
      })}
    </ul>
  );
}
