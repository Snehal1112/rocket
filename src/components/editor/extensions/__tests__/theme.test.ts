import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { afterEach, describe, expect, it } from 'vitest';
import { rocketTheme } from '../theme';

let view: EditorView | null = null;
let container: HTMLDivElement | null = null;

afterEach(() => {
  view?.destroy();
  view = null;
  container?.remove();
  container = null;
});

function createView(doc: string) {
  // Attach to document.body so the mounted <style> tag lands in document.head.
  container = document.createElement('div');
  document.body.appendChild(container);
  view = new EditorView({
    state: EditorState.create({ doc, extensions: [rocketTheme] }),
    parent: container,
  });
  return view;
}

function mountedCss(): string {
  return Array.from(document.head.querySelectorAll('style'))
    .map((el) => el.textContent ?? '')
    .join('\n');
}

// Regression test for a real bug: rocketTheme's `.cm-scroller { align-items:
// center !important }` looked correct and *did* win the cascade over CM's own
// base theme, but had no visible effect because CM's base theme also sets
// `.cm-content { min-height: 100% }` (not !important, never overridden),
// which forces the flex item to always fill the full height, leaving
// align-items no slack to center into. Both overrides are required together.
describe('rocketTheme single-line vertical centering', () => {
  it('overrides .cm-scroller align-items to center, after CM base theme sets flex-start', () => {
    createView('hello');
    const css = mountedCss();
    const baseIdx = css.indexOf('align-items: flex-start !important');
    const overrideIdx = css.indexOf('align-items: center !important');
    expect(baseIdx).toBeGreaterThanOrEqual(0);
    expect(overrideIdx).toBeGreaterThan(baseIdx);
  });

  it('overrides .cm-content min-height so it does not stretch to fill the scroller', () => {
    createView('hello');
    const css = mountedCss();
    const baseIdx = css.indexOf('min-height: 100%');
    const overrideIdx = css.indexOf('min-height: auto');
    expect(baseIdx).toBeGreaterThanOrEqual(0);
    expect(overrideIdx).toBeGreaterThan(baseIdx);
  });
});
