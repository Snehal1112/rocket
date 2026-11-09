import { Unlink } from 'lucide-react';
import { ListRow } from '@/components/network/ListRow';
import { NetworkIdentity } from '@/components/network/NetworkIdentity';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { linkStateLabel } from '@/lib/network-sync-utils';
import { peerName } from '@/lib/network-utils';
import type { NetworkLink } from '@/lib/tauri-api';

interface LinkListProps {
  links: NetworkLink[];
  /** Names of the collections in the active workspace. Null while unknown: every link counts as here. */
  workspaceCollections?: readonly string[] | null;
  onUnlink: (link: NetworkLink) => void;
}

/** Rows of linked collections: collection name, node with short id, state, and an Unlink button. */
export function LinkList({ links, workspaceCollections = null, onUnlink }: LinkListProps) {
  if (links.length === 0) return null;
  return (
    <ul aria-label='Linked collections'>
      {links.map((link) => {
        const name = peerName(link.displayName);
        return (
          <ListRow key={link.linkId} className='px-3 py-1.5'>
            <div className='min-w-0 flex-1'>
              <p className='break-words text-sm'>{link.collectionName}</p>
              <NetworkIdentity name={name} shortId={link.shortId} />
            </div>
            <Badge variant='secondary' className='px-1.5 py-0 text-2xs'>
              {linkStateLabel(
                link.state,
                workspaceCollections?.includes(link.collectionName) ?? true,
              )}
            </Badge>
            <Button
              size='sm'
              variant='outline'
              className='h-6 gap-1 px-2 text-xs'
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
