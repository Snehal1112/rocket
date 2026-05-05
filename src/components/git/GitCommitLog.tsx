import { useState } from 'react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import type { CommitInfo } from '@/lib/tauri-api';
import { useGitStore } from '@/stores/git-store';

function relativeTime(timestamp: string): string {
  const now = Date.now();
  const then = new Date(timestamp).getTime();
  const diff = now - then;
  const minutes = Math.floor(diff / 60000);
  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes}m ago`;
  const hours = Math.floor(minutes / 60);
  if (hours < 24) return `${hours}h ago`;
  const days = Math.floor(hours / 24);
  if (days < 30) return `${days}d ago`;
  const months = Math.floor(days / 30);
  return `${months}mo ago`;
}

interface GitCommitLogProps {
  onCommitClick: (commit: CommitInfo) => void;
}

export function GitCommitLog({ onCommitClick }: GitCommitLogProps) {
  const { commitLog, refreshLog } = useGitStore();
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
              if (e.key === 'Enter' || e.key === ' ') onCommitClick(commit);
            }}
          >
            <TooltipProvider delayDuration={300}>
              <Tooltip>
                <TooltipTrigger asChild>
                  <button
                    type='button'
                    className='shrink-0 cursor-pointer font-mono text-[10px] px-1 py-0.5 bg-muted rounded text-muted-foreground hover:text-foreground transition-colors'
                    onClick={(e) => {
                      e.stopPropagation();
                      navigator.clipboard.writeText(commit.fullId);
                    }}
                  >
                    {commit.id}
                  </button>
                </TooltipTrigger>
                <TooltipContent>
                  <p>{commit.fullId} (click to copy)</p>
                </TooltipContent>
              </Tooltip>
            </TooltipProvider>
            <div className='min-w-0 flex-1'>
              <p className='truncate text-[13px] font-medium leading-snug'>{commit.message}</p>
              <p className='text-[10px] text-muted-foreground/70 mt-0.5'>
                {commit.author} · {relativeTime(commit.timestamp)}
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
