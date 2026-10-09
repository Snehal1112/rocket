import type { EditorState } from '@codemirror/state';

/** Where Up and Down recall stands. `index` is null while the user edits their own draft. */
export interface HistoryCursor {
  index: number | null;
  draft: string;
}

export type HistoryDirection = 'up' | 'down';

export const IDLE_HISTORY_CURSOR: HistoryCursor = { index: null, draft: '' };

/**
 * One Up or Down step through `history`, which is ordered oldest first. Returns the
 * new cursor and the text to show, or null when there is nothing in that direction.
 * The first Up keeps `current` as the draft, and Down past the newest prompt brings
 * the draft back.
 */
export function stepHistory(
  history: readonly string[],
  cursor: HistoryCursor,
  current: string,
  direction: HistoryDirection,
): { cursor: HistoryCursor; text: string } | null {
  if (direction === 'up') {
    if (history.length === 0) return null;
    if (cursor.index === null) {
      const index = history.length - 1;
      return { cursor: { index, draft: current }, text: history[index] };
    }
    if (cursor.index === 0) return null;
    // The history can shrink between steps, so the index is clamped.
    const index = Math.min(cursor.index - 1, history.length - 1);
    return { cursor: { index, draft: cursor.draft }, text: history[index] };
  }
  if (cursor.index === null) return null;
  if (cursor.index >= history.length - 1) {
    return { cursor: IDLE_HISTORY_CURSOR, text: cursor.draft };
  }
  const index = cursor.index + 1;
  return { cursor: { index, draft: cursor.draft }, text: history[index] };
}

/**
 * True when Up (first line) or Down (last line) should recall history instead of
 * moving the cursor. A selection always moves the cursor.
 */
export function atRecallEdge(state: EditorState, direction: HistoryDirection): boolean {
  const { main } = state.selection;
  if (!main.empty) return false;
  const line = state.doc.lineAt(main.head);
  return direction === 'up' ? line.number === 1 : line.number === state.doc.lines;
}
