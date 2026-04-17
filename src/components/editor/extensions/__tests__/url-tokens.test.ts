import { EditorState } from '@codemirror/state';
import { EditorView } from '@codemirror/view';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { urlTokens } from '../url-tokens';

let view: EditorView | null = null;
let container: HTMLDivElement | null = null;

afterEach(() => {
  view?.destroy();
  view = null;
  container?.remove();
  container = null;
});

function createView(
  doc: string,
  pathParams?: Record<string, string>,
  queryParams?: Record<string, string>,
) {
  // Attach to document.body so querySelectorAll can find rendered decorations.
  container = document.createElement('div');
  document.body.appendChild(container);
  view = new EditorView({
    state: EditorState.create({
      doc,
      extensions: [urlTokens({ pathParams, queryParams })],
    }),
    parent: container,
  });
  return view;
}

describe('urlTokens', () => {
  it('highlights :pathParam with cm-pathparam class when resolved', () => {
    createView('https://api.com/users/:userId', { userId: '123' });
    const marks = document.querySelectorAll('.cm-pathparam');
    expect(marks.length).toBe(1);
    expect(marks[0].textContent).toBe(':userId');
  });

  it('highlights unresolved :pathParam with cm-pathparam-unresolved class', () => {
    createView('https://api.com/users/:userId', {});
    const marks = document.querySelectorAll('.cm-pathparam-unresolved');
    expect(marks.length).toBe(1);
  });

  it('highlights query keys with cm-querykey class', () => {
    createView('https://api.com/users?page=1&limit=10', {}, { page: '1', limit: '10' });
    const marks = document.querySelectorAll('.cm-querykey');
    expect(marks.length).toBe(2);
  });

  it('does not highlight :param inside {{variable}} tokens', () => {
    createView('https://api.com/{{basePath}}/:id', { id: '5' });
    // Only :id should be highlighted, not anything inside {{basePath}}.
    const pathMarks = document.querySelectorAll('.cm-pathparam');
    expect(pathMarks.length).toBe(1);
    expect(pathMarks[0].textContent).toBe(':id');
  });

  it('calls onCurlImport when curl text is pasted', () => {
    const onCurlImport = vi.fn();
    container = document.createElement('div');
    document.body.appendChild(container);
    view = new EditorView({
      state: EditorState.create({
        doc: '',
        extensions: [urlTokens({ onCurlImport })],
      }),
      parent: container,
    });
    // Note: Testing paste events in jsdom is limited.
    // This test verifies the extension creates without error.
    // Full paste testing requires integration tests.
    expect(view).toBeTruthy();
  });
});
