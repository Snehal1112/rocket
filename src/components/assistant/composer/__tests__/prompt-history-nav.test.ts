import { EditorSelection, EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';
import { atRecallEdge, IDLE_HISTORY_CURSOR, stepHistory } from '../prompt-history-nav';

const HISTORY = ['first', 'second', 'third'];

describe('stepHistory', () => {
  it('returns null on Up with an empty history', () => {
    expect(stepHistory([], IDLE_HISTORY_CURSOR, 'draft', 'up')).toBeNull();
  });

  it('returns null on Down while the user edits the draft', () => {
    expect(stepHistory(HISTORY, IDLE_HISTORY_CURSOR, 'draft', 'down')).toBeNull();
  });

  it('keeps the draft and shows the newest prompt on the first Up', () => {
    expect(stepHistory(HISTORY, IDLE_HISTORY_CURSOR, 'draft', 'up')).toEqual({
      cursor: { index: 2, draft: 'draft' },
      text: 'third',
    });
  });

  it('walks to older prompts and stops at the oldest', () => {
    const second = stepHistory(HISTORY, { index: 2, draft: 'd' }, 'third', 'up');
    expect(second).toEqual({ cursor: { index: 1, draft: 'd' }, text: 'second' });
    expect(stepHistory(HISTORY, { index: 0, draft: 'd' }, 'first', 'up')).toBeNull();
  });

  it('walks forward and brings the draft back after the newest prompt', () => {
    expect(stepHistory(HISTORY, { index: 1, draft: 'd' }, 'second', 'down')).toEqual({
      cursor: { index: 2, draft: 'd' },
      text: 'third',
    });
    expect(stepHistory(HISTORY, { index: 2, draft: 'd' }, 'third', 'down')).toEqual({
      cursor: IDLE_HISTORY_CURSOR,
      text: 'd',
    });
  });

  it('clamps an index left over from a longer history', () => {
    expect(stepHistory(['only'], { index: 5, draft: 'd' }, 'x', 'up')).toEqual({
      cursor: { index: 0, draft: 'd' },
      text: 'only',
    });
  });
});

describe('atRecallEdge', () => {
  const at = (doc: string, anchor: number, head = anchor) =>
    EditorState.create({ doc, selection: EditorSelection.single(anchor, head) });

  it('allows Up only on the first line', () => {
    expect(atRecallEdge(at('one\ntwo', 0), 'up')).toBe(true);
    expect(atRecallEdge(at('one\ntwo', 7), 'up')).toBe(false);
  });

  it('allows Down only on the last line', () => {
    expect(atRecallEdge(at('one\ntwo', 7), 'down')).toBe(true);
    expect(atRecallEdge(at('one\ntwo', 0), 'down')).toBe(false);
  });

  it('allows both on a single line', () => {
    expect(atRecallEdge(at('one', 1), 'up')).toBe(true);
    expect(atRecallEdge(at('one', 1), 'down')).toBe(true);
  });

  it('refuses while text is selected', () => {
    expect(atRecallEdge(at('one', 0, 3), 'up')).toBe(false);
  });
});
