import {
  AlertCircle,
  Archive,
  FileText,
  GitBranch,
  Loader2,
  MoreHorizontal,
  PackageCheck,
  PackageOpen,
  Trash2,
  X,
} from 'lucide-react';
import { useState } from 'react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Input } from '@/components/ui/input';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { useGitStore } from '@/stores/git-store';

/** Format a UTC timestamp string into a concise relative label. */
function formatAge(timestamp: string): string {
  const diff = Date.now() - new Date(timestamp).getTime();
  const minutes = Math.floor(diff / 60_000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  return new Date(timestamp).toLocaleDateString(undefined, { month: 'short', day: 'numeric' });
}

export function GitStashSection() {
  const [message, setMessage] = useState('');
  const [isSaving, setIsSaving] = useState(false);
  const [openStashIndex, setOpenStashIndex] = useState<number | null>(null);
  const [selectedIndices, setSelectedIndices] = useState<Set<number>>(new Set());
  const [hoveredIndex, setHoveredIndex] = useState<number | null>(null);
  const [isBatchRunning, setIsBatchRunning] = useState(false);

  const {
    stashes,
    saveStash,
    popStash,
    applyStash,
    dropStash,
    applyStashMany,
    popStashMany,
    dropStashMany,
    error,
    clearError,
  } = useGitStore();

  const isSelecting = selectedIndices.size > 0;

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

  const toggleSelect = (index: number, checked: boolean) => {
    setSelectedIndices((prev) => {
      const next = new Set(prev);
      if (checked) next.add(index);
      else next.delete(index);
      return next;
    });
  };

  const clearSelection = () => setSelectedIndices(new Set());

  const handleApplyMany = async () => {
    setIsBatchRunning(true);
    clearError();
    try {
      await applyStashMany([...selectedIndices]);
      if (!useGitStore.getState().error) clearSelection();
    } finally {
      setIsBatchRunning(false);
    }
  };

  const handlePopMany = async () => {
    setIsBatchRunning(true);
    clearError();
    try {
      await popStashMany([...selectedIndices]);
      if (!useGitStore.getState().error) clearSelection();
    } finally {
      setIsBatchRunning(false);
    }
  };

  const handleDropMany = async () => {
    setIsBatchRunning(true);
    clearError();
    try {
      await dropStashMany([...selectedIndices]);
      if (!useGitStore.getState().error) clearSelection();
    } finally {
      setIsBatchRunning(false);
    }
  };

  return (
    <div>
      {/* Section header */}
      <div className='flex items-center gap-1.5 px-3 py-2'>
        <Archive className='h-3.5 w-3.5 text-muted-foreground shrink-0' />
        <span className='text-[11px] uppercase tracking-[0.06em] font-semibold text-muted-foreground flex-1'>
          Stashes
        </span>
        {stashes.length > 0 && (
          <span className='text-[10px] font-mono bg-muted text-muted-foreground px-1.5 py-px rounded-full leading-none'>
            {stashes.length}
          </span>
        )}
      </div>

      {/* Save input row */}
      <div className='flex gap-1.5 px-3 pb-2'>
        <Input
          placeholder='Describe your stash…'
          value={message}
          onChange={(e) => setMessage(e.target.value)}
          className='h-7 text-xs flex-1'
          disabled={isSaving}
          onKeyDown={(e) => e.key === 'Enter' && !isSaving && void handleSave()}
        />
        <Button
          variant='outline'
          size='sm'
          className='h-7 px-2.5 text-xs shrink-0 gap-1'
          onClick={() => void handleSave()}
          disabled={!message.trim() || isSaving}
        >
          {isSaving && <Loader2 className='h-3 w-3 animate-spin' />}
          {isSaving ? 'Saving…' : 'Stash'}
        </Button>
      </div>

      {/* Error banner */}
      {error && (
        <div className='mx-3 mb-2 flex items-start gap-1.5 rounded-md border border-destructive/30 bg-destructive/10 px-2.5 py-1.5 text-xs text-destructive'>
          <AlertCircle className='mt-px h-3 w-3 shrink-0' />
          <span className='break-all leading-relaxed'>{error}</span>
        </div>
      )}

      {/* Empty state */}
      {stashes.length === 0 && !error && (
        <div className='flex flex-col items-center gap-2 px-4 py-5 text-center'>
          <Archive className='h-7 w-7 text-muted-foreground/20' />
          <p className='text-xs text-muted-foreground/50 leading-relaxed'>
            No stashes yet.
            <br />
            <span className='text-[11px]'>Type a message above and press Stash.</span>
          </p>
        </div>
      )}

      {/* Stash list */}
      <ul className='list-none p-0 m-0'>
        {stashes.map((stash) => {
          const isSelected = selectedIndices.has(stash.index);
          const showCheckbox = isSelecting || hoveredIndex === stash.index;

          return (
            <li
              key={stash.index}
              className='stash-row flex items-center gap-2 px-3 py-1.5 hover:bg-muted/50 transition-colors'
              onMouseEnter={() => setHoveredIndex(stash.index)}
              onMouseLeave={() => setHoveredIndex(null)}
            >
              {/* Checkbox / index badge slot — fixed width, no layout shift */}
              <div className='shrink-0 w-6 flex items-center justify-end'>
                {showCheckbox ? (
                  <input
                    type='checkbox'
                    className='h-3.5 w-3.5 accent-primary cursor-pointer'
                    checked={isSelected}
                    disabled={isBatchRunning}
                    onChange={(e) => toggleSelect(stash.index, e.target.checked)}
                  />
                ) : (
                  <span className='text-[10px] font-mono text-muted-foreground/35 select-none leading-none'>
                    @{stash.index}
                  </span>
                )}
              </div>

              {/* Message + metadata */}
              <div className='flex-1 min-w-0'>
                <TooltipProvider delayDuration={400}>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <p className='truncate text-[13px] font-mono leading-snug cursor-default'>
                        {stash.message}
                      </p>
                    </TooltipTrigger>
                    <TooltipContent side='bottom' className='max-w-64'>
                      <p className='break-words font-mono text-xs'>{stash.message}</p>
                    </TooltipContent>
                  </Tooltip>
                </TooltipProvider>

                {/* Metadata row: files · +ins −del · branch · age */}
                <div className='flex items-center gap-1 mt-0.5 flex-wrap'>
                  {stash.filesChanged > 0 && (
                    <TooltipProvider delayDuration={300}>
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <span className='flex items-center gap-0.5 text-[10px] text-muted-foreground/60 cursor-default'>
                            <FileText className='h-2.5 w-2.5 shrink-0' />
                            {stash.filesChanged} {stash.filesChanged === 1 ? 'file' : 'files'}
                          </span>
                        </TooltipTrigger>
                        <TooltipContent side='bottom' className='max-w-56 p-2'>
                          <ul className='space-y-0.5'>
                            {stash.changedFiles.slice(0, 10).map((f) => (
                              <li key={f} className='font-mono text-[11px] truncate'>
                                {f}
                              </li>
                            ))}
                            {stash.changedFiles.length > 10 && (
                              <li className='text-[11px] text-muted-foreground'>
                                +{stash.changedFiles.length - 10} more
                              </li>
                            )}
                          </ul>
                        </TooltipContent>
                      </Tooltip>
                    </TooltipProvider>
                  )}

                  {(stash.insertions > 0 || stash.deletions > 0) && (
                    <>
                      <span className='text-muted-foreground/25 text-[10px] select-none'>·</span>
                      <span className='text-[10px] font-mono text-emerald-600 dark:text-emerald-400'>
                        +{stash.insertions}
                      </span>
                      <span className='text-[10px] font-mono text-destructive'>
                        −{stash.deletions}
                      </span>
                    </>
                  )}

                  {stash.branch && (
                    <>
                      <span className='text-muted-foreground/25 text-[10px] select-none'>·</span>
                      <span className='flex items-center gap-0.5 text-[10px] text-muted-foreground/50 truncate max-w-[5rem]'>
                        <GitBranch className='h-2.5 w-2.5 shrink-0 text-muted-foreground/35' />
                        {stash.branch}
                      </span>
                    </>
                  )}

                  <span className='text-muted-foreground/25 text-[10px] select-none'>·</span>
                  <span className='text-[10px] text-muted-foreground/50 shrink-0'>
                    {formatAge(stash.timestamp)}
                  </span>
                </div>
              </div>

              {/* Per-row hover actions — hidden when row is selected */}
              <div
                className='stash-row-actions shrink-0'
                style={
                  isSelected
                    ? { display: 'none' }
                    : openStashIndex === stash.index
                      ? { display: 'flex' }
                      : undefined
                }
              >
                <DropdownMenu
                  open={openStashIndex === stash.index}
                  onOpenChange={(open) => setOpenStashIndex(open ? stash.index : null)}
                >
                  <DropdownMenuTrigger asChild>
                    <Button variant='ghost' size='icon' className='h-6 w-6'>
                      <MoreHorizontal className='h-3.5 w-3.5' />
                    </Button>
                  </DropdownMenuTrigger>
                  <DropdownMenuContent align='end' className='w-48'>
                    <DropdownMenuItem onClick={() => void popStash(stash.index)}>
                      <PackageOpen className='h-3.5 w-3.5 mr-2 shrink-0' />
                      <span>Pop</span>
                      <span className='ml-auto text-[11px] text-muted-foreground'>
                        apply + remove
                      </span>
                    </DropdownMenuItem>
                    <DropdownMenuItem onClick={() => void applyStash(stash.index)}>
                      <PackageCheck className='h-3.5 w-3.5 mr-2 shrink-0' />
                      <span>Apply</span>
                      <span className='ml-auto text-[11px] text-muted-foreground'>keep stash</span>
                    </DropdownMenuItem>
                    <DropdownMenuSeparator />
                    <DropdownMenuItem
                      className='text-destructive focus:text-destructive'
                      onClick={() => void dropStash(stash.index)}
                    >
                      <Trash2 className='h-3.5 w-3.5 mr-2 shrink-0' /> Drop
                    </DropdownMenuItem>
                  </DropdownMenuContent>
                </DropdownMenu>
              </div>
            </li>
          );
        })}
      </ul>

      {/* Sticky action bar — visible when 1+ stashes are selected */}
      {isSelecting && (
        <div className='flex items-center gap-1 px-3 py-1.5 border-t border-border/70 bg-background animate-in slide-in-from-bottom-2 duration-150'>
          <span className='text-[11px] text-muted-foreground flex-1'>
            {selectedIndices.size} selected
          </span>
          <Button
            variant='outline'
            size='sm'
            className='h-6 px-2 text-xs'
            onClick={() => void handleApplyMany()}
            disabled={isBatchRunning}
          >
            {isBatchRunning ? <Loader2 className='h-3 w-3 animate-spin' /> : 'Apply'}
          </Button>
          <Button
            variant='outline'
            size='sm'
            className='h-6 px-2 text-xs'
            onClick={() => void handlePopMany()}
            disabled={isBatchRunning}
          >
            {isBatchRunning ? <Loader2 className='h-3 w-3 animate-spin' /> : 'Pop'}
          </Button>
          <Button
            variant='outline'
            size='sm'
            className='h-6 px-2 text-xs text-destructive border-destructive/40 hover:bg-destructive/10'
            onClick={() => void handleDropMany()}
            disabled={isBatchRunning}
          >
            {isBatchRunning ? <Loader2 className='h-3 w-3 animate-spin' /> : 'Drop'}
          </Button>
          <Button
            variant='ghost'
            size='icon'
            className='h-6 w-6 shrink-0'
            onClick={clearSelection}
            disabled={isBatchRunning}
          >
            <X className='h-3 w-3' />
          </Button>
        </div>
      )}
    </div>
  );
}
