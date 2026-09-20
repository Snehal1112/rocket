import '@/components/editor/monaco-setup';
import { DiffEditor, type DiffOnMount } from '@monaco-editor/react';
import type * as monacoNs from 'monaco-editor';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useMonacoTheme } from '@/components/editor/useMonacoTheme';
import { gitDiff, gitDiffStaged } from '@/lib/tauri-api';
import type { DiffState } from '@/types/pane-types';
import { DiffHeader } from './DiffHeader';
import { VisualDiffView } from './VisualDiffView';

interface DiffViewerProps {
  diffState: DiffState;
  hideStageToggle?: boolean;
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

// Renders a side-by-side Monaco diff or visual structured diff for a single file.
export function DiffViewer({
  diffState: initialDiffState,
  hideStageToggle = false,
}: DiffViewerProps) {
  const [diffState, setDiffState] = useState(initialDiffState);
  const { themeName } = useMonacoTheme();

  // Hold the editor instance so we can dispose it explicitly before React
  // unmounts the DOM, preventing "TextModel disposed before DiffEditorWidget
  // model got reset" errors caused by Monaco's internal teardown order.
  const editorRef = useRef<monacoNs.editor.IDiffEditor | null>(null);

  useEffect(() => {
    return () => {
      editorRef.current?.dispose();
      editorRef.current = null;
    };
  }, []);

  const handleDiffMount: DiffOnMount = (editor) => {
    editorRef.current = editor;
  };

  // Persist mode preference across sessions. Validate the stored value —
  // it's user/session-editable localStorage, not a value this code controls.
  const [mode, setMode] = useState<'text' | 'visual'>(() => {
    const stored = localStorage.getItem('git-diff-mode');
    return stored === 'text' || stored === 'visual' ? stored : 'text';
  });

  const handleModeChange = useCallback((m: 'text' | 'visual') => {
    setMode(m);
    localStorage.setItem('git-diff-mode', m);
  }, []);

  const handleToggleStaged = useCallback(
    async (isStaged: boolean) => {
      try {
        const diff = isStaged
          ? await gitDiffStaged(diffState.repositoryId, diffState.filePath)
          : await gitDiff(diffState.repositoryId, diffState.filePath);
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
    [diffState.repositoryId, diffState.filePath],
  );

  // Visual mode is only available for .yml collection files.
  const canShowVisual = diffState.filePath.endsWith('.yml');
  const language = getLanguage(diffState.filePath);

  return (
    <div className='flex flex-col h-full'>
      <DiffHeader
        diffState={diffState}
        onToggleStaged={handleToggleStaged}
        mode={mode}
        onModeChange={handleModeChange}
        canShowVisual={canShowVisual}
        hideStageToggle={hideStageToggle}
      />
      {mode === 'visual' && canShowVisual ? (
        <VisualDiffView oldContent={diffState.oldContent} newContent={diffState.newContent} />
      ) : (
        <div className='flex-1'>
          <DiffEditor
            original={diffState.oldContent}
            modified={diffState.newContent}
            language={language}
            theme={themeName}
            onMount={handleDiffMount}
            options={{
              readOnly: true,
              renderSideBySide: true,
              minimap: { enabled: false },
              scrollBeyondLastLine: false,
              fontSize: 12,
            }}
          />
        </div>
      )}
    </div>
  );
}
