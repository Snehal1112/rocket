import {
  type Completion,
  CompletionContext,
  type CompletionResult,
  type CompletionSource,
} from '@codemirror/autocomplete';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { afterEach, describe, expect, it, vi } from 'vitest';
import type { ReferenceItem, SlashCommandItem } from '@/lib/assistant/types';
import { commandCompletions, referenceCompletions } from '../prompt-completions';

const ORDERS: ReferenceItem = {
  kind: 'request',
  collection: 'shop',
  path: 'orders/list.yml',
  label: 'GET List orders',
};
const EXPLAIN: SlashCommandItem = {
  name: 'explain',
  description: 'Explain a request',
  template: 'Explain this request: ',
};

type Apply = (view: EditorView, completion: Completion, from: number, to: number) => void;

let view: EditorView | null = null;

afterEach(() => {
  const parent = view?.dom.parentElement;
  view?.destroy();
  parent?.remove();
  view = null;
});

function makeView(doc: string): EditorView {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  view = new EditorView({ state: EditorState.create({ doc }), parent });
  return view;
}

function complete(source: CompletionSource, v: EditorView): CompletionResult | null {
  const result = source(new CompletionContext(v.state, v.state.doc.length, false));
  if (result instanceof Promise) throw new Error('sync result expected');
  return result;
}

function applyFirst(result: CompletionResult, v: EditorView) {
  const option = result.options[0];
  if (typeof option.apply !== 'function') throw new Error('apply missing');
  (option.apply as Apply)(v, option, result.from, v.state.doc.length);
}

describe('referenceCompletions', () => {
  it('lists the source items for the typed query, starting at the #', () => {
    const source = vi.fn(() => [ORDERS]);
    const v = makeView('see #ord');
    const result = complete(referenceCompletions({ current: source }, { current: vi.fn() }), v);
    expect(source).toHaveBeenCalledWith('ord');
    expect(result?.from).toBe(4);
    expect(result?.filter).toBe(false);
    expect(result?.options.map((o) => o.label)).toEqual(['GET List orders']);
  });

  it('removes the typed #query and reports the picked item', () => {
    const onPicked = vi.fn();
    const v = makeView('see #ord');
    const result = complete(
      referenceCompletions({ current: () => [ORDERS] }, { current: onPicked }),
      v,
    );
    if (!result) throw new Error('result expected');
    applyFirst(result, v);
    expect(v.state.doc.toString()).toBe('see ');
    expect(onPicked).toHaveBeenCalledWith(ORDERS);
  });

  it('does not open for a # inside a word', () => {
    const source = vi.fn(() => [ORDERS]);
    const v = makeView('issue#12');
    expect(complete(referenceCompletions({ current: source }, { current: vi.fn() }), v)).toBeNull();
    expect(source).not.toHaveBeenCalled();
  });

  it('does not open when nothing matches', () => {
    const v = makeView('#zzz');
    expect(
      complete(referenceCompletions({ current: () => [] }, { current: vi.fn() }), v),
    ).toBeNull();
  });
});

describe('commandCompletions', () => {
  it('lists commands for a / at the start and inserts the template', () => {
    const source = vi.fn(() => [EXPLAIN]);
    const v = makeView('/ex');
    const result = complete(commandCompletions({ current: source }), v);
    expect(source).toHaveBeenCalledWith('ex');
    expect(result?.from).toBe(0);
    expect(result?.options[0].label).toBe('/explain');
    if (!result) throw new Error('result expected');
    applyFirst(result, v);
    expect(v.state.doc.toString()).toBe('Explain this request: ');
    expect(v.state.selection.main.head).toBe('Explain this request: '.length);
  });

  it('does not open for a / after other text', () => {
    const v = makeView('hi /ex');
    expect(complete(commandCompletions({ current: () => [EXPLAIN] }), v)).toBeNull();
  });
});
