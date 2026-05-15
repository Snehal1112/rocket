import type { EditorProps } from '@monaco-editor/react';
import type * as monacoNs from 'monaco-editor';

// Alias for the Monaco standalone editor construction options type.
type EditorOptions = EditorProps['options'];

export const BASE_EDITOR_OPTIONS: EditorOptions = {
  fontSize: 15,
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

/** Read a CSS custom property value from the document root. */
function cssVar(name: string): string {
  if (typeof document === 'undefined') return '';
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

/** Convert a CSS custom property (HSL triplet or rgba) to a hex string for Monaco. */
function cssVarHex(name: string): string {
  const val = cssVar(name);
  if (!val) return '#000000';
  if (val.startsWith('#') || val.startsWith('rgb')) return val;
  // HSL triplet "H S% L%" → hex via canvas
  const parts = val.split(' ').map((v) => Number.parseFloat(v));
  const [h, s, l] = parts;
  const canvas = document.createElement('canvas');
  canvas.width = 1;
  canvas.height = 1;
  const ctx = canvas.getContext('2d');
  if (!ctx) return '#000000';
  ctx.fillStyle = `hsl(${h},${s}%,${l}%)`;
  ctx.fillRect(0, 0, 1, 1);
  const d = ctx.getImageData(0, 0, 1, 1).data;
  return `#${d[0].toString(16).padStart(2, '0')}${d[1].toString(16).padStart(2, '0')}${d[2].toString(16).padStart(2, '0')}`;
}

// VSCode 2026 Light — sourced from
// github.com/microsoft/vscode/blob/main/extensions/theme-defaults/themes/2026-light.json
// Token colors follow the GitHub Light theme (included via light_modern.json inheritance).
// Factory function so cssVarHex() is evaluated at call time (not module load).
function getRocketLightTheme(): monacoNs.editor.IStandaloneThemeData {
  return {
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
      'editor.background': cssVarHex('--editor-bg'),
      'editor.foreground': '#202020',
      'editorGutter.background': cssVarHex('--editor-bg'),
      'editorWidget.background': cssVarHex('--editor-widget-bg'),
      'editorWidget.border': cssVarHex('--editor-widget-border'),
      'editorWidget.foreground': '#202020',
      'editorSuggestWidget.background': cssVarHex('--editor-widget-bg'),
      'editorSuggestWidget.border': cssVarHex('--editor-widget-border'),
      'editorSuggestWidget.foreground': '#202020',
      'editorSuggestWidget.highlightForeground': '#0069CC',
      'editorSuggestWidget.selectedBackground': '#0069CC26',
      'editorHoverWidget.background': cssVarHex('--editor-widget-bg'),
      'editorHoverWidget.border': cssVarHex('--editor-widget-border'),
      'editorCursor.foreground': '#202020',
      'editor.selectionBackground': '#0069CC40',
      'editor.inactiveSelectionBackground': '#0069CC1A',
      'editor.selectionHighlightBackground': '#0069CC15',
      'editor.wordHighlightBackground': '#0069CC26',
      'editor.wordHighlightStrongBackground': '#0069CC26',
      'editor.lineHighlightBackground': `${cssVarHex('--editor-line-highlight')}40`,
      'editor.findMatchBackground': '#0069CC40',
      'editor.findMatchHighlightBackground': '#0069CC1A',
      'editor.findRangeHighlightBackground': cssVarHex('--editor-line-highlight'),
      'editor.rangeHighlightBackground': cssVarHex('--editor-line-highlight'),
      'editor.hoverHighlightBackground': cssVarHex('--editor-line-highlight'),
      'editorLineNumber.foreground': cssVarHex('--editor-line-number-fg'),
      'editorLineNumber.activeForeground': cssVarHex('--editor-line-number-active'),
      'editorIndentGuide.background1': '#F7F7F7',
      'editorIndentGuide.activeBackground1': '#EEEEEE',
      'editorBracketMatch.background': '#0069CC40',
      'editorBracketMatch.border': '#F0F1F2',
      'scrollbar.shadow': '#00000000',
      'scrollbarSlider.background': '#99999926',
      'scrollbarSlider.hoverBackground': '#99999940',
      'scrollbarSlider.activeBackground': '#99999955',
      'minimapSlider.background': '#99999926',
      'minimapSlider.hoverBackground': '#99999940',
      'minimapSlider.activeBackground': '#99999955',
      'diffEditor.insertedTextBackground': '#587c0c26',
      'diffEditor.removedTextBackground': '#ad070726',
      'peekView.border': '#0069CC',
      'peekViewEditor.background': cssVarHex('--editor-widget-bg'),
      'peekViewEditor.matchHighlightBackground': '#0069CC33',
      'peekViewResult.background': cssVarHex('--editor-widget-bg'),
      'peekViewResult.fileForeground': '#202020',
      'peekViewResult.lineForeground': '#606060',
      'peekViewResult.matchHighlightBackground': '#0069CC33',
      'peekViewResult.selectionBackground': '#0069CC26',
      'peekViewResult.selectionForeground': '#202020',
      'peekViewTitle.background': cssVarHex('--editor-widget-bg'),
      'peekViewTitleDescription.foreground': '#606060',
      'peekViewTitleLabel.foreground': '#202020',
      'editorGutter.addedBackground': cssVarHex('--editor-gutter-added'),
      'editorGutter.deletedBackground': cssVarHex('--editor-gutter-deleted'),
    },
  };
}

// VSCode 2026 Dark — sourced from
// github.com/microsoft/vscode/blob/main/extensions/theme-defaults/themes/2026-dark.json
// Token colors follow the GitHub Dark theme (included via dark_modern.json inheritance).
// Factory function so cssVarHex() is evaluated at call time (not module load).
function getRocketDarkTheme(): monacoNs.editor.IStandaloneThemeData {
  return {
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
      'editor.background': cssVarHex('--editor-bg'),
      'editor.foreground': '#BBBEBF',
      'editorGutter.background': cssVarHex('--editor-bg'),
      'editorStickyScroll.background': cssVarHex('--editor-bg'),
      'editorStickyScrollHover.background': cssVarHex('--editor-widget-bg'),
      'editorWidget.background': cssVarHex('--editor-widget-bg'),
      'editorWidget.border': cssVarHex('--editor-widget-border'),
      'editorWidget.foreground': '#bfbfbf',
      'editorSuggestWidget.background': cssVarHex('--editor-widget-bg'),
      'editorSuggestWidget.border': cssVarHex('--editor-widget-border'),
      'editorSuggestWidget.foreground': '#bfbfbf',
      'editorSuggestWidget.highlightForeground': '#bfbfbf',
      'editorSuggestWidget.selectedBackground': '#3994BC26',
      'editorHoverWidget.background': cssVarHex('--editor-widget-bg'),
      'editorHoverWidget.border': cssVarHex('--editor-widget-border'),
      'editorCursor.foreground': '#BBBEBF',
      'editor.selectionBackground': '#276782dd',
      'editor.inactiveSelectionBackground': '#27678260',
      'editor.selectionHighlightBackground': '#27678260',
      'editor.wordHighlightBackground': '#27678250',
      'editor.wordHighlightStrongBackground': '#27678280',
      'editor.lineHighlightBackground': cssVarHex('--editor-line-highlight'),
      'editor.findMatchBackground': '#27678290',
      'editor.findMatchHighlightBackground': '#27678280',
      'editor.findRangeHighlightBackground': cssVarHex('--editor-line-highlight'),
      'editor.rangeHighlightBackground': cssVarHex('--editor-line-highlight'),
      'editor.hoverHighlightBackground': cssVarHex('--editor-line-highlight'),
      'editorLineNumber.foreground': cssVarHex('--editor-line-number-fg'),
      'editorLineNumber.activeForeground': cssVarHex('--editor-line-number-active'),
      'editorIndentGuide.background1': '#8384854D',
      'editorIndentGuide.activeBackground1': '#838485',
      'editorBracketMatch.background': '#3994BC55',
      'editorBracketMatch.border': '#2A2B2C',
      'scrollbar.shadow': '#191B1D4D',
      'scrollbarSlider.background': '#83848533',
      'scrollbarSlider.hoverBackground': '#83848566',
      'scrollbarSlider.activeBackground': '#83848599',
      'minimapSlider.background': '#83848533',
      'minimapSlider.hoverBackground': '#83848566',
      'minimapSlider.activeBackground': '#83848599',
      'diffEditor.insertedTextBackground': '#57ab5a4d',
      'diffEditor.removedTextBackground': '#f470674d',
      'diffEditor.insertedLineBackground': '#347d3926',
      'diffEditor.removedLineBackground': '#c93c3726',
      'peekView.border': '#2A2B2C',
      'peekViewEditor.background': cssVarHex('--editor-bg'),
      'peekViewEditor.matchHighlightBackground': '#3994BC33',
      'peekViewResult.background': cssVarHex('--editor-bg'),
      'peekViewResult.fileForeground': '#bfbfbf',
      'peekViewResult.lineForeground': '#8C8C8C',
      'peekViewResult.matchHighlightBackground': '#3994BC33',
      'peekViewResult.selectionBackground': '#3994BC26',
      'peekViewResult.selectionForeground': '#bfbfbf',
      'peekViewTitle.background': cssVarHex('--editor-widget-bg'),
      'peekViewTitleDescription.foreground': '#8C8C8C',
      'peekViewTitleLabel.foreground': '#bfbfbf',
      'editorGutter.addedBackground': cssVarHex('--editor-gutter-added'),
      'editorGutter.deletedBackground': cssVarHex('--editor-gutter-deleted'),
    },
  };
}

// Call this with the monaco instance whenever the app theme changes so the
// themes are re-registered with the current CSS variable values. Called at
// module load time (monaco-setup.ts) and on each theme toggle (useMonacoTheme).
export function defineMonacoThemes(monacoInstance: typeof monacoNs) {
  monacoInstance.editor.defineTheme('rocket-light', getRocketLightTheme());
  monacoInstance.editor.defineTheme('rocket-dark', getRocketDarkTheme());
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
