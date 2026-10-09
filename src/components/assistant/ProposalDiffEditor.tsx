import '@/components/editor/monaco-setup';
import { DiffEditor, type DiffOnMount } from '@monaco-editor/react';
import type * as monacoNs from 'monaco-editor';
import { useEffect, useRef } from 'react';
import { MONACO_FONT_FAMILY } from '@/components/editor/monaco-config';
import { acquireJsWorker, releaseJsWorker } from '@/components/editor/monaco-js-worker-lifecycle';
import { useMonacoTheme } from '@/components/editor/useMonacoTheme';

interface ProposalDiffEditorProps {
  original: string;
  modified: string;
  language: string;
}

/** Read-only inline diff of one proposal. Mirrors DiffViewer's Monaco setup. */
export function ProposalDiffEditor({ original, modified, language }: ProposalDiffEditorProps) {
  const { themeName } = useMonacoTheme();
  // Dispose before React removes the DOM, as DiffViewer does, to avoid
  // "TextModel disposed before DiffEditorWidget model got reset".
  const editorRef = useRef<monacoNs.editor.IDiffEditor | null>(null);

  useEffect(() => {
    return () => {
      editorRef.current?.dispose();
      editorRef.current = null;
    };
  }, []);

  useEffect(() => {
    if (language !== 'javascript' && language !== 'typescript') return;
    acquireJsWorker();
    return () => releaseJsWorker();
  }, [language]);

  const handleMount: DiffOnMount = (editor) => {
    editorRef.current = editor;
  };

  return (
    <div className='h-56 overflow-hidden rounded-md border'>
      <DiffEditor
        original={original}
        modified={modified}
        language={language}
        theme={themeName}
        onMount={handleMount}
        options={{
          readOnly: true,
          renderSideBySide: false,
          minimap: { enabled: false },
          scrollBeyondLastLine: false,
          fontSize: 13,
          fontFamily: MONACO_FONT_FAMILY,
          hideUnchangedRegions: { enabled: true },
        }}
      />
    </div>
  );
}
