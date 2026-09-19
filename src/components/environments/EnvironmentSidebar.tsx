// src/components/environments/EnvironmentSidebar.tsx

import { Plus, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { cn } from '@/lib/utils';
import { InlineEnvName } from './InlineEnvName';

interface EnvironmentSidebarProps {
  environments: { name: string }[];
  selectedName: string | null;
  /** The environment currently active/applied elsewhere in the app — distinct from `selectedName`, which is just what's open for editing here. */
  activeName?: string | null;
  onSelect: (name: string) => void;
  onRename: (oldName: string, newName: string) => Promise<void>;
  onAdd: (name: string) => void;
  onDelete: () => void;
  canDelete: boolean;
  className?: string;
}

export function EnvironmentSidebar({
  environments,
  selectedName,
  activeName = null,
  onSelect,
  onRename,
  onAdd,
  onDelete,
  canDelete,
  className,
}: EnvironmentSidebarProps) {
  const [isAdding, setIsAdding] = useState(false);
  const [draftName, setDraftName] = useState('');

  const cancelAdd = () => {
    setIsAdding(false);
    setDraftName('');
  };

  const commitAdd = () => {
    const trimmed = draftName.trim();
    cancelAdd();
    if (trimmed) onAdd(trimmed);
  };

  return (
    <div
      className={cn('w-52 shrink-0 border-r border-border/60 flex flex-col bg-card/50', className)}
    >
      <div className='px-3 pt-3 pb-1.5'>
        <p className='text-[10px] font-semibold uppercase tracking-[0.06em] text-muted-foreground/70'>
          Environments
        </p>
      </div>
      <ScrollArea className='flex-1 px-2'>
        <div className='pb-2 space-y-0.5'>
          {environments.map((env) => (
            <InlineEnvName
              key={env.name}
              name={env.name}
              isSelected={selectedName === env.name}
              isActive={activeName === env.name}
              existingNames={environments.map((e) => e.name)}
              onClick={() => onSelect(env.name)}
              onRename={(newName) => onRename(env.name, newName)}
            />
          ))}
          {isAdding && (
            <Input
              autoFocus
              className='h-7 text-sm'
              placeholder='Environment name'
              value={draftName}
              onChange={(e) => setDraftName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') commitAdd();
                if (e.key === 'Escape') cancelAdd();
              }}
              onBlur={cancelAdd}
            />
          )}
        </div>
      </ScrollArea>
      <div className='p-2 border-t border-border/60 flex gap-1'>
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant='ghost'
                size='icon'
                className='h-7 w-7'
                onClick={() => setIsAdding(true)}
                aria-label='Add environment'
              >
                <Plus className='h-3.5 w-3.5' />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Add environment</TooltipContent>
          </Tooltip>
        </TooltipProvider>
        <TooltipProvider>
          <Tooltip>
            <TooltipTrigger asChild>
              <Button
                variant='ghost'
                size='icon'
                className='h-7 w-7 text-destructive hover:text-destructive'
                onClick={onDelete}
                disabled={!canDelete}
                aria-label='Delete environment'
              >
                <Trash2 className='h-3.5 w-3.5' />
              </Button>
            </TooltipTrigger>
            <TooltipContent>Delete environment</TooltipContent>
          </Tooltip>
        </TooltipProvider>
      </div>
    </div>
  );
}
