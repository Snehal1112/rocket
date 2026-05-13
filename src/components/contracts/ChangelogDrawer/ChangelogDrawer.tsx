import { useRef } from 'react';
import { Sheet, SheetContent } from '@/components/ui/sheet';
import { useChangelogEntries } from '@/hooks/useChangelogEntries';
import { useContractsStore } from '@/stores/contracts/contractsSlice';
import { useDrawerStore } from '@/stores/contracts/drawerSlice';
import type { Contract } from '@/types/contracts';
import { ChangelogDrawerFooter } from './ChangelogDrawerFooter';
import { ChangelogDrawerHeader } from './ChangelogDrawerHeader';
import { ChangelogDrawerToolbar } from './ChangelogDrawerToolbar';
import { ChangelogTimeline } from './ChangelogTimeline';

export function ChangelogDrawer() {
  const isOpen = useDrawerStore((s) => s.isOpen);
  const contractId = useDrawerStore((s) => s.contractId);
  const close = useDrawerStore((s) => s.close);
  const clearContract = useDrawerStore((s) => s.clearContract);
  const contract = useContractsStore((s) => (contractId ? s.byId[contractId] : null));

  // Keep last known contract alive during the close animation so the Sheet
  // can complete its slide-out before we null-guard and unmount the content.
  const lastContractRef = useRef<Contract | null>(null);
  if (contract) lastContractRef.current = contract;
  const activeContract = contract ?? lastContractRef.current;

  if (!activeContract) return null;

  return (
    <Sheet open={isOpen} onOpenChange={(open) => !open && close()}>
      <SheetContent
        side='right'
        className='w-full sm:w-[540px] sm:max-w-[540px] p-0 flex flex-col gap-0'
        aria-labelledby='changelog-drawer-title'
        onCloseAutoFocus={() => clearContract()}
      >
        <ChangelogDrawerInner contract={activeContract} onClose={close} />
      </SheetContent>
    </Sheet>
  );
}

function ChangelogDrawerInner({ contract, onClose }: { contract: Contract; onClose: () => void }) {
  const groups = useChangelogEntries(contract);
  const shownCount = groups.reduce((acc, g) => acc + g.items.length, 0);

  return (
    <>
      <ChangelogDrawerHeader contract={contract} onClose={onClose} />
      <ChangelogDrawerToolbar contract={contract} />
      <ChangelogTimeline groups={groups} />
      <ChangelogDrawerFooter contract={contract} shownCount={shownCount} groups={groups} />
    </>
  );
}
