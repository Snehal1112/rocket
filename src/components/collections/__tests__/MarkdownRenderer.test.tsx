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
