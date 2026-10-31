import '@/components/editor/monaco-setup';
import Editor, { type OnMount } from '@monaco-editor/react';
import type { editor } from 'monaco-editor';
import { useCallback, useEffect, useRef } from 'react';
import { MONACO_FONT_FAMILY } from '@/components/editor/monaco-config';
import { useMonacoTheme } from '@/components/editor/useMonacoTheme';
import type { OpenApiSpecSource } from '@/lib/tauri-api';

const MIB = 1024 * 1024;

/** Above this size the editor drops its costly extras. */
export const LARGE_SOURCE_BYTES = 4 * MIB;

/** The editor options for a spec of this size. Big specs lose the minimap and highlight work. */
export function editorOptions(sizeBytes: number): editor.IStandaloneEditorConstructionOptions {
  const large = sizeBytes >= LARGE_SOURCE_BYTES;
  return {
    readOnly: true,
    domReadOnly: true,
    fontSize: 13,
    fontFamily: MONACO_FONT_FAMILY,
    minimap: { enabled: !large },
    scrollBeyondLastLine: false,
    automaticLayout: true,
    folding: true,
    lineNumbers: 'on',
    renderValidationDecorations: 'off',
    wordWrap: 'off',
    quickSuggestions: false,
    contextmenu: false,
    ...(large
      ? {
          bracketPairColorization: { enabled: false },
          matchBrackets: 'never',
          occurrencesHighlight: 'off',
          selectionHighlight: false,
          links: false,
          colorDecorators: false,
          guides: { indentation: false, bracketPairs: false },
          renderLineHighlight: 'none',
          stickyScroll: { enabled: false },
        }
      : {}),
  };
}

function formatSize(bytes: number): string {
  if (bytes >= MIB) return `${(bytes / MIB).toFixed(1)} MB`;
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} B`;
}

/** How long the flash on a revealed line stays. */
const FLASH_MS = 1500;

/** A request to show a line. A new nonce reveals the same line again. */
export interface RevealRequest {
  line: number;
  nonce: number;
}

type CodeEditor = Parameters<OnMount>[0];

/** The raw spec in a read-only Monaco editor, with a thin header for the format and size. */
export function SpecSourcePane({
  source,
  reveal = null,
}: {
  source: OpenApiSpecSource & { text: string };
  reveal?: RevealRequest | null;
}) {
  const { themeName, defineThemes } = useMonacoTheme();
  const editorRef = useRef<CodeEditor | null>(null);
  const flashRef = useRef<{ clear: () => void } | null>(null);
  const timerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const requestRef = useRef<RevealRequest | null>(reveal);

  const show = useCallback((editor: CodeEditor, line: number) => {
    editor.revealLineInCenter(line);
    editor.setPosition({ lineNumber: line, column: 1 });
    flashRef.current?.clear();
    if (timerRef.current) clearTimeout(timerRef.current);
    flashRef.current = editor.createDecorationsCollection([
      {
        range: { startLineNumber: line, startColumn: 1, endLineNumber: line, endColumn: 1 },
        options: { isWholeLine: true, className: 'spec-source-flash' },
      },
    ]);
    timerRef.current = setTimeout(() => {
      flashRef.current?.clear();
      flashRef.current = null;
      timerRef.current = null;
    }, FLASH_MS);
  }, []);

  useEffect(() => {
    requestRef.current = reveal;
    if (reveal && editorRef.current) show(editorRef.current, reveal.line);
  }, [reveal, show]);

  useEffect(
    () => () => {
      if (timerRef.current) clearTimeout(timerRef.current);
      flashRef.current?.clear();
    },
    [],
  );

  const onMount: OnMount = (editor) => {
    editorRef.current = editor;
    // A request that came in before the editor was ready still shows.
    if (requestRef.current) show(editor, requestRef.current.line);
  };
  return (
    <div className='flex h-full min-h-0 flex-col'>
      <div className='flex items-center border-b border-border px-3 py-1.5 text-xs text-muted-foreground'>
        {`${source.format.toUpperCase()}, ${formatSize(source.sizeBytes)}`}
      </div>
      <div className='min-h-0 flex-1'>
        <Editor
          value={source.text}
          language={source.format}
          theme={themeName}
          beforeMount={defineThemes}
          onMount={onMount}
          options={editorOptions(source.sizeBytes)}
        />
      </div>
    </div>
  );
}
