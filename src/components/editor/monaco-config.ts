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

// VSCode 2026 Light — sourced from
// github.com/microsoft/vscode/blob/main/extensions/theme-defaults/themes/2026-light.json
// Token colors follow the GitHub Light theme (included via light_modern.json inheritance).
const ROCKET_LIGHT_THEME: monacoNs.editor.IStandaloneThemeData = {
  base: 'vs',
  inherit: false,
  rules: [
    // Base / default text
    { token: '', foreground: '202020', background: 'FFFFFF' },

    // Comments
    { token: 'comment', foreground: '6e7781' },
    { token: 'punctuation.definition.comment', foreground: '6e7781' },
    { token: 'string.comment', foreground: '6e7781' },

    // Constants & literals
    { token: 'constant', foreground: '0550ae' },
    { token: 'entity.name.constant', foreground: '0550ae' },
    { token: 'variable.other.constant', foreground: '0550ae' },
    { token: 'variable.other.enummember', foreground: '0550ae' },
    { token: 'variable.language', foreground: '0550ae' },
    { token: 'constant.other.placeholder', foreground: 'cf222e' },
    { token: 'constant.character', foreground: 'cf222e' },

    // Entity names
    { token: 'entity.name', foreground: '953800' },
    { token: 'meta.export.default', foreground: '953800' },
    { token: 'meta.definition.variable', foreground: '953800' },
    { token: 'entity.name.function', foreground: '8250df' },
    { token: 'entity.name.tag', foreground: '116329' },
    { token: 'support.class.component', foreground: '116329' },

    // Keywords & storage
    { token: 'keyword', foreground: 'cf222e' },
    { token: 'storage', foreground: 'cf222e' },
    { token: 'storage.type', foreground: 'cf222e' },
    { token: 'storage.modifier.package', foreground: '1f2328' },
    { token: 'storage.modifier.import', foreground: '1f2328' },
    { token: 'storage.type.java', foreground: '1f2328' },
    { token: 'punctuation.section.embedded', foreground: 'cf222e' },

    // Strings
    { token: 'string', foreground: '0a3069' },
    { token: 'string.key.json', foreground: '116329' },
    { token: 'string.value.json', foreground: '0a3069' },
    { token: 'support.type.property-name.json', foreground: '116329' },
    { token: 'string variable', foreground: '0550ae' },
    { token: 'source.regexp', foreground: '0a3069' },
    { token: 'string.regexp', foreground: '0a3069' },
    { token: 'string.regexp.character-class', foreground: '0a3069' },
    { token: 'string.regexp constant.character.escape', foreground: '116329' },
    { token: 'constant.other.reference.link', foreground: '0a3069' },
    { token: 'string.other.link', foreground: '0a3069' },

    // Numbers
    { token: 'number', foreground: '0550ae' },

    // Variables
    { token: 'variable', foreground: '953800' },
    { token: 'variable.other', foreground: '1f2328' },
    { token: 'variable.parameter.function', foreground: '1f2328' },
    { token: 'meta.jsx.children', foreground: '1f2328' },
    { token: 'meta.block', foreground: '1f2328' },
    { token: 'meta.tag.attributes', foreground: '1f2328' },
    { token: 'meta.object.member', foreground: '1f2328' },
    { token: 'meta.embedded.expression', foreground: '1f2328' },

    // Support
    { token: 'support', foreground: '0550ae' },
    { token: 'meta.property-name', foreground: '0550ae' },
    { token: 'support.constant', foreground: '0550ae' },
    { token: 'support.variable', foreground: '0550ae' },
    { token: 'meta.module-reference', foreground: '0550ae' },

    // Types
    { token: 'type', foreground: '0550ae' },

    // Invalid
    { token: 'invalid.broken', foreground: '82071e' },
    { token: 'invalid.deprecated', foreground: '82071e' },
    { token: 'invalid.illegal', foreground: '82071e' },
    { token: 'invalid.unimplemented', foreground: '82071e' },
    { token: 'message.error', foreground: '82071e' },

    // Markup
    { token: 'markup.heading', foreground: '0550ae' },
    { token: 'markup.quote', foreground: '116329' },
    { token: 'markup.italic', foreground: '1f2328' },
    { token: 'markup.bold', foreground: '1f2328' },
    { token: 'markup.inline.raw', foreground: '0550ae' },
    { token: 'markup.deleted', foreground: '82071e' },
    { token: 'markup.inserted', foreground: '116329' },
    { token: 'markup.changed', foreground: '953800' },
    { token: 'meta.diff.range', foreground: '8250df' },
    { token: 'meta.diff.header', foreground: '0550ae' },
    { token: 'meta.separator', foreground: '0550ae' },
    { token: 'meta.output', foreground: '0550ae' },
    { token: 'punctuation.definition.list.begin.markdown', foreground: '953800' },

    // Bracket highlighter
    { token: 'brackethighlighter.tag', foreground: '57606a' },
    { token: 'brackethighlighter.curly', foreground: '57606a' },
    { token: 'brackethighlighter.round', foreground: '57606a' },
    { token: 'brackethighlighter.square', foreground: '57606a' },
    { token: 'brackethighlighter.angle', foreground: '57606a' },
    { token: 'brackethighlighter.quote', foreground: '57606a' },
    { token: 'brackethighlighter.unmatched', foreground: '82071e' },
  ],
  colors: {
    // Surfaces
    'editor.background': '#FFFFFF',
    'editor.foreground': '#202020',
    'editorGutter.background': '#FFFFFF',
    'editorWidget.background': '#FAFAFD',
    'editorWidget.border': '#E4E5E6',
    'editorWidget.foreground': '#202020',
    'editorSuggestWidget.background': '#FAFAFD',
    'editorSuggestWidget.border': '#E4E5E6',
    'editorSuggestWidget.foreground': '#202020',
    'editorSuggestWidget.highlightForeground': '#0069CC',
    'editorSuggestWidget.selectedBackground': '#0069CC26',
    'editorHoverWidget.background': '#FAFAFD',
    'editorHoverWidget.border': '#E4E5E6',

    // Cursor & selection
    'editorCursor.foreground': '#202020',
    'editor.selectionBackground': '#0069CC40',
    'editor.inactiveSelectionBackground': '#0069CC1A',
    'editor.selectionHighlightBackground': '#0069CC15',
    'editor.wordHighlightBackground': '#0069CC26',
    'editor.wordHighlightStrongBackground': '#0069CC26',

    // Line highlight & find
    'editor.lineHighlightBackground': '#EAEAEA40',
    'editor.findMatchBackground': '#0069CC40',
    'editor.findMatchHighlightBackground': '#0069CC1A',
    'editor.findRangeHighlightBackground': '#EAEAEA',
    'editor.rangeHighlightBackground': '#EAEAEA',
    'editor.hoverHighlightBackground': '#EAEAEA',

    // Line numbers
    'editorLineNumber.foreground': '#606060',
    'editorLineNumber.activeForeground': '#202020',

    // Indent guides
    'editorIndentGuide.background1': '#F7F7F7',
    'editorIndentGuide.activeBackground1': '#EEEEEE',

    // Brackets
    'editorBracketMatch.background': '#0069CC40',
    'editorBracketMatch.border': '#F0F1F2',

    // Scrollbar
    'scrollbar.shadow': '#00000000',
    'scrollbarSlider.background': '#99999926',
    'scrollbarSlider.hoverBackground': '#99999940',
    'scrollbarSlider.activeBackground': '#99999955',

    // Minimap
    'minimapSlider.background': '#99999926',
    'minimapSlider.hoverBackground': '#99999940',
    'minimapSlider.activeBackground': '#99999955',

    // Diff editor
    'diffEditor.insertedTextBackground': '#587c0c26',
    'diffEditor.removedTextBackground': '#ad070726',

    // Peek view
    'peekView.border': '#0069CC',
    'peekViewEditor.background': '#FAFAFD',
    'peekViewEditor.matchHighlightBackground': '#0069CC33',
    'peekViewResult.background': '#FAFAFD',
    'peekViewResult.fileForeground': '#202020',
    'peekViewResult.lineForeground': '#606060',
    'peekViewResult.matchHighlightBackground': '#0069CC33',
    'peekViewResult.selectionBackground': '#0069CC26',
    'peekViewResult.selectionForeground': '#202020',
    'peekViewTitle.background': '#FAFAFD',
    'peekViewTitleDescription.foreground': '#606060',
    'peekViewTitleLabel.foreground': '#202020',

    // Gutter decorations
    'editorGutter.addedBackground': '#587c0c',
    'editorGutter.deletedBackground': '#ad0707',
  },
};

