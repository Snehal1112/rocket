import { EditorView } from '@codemirror/view';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { render } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { SingleLineEditor } from '../SingleLineEditor';

function mount(onSubmit?: () => void) {
  const qc = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const { container } = render(
    <QueryClientProvider client={qc}>
      <SingleLineEditor value='abc' onChange={vi.fn()} onSubmit={onSubmit} />
    </QueryClientProvider>,
  );
  const dom = container.querySelector('.cm-editor') as HTMLElement;
  const view = EditorView.findFromDOM(dom) as EditorView;
  return { view, content: container.querySelector('.cm-content') as HTMLElement };
}

function pressEnter(content: HTMLElement) {
  const event = new KeyboardEvent('keydown', {
    key: 'Enter',
    code: 'Enter',
    keyCode: 13,
    bubbles: true,
    cancelable: true,
  });
  content.dispatchEvent(event);
}

describe('SingleLineEditor Enter handling', () => {
  it('calls onSubmit once when Enter is pressed', () => {
    const onSubmit = vi.fn();
    const { content, view } = mount(onSubmit);
    pressEnter(content);
    expect(onSubmit).toHaveBeenCalledTimes(1);
    expect(view.state.doc.toString()).toBe('abc');
  });

  it('inserts nothing on Enter without onSubmit', () => {
    const { content, view } = mount();
    pressEnter(content);
    expect(view.state.doc.toString()).toBe('abc');
    expect(view.state.doc.lines).toBe(1);
  });
});
