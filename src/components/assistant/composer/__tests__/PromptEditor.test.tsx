import { EditorView } from '@codemirror/view';
import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { PromptEditor, type PromptEditorProps } from '../PromptEditor';

const KEY_CODES: Record<string, number> = { Enter: 13, Escape: 27, ArrowUp: 38, ArrowDown: 40 };

function press(target: HTMLElement, key: string, init: KeyboardEventInit = {}) {
  target.dispatchEvent(
    new KeyboardEvent('keydown', {
      key,
      code: key,
      keyCode: KEY_CODES[key],
      bubbles: true,
      cancelable: true,
      ...init,
    }),
  );
}

function setup(overrides: Partial<PromptEditorProps> = {}) {
  const props: PromptEditorProps = {
    value: '',
    onChange: vi.fn(),
    onSubmit: vi.fn(),
    onStop: vi.fn(),
    running: false,
    history: [],
    onHistoryCommit: vi.fn(),
    referenceSource: () => [],
    commandSource: () => [],
    onReferencePicked: vi.fn(),
    'aria-label': 'Prompt',
    ...overrides,
  };
  const utils = render(<PromptEditor {...props} />);
  const content = utils.container.querySelector('.cm-content') as HTMLElement;
  const view = EditorView.findFromDOM(
    utils.container.querySelector('.cm-editor') as HTMLElement,
  ) as EditorView;
  return { ...utils, props, content, view };
}

describe('PromptEditor keys', () => {
  it('sends on Enter and commits the prompt to history', () => {
    const { content, props, view } = setup({ value: 'hello' });
    press(content, 'Enter');
    expect(props.onHistoryCommit).toHaveBeenCalledWith('hello');
    expect(props.onSubmit).toHaveBeenCalledTimes(1);
    expect(view.state.doc.toString()).toBe('hello');
  });

  it('ignores Enter while a turn runs', () => {
    const { content, props } = setup({ value: 'hello', running: true });
    press(content, 'Enter');
    expect(props.onSubmit).not.toHaveBeenCalled();
    expect(props.onHistoryCommit).not.toHaveBeenCalled();
  });

  it('ignores Enter on a blank prompt', () => {
    const { content, props } = setup({ value: '   ' });
    press(content, 'Enter');
    expect(props.onSubmit).not.toHaveBeenCalled();
  });

  it('inserts a newline on Shift+Enter', () => {
    const { content, props, view } = setup({ value: 'a' });
    view.dispatch({ selection: { anchor: 1 } });
    press(content, 'Enter', { shiftKey: true });
    expect(view.state.doc.toString()).toBe('a\n');
    expect(props.onChange).toHaveBeenCalledWith('a\n');
    expect(props.onSubmit).not.toHaveBeenCalled();
  });

  it('stops a running turn on Escape', () => {
    const { content, props } = setup({ running: true });
    press(content, 'Escape');
    expect(props.onStop).toHaveBeenCalledTimes(1);
  });

  it('does not stop anything on Escape when idle', () => {
    const { content, props } = setup();
    press(content, 'Escape');
    expect(props.onStop).not.toHaveBeenCalled();
  });

  it('keeps Cmd/Ctrl+Enter away from the window shortcut', () => {
    const windowKeydown = vi.fn();
    window.addEventListener('keydown', windowKeydown);
    try {
      const { content, props } = setup({ value: 'hello' });
      press(content, 'Enter', { ctrlKey: true });
      press(content, 'Enter', { metaKey: true });
      press(content, 'Enter');
      expect(windowKeydown).not.toHaveBeenCalled();
      expect(props.onSubmit).toHaveBeenCalled();
    } finally {
      window.removeEventListener('keydown', windowKeydown);
    }
  });
});

describe('PromptEditor history', () => {
  it('walks back through history and returns to the draft', () => {
    const { content, view, props } = setup({ value: 'draft', history: ['first', 'second'] });
    press(content, 'ArrowUp');
    expect(view.state.doc.toString()).toBe('second');
    expect(props.onChange).toHaveBeenLastCalledWith('second');
    press(content, 'ArrowUp');
    expect(view.state.doc.toString()).toBe('first');
    press(content, 'ArrowDown');
    expect(view.state.doc.toString()).toBe('second');
    press(content, 'ArrowDown');
    expect(view.state.doc.toString()).toBe('draft');
  });

  it('starts a new draft after the user types', () => {
    const { content, view } = setup({ history: ['first'] });
    press(content, 'ArrowUp');
    expect(view.state.doc.toString()).toBe('first');
    view.dispatch({ changes: { from: 5, insert: '!' } });
    press(content, 'ArrowUp');
    expect(view.state.doc.toString()).toBe('first');
    press(content, 'ArrowDown');
    expect(view.state.doc.toString()).toBe('first!');
  });
});

describe('PromptEditor stability', () => {
  it('does not send on Enter during IME composition', () => {
    const { content, props, view } = setup({ value: 'hello' });
    Object.defineProperty(view, 'composing', { value: true });
    press(content, 'Enter');
    expect(props.onSubmit).not.toHaveBeenCalled();
  });

  it('keeps the view, cursor and undo history when placeholder or disabled change', () => {
    const { props, rerender, view, container } = setup({ value: 'abc' });
    view.dispatch({ selection: { anchor: 2 } });
    rerender(<PromptEditor {...props} placeholder='Ask' disabled aria-label='Other' />);
    rerender(<PromptEditor {...props} placeholder='Ask' aria-label='Other' />);
    const after = EditorView.findFromDOM(
      container.querySelector('.cm-editor') as HTMLElement,
    ) as EditorView;
    expect(after).toBe(view);
    expect(after.state.selection.main.head).toBe(2);
    expect(container.querySelector('.cm-content')?.getAttribute('aria-label')).toBe('Other');
  });
});

describe('PromptEditor setup', () => {
  it('labels the editor as a multi-line textbox', () => {
    const { content } = setup();
    expect(content.getAttribute('aria-label')).toBe('Prompt');
    expect(content.getAttribute('aria-multiline')).toBe('true');
  });

  it('applies a new value prop without echoing it through onChange', () => {
    const { props, rerender, view } = setup({ value: 'old' });
    rerender(<PromptEditor {...props} value='next' />);
    expect(view.state.doc.toString()).toBe('next');
    expect(props.onChange).not.toHaveBeenCalled();
  });

  it('caps the height at twelve lines and lets the text start at the top', () => {
    setup();
    const css = Array.from(document.head.querySelectorAll('style'))
      .map((el) => el.textContent ?? '')
      .join('\n');
    expect(css).toContain('max-height: calc(18em + 16px)');
    expect(css.lastIndexOf('align-items: flex-start !important')).toBeGreaterThan(
      css.indexOf('align-items: center !important'),
    );
  });
});
