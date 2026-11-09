import { Plus, Radar } from 'lucide-react';
import { type ReactNode, useEffect, useMemo, useState } from 'react';
import { toast } from 'sonner';
import { LinkList } from '@/components/network/LinkList';
import { ListRow } from '@/components/network/ListRow';
import { NetworkIdentity } from '@/components/network/NetworkIdentity';
import { NodeRow } from '@/components/network/NodeRow';
import { filterNodes, groupNodes, sortNodes } from '@/components/network/network-helpers';
import { SectionLabel } from '@/components/network/SectionLabel';
import { Button } from '@/components/ui/button';
import { EmptyState } from '@/components/ui/empty-state';
import { InitialsAvatar } from '@/components/ui/initials-avatar';
import { SearchInput } from '@/components/ui/search-input';
import { StateBlock } from '@/components/ui/state-block';
import { useWorkspaceCollectionNames } from '@/hooks/useWorkspaceCollectionNames';
import { errorText } from '@/lib/error-text';
import { formatPairingCode, PAIRING_FAILED_TOAST_ID } from '@/lib/network-utils';
import type { NetworkLink, NetworkNode } from '@/lib/tauri-api';
import { startNetworkPairing, useNetworkStore } from '@/stores/network-store';

const LISTENER_BANNER = "Others can't connect to you. Allow Rocket in your firewall settings.";
const NO_DISCOVERY = "Automatic discovery isn't available here.";
const EMPTY_TITLE = 'No Rocket nodes found on this network.';
const EMPTY_HINT = 'VPNs and some Wi-Fi networks block discovery.';

interface NetworkPanelProps {
  onAddAddress: () => void;
  onRemove: (node: NetworkNode) => void;
  onUnlink?: (link: NetworkLink) => void;
}

function Shell({ children }: { children: ReactNode }) {
  return (
    <div className='flex max-h-[28rem] flex-col text-sm'>
      <div className='flex items-center gap-2 border-b border-border px-3 py-2'>
        <Radar className='h-3.5 w-3.5 text-muted-foreground' aria-hidden='true' />
        <SectionLabel as='p'>Team network</SectionLabel>
      </div>
      {children}
    </div>
  );
}

interface NodeGroupProps {
  title: string;
  nodes: NetworkNode[];
  pairingDeviceId: string | null;
  onPair: (node: NetworkNode) => void;
  onRemove: (node: NetworkNode) => void;
}

function NodeGroup({ title, nodes, pairingDeviceId, onPair, onRemove }: NodeGroupProps) {
  if (nodes.length === 0) return null;
  return (
    <section className='py-1'>
      <SectionLabel className='px-3 py-1'>{`${title} (${nodes.length})`}</SectionLabel>
      <ul aria-label={title}>
        {nodes.map((node) => (
          <NodeRow
            key={node.deviceId}
            node={node}
            busy={pairingDeviceId === node.deviceId}
            onPair={onPair}
            onRemove={onRemove}
          />
        ))}
      </ul>
    </section>
  );
}

