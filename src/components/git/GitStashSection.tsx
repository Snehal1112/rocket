import { Archive, MoreHorizontal } from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { useGitStore } from '@/stores/git-store';

export function GitStashSection() {
  const [message, setMessage] = useState('');
  const { stashes, saveStash, popStash, applyStash, dropStash } = useGitStore();

  const handleSave = async () => {
    if (!message.trim()) return;
    await saveStash(message.trim());
    setMessage('');
  };

  return (
    <div>
      <div className='flex items-center gap-1.5 px-2 py-1.5'>
        <Archive className='h-3.5 w-3.5 text-muted-foreground' />
        <span className='text-[11px] uppercase tracking-[0.06em] font-semibold text-muted-foreground'>
          Stash
          <span className='ml-1.5 font-mono normal-case tracking-normal opacity-70'>
            {stashes.length}
          </span>
        </span>
      </div>
      <div className='flex gap-1 px-2 pb-1'>
        <Input
          placeholder='Stash message...'
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          className='h-7 text-sm'
          onKeyDown={(e) => e.key === 'Enter' && handleSave()}
        />
        <Button
          variant='outline'
          size='sm'
          className='h-7 text-sm shrink-0'
          onClick={handleSave}
          disabled={!message.trim()}
        >
          Save
        </Button>
      </div>
      {stashes.map((stash) => (
        <div
          key={stash.index}
          className='group flex items-center gap-1.5 px-2 py-[3px] hover:bg-muted/50 text-[13px]'
        >
          <TooltipProvider delayDuration={300}>
            <Tooltip>
              <TooltipTrigger asChild>
                <span className='truncate font-mono text-[13px] cursor-default'>
                  {stash.message}
                </span>
              </TooltipTrigger>
              <TooltipContent>
                <p>{stash.message}</p>
              </TooltipContent>
            </Tooltip>
          </TooltipProvider>
          {stash.branch && (
            <span className='text-[11px] text-muted-foreground/60 shrink-0'>{stash.branch}</span>
          )}
          <DropdownMenu>
            <DropdownMenuTrigger asChild>
              <Button
                variant='ghost'
                size='icon'
                className='ml-auto h-5 w-5 opacity-0 group-hover:opacity-100'
              >
                <MoreHorizontal className='h-3.5 w-3.5' />
              </Button>
            </DropdownMenuTrigger>
            <DropdownMenuContent align='end'>
              <DropdownMenuItem onClick={() => popStash(stash.index)}>Pop</DropdownMenuItem>
              <DropdownMenuItem onClick={() => applyStash(stash.index)}>Apply</DropdownMenuItem>
              <DropdownMenuItem className='text-destructive' onClick={() => dropStash(stash.index)}>
                Drop
              </DropdownMenuItem>
            </DropdownMenuContent>
          </DropdownMenu>
        </div>
      ))}
    </div>
  );
}
