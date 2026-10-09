import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { MarkdownRenderer } from '../MarkdownRenderer';

describe('MarkdownRenderer — code block actions', () => {
  it('renders no action UI when renderCodeActions is not given', () => {
    render(<MarkdownRenderer>{'```js\nconst x = 1;\n```'}</MarkdownRenderer>);
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });

  it('renders the caller-provided action for each fenced code block', async () => {
    const onInsert = vi.fn();
    render(
      <MarkdownRenderer
        renderCodeActions={(code) => (
          <button type='button' onClick={() => onInsert(code)}>
            Insert
          </button>
        )}
      >
        {'```js\nconst x = 1;\n```'}
      </MarkdownRenderer>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Insert' }));
    expect(onInsert).toHaveBeenCalledWith('const x = 1;');
  });

  it('does not render an action for inline code (no language fence)', () => {
    render(
      <MarkdownRenderer renderCodeActions={() => <button type='button'>Insert</button>}>
        {'Use `const x = 1` inline.'}
      </MarkdownRenderer>,
    );
    expect(screen.queryByRole('button')).not.toBeInTheDocument();
  });
});

describe('MarkdownRenderer — restricted mode', () => {
  it('never renders an image element, so nothing is fetched', () => {
    const { container } = render(
      <MarkdownRenderer restricted>{'![secret](https://evil.test/p.png?d=abc)'}</MarkdownRenderer>,
    );
    expect(container.querySelector('img')).toBeNull();
    expect(screen.getByText('[image omitted: secret]')).toBeInTheDocument();
  });

  it('shows a plain placeholder for an image without alt text', () => {
    const { container } = render(
      <MarkdownRenderer restricted>{'![](https://evil.test/p.png)'}</MarkdownRenderer>,
    );
    expect(container.querySelector('img')).toBeNull();
    expect(screen.getByText('[image omitted]')).toBeInTheDocument();
  });

  it('shows a link as text with its URL and renders no anchor', () => {
    const { container } = render(
      <MarkdownRenderer restricted>{'[docs](https://example.com/a)'}</MarkdownRenderer>,
    );
    expect(container.querySelector('a')).toBeNull();
    expect(screen.getByText('docs')).toBeInTheDocument();
    expect(screen.getByText('(https://example.com/a)')).toBeInTheDocument();
  });

  it.each(['javascript:alert(1)', 'data:text/html,hi', 'file:///etc/passwd'])(
    'blocks the %s link',
    (href) => {
      const { container } = render(
        <MarkdownRenderer restricted>{`[click](${href})`}</MarkdownRenderer>,
      );
      expect(container.querySelector('a')).toBeNull();
      expect(screen.getByText('[link blocked]')).toBeInTheDocument();
      expect(screen.queryByText(href)).not.toBeInTheDocument();
    },
  );

  it('still renders links as anchors when not restricted', () => {
    const { container } = render(<MarkdownRenderer>{'[docs](https://example.com/a)'}</MarkdownRenderer>);
    expect(container.querySelector('a')).not.toBeNull();
  });
});
