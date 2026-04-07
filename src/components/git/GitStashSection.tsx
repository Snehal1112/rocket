import { AlertCircle, Archive, Loader2, MoreHorizontal } from 'lucide-react';
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
  const [isSaving, setIsSaving] = useState(false);
  const { stashes, saveStash, popStash, applyStash, dropStash, error, clearError } = useGitStore();

  const handleSave = async () => {
    if (!message.trim()) return;
    clearError();
    setIsSaving(true);
    try {
      await saveStash(message.trim());
      // Only clear input if the save succeeded.
      if (!useGitStore.getState().error) {
        setMessage('');
      }
    } finally {
      setIsSaving(false);
    }
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

      {/* Save input row. */}
      <div className='flex gap-1 px-2 pb-1'>
        <Input
          placeholder='Stash message...'
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          className='h-7 text-sm'
          disabled={isSaving}
          onKeyDown={(e) => e.key === 'Enter' && !isSaving && void handleSave()}
        />
        <Button
          variant='outline'
          size='sm'
          className='h-7 text-sm shrink-0'
          onClick={() => void handleSave()}
          disabled={!message.trim() || isSaving}
        >
          {isSaving ? <Loader2 className='h-3.5 w-3.5 animate-spin' /> : null}
          {isSaving ? 'Saving…' : 'Save'}
        </Button>
      </div>

      {/* Error banner — shown when a stash operation fails. */}
      {error && (
        <div className='mx-2 mb-1.5 flex items-start gap-1.5 rounded-md border border-destructive/30 bg-destructive/10 px-2 py-1.5 text-xs text-destructive'>
          <AlertCircle className='mt-px h-3 w-3 shrink-0' />
          <span className='break-all'>{error}</span>
        </div>
      )}

      {/* Empty state. */}
      {stashes.length === 0 && (
        <p className='px-2 py-1 text-xs text-muted-foreground/60'>No stashes yet.</p>
      )}

      {/* Stash list. */}
      {stashes.map((stash) => (
        <div
          key={stash.index}
          className='stash-row flex items-center gap-1.5 px-2 py-[3px] hover:bg-muted/50 text-[13px]'
        >
          <TooltipProvider delayDuration={300}>
            <Tooltip>
              <TooltipTrigger asChild>
                <span className='flex-1 min-w-0 truncate font-mono text-[13px] cursor-default'>
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
          <div className='stash-row-actions ml-auto shrink-0'>
            <DropdownMenu>
              <DropdownMenuTrigger asChild>
                <Button variant='ghost' size='icon' className='h-5 w-5'>
                  <MoreHorizontal className='h-3.5 w-3.5' />
                </Button>
              </DropdownMenuTrigger>
              <DropdownMenuContent align='end'>
                <DropdownMenuItem onClick={() => popStash(stash.index)}>Pop</DropdownMenuItem>
                <DropdownMenuItem onClick={() => applyStash(stash.index)}>Apply</DropdownMenuItem>
                <DropdownMenuItem
                  className='text-destructive'
                  onClick={() => dropStash(stash.index)}
                >
                  Drop
                </DropdownMenuItem>
              </DropdownMenuContent>
            </DropdownMenu>
          </div>
        </div>
      ))}
    </div>
  );
}
