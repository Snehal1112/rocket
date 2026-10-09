import { EditorView } from '@codemirror/view';

/** Lines the prompt grows to before it scrolls. */
export const PROMPT_MAX_LINES = 12;
export const PROMPT_LINE_HEIGHT = 1.5;

/**
 * Turns the single-line `rocketTheme` into a growing prose box. It must load with a
 * higher precedence than `rocketTheme`, so its rules come later in the cascade:
 * height follows the content up to the cap, the text starts at the top instead of
 * the vertical center, and the scroller scrolls instead of clipping.
 */
export const promptEditorTheme = EditorView.theme({
  '&': {
    height: 'auto',
    maxHeight: `calc(${PROMPT_MAX_LINES * PROMPT_LINE_HEIGHT}em + 16px)`,
    fontFamily: 'inherit',
    fontSize: '13px',
  },
  '.cm-scroller': {
    overflow: 'auto',
    alignItems: 'flex-start !important',
    lineHeight: String(PROMPT_LINE_HEIGHT),
    fontFamily: 'inherit',
  },
  '.cm-content': {
    padding: '8px 0',
    minHeight: `${PROMPT_LINE_HEIGHT}em`,
  },
  '.cm-line': {
    padding: '0 10px',
  },
});
