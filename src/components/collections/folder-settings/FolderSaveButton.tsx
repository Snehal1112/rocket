import { Check, Loader2, Save } from 'lucide-react';
import { Button } from '@/components/ui/button';
import type { SaveButtonState } from '@/hooks/use-save-button';
import { cn } from '@/lib/utils';

interface FolderSaveButtonProps {
  isDirty: boolean;
  isLoaded: boolean;
  saveState: SaveButtonState;
  onSave: () => void;
}

/** The one Save button and dirty indicator for the folder settings tab header. */
export function FolderSaveButton({ isDirty, isLoaded, saveState, onSave }: FolderSaveButtonProps) {
  return (
    <div className='flex items-center gap-2'>
      {isDirty && (
        <span className='flex items-center gap-1.5 text-xs text-muted-foreground'>
          <span aria-hidden='true' className='h-1.5 w-1.5 rounded-full bg-amber-500' />
          Unsaved changes
        </span>
      )}
      <Button
        size='sm'
        onClick={onSave}
        disabled={!isLoaded || !isDirty || saveState !== 'idle'}
        className={cn('gap-1.5', saveState === 'success' && 'text-green-600')}
      >
        {saveState === 'saving' ? (
          <Loader2 className='h-3.5 w-3.5 animate-spin' />
        ) : saveState === 'success' ? (
          <Check className='h-3.5 w-3.5' />
        ) : (
          <Save className='h-3.5 w-3.5' />
        )}
        {saveState === 'success' ? 'Saved' : 'Save'}
      </Button>
    </div>
  );
}
