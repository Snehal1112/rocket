import type { EditorProps } from '@monaco-editor/react';

// Alias for the Monaco standalone editor construction options type.
type EditorOptions = EditorProps['options'];

export const BASE_EDITOR_OPTIONS: EditorOptions = {
  fontSize: 13,
  fontFamily: "'JetBrains Mono', monospace",
  tabSize: 2,
  wordWrap: 'on',
  bracketPairColorization: { enabled: true },
  minimap: { enabled: false },
  scrollBeyondLastLine: false,
  automaticLayout: true,
  padding: { top: 8, bottom: 8 },
  lineNumbers: 'on',
  renderLineHighlight: 'line',
  folding: true,
};

export const READONLY_OPTIONS: EditorOptions = {
  ...BASE_EDITOR_OPTIONS,
  readOnly: true,
  minimap: { enabled: true },
  lineNumbers: 'off',
};

// Detect Monaco language from body mode or content-type header.
export function detectLanguage(bodyMode?: string, contentType?: string): string {
  if (bodyMode === 'json' || contentType?.includes('json')) return 'json';
  if (bodyMode === 'xml' || contentType?.includes('xml')) return 'xml';
  if (bodyMode === 'text') return 'plaintext';
  if (contentType?.includes('html')) return 'html';
  if (contentType?.includes('javascript')) return 'javascript';
  if (contentType?.includes('css')) return 'css';
  return 'plaintext';
}