// VSCode 2026 Dark — sourced from
// github.com/microsoft/vscode/blob/main/extensions/theme-defaults/themes/2026-dark.json
// Token colors follow the GitHub Dark theme (included via dark_modern.json inheritance).
const ROCKET_DARK_THEME: monacoNs.editor.IStandaloneThemeData = {
  base: 'vs-dark',
  inherit: false,
  rules: [
    // Base / default text
    { token: '', foreground: 'BBBEBF', background: '121314' },

    // Comments
    { token: 'comment', foreground: '8b949e' },
    { token: 'punctuation.definition.comment', foreground: '8b949e' },
    { token: 'string.comment', foreground: '8b949e' },

    // Constants & literals
    { token: 'constant', foreground: '79c0ff' },
    { token: 'entity.name.constant', foreground: '79c0ff' },
    { token: 'variable.other.constant', foreground: '79c0ff' },
    { token: 'variable.other.enummember', foreground: '79c0ff' },
    { token: 'variable.language', foreground: '79c0ff' },
    { token: 'constant.other.placeholder', foreground: 'ff7b72' },
    { token: 'constant.character', foreground: 'ff7b72' },

    // Entity names
    { token: 'entity.name', foreground: 'ffa657' },
    { token: 'meta.export.default', foreground: 'ffa657' },
    { token: 'meta.definition.variable', foreground: 'ffa657' },
    { token: 'entity.name.function', foreground: 'd2a8ff' },
    { token: 'entity.name.tag', foreground: '7ee787' },
    { token: 'support.class.component', foreground: '7ee787' },

    // Keywords & storage
    { token: 'keyword', foreground: 'ff7b72' },
    { token: 'storage', foreground: 'ff7b72' },
    { token: 'storage.type', foreground: 'ff7b72' },
    { token: 'storage.modifier.package', foreground: 'c9d1d9' },
    { token: 'storage.modifier.import', foreground: 'c9d1d9' },
    { token: 'storage.type.java', foreground: 'c9d1d9' },
    { token: 'punctuation.section.embedded', foreground: 'ff7b72' },

    // Strings
    { token: 'string', foreground: 'a5d6ff' },
    { token: 'string.key.json', foreground: '7ee787' },
    { token: 'string.value.json', foreground: 'a5d6ff' },
    { token: 'support.type.property-name.json', foreground: '7ee787' },
    { token: 'string variable', foreground: '79c0ff' },
    { token: 'source.regexp', foreground: 'a5d6ff' },
    { token: 'string.regexp', foreground: 'a5d6ff' },
    { token: 'string.regexp.character-class', foreground: 'a5d6ff' },
    { token: 'string.regexp constant.character.escape', foreground: '7ee787' },
    { token: 'constant.other.reference.link', foreground: 'a5d6ff' },
    { token: 'string.other.link', foreground: 'a5d6ff' },

    // Numbers
    { token: 'number', foreground: '79c0ff' },

    // Variables
    { token: 'variable', foreground: 'ffa657' },
    { token: 'variable.other', foreground: 'c9d1d9' },
    { token: 'variable.parameter.function', foreground: 'c9d1d9' },
    { token: 'meta.jsx.children', foreground: 'c9d1d9' },
    { token: 'meta.block', foreground: 'c9d1d9' },
    { token: 'meta.tag.attributes', foreground: 'c9d1d9' },
    { token: 'meta.object.member', foreground: 'c9d1d9' },
    { token: 'meta.embedded.expression', foreground: 'c9d1d9' },

    // Support
    { token: 'support', foreground: '79c0ff' },
    { token: 'meta.property-name', foreground: '79c0ff' },
    { token: 'support.constant', foreground: '79c0ff' },
    { token: 'support.variable', foreground: '79c0ff' },
    { token: 'meta.module-reference', foreground: '79c0ff' },

    // Types
    { token: 'type', foreground: '79c0ff' },

    // Invalid
    { token: 'invalid.broken', foreground: 'ffa198' },
    { token: 'invalid.deprecated', foreground: 'ffa198' },
    { token: 'invalid.illegal', foreground: 'ffa198' },
    { token: 'invalid.unimplemented', foreground: 'ffa198' },
    { token: 'message.error', foreground: 'ffa198' },

    // Markup
    { token: 'markup.heading', foreground: '79c0ff' },
    { token: 'markup.quote', foreground: '7ee787' },
    { token: 'markup.italic', foreground: 'c9d1d9' },
    { token: 'markup.bold', foreground: 'c9d1d9' },
    { token: 'markup.inline.raw', foreground: '79c0ff' },
    { token: 'markup.deleted', foreground: 'ffa198' },
    { token: 'markup.inserted', foreground: '7ee787' },
    { token: 'markup.changed', foreground: 'ffa657' },
    { token: 'meta.diff.range', foreground: 'd2a8ff' },
    { token: 'meta.diff.header', foreground: '79c0ff' },
    { token: 'meta.separator', foreground: '79c0ff' },
    { token: 'meta.output', foreground: '79c0ff' },
    { token: 'punctuation.definition.list.begin.markdown', foreground: 'ffa657' },

    // Bracket highlighter
    { token: 'brackethighlighter.tag', foreground: '8b949e' },
    { token: 'brackethighlighter.curly', foreground: '8b949e' },
    { token: 'brackethighlighter.round', foreground: '8b949e' },
    { token: 'brackethighlighter.square', foreground: '8b949e' },
    { token: 'brackethighlighter.angle', foreground: '8b949e' },
    { token: 'brackethighlighter.quote', foreground: '8b949e' },
    { token: 'brackethighlighter.unmatched', foreground: 'ffa198' },

    // Internal Monaco diagnostic tokens
    { token: 'token.info-token', foreground: '6796E6' },
    { token: 'token.warn-token', foreground: 'CD9731' },
    { token: 'token.error-token', foreground: 'F44747' },
    { token: 'token.debug-token', foreground: 'B267E6' },
  ],
  colors: {
    // Surfaces
    'editor.background': '#121314',
    'editor.foreground': '#BBBEBF',
    'editorGutter.background': '#121314',
    'editorWidget.background': '#202122',
    'editorWidget.border': '#2A2B2C',
    'editorWidget.foreground': '#bfbfbf',
    'editorSuggestWidget.background': '#202122',
    'editorSuggestWidget.border': '#2A2B2C',
    'editorSuggestWidget.foreground': '#bfbfbf',
    'editorSuggestWidget.highlightForeground': '#bfbfbf',
    'editorSuggestWidget.selectedBackground': '#3994BC26',
    'editorHoverWidget.background': '#202122',
    'editorHoverWidget.border': '#2A2B2C',

    // Cursor & selection
    'editorCursor.foreground': '#BBBEBF',
    'editor.selectionBackground': '#276782dd',
    'editor.inactiveSelectionBackground': '#27678260',
    'editor.selectionHighlightBackground': '#27678260',
    'editor.wordHighlightBackground': '#27678250',
    'editor.wordHighlightStrongBackground': '#27678280',

    // Line highlight & find
    'editor.lineHighlightBackground': '#242526',
    'editor.findMatchBackground': '#27678290',
    'editor.findMatchHighlightBackground': '#27678280',
    'editor.findRangeHighlightBackground': '#242526',
    'editor.rangeHighlightBackground': '#242526',
    'editor.hoverHighlightBackground': '#242526',

    // Line numbers
    'editorLineNumber.foreground': '#858889',
    'editorLineNumber.activeForeground': '#BBBEBF',

    // Indent guides
    'editorIndentGuide.background1': '#8384854D',
    'editorIndentGuide.activeBackground1': '#838485',

    // Brackets
    'editorBracketMatch.background': '#3994BC55',
    'editorBracketMatch.border': '#2A2B2C',

    // Scrollbar
    'scrollbar.shadow': '#191B1D4D',
    'scrollbarSlider.background': '#83848533',
    'scrollbarSlider.hoverBackground': '#83848566',
    'scrollbarSlider.activeBackground': '#83848599',

    // Minimap
    'minimapSlider.background': '#83848533',
    'minimapSlider.hoverBackground': '#83848566',
    'minimapSlider.activeBackground': '#83848599',

    // Diff editor
    'diffEditor.insertedLineBackground': '#347d3926',
    'diffEditor.insertedTextBackground': '#57ab5a4d',
    'diffEditor.removedLineBackground': '#c93c3726',
    'diffEditor.removedTextBackground': '#f470674d',

    // Peek view
    'peekView.border': '#2A2B2C',
    'peekViewEditor.background': '#191A1B',
    'peekViewEditor.matchHighlightBackground': '#3994BC33',
    'peekViewResult.background': '#191A1B',
    'peekViewResult.fileForeground': '#bfbfbf',
    'peekViewResult.lineForeground': '#8C8C8C',
    'peekViewResult.matchHighlightBackground': '#3994BC33',
    'peekViewResult.selectionBackground': '#3994BC26',
    'peekViewResult.selectionForeground': '#bfbfbf',
    'peekViewTitle.background': '#242526',
    'peekViewTitleDescription.foreground': '#8C8C8C',
    'peekViewTitleLabel.foreground': '#bfbfbf',

    // Gutter decorations
    'editorGutter.addedBackground': '#72C892',
    'editorGutter.deletedBackground': '#F28772',
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
