import { ChevronLeft, Lock, Plus } from 'lucide-react';
import { Button } from '@/components/ui/button';

type ViewKind = 'list' | 'create' | 'edit' | 'changelog';

interface ContractTabTopBarProps {
  collectionName: string;
  view: ViewKind;
  viewTitle?: string;
  onBack?: () => void;
  onNew?: () => void;
}

export function ContractTabTopBar({
  collectionName,
  view,
  viewTitle,
  onBack,
  onNew,
}: ContractTabTopBarProps) {
  return (
    <div className='flex items-center justify-between px-5 py-3 border-b border-border shrink-0 bg-background'>
      <div className='flex items-center gap-2.5 min-w-0'>
        {/* Back button — all non-list views */}
        {view !== 'list' && onBack && (
          <Button
            variant='ghost'
            size='sm'
            className='h-7 px-2 -ml-1 text-muted-foreground shrink-0'
            onClick={onBack}
          >
            <ChevronLeft className='h-4 w-4' />
            Back
          </Button>
        )}

        {/* List view title */}
        {view === 'list' && (
          <>
            <div className='w-6 h-6 rounded-md bg-primary/10 flex items-center justify-center shrink-0'>
              <Lock className='h-3.5 w-3.5 text-primary' />
            </div>
            <div className='min-w-0'>
              <p className='text-sm font-medium text-foreground leading-tight'>Contracts</p>
              <p className='text-xs text-muted-foreground leading-tight truncate'>
                {collectionName}
              </p>
            </div>
          </>
        )}

        {/* Create / edit / changelog title */}
        {view !== 'list' && viewTitle && (
          <p className='text-sm font-medium text-foreground truncate'>{viewTitle}</p>
        )}
      </div>

      {/* New contract button — list view only */}
      {view === 'list' && onNew && (
        <Button size='sm' className='h-7 text-xs shrink-0' onClick={onNew}>
          <Plus className='h-3.5 w-3.5 mr-1' />
          New contract
        </Button>
      )}
    </div>
  );
}
