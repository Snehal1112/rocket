import { EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';
import { singleLineFilter } from '../single-line-filter';

function createState(doc: string) {
  return EditorState.create({ doc, extensions: [singleLineFilter] });
}

describe('singleLineFilter', () => {
  it('allows single-line text insertion', () => {
    const state = createState('hello');
    const tr = state.update({ changes: { from: 5, insert: ' world' } });
    expect(tr.state.doc.toString()).toBe('hello world');
  });

  it('rejects newline insertion (Enter key)', () => {
    const state = createState('hello');
    const tr = state.update({ changes: { from: 5, insert: '\n' } });
    // Transaction is rejected — state unchanged.
    expect(tr.state.doc.toString()).toBe('hello');
  });

  it('rejects pasted multi-line text', () => {
    const state = createState('hello');
    const tr = state.update({ changes: { from: 5, insert: '\nworld\nfoo' } });
    expect(tr.state.doc.toString()).toBe('hello');
  });

  it('allows replacement within single line', () => {
    const state = createState('hello world');
    const tr = state.update({ changes: { from: 0, to: 5, insert: 'hi' } });
    expect(tr.state.doc.toString()).toBe('hi world');
  });

  it('rejects replacement that introduces newline', () => {
    const state = createState('hello world');
    const tr = state.update({ changes: { from: 5, to: 6, insert: '\n' } });
    expect(tr.state.doc.toString()).toBe('hello world');
  });

  it('works with empty document', () => {
    const state = createState('');
    const tr = state.update({ changes: { from: 0, insert: 'hello' } });
    expect(tr.state.doc.toString()).toBe('hello');
  });
});
