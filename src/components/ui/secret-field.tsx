import { useEffect, useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { SavedPill } from '@/components/ui/saved-pill';

export interface SecretFieldProps {
  id: string;
  value: string;
  onValueChange: (value: string) => void;
  hasSaved: boolean;
  markedForRemoval: boolean;
  onToggleRemove: () => void;
  placeholder?: string;
  'aria-invalid'?: boolean;
  'aria-describedby'?: string;
}

/**
 * A secret entry that knows about an already stored credential.
 * Saved state shows a pill with Replace and Remove text buttons.
 */
export function SecretField({
  id,
  value,
  onValueChange,
  hasSaved,
  markedForRemoval,
  onToggleRemove,
  placeholder = 'Paste the token',
  'aria-invalid': ariaInvalid,
  'aria-describedby': ariaDescribedBy,
}: SecretFieldProps) {
  const [replacing, setReplacing] = useState(false);

  useEffect(() => {
    if (replacing) document.getElementById(id)?.focus();
  }, [replacing, id]);

  if (hasSaved && markedForRemoval) {
    return (
      <div id={id} className='flex items-center gap-2'>
        <span className='min-w-0 flex-1 break-words text-xs text-destructive'>
          The saved credential will be removed when you save.
        </span>
        <Button variant='outline' size='sm' className='h-7' onClick={onToggleRemove}>
          Undo
        </Button>
      </div>
    );
  }

  const showInput = !hasSaved || replacing || value !== '';

  if (!showInput) {
    return (
      <div id={id} className='flex items-center gap-2'>
        <div className='min-w-0 flex-1'>
          <SavedPill persistent>Saved in the keychain</SavedPill>
        </div>
        <Button variant='outline' size='sm' className='h-7' onClick={() => setReplacing(true)}>
          Replace
        </Button>
        <Button
          variant='outline'
          size='sm'
          className='h-7'
          onClick={() => {
            onValueChange('');
            onToggleRemove();
          }}
        >
          Remove
        </Button>
      </div>
    );
  }

  return (
    <div className='flex items-center gap-2'>
      <Input
        id={id}
        type='password'
        autoComplete='off'
        value={value}
        onChange={(e) => onValueChange(e.target.value)}
        placeholder={placeholder}
        aria-invalid={ariaInvalid}
        aria-describedby={ariaDescribedBy}
        className='h-8 text-sm'
      />
      {hasSaved && (
        <Button
          variant='outline'
          size='sm'
          className='h-7'
          onClick={() => {
            onValueChange('');
            setReplacing(false);
          }}
        >
          Cancel
        </Button>
      )}
    </div>
  );
}
