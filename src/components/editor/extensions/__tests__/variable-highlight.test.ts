import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { afterEach, describe, expect, it } from 'vitest';
import type { VariableScopeEntry } from '@/lib/url-variables';
import { variableContextFacet } from '../variable-context-facet';
import { variableHighlight } from '../variable-highlight';

function makeContext(
  entries: Record<string, Pick<VariableScopeEntry, 'source' | 'value'>>,
): Map<string, VariableScopeEntry> {
  const map = new Map<string, VariableScopeEntry>();
  for (const [key, val] of Object.entries(entries)) {
    map.set(key, { value: val.value, source: val.source, label: val.source, secret: false });
  }
  return map;
}

let view: EditorView | null = null;
let container: HTMLDivElement | null = null;

afterEach(() => {
  view?.destroy();
  view = null;
  container?.remove();
  container = null;
});

function createView(doc: string, context: Map<string, VariableScopeEntry>) {
  // Attach to document.body so querySelectorAll can find rendered decorations.
  container = document.createElement('div');
  document.body.appendChild(container);
  view = new EditorView({
    state: EditorState.create({
      doc,
      extensions: [variableContextFacet.of(context), variableHighlight()],
    }),
    parent: container,
  });
  return view;
}

describe('variableHighlight', () => {
  it('applies cm-var-environment class to resolved env variable', () => {
    const ctx = makeContext({
      baseUrl: { source: 'environment', value: 'https://api.example.com' },
    });
    createView('https://{{baseUrl}}/api', ctx);
    const marks = document.querySelectorAll('.cm-var-environment');
    expect(marks.length).toBe(1);
    expect(marks[0].textContent).toBe('{{baseUrl}}');
  });

  it('applies cm-var-unresolved class to unknown variable', () => {
    createView('Bearer {{token}}', new Map());
    const marks = document.querySelectorAll('.cm-var-unresolved');
    expect(marks.length).toBe(1);
    expect(marks[0].textContent).toBe('{{token}}');
  });

  it('highlights multiple variables with different scopes', () => {
    const ctx = makeContext({
      host: { source: 'environment', value: 'localhost' },
      key: { source: 'collection', value: 'abc' },
    });
    createView('{{host}}/{{key}}/{{missing}}', ctx);
    expect(document.querySelectorAll('.cm-var-environment').length).toBe(1);
    expect(document.querySelectorAll('.cm-var-collection').length).toBe(1);
    expect(document.querySelectorAll('.cm-var-unresolved').length).toBe(1);
  });

  it('does not highlight plain text', () => {
    createView('just plain text', new Map());
    expect(document.querySelectorAll('.cm-var').length).toBe(0);
  });

  it('handles process.env dot notation', () => {
    const ctx = makeContext({ 'process.env.API_KEY': { source: 'process', value: 'sk-123' } });
    createView('key={{process.env.API_KEY}}', ctx);
    expect(document.querySelectorAll('.cm-var-process').length).toBe(1);
  });
});
