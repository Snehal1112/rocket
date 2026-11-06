import { Radar } from 'lucide-react';
import { useState } from 'react';
import { AddAddressDialog } from '@/components/network/AddAddressDialog';
import { NetworkPanel } from '@/components/network/NetworkPanel';
import { RemovePairedDialog } from '@/components/network/RemovePairedDialog';
import { StatusBarCount } from '@/components/status-bar/StatusBarCount';
import { StatusBarItem } from '@/components/status-bar/StatusBarItem';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import type { NetworkNode } from '@/lib/tauri-api';
import { cn } from '@/lib/utils';
import { selectOnlinePairedCount, useNetworkStore } from '@/stores/network-store';

/**
 * Status bar button with the number of online paired nodes. It opens the Network panel.
 * The Add and Remove dialogs live here, outside the Popover, so closing the panel to open
 * one of them does not unmount it. They render in portals, so they sit next to the item.
 */
export function NetworkStatusItem() {
  const online = useNetworkStore(selectOnlinePairedCount);
  const off = useNetworkStore((s) => s.status?.state === 'off');
  const open = useNetworkStore((s) => s.panelOpen);
  const setOpen = useNetworkStore((s) => s.setPanelOpen);
  const [addOpen, setAddOpen] = useState(false);
  const [removeTarget, setRemoveTarget] = useState<NetworkNode | null>(null);
  const label = off ? 'Team network, off' : `Team network, ${online} online`;

  function handleOpenChange(next: boolean) {
    setOpen(next);
    // The panel always shows fresh data, also when an event was missed.
    if (next) {
      void useNetworkStore.getState().refresh();
      void useNetworkStore.getState().refreshNodes();
    }
  }

  return (
    <>
      <Popover open={open} onOpenChange={handleOpenChange}>
        <PopoverTrigger asChild>
          <StatusBarItem title='Team network' aria-label={label}>
            <Radar className={cn('w-2.75 h-2.75', off && 'opacity-50')} aria-hidden='true' />
            {online > 0 && <StatusBarCount>{online}</StatusBarCount>}
          </StatusBarItem>
        </PopoverTrigger>
        <PopoverContent side='top' align='start' className='w-80 overflow-hidden p-0'>
          <NetworkPanel
            onAddAddress={() => {
              setOpen(false);
              setAddOpen(true);
            }}
            onRemove={(node) => {
              setOpen(false);
              setRemoveTarget(node);
            }}
          />
        </PopoverContent>
      </Popover>
      <AddAddressDialog open={addOpen} onOpenChange={setAddOpen} />
      <RemovePairedDialog node={removeTarget} onClose={() => setRemoveTarget(null)} />
    </>
  );
}
