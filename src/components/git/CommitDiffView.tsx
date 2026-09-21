import { lazy, Suspense, useState } from 'react';
import { GitStatusBadge } from '@/components/git/GitStatusBadge';
import { Button } from '@/components/ui/button';
import { ResizableHandle, ResizablePanel, ResizablePanelGroup } from '@/components/ui/resizable';
import { ScrollArea } from '@/components/ui/scroll-area';
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from '@/components/ui/tooltip';
import type { FileDiff, GitStatusKind } from '@/lib/tauri-api';
import type { DiffState } from '@/types/pane-types';

const DiffViewer = lazy(() => import('./DiffViewer').then((m) => ({ default: m.DiffViewer })));

interface CommitDiffViewProps {
  diffs: FileDiff[];
  repositoryId: string;
  repositoryLabel: string;
}

// Splits a repo-relative path into its containing directory and file name, so
// the file list can show the name — the part that most distinguishes one row
// from the next — on its own line instead of losing it to a mid-string ellipsis.
function splitPath(path: string): { dir: string; name: string } {
  const idx = path.lastIndexOf('/');
  return idx === -1
    ? { dir: '', name: path }
    : { dir: path.slice(0, idx), name: path.slice(idx + 1) };
}

function fileDiffToDiffState(
  diff: FileDiff,
  repositoryId: string,
  repositoryLabel: string,
): DiffState {
  return {
    filePath: diff.path,
    repositoryId,
    repositoryLabel,
    oldContent: diff.oldContent ?? '',
    newContent: diff.newContent ?? '',
    status: diff.oldContent == null ? 'added' : diff.newContent == null ? 'deleted' : 'modified',
    isStaged: true,
  };
}

function fileStatus(diff: FileDiff): GitStatusKind {
  if (diff.oldContent == null) return 'added';
  if (diff.newContent == null) return 'deleted';
  return 'modified';
}

export function CommitDiffView({ diffs, repositoryId, repositoryLabel }: CommitDiffViewProps) {
  const [selectedPath, setSelectedPath] = useState<string | null>(
    diffs.length > 0 ? diffs[0].path : null,
  );

  const selectedDiff = diffs.find((d) => d.path === selectedPath);

  if (diffs.length === 0) {
    return (
      <div className='flex items-center justify-center h-full text-xs text-muted-foreground'>
        No changes in this commit.
      </div>
    );
  }

  return (
    <ResizablePanelGroup className='h-full'>
      {/* File list sidebar */}
      {/* minSize/maxSize take pixels as a plain number in this version of
          react-resizable-panels — unlike defaultSize, where a plain number
          is a percent. Percentage strings are required here, or a small
          maxSize silently clamps the panel to that many pixels instead of
          that percent, freezing it far narrower than intended. */}
      <ResizablePanel defaultSize={28} minSize='20%' maxSize='45%'>
        <div className='h-full border-r border-border/70 flex flex-col'>
          <div className='px-3 py-2 text-xs font-medium text-muted-foreground border-b border-border/70'>
            {diffs.length} file{diffs.length !== 1 ? 's' : ''} changed
          </div>
          <TooltipProvider>
            <ScrollArea className='flex-1'>
              <div className='p-1'>
                {diffs.map((diff) => {
                  const { dir, name } = splitPath(diff.path);
                  return (
                    <Tooltip key={diff.path}>
                      <TooltipTrigger asChild>
                        <Button
                          type='button'
                          variant='ghost'
                          className={`w-full h-auto flex-col items-stretch gap-0 px-2 py-1.5 rounded justify-start font-normal text-left hover:bg-muted/50 ${
                            selectedPath === diff.path ? 'bg-muted/70' : ''
                          }`}
                          onClick={() => setSelectedPath(diff.path)}
                        >
                          <div className='flex items-center gap-1.5 w-full'>
                            <GitStatusBadge status={fileStatus(diff)} />
                            <span className='truncate flex-1 min-w-0 text-xs font-mono'>
                              {name}
                            </span>
                          </div>
                          {dir && (
                            <span className='pl-5 truncate text-[10px] font-mono text-muted-foreground/70'>
                              {dir}
                            </span>
                          )}
                        </Button>
                      </TooltipTrigger>
                      <TooltipContent side='right' className='font-mono text-xs'>
                        {diff.path}
                      </TooltipContent>
                    </Tooltip>
                  );
                })}
              </div>
            </ScrollArea>
          </TooltipProvider>
        </div>
      </ResizablePanel>

      <ResizableHandle withHandle />

      {/* Diff viewer */}
      <ResizablePanel defaultSize={72} minSize='40%'>
        {selectedDiff ? (
          <Suspense fallback={null}>
            <DiffViewer
              key={selectedDiff.path}
              diffState={fileDiffToDiffState(selectedDiff, repositoryId, repositoryLabel)}
              hideStageToggle
            />
          </Suspense>
        ) : (
          <div className='flex items-center justify-center h-full text-xs text-muted-foreground'>
            Select a file to view its diff.
          </div>
        )}
      </ResizablePanel>
    </ResizablePanelGroup>
  );
}
