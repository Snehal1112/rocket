import { lazy, Suspense, useState } from 'react';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { FileDiff } from '@/lib/tauri-api';
import type { DiffState } from '@/types/pane-types';
import { GitStatusBadge } from '@/components/git/GitStatusBadge';
import type { GitStatusKind } from '@/lib/tauri-api';

const DiffViewer = lazy(() => import('./DiffViewer').then((m) => ({ default: m.DiffViewer })));

interface CommitDiffViewProps {
  diffs: FileDiff[];
  collectionPath: string;
}

function fileDiffToDiffState(diff: FileDiff, collectionPath: string): DiffState {
  return {
    filePath: diff.path,
    collectionPath,
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

export function CommitDiffView({ diffs, collectionPath }: CommitDiffViewProps) {
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
    <div className='flex h-full'>
      {/* File list sidebar */}
      <div className='w-52 shrink-0 border-r border-border/70 flex flex-col'>
        <div className='px-3 py-2 text-xs font-medium text-muted-foreground border-b border-border/70'>
          {diffs.length} file{diffs.length !== 1 ? 's' : ''} changed
        </div>
        <ScrollArea className='flex-1'>
          <div className='p-1'>
            {diffs.map((diff) => (
              <button
                key={diff.path}
                type='button'
                className={`w-full flex items-center gap-1.5 px-2 py-1 rounded text-left text-sm hover:bg-muted/50 ${
                  selectedPath === diff.path ? 'bg-muted/70' : ''
                }`}
                onClick={() => setSelectedPath(diff.path)}
              >
                <GitStatusBadge status={fileStatus(diff)} />
                <span className='truncate flex-1 text-xs font-mono'>{diff.path}</span>
              </button>
            ))}
          </div>
        </ScrollArea>
      </div>

      {/* Diff viewer */}
      <div className='flex-1 overflow-hidden'>
        {selectedDiff ? (
          <Suspense fallback={null}>
            <DiffViewer
              key={selectedDiff.path}
              diffState={fileDiffToDiffState(selectedDiff, collectionPath)}
              hideStageToggle
            />
          </Suspense>
        ) : (
          <div className='flex items-center justify-center h-full text-xs text-muted-foreground'>
            Select a file to view its diff.
          </div>
        )}
      </div>
    </div>
  );
}
