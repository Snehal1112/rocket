import { EditorState } from '@codemirror/state';

/**
 * Transaction filter that rejects any change producing more than one line.
 * Blocks Enter key, pasted newlines, and programmatic multi-line insertions.
 *
 * This is the CM6-recommended approach for single-line editors.
 * See: https://discuss.codemirror.net/t/codemirror-6-single-line-and-or-avoid-carriage-return/2979/2
 */
export const singleLineFilter = EditorState.transactionFilter.of((tr) =>
  tr.newDoc.lines > 1 ? [] : tr,
);
