import { type Completion, CompletionContext } from '@codemirror/autocomplete';
import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { describe, expect, it } from 'vitest';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import { variableCompletionSource } from '../variable-autocomplete';
import { variableContextFacet } from '../variable-context-facet';

function makeContext(
  entries: Record<string, { source: VariableSource; value: string }>,
): Map<string, VariableScopeEntry> {
  const map = new Map<string, VariableScopeEntry>();
  for (const [key, val] of Object.entries(entries)) {
    map.set(key, { value: val.value, source: val.source, label: val.source, secret: false });
  }
  return map;
}

function createState(doc: string, context: Map<string, VariableScopeEntry>) {
  return EditorState.create({
    doc,
    extensions: [variableContextFacet.of(context)],
  });
}

describe('variableAutocomplete', () => {
  it('matchBefore returns match when cursor follows {{', () => {
    const state = createState(
      'url/{{',
      makeContext({ baseUrl: { source: 'environment', value: 'x' } }),
    );
    const ctx = new CompletionContext(state, 6, false);
    const match = ctx.matchBefore(/\{\{[\w.-]*/);
    expect(match).not.toBeNull();
    if (match) {
      expect(match.text).toBe('{{');
    }
  });

  it('matchBefore returns match with partial name', () => {
    const state = createState(
      'url/{{base',
      makeContext({ baseUrl: { source: 'environment', value: 'x' } }),
    );
    const ctx = new CompletionContext(state, 10, false);
    const match = ctx.matchBefore(/\{\{[\w.-]*/);
    expect(match).not.toBeNull();
    if (match) {
      expect(match.text).toBe('{{base');
    }
  });

  it('matchBefore returns null when not inside braces', () => {
    const state = createState(
      'url/path',
      makeContext({ baseUrl: { source: 'environment', value: 'x' } }),
    );
    const ctx = new CompletionContext(state, 8, false);
    const match = ctx.matchBefore(/\{\{[\w.-]*/);
    expect(match).toBeNull();
  });

  it('matchBefore handles process.env dot notation', () => {
    const state = createState('{{process.env.', makeContext({}));
    const ctx = new CompletionContext(state, 14, false);
    const match = ctx.matchBefore(/\{\{[\w.-]*/);
    expect(match).not.toBeNull();
    if (match) {
      expect(match.text).toBe('{{process.env.');
    }
  });

  it('returns from = position after {{ so CM6 matches bare key prefix', () => {
    const ctx = makeContext({ baseUrl: { source: 'environment', value: 'x' } });
    const state = createState('{{', ctx);
    const completionCtx = new CompletionContext(state, 2, false);
    const result = variableCompletionSource(completionCtx);
    expect(result).not.toBeNull();
    if (result && !(result instanceof Promise)) {
      // from must skip '{{' so CM6 filters against the bare prefix, not '{{...'.
      expect(result.from).toBe(2);
    }
  });

  it('apply inserts key and }} at the correct position', () => {
    const ctx = makeContext({ baseUrl: { source: 'environment', value: 'x' } });
    const state = createState('{{', ctx);
    const completionCtx = new CompletionContext(state, 2, false);
    const result = variableCompletionSource(completionCtx);
    expect(result).not.toBeNull();
    if (!result || result instanceof Promise) throw new Error('sync result expected');

    const option = result.options.find((o) => o.label === 'baseUrl');
    if (!option || typeof option.apply !== 'function') throw new Error('option.apply missing');

    const container = document.createElement('div');
    document.body.appendChild(container);
    const view = new EditorView({
      state: EditorState.create({ doc: '{{', extensions: [variableContextFacet.of(ctx)] }),
      parent: container,
    });
    try {
      // from = 2 (after '{{'), to = 2 (cursor at end of doc).
      (option.apply as (v: EditorView, c: Completion, from: number, to: number) => void)(
        view,
        option,
        2,
        2,
      );
      expect(view.state.doc.toString()).toBe('{{baseUrl}}');
      // No `}}` was present, so insert = 'baseUrl}}'. Cursor lands after the inserted text.
      expect(view.state.selection.main.head).toBe(2 + 'baseUrl}}'.length);
    } finally {
      view.destroy();
      container.remove();
    }
  });
});
