import { Clock, X } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { SheetDescription, SheetTitle } from '@/components/ui/sheet';
import type { Contract } from '@/types/contracts';

interface ChangelogDrawerHeaderProps {
  contract: Contract;
  onClose: () => void;
}

export function ChangelogDrawerHeader({ contract, onClose }: ChangelogDrawerHeaderProps) {
  const sinceSignedCount = contract.changelog.filter((e) => e.at >= contract.effectiveAt).length;

  return (
    <div className='p-5 pb-3.5 border-b border-border flex justify-between items-start gap-3'>
      <SheetTitle className='sr-only'>Changelog for {contract.name}</SheetTitle>
      <SheetDescription className='sr-only'>Changelog entries for this contract</SheetDescription>
      <div>
        <div
          id='changelog-drawer-title'
          className='text-[15px] font-semibold tracking-tight flex items-center gap-2'
        >
          <Clock className='w-4 h-4' aria-hidden='true' />
          Changelog · {contract.name}
        </div>
        <div className='text-[12px] text-muted-foreground mt-0.5'>
          v{contract.version} · {contract.changelog.length} entries · {sinceSignedCount} since
          published
        </div>
      </div>
      <Button
        variant='ghost'
        size='icon'
        className='h-7 w-7 rounded-md shrink-0'
        aria-label='Close changelog'
        onClick={onClose}
      >
        <X className='w-4 h-4' aria-hidden='true' />
      </Button>
    </div>
  );
}
