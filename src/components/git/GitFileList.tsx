import { Plus, Minus, RotateCcw } from 'lucide-react';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Button } from '@/components/ui/button';
import { Separator } from '@/components/ui/separator';
import {
  Tooltip,
  TooltipTrigger,
  TooltipContent,
  TooltipProvider,
} from '@/components/ui/tooltip';
import { useGitStore } from '@/stores/git-store';
import { GIT_STATUS_CONFIG } from '@/lib/colors';
import type { FileStatus } from '@/lib/tauri-api';

interface GitFileListProps {
  onFileClick: (file: FileStatus) => void;
}

export function GitFileList({ onFileClick }: GitFileListProps) {
  const { status, stageFiles, stageAll, unstageFiles, unstageAll, discardFiles } = useGitStore();

  const staged = status?.files.filter((f) => f.staged) ?? [];
  const unstaged = status?.files.filter((f) => !f.staged && f.status !== 'unchanged') ?? [];

  const handleDiscardAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    discardFiles(unstaged.map((f) => f.path));
  };

  const handleStageAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    stageAll();
  };

  const handleUnstageAll = (e: React.MouseEvent) => {
    e.stopPropagation();
    unstageAll();
  };

  return (
    <TooltipProvider>
      <ScrollArea className="flex-1">
        <div className="p-3 space-y-1">
          {/* Unstaged section header. */}
          <div className="flex items-center justify-between px-2 py-1">
            <span className="text-xs font-medium text-muted-foreground">Unstaged Changes</span>
            <div className="flex items-center gap-1.5">
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-5 w-5"
                    onClick={handleDiscardAll}
                  >
                    <RotateCcw className="h-3 w-3" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Discard all unstaged</TooltipContent>
              </Tooltip>
              <Tooltip>
                <TooltipTrigger asChild>
                  <Button
                    variant="ghost"
                    size="icon"
                    className="h-5 w-5"
                    onClick={handleStageAll}
                  >
                    <Plus className="h-3 w-3" />
                  </Button>
                </TooltipTrigger>
                <TooltipContent>Stage all</TooltipContent>
              </Tooltip>
              <span className="text-xs text-muted-foreground">{unstaged.length}</span>
            </div>
          </div>

          {/* Unstaged file rows. */}
          {unstaged.map((file) => (
            <div
              key={file.path}
              className="group flex items-center justify-between px-2 py-1 rounded-md hover:bg-muted/50 cursor-pointer"
              onClick={() => onFileClick(file)}
            >
              <span className="text-sm truncate flex-1 min-w-0">{file.path}</span>
              <div className="flex items-center gap-0.5">
                <span className={`text-xs font-medium shrink-0 ${GIT_STATUS_CONFIG[file.status].className}`}>
                  {GIT_STATUS_CONFIG[file.status].label}
                </span>
                <div className="flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-5 w-5"
                        onClick={(e) => { e.stopPropagation(); discardFiles([file.path]); }}
                      >
                        <RotateCcw className="h-3 w-3" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>Discard</TooltipContent>
                  </Tooltip>
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-5 w-5"
                        onClick={(e) => { e.stopPropagation(); stageFiles([file.path]); }}
                      >
                        <Plus className="h-3 w-3" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>Stage</TooltipContent>
                  </Tooltip>
                </div>
              </div>
            </div>
          ))}

          {/* Staged section — only rendered when there are staged files. */}
          {staged.length > 0 && (
            <>
              <Separator className="my-2" />
              <div className="flex items-center justify-between px-2 py-1">
                <span className="text-xs font-medium text-muted-foreground">Staged Changes</span>
                <div className="flex items-center gap-1.5">
                  <Tooltip>
                    <TooltipTrigger asChild>
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-5 w-5"
                        onClick={handleUnstageAll}
                      >
                        <Minus className="h-3 w-3" />
                      </Button>
                    </TooltipTrigger>
                    <TooltipContent>Unstage all</TooltipContent>
                  </Tooltip>
                  <span className="text-xs text-muted-foreground">{staged.length}</span>
                </div>
              </div>

              {/* Staged file rows. */}
              {staged.map((file) => (
                <div
                  key={file.path}
                  className="group flex items-center justify-between px-2 py-1 rounded-md hover:bg-muted/50 cursor-pointer"
                  onClick={() => onFileClick(file)}
                >
                  <span className="text-sm truncate flex-1 min-w-0">{file.path}</span>
                  <div className="flex items-center gap-0.5">
                    <span className={`text-xs font-medium shrink-0 ${GIT_STATUS_CONFIG[file.status].className}`}>
                      {GIT_STATUS_CONFIG[file.status].label}
                    </span>
                    <div className="flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
                      <Tooltip>
                        <TooltipTrigger asChild>
                          <Button
                            variant="ghost"
                            size="icon"
                            className="h-5 w-5"
                            onClick={(e) => { e.stopPropagation(); unstageFiles([file.path]); }}
                          >
                            <Minus className="h-3 w-3" />
                          </Button>
                        </TooltipTrigger>
                        <TooltipContent>Unstage</TooltipContent>
                      </Tooltip>
                    </div>
                  </div>
                </div>
              ))}
            </>
          )}
        </div>
      </ScrollArea>
    </TooltipProvider>
  );
}
