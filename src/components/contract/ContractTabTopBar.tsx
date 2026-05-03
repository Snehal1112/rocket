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
    <div className='flex items-center justify-between px-4 py-2.5 border-b border-border shrink-0 bg-card'>
      <div className='flex items-center gap-2 min-w-0'>
        {/* Back button — all non-list views */}
        {view !== 'list' && onBack && (
          <Button
            variant='ghost'
            size='sm'
            className='h-7 px-2 -ml-1 gap-0.5 text-muted-foreground shrink-0'
            onClick={onBack}
          >
            <ChevronLeft className='h-4 w-4' />
            Back
          </Button>
        )}

        {/* List view title */}
        {view === 'list' && (
          <>
            <div className='w-5 h-5 rounded flex items-center justify-center shrink-0 bg-primary/10'>
              <Lock className='h-3 w-3 text-primary' />
            </div>
            <div className='min-w-0 flex items-baseline gap-1.5'>
              <p className='text-sm font-medium text-foreground leading-tight'>Contracts</p>
              <span className='text-muted-foreground/50 text-xs select-none'>·</span>
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
        <Button size='sm' className='h-7 text-xs shrink-0 gap-1' onClick={onNew}>
          <Plus className='h-3.5 w-3.5' />
          New contract
        </Button>
      )}
    </div>
  );
}
