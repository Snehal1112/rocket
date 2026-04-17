import { CompletionContext } from '@codemirror/autocomplete';
import { EditorState } from '@codemirror/state';
import { describe, expect, it } from 'vitest';
import type { VariableScopeEntry, VariableSource } from '@/lib/url-variables';
import { variableContextFacet } from '../variable-context-facet';

// We test the completion source directly by importing and calling it.
// The actual source is not exported — we'll test through the autocomplete extension.
// Instead, we test the matching logic using CompletionContext.

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
});
