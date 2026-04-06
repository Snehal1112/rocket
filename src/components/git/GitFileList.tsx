import { AlertTriangle, Minus, Plus, Trash2 } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import { GIT_STATUS_CONFIG } from '@/lib/colors';
import type { ConflictFile, FileStatus } from '@/lib/tauri-api';
import { useGitStore } from '@/stores/git-store';

interface GitFileListProps {
  onFileClick: (file: FileStatus) => void;
  onConflictClick: (conflictFile: ConflictFile) => void;
}

export function GitFileList({ onFileClick, onConflictClick }: GitFileListProps) {
  const {
    status,
    conflicts,
    refreshConflicts,
    refreshStatus,
    stageFiles,
    stageAll,
    unstageFiles,
    unstageAll,
    discardFiles,
  } = useGitStore();

  const staged = status?.files.filter((f) => f.staged) ?? [];
  const unstaged = status?.files.filter((f) => !f.staged && f.status !== 'unchanged') ?? [];

  const handleDiscardAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    discardFiles(unstaged.filter((f) => f.status !== 'conflicted').map((f) => f.path));
  };

  const handleStageAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    stageAll();
  };

  const handleUnstageAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    unstageAll();
  };

  const handleConflictClick = async (file: FileStatus) => {
    await refreshConflicts();
    const conflictFile = conflicts.find((c) => c.path === file.path);
    if (conflictFile) {
      onConflictClick(conflictFile);
    } else {
      await refreshStatus();
    }
  };

  return (
    <TooltipProvider>
      <div className='overflow-y-auto flex-1'>
        <div className='p-3 space-y-1'>
          {/* Staged section — shown above unstaged, only when files are staged. */}
          {staged.length > 0 && (
            <>
              <div className='flex items-center justify-between px-2 py-1'>
                <span className='text-xs font-medium text-muted-foreground'>Staged Changes</span>
                <div className='flex items-center gap-1.5'>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant='ghost'
                        size='icon'
                        className='h-5 w-5'
                        onClick={handleUnstageAll}
                      >
                        <Minus className='h-3.5 w-3.5' />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>Unstage all</TooltipContent>
                  </Tooltip>
                  <span className='text-xs text-muted-foreground'>{staged.length}</span>
                </div>
              </div>

              {/* Staged file rows. */}
              {staged.map((file) => (
                // biome-ignore lint/a11y/useSemanticElements: outer <button> nesting inner <button> is invalid HTML; WebKitGTK reparses it and breaks mouseleave tracking.
                <div
                  key={file.path}
                  role='button'
                  tabIndex={0}
                  className='git-file-row flex w-full items-center px-2 py-1 rounded-md hover:bg-muted/50 cursor-pointer gap-1.5 text-left'
                  onClick={() => onFileClick(file)}
                  onKeyDown={(e) => {
                    if (e.key === 'Enter' || e.key === ' ') onFileClick(file);
                  }}
                >
                  <span className='text-sm truncate flex-1 min-w-0'>{file.path}</span>
                  <span
                    className={`text-xs font-medium shrink-0 ${GIT_STATUS_CONFIG[file.status].className}`}
                  >
                    {GIT_STATUS_CONFIG[file.status].label}
                  </span>
                  <div className='git-row-actions gap-0.5 shrink-0'>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          onClick={(e) => {
                            e.stopPropagation();
                            unstageFiles([file.path]);
                          }}
                        >
                          <Minus className='h-3.5 w-3.5' />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Unstage</TooltipContent>
                    </Tooltip>
                  </div>
                </div>
              ))}
              <Separator className='my-2' />
            </>
          )}

          {/* Unstaged section header. */}
          <div className='flex items-center justify-between px-2 py-1'>
            <span className='text-xs font-medium text-muted-foreground'>Unstaged Changes</span>
            <div className='flex items-center gap-1.5'>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant='ghost'
                    size='icon'
                    className='h-5 w-5'
                    onClick={handleDiscardAll}
                  >
                    <Trash2 className='h-3.5 w-3.5' />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Discard all unstaged</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button variant='ghost' size='icon' className='h-5 w-5' onClick={handleStageAll}>
                    <Plus className='h-3.5 w-3.5' />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Stage all</TooltipContent>
              </Tooltip>
              <span className='text-xs text-muted-foreground'>{unstaged.length}</span>
            </div>
          </div>

          {/* Unstaged file rows. */}
          {unstaged.map((file) => {
            const isConflicted = file.status === 'conflicted';
            return (
              // biome-ignore lint/a11y/useSemanticElements: outer <button> nesting inner <button> is invalid HTML; WebKitGTK reparses it and breaks mouseleave tracking.
              <div
                key={file.path}
                role='button'
                tabIndex={0}
                className='git-file-row flex w-full items-center px-2 py-1 rounded-md hover:bg-muted/50 cursor-pointer gap-1.5 text-left'
                onClick={() => {
                  if (isConflicted) {
                    void handleConflictClick(file);
                  } else {
                    onFileClick(file);
                  }
                }}
                onKeyDown={(e) => {
                  if (e.key === 'Enter' || e.key === ' ') {
                    if (isConflicted) {
                      void handleConflictClick(file);
                    } else {
                      onFileClick(file);
                    }
                  }
                }}
              >
                {isConflicted && (
                  <AlertTriangle className='h-3.5 w-3.5 text-destructive shrink-0' />
                )}
                <span className='text-sm truncate flex-1 min-w-0'>{file.path}</span>
                <span
                  className={`text-xs font-medium shrink-0 ${GIT_STATUS_CONFIG[file.status].className}`}
                >
                  {GIT_STATUS_CONFIG[file.status].label}
                </span>
                {!isConflicted && (
                  <div className='git-row-actions gap-0.5 shrink-0'>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          onClick={(e) => {
                            e.stopPropagation();
                            discardFiles([file.path]);
                          }}
                        >
                          <Trash2 className='h-3.5 w-3.5' />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Discard</TooltipContent>
                    </Tooltip>
                    <Tooltip>
                      <TooltipTrigger asChild>
                        <Button
                          variant='ghost'
                          size='icon'
                          className='h-5 w-5'
                          onClick={(e) => {
                            e.stopPropagation();
                            stageFiles([file.path]);
                          }}
                        >
                          <Plus className='h-3.5 w-3.5' />
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent>Stage</TooltipContent>
                    </Tooltip>
                  </div>
                )}
              </div>
            );
          })}
        </div>
      </div>
    </TooltipProvider>
  );
}
