import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import type { CommitInfo } from '@/lib/tauri-api';
import { formatRelativeTime } from '@/lib/relative-time';
import { useGitStore } from '@/stores/git-store-context';

interface GitCommitLogProps {
  onCommitClick: (commit: CommitInfo) => void;
}

export function GitCommitLog({ onCommitClick }: GitCommitLogProps) {
  const commitLog = useGitStore((state) => state.commitLog);
  const refreshLog = useGitStore((state) => state.refreshLog);
  const [limit, setLimit] = useState(50);

  const handleLoadMore = async () => {
    const newLimit = limit + 50;
    setLimit(newLimit);
    await refreshLog(newLimit);
  };

  if (commitLog.length === 0) {
    return (
      <div className='flex items-center justify-center h-20 text-xs text-muted-foreground'>
        No commits yet.
      </div>
    );
  }

  return (
    <ScrollArea className='h-full'>
      <div className='p-1'>
        {commitLog.map((commit) => (
          // biome-ignore lint/a11y/useSemanticElements: contains nested button (copy SHA), so role="button" div is intentional
          <div
            key={commit.fullId}
            role='button'
            tabIndex={0}
            className='flex items-start gap-2 px-2 py-1.5 rounded hover:bg-muted/50 cursor-pointer'
            onClick={() => onCommitClick(commit)}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                onCommitClick(commit);
              }
            }}
          >
            <TooltipProvider delayDuration={300}>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    type='button'
                    variant='ghost'
                    size='sm'
                    className='shrink-0 h-auto px-1 py-0.5 font-mono text-[10px] bg-muted rounded hover:bg-muted text-muted-foreground hover:text-foreground transition-colors'
                    onClick={(e) => {
                      e.stopPropagation();
                      navigator.clipboard.writeText(commit.fullId);
                    }}
                  >
                    {commit.id}
                  </Button>
                </TooltipTrigger>
                <TooltipContent>
                  <p>{commit.fullId} (click to copy)</p>
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
            <div className='min-w-0 flex-1'>
              <p className='truncate text-[13px] font-medium leading-snug'>{commit.message}</p>
              <p className='text-[10px] text-muted-foreground/70 mt-0.5'>
                {commit.author} · {formatRelativeTime(commit.timestamp)}
                {commit.filesChanged > 0 && (
                  <span className='ml-1.5'>
                    · {commit.filesChanged} file{commit.filesChanged !== 1 ? 's' : ''}
                  </span>
                )}
              </p>
            </div>
          </div>
        ))}
        {commitLog.length >= limit && (
          <div className='p-2'>
            <Button variant='outline' size='sm' className='w-full text-sm' onClick={handleLoadMore}>
              Load more
            </Button>
          </div>
        )}
      </div>
    </ScrollArea>
  );
}
