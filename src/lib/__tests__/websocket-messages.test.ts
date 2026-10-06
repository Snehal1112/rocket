import { describe, expect, it } from 'vitest';
import {
  addMessage,
  createDefaultWebSocketDraft,
  newMessage,
  normalizeSelection,
  removeMessage,
  selectedMessage,
  selectMessage,
  updateMessage,
} from '@/lib/websocket-messages';
import type { WebSocketDraftMessage } from '@/types/pane-types';

function msg(id: string, selected = false): WebSocketDraftMessage {
  return { id, title: id, selected, kind: 'text', data: '' };
}

describe('websocket message helpers', () => {
  it('normalizeSelection keeps exactly one selected message', () => {
    expect(normalizeSelection([msg('a'), msg('b')]).map((m) => m.selected)).toEqual([true, false]);
    expect(normalizeSelection([msg('a', true), msg('b', true)]).map((m) => m.selected)).toEqual([
      true,
      false,
    ]);
    expect(normalizeSelection([msg('a'), msg('b', true)]).map((m) => m.selected)).toEqual([
      false,
      true,
    ]);
    expect(normalizeSelection([])).toEqual([]);
  });

  it('addMessage appends a titled message and selects only it', () => {
    const next = addMessage([msg('a', true)]);
    expect(next).toHaveLength(2);
    expect(next[1].title).toBe('message 2');
    expect(next.map((m) => m.selected)).toEqual([false, true]);
  });

  it('selectMessage selects exactly the given id', () => {
    const next = selectMessage([msg('a', true), msg('b')], 'b');
    expect(next.map((m) => m.selected)).toEqual([false, true]);
  });

  it('removing the selected message selects the first remaining one', () => {
    const next = removeMessage([msg('a'), msg('b', true), msg('c')], 'b');
    expect(next.map((m) => m.id)).toEqual(['a', 'c']);
    expect(next.map((m) => m.selected)).toEqual([true, false]);
  });

  it('removing an unselected message keeps the selection', () => {
    const next = removeMessage([msg('a'), msg('b', true)], 'a');
    expect(next.map((m) => m.selected)).toEqual([true]);
  });

  it('removing the last message leaves an empty list', () => {
    expect(removeMessage([msg('a', true)], 'a')).toEqual([]);
  });

  it('updateMessage patches only the matching message and never changes the id', () => {
    const next = updateMessage([msg('a'), msg('b')], 'b', { data: 'x', kind: 'json' });
    expect(next[0]).toEqual(msg('a'));
    expect(next[1]).toMatchObject({ id: 'b', data: 'x', kind: 'json' });
  });

  it('selectedMessage falls back to the first message', () => {
    expect(selectedMessage([msg('a'), msg('b')])?.id).toBe('a');
    expect(selectedMessage([msg('a'), msg('b', true)])?.id).toBe('b');
    expect(selectedMessage([])).toBeUndefined();
  });

  it('newMessage creates an unselected text message with a unique id', () => {
    const a = newMessage('one');
    const b = newMessage('two');
    expect(a.id).not.toBe(b.id);
    expect(a).toMatchObject({ title: 'one', selected: false, kind: 'text', data: '' });
  });

  it('a default draft has one selected message and inherited settings', () => {
    const draft = createDefaultWebSocketDraft();
    expect(draft.messages).toHaveLength(1);
    expect(draft.messages[0]).toMatchObject({ title: 'message 1', selected: true });
    expect(draft.timeoutMs).toBe('inherit');
    expect(draft.keepAliveMs).toBe('inherit');
    expect(draft.passthrough).toEqual({});
  });
});
