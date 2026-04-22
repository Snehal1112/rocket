import type { EditorProps } from '@monaco-editor/react';
import type * as monacoNs from 'monaco-editor';

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
  domReadOnly: true,
  minimap: { enabled: false },
  lineNumbers: 'on',
  renderLineHighlight: 'none',
  folding: true,
  matchBrackets: 'always' as const,
};

const ROCKET_LIGHT_THEME: monacoNs.editor.IStandaloneThemeData = {
  base: 'vs',
  inherit: true,
  rules: [
    { token: 'string', foreground: 'a31515' },
    { token: 'string.key.json', foreground: '0451a5' },
    { token: 'string.value.json', foreground: 'a31515' },
    { token: 'number', foreground: '098658' },
    { token: 'keyword', foreground: '0000ff' },
    { token: 'comment', foreground: '008000' },
    { token: 'type', foreground: '267f99' },
    { token: 'variable', foreground: '001080' },
    { token: 'constant', foreground: '0070c1' },
  ],
  colors: {
    'editor.background': '#ffffff',
    'editor.foreground': '#000000',
    'editor.lineHighlightBackground': '#add6ff26',
    'editorLineNumber.foreground': '#237893',
    'editorLineNumber.activeForeground': '#0b216f',
  },
};

const ROCKET_DARK_THEME: monacoNs.editor.IStandaloneThemeData = {
  base: 'vs-dark',
  inherit: true,
  rules: [
    { token: 'string', foreground: 'ce9178' },
    { token: 'string.key.json', foreground: '9cdcfe' },
    { token: 'number', foreground: 'b5cea8' },
    { token: 'keyword', foreground: '569cd6' },
    { token: 'comment', foreground: '6a9955' },
    { token: 'type', foreground: '4ec9b0' },
    { token: 'variable', foreground: '9cdcfe' },
    { token: 'constant', foreground: '4fc1ff' },
  ],
  colors: {
    'editor.background': '#1f1f1f',
    'editor.foreground': '#d4d4d4',
    'editor.lineHighlightBackground': '#2a2d2e',
    'editorLineNumber.foreground': '#858585',
    'editorLineNumber.activeForeground': '#c6c6c6',
  },
};

// Call this once with the locally-bundled monaco instance before any editor
// mounts. Both MonacoWrapper and DiffViewer call this at module load time so
// the themes are always registered before the first render, regardless of which
// component opens first.
export function defineMonacoThemes(monacoInstance: typeof monacoNs) {
  monacoInstance.editor.defineTheme('rocket-light', ROCKET_LIGHT_THEME);
  monacoInstance.editor.defineTheme('rocket-dark', ROCKET_DARK_THEME);
}

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
