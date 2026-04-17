import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { afterEach, describe, expect, it } from 'vitest';
import { secretMask } from '../secret-mask';

let view: EditorView | null = null;
let container: HTMLDivElement | null = null;

afterEach(() => {
  view?.destroy();
  view = null;
  container?.remove();
  container = null;
});

function createView(doc: string) {
  // Attach to document.body so querySelectorAll can find rendered decorations.
  container = document.createElement('div');
  document.body.appendChild(container);
  view = new EditorView({
    state: EditorState.create({
      doc,
      extensions: [secretMask()],
    }),
    parent: container,
  });
  return view;
}

describe('secretMask', () => {
  it('masks plain text with ● characters', () => {
    createView('mysecretpassword');
    const masks = document.querySelectorAll('.cm-secret-mask');
    expect(masks.length).toBeGreaterThan(0);
    const totalDots = Array.from(masks).reduce((sum, el) => sum + (el.textContent?.length ?? 0), 0);
    expect(totalDots).toBe(16); // 'mysecretpassword'.length
  });

  it('preserves actual document text (copy still works)', () => {
    const v = createView('secretvalue');
    expect(v.state.doc.toString()).toBe('secretvalue');
  });

  it('does not mask {{variable}} tokens', () => {
    const v = createView('prefix{{token}}suffix');
    // {{token}} should remain visible; only prefix and suffix are masked.
    const content = v.dom.querySelector('.cm-content');
    const text = content?.textContent ?? '';
    expect(text).toContain('{{token}}');
  });

  it('handles empty document', () => {
    createView('');
    const masks = document.querySelectorAll('.cm-secret-mask');
    expect(masks.length).toBe(0);
  });

  it('handles document with only a variable', () => {
    createView('{{onlyvar}}');
    // No masking needed — entire document is a variable.
    const masks = document.querySelectorAll('.cm-secret-mask');
    expect(masks.length).toBe(0);
  });
});
