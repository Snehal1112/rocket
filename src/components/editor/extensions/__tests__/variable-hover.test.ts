import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { afterEach, describe, expect, it } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { variableContextFacet } from '../variable-context-facet';
import { variableHoverTooltip } from '../variable-hover';

let view: EditorView | null = null;

afterEach(() => {
  view?.destroy();
  view = null;
});

function makeContext(
  entries: Record<string, Pick<VariableScopeEntry, 'source' | 'value' | 'secret'>>,
): Map<string, VariableScopeEntry> {
  const map = new Map<string, VariableScopeEntry>();
  for (const [key, val] of Object.entries(entries)) {
    map.set(key, {
      value: val.value,
      source: val.source,
      label: val.source,
      secret: val.secret ?? false,
    });
  }
  return map;
}

function createView(doc: string, context: Map<string, VariableScopeEntry>) {
  const container = document.createElement('div');
  view = new EditorView({
    state: EditorState.create({
      doc,
      extensions: [variableContextFacet.of(context), variableHoverTooltip()],
    }),
    parent: container,
  });
  return view;
}

describe('variableHoverTooltip', () => {
  it('creates extension without error', () => {
    const ctx = makeContext({
      baseUrl: { source: 'environment', value: 'https://api.com', secret: false },
    });
    const v = createView('{{baseUrl}}', ctx);
    expect(v).toBeTruthy();
  });

  it('extension does not crash on document without variables', () => {
    const v = createView('plain text without variables', new Map());
    expect(v).toBeTruthy();
  });

  it('extension does not crash on empty document', () => {
    const v = createView('', new Map());
    expect(v).toBeTruthy();
  });
});
