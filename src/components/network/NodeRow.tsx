import { MoreHorizontal, Trash2 } from 'lucide-react';
import { ListRow } from '@/components/network/ListRow';
import { NetworkIdentity } from '@/components/network/NetworkIdentity';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { InitialsAvatar } from '@/components/ui/initials-avatar';
import type { NetworkNode, NetworkNodeStatus } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';

interface Chip {
  label: string;
  variant: 'secondary' | 'outline' | 'warning' | 'success';
}

// A nearby node that can be paired has no chip. Its Pair button says enough.
const CHIPS: Partial<Record<NetworkNodeStatus, Chip>> = {
  online: { label: 'Online', variant: 'success' },
  offline: { label: 'Offline', variant: 'outline' },
  pairing: { label: 'Pairing…', variant: 'secondary' },
  needs_update: { label: 'Needs update', variant: 'warning' },
};

interface NodeRowProps {
  node: NetworkNode;
  /** A pairing request for this node is in flight. */
  busy: boolean;
  onPair: (node: NetworkNode) => void;
  onRemove: (node: NetworkNode) => void;
}

// One node: initials, name, short id, status chip, and Pair or a menu.
export function NodeRow({ node, busy, onPair, onRemove }: NodeRowProps) {
  const chip = CHIPS[node.status];
  const label = `${node.displayName} (${node.shortId})`;
  const canPair = node.status === 'available';

  return (
    <ListRow className={cn('px-3 py-1.5', node.status === 'offline' && 'opacity-60')}>
      <InitialsAvatar name={node.displayName} initials={node.initials} />
      <NetworkIdentity name={node.displayName} shortId={node.shortId} />
      {chip && (
        <Badge variant={chip.variant} className='px-1.5 py-0 text-2xs'>
          {chip.label}
        </Badge>
      )}
      {node.group === 'nearby' ? (
        <Button
          size='sm'
          variant='outline'
          className='h-6 px-2 text-xs'
          disabled={!canPair || busy}
          aria-label={`Pair with ${label}`}
          onClick={() => onPair(node)}
        >
          Pair
        </Button>
      ) : (
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button
              size='icon'
              variant='ghost'
              className='h-6 w-6'
              aria-label={`Actions for ${label}`}
            >
              <MoreHorizontal className='h-3.5 w-3.5' aria-hidden='true' />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align='end'>
            <DropdownMenuItem onSelect={() => onRemove(node)}>
              <Trash2 className='h-3.5 w-3.5' aria-hidden='true' />
              Remove
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      )}
    </ListRow>
  );
}
