import { useCallback, useState } from 'react';
import { gitDiff, gitDiffStaged } from '@/lib/tauri-api';
import type { DiffState } from '@/types/pane-types';
import { CM6DiffViewer } from './CM6DiffViewer';
import { DiffHeader } from './DiffHeader';
import { VisualDiffView } from './VisualDiffView';

interface DiffViewerProps {
  diffState: DiffState;
}

// Renders a side-by-side CM6 diff or visual structured diff for a single file.
export function DiffViewer({ diffState: initialDiffState }: DiffViewerProps) {
  const [diffState, setDiffState] = useState(initialDiffState);

  // Persist mode preference across sessions.
  const [mode, setMode] = useState<'text' | 'visual'>(() => {
    return (localStorage.getItem('git-diff-mode') as 'text' | 'visual') ?? 'text';
  });

  const handleModeChange = useCallback((m: 'text' | 'visual') => {
    setMode(m);
    localStorage.setItem('git-diff-mode', m);
  }, []);

  const handleToggleStaged = useCallback(
    async (isStaged: boolean) => {
      try {
        const diff = isStaged
          ? await gitDiffStaged(diffState.collectionPath, diffState.filePath)
          : await gitDiff(diffState.collectionPath, diffState.filePath);
        setDiffState((prev) => ({
          ...prev,
          oldContent: diff.oldContent ?? '',
          newContent: diff.newContent ?? '',
          isStaged,
        }));
      } catch {
        // Keep current state on error.
      }
    },
    [diffState.collectionPath, diffState.filePath],
  );

  // Visual mode is only available for JSON request files.
  const canShowVisual = diffState.filePath.endsWith('.yml');

  return (
    <div className='flex flex-col h-full'>
      <DiffHeader
        diffState={diffState}
        onToggleStaged={handleToggleStaged}
        mode={mode}
        onModeChange={handleModeChange}
        canShowVisual={canShowVisual}
      />
      {mode === 'visual' && canShowVisual ? (
        <VisualDiffView oldContent={diffState.oldContent} newContent={diffState.newContent} />
      ) : (
        <div className='flex-1'>
          <CM6DiffViewer
            oldContent={diffState.oldContent}
            newContent={diffState.newContent}
            filePath={diffState.filePath}
          />
        </div>
      )}
    </div>
  );
}