// The body of the Network popover. The host mounts the Add and Remove dialogs.
export function NetworkPanel({ onAddAddress, onRemove, onUnlink }: NetworkPanelProps) {
  const status = useNetworkStore((s) => s.status);
  const nodes = useNetworkStore((s) => s.nodes);
  const incoming = useNetworkStore((s) => s.incoming);
  const links = useNetworkStore((s) => s.links);
  const workspaceCollections = useWorkspaceCollectionNames();
  const error = useNetworkStore((s) => s.error);
  const [query, setQuery] = useState('');
  // The link states are derived when the list loads, so opening the panel reloads them.
  useEffect(() => {
    void useNetworkStore.getState().refreshLinks();
  }, []);
  const [pairingDeviceId, setPairingDeviceId] = useState<string | null>(null);
  const groups = useMemo(() => groupNodes(sortNodes(filterNodes(nodes, query))), [nodes, query]);

  async function pair(node: NetworkNode) {
    setPairingDeviceId(node.deviceId);
    try {
      await startNetworkPairing(node);
    } catch (err) {
      toast.error(errorText(err), { id: PAIRING_FAILED_TOAST_ID });
    } finally {
      setPairingDeviceId(null);
    }
  }

  function review(pairingId: string) {
    const store = useNetworkStore.getState();
    store.openPairingDialog(pairingId);
    store.setPanelOpen(false);
  }

  if (!status) {
    return (
      <Shell>
        <div className='p-3'>
          <StateBlock tone={error ? 'error' : 'loading'}>{error ?? 'Loading...'}</StateBlock>
        </div>
      </Shell>
    );
  }

  if (status.state === 'off') {
    return (
      <Shell>
        <div className='p-3'>
          <StateBlock tone='empty'>{status.offReason ?? 'Team network is off.'}</StateBlock>
        </div>
      </Shell>
    );
  }

  const shown = groups.paired.length + groups.nearby.length;

  return (
    <Shell>
      {!status.listenerAvailable && (
        <div role='alert' className='border-b border-border bg-warning/10 px-3 py-2 text-xs'>
          {LISTENER_BANNER}
        </div>
      )}
      {!status.discoveryAvailable && (
        <p className='border-b border-border px-3 py-2 text-xs text-muted-foreground'>
          {NO_DISCOVERY}
        </p>
      )}
      <div className='border-b border-border p-2'>
        <SearchInput
          value={query}
          onChange={setQuery}
          label='Search nodes'
          placeholder='Search by name or ID'
        />
      </div>
      <div className='min-h-0 flex-1 overflow-y-auto'>
        {incoming.length > 0 && (
          <section className='border-b border-border py-1'>
            <SectionLabel className='px-3 py-1'>{`Requests (${incoming.length})`}</SectionLabel>
            <ul aria-label='Requests'>
              {incoming.map((request) => (
                <ListRow key={request.pairingId} className='px-3 py-1.5'>
                  <InitialsAvatar name={request.displayName} />
                  <NetworkIdentity
                    name={request.displayName}
                    shortId={request.shortId}
                    extra={formatPairingCode(request.code)}
                  />
                  <Button
                    size='sm'
                    variant='outline'
                    className='h-6 px-2 text-xs'
                    aria-label={`Review request from ${request.displayName} (${request.shortId})`}
                    onClick={() => review(request.pairingId)}
                  >
                    Review
                  </Button>
                </ListRow>
              ))}
            </ul>
          </section>
        )}
        {nodes.length === 0 ? (
          <EmptyState
            cause='none-yet'
            size='compact'
            illustration={false}
            title={EMPTY_TITLE}
            reason={status.discoveryAvailable ? EMPTY_HINT : undefined}
          />
        ) : shown === 0 ? (
          <div className='p-3'>
            <StateBlock tone='empty'>{`No nodes match "${query.trim()}".`}</StateBlock>
          </div>
        ) : (
          <>
            <NodeGroup
              title='Paired'
              nodes={groups.paired}
              pairingDeviceId={pairingDeviceId}
              onPair={(n) => void pair(n)}
              onRemove={onRemove}
            />
            <NodeGroup
              title='Nearby'
              nodes={groups.nearby}
              pairingDeviceId={pairingDeviceId}
              onPair={(n) => void pair(n)}
              onRemove={onRemove}
            />
          </>
        )}
        {links.length > 0 && onUnlink && (
          <section className='border-t border-border py-1'>
            <SectionLabel className='px-3 py-1'>{`Linked collections (${links.length})`}</SectionLabel>
            <LinkList
              links={links}
              workspaceCollections={workspaceCollections}
              onUnlink={onUnlink}
            />
          </section>
        )}
      </div>
      <div className='border-t border-border p-1'>
        <Button
          variant='ghost'
          size='sm'
          className='w-full justify-start gap-2 text-xs'
          onClick={onAddAddress}
        >
          <Plus className='h-3.5 w-3.5' aria-hidden='true' />
          Add by address
        </Button>
      </div>
    </Shell>
  );
}
