// src/components/environments/InlineEnvName.tsx

import { Check, X } from 'lucide-react';
import { useState } from 'react';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { cn } from '@/lib/utils';

interface InlineEnvNameProps {
  name: string;
  isSelected: boolean;
  existingNames: string[];
  onClick: () => void;
  onRename: (newName: string) => Promise<void>;
}

export function InlineEnvName({
  name,
  isSelected,
  existingNames,
  onClick,
  onRename,
}: InlineEnvNameProps) {
  const [isEditing, setIsEditing] = useState(false);
  const [draftName, setDraftName] = useState('');

  const enterEdit = () => {
    onClick();
    setDraftName(name);
    setIsEditing(true);
  };

  const cancel = () => {
    setIsEditing(false);
    setDraftName('');
  };

  const accept = async () => {
    const trimmed = draftName.trim();
    if (!trimmed || trimmed === name) {
      cancel();
      return;
    }
    if (existingNames.some((n) => n !== name && n === trimmed)) {
      toast.warning('An environment with that name already exists');
      return;
    }
    try {
      await onRename(trimmed);
      setIsEditing(false);
    } catch {
      // Keep edit mode open — caller shows the toast.
    }
  };

  if (isEditing) {
    return (
      <div className='flex items-center gap-1 px-1'>
        <Input
          autoFocus
          className='h-7 text-sm flex-1 min-w-0'
          value={draftName}
          onChange={(e) => setDraftName(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === 'Enter') void accept();
            if (e.key === 'Escape') cancel();
          }}
          onBlur={cancel}
        />
        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6 shrink-0 text-green-500 hover:text-green-600'
          onMouseDown={(e) => e.preventDefault()}
          onClick={() => void accept()}
          title='Accept rename'
        >
          <Check className='h-3.5 w-3.5' />
        </Button>
        <Button
          variant='ghost'
          size='icon'
          className='h-6 w-6 shrink-0 text-destructive hover:text-destructive/80'
          onMouseDown={(e) => e.preventDefault()}
          onClick={cancel}
          title='Cancel rename'
        >
          <X className='h-3.5 w-3.5' />
        </Button>
      </div>
    );
  }

  return (
    <button
      type='button'
      onClick={onClick}
      onDoubleClick={enterEdit}
      className={cn(
        'w-full text-left px-2 py-1.5 text-sm rounded-sm truncate',
        isSelected ? 'bg-accent text-accent-foreground' : 'text-foreground hover:bg-muted/60',
      )}
    >
      {name}
    </button>
  );
}
