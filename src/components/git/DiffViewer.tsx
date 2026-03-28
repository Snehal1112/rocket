import { useState, useCallback } from 'react';
import { DiffEditor } from '@monaco-editor/react';
import { DiffHeader } from './DiffHeader';
import { gitDiff, gitDiffStaged } from '@/lib/tauri-api';
import { useTheme } from '@/hooks/useTheme';
import type { DiffState } from '@/types/pane-types';

interface DiffViewerProps {
  diffState: DiffState;
}

// Maps file extension to a Monaco language identifier.
function getLanguage(filePath: string): string {
  const ext = filePath.split('.').pop()?.toLowerCase() ?? '';
  const map: Record<string, string> = {
    json: 'json',
    js: 'javascript',
    ts: 'typescript',
    tsx: 'typescript',
    jsx: 'javascript',
    md: 'markdown',
    yaml: 'yaml',
    yml: 'yaml',
    xml: 'xml',
    html: 'html',
    css: 'css',
    bru: 'plaintext',
  };
  return map[ext] ?? 'plaintext';
}

// Renders a side-by-side Monaco diff for a single file with staged/working toggle.
export function DiffViewer({ diffState: initialDiffState }: DiffViewerProps) {
  const [diffState, setDiffState] = useState(initialDiffState);
  const { isDark } = useTheme();

  const handleToggleStaged = useCallback(async (isStaged: boolean) => {
    try {
      const diff = isStaged
        ? await gitDiffStaged(diffState.collectionPath, diffState.filePath)
        : await gitDiff(diffState.collectionPath, diffState.filePath);
      setDiffState({
        ...diffState,
        oldContent: diff.oldContent ?? '',
        newContent: diff.newContent ?? '',
        isStaged,
      });
    } catch {
      // Keep current state on error.
    }
  }, [diffState.collectionPath, diffState.filePath]);

  const language = getLanguage(diffState.filePath);

  return (
    <div className="flex flex-col h-full">
      <DiffHeader diffState={diffState} onToggleStaged={handleToggleStaged} />
      <div className="flex-1">
        <DiffEditor
          original={diffState.oldContent}
          modified={diffState.newContent}
          language={language}
          theme={isDark ? 'vs-dark' : 'vs'}
          options={{
            readOnly: true,
            renderSideBySide: true,
            minimap: { enabled: false },
            scrollBeyondLastLine: false,
            fontSize: 12,
          }}
        />
      </div>
    </div>
  );
}
