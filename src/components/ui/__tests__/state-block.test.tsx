import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { StateBlock } from '@/components/ui/state-block';

describe('StateBlock', () => {
  it('uses role status for loading and empty', () => {
    const { rerender } = render(<StateBlock tone='loading'>Loading secrets.</StateBlock>);
    expect(screen.getByRole('status')).toHaveTextContent('Loading secrets.');
    rerender(<StateBlock tone='empty'>No secrets found.</StateBlock>);
    expect(screen.getByRole('status')).toHaveTextContent('No secrets found.');
  });

  it('uses role alert for the error tone', () => {
    render(<StateBlock tone='error'>Could not list secrets.</StateBlock>);
    const block = screen.getByRole('alert');
    expect(block).toHaveTextContent('Could not list secrets.');
    expect(block).toHaveAttribute('data-tone', 'error');
    expect(block).toHaveClass('border-dashed', 'text-xs');
  });

  it('renders a hidden status dot with a tone colour', () => {
    render(<StateBlock tone='error'>Failed.</StateBlock>);
    const dot = screen.getByRole('alert').querySelector('[data-slot="state-dot"]');
    expect(dot).not.toBeNull();
    expect(dot).toHaveAttribute('aria-hidden', 'true');
    expect(dot).toHaveClass('bg-destructive');
  });

  it('runs the action when its button is clicked', async () => {
    const onClick = vi.fn();
    render(
      <StateBlock tone='error' action={{ label: 'Retry', onClick }}>
        Failed.
      </StateBlock>,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Retry' }));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('renders no button without an action', () => {
    render(<StateBlock tone='empty'>Nothing here.</StateBlock>);
    expect(screen.queryByRole('button')).toBeNull();
  });

  it('wraps a very long message', () => {
    const long = 'y'.repeat(400);
    render(<StateBlock tone='error'>{long}</StateBlock>);
    expect(screen.getByText(long)).toHaveClass('break-words');
  });

  it('disables the pulse for reduced motion', () => {
    render(<StateBlock tone='loading'>Loading.</StateBlock>);
    const dot = screen.getByRole('status').querySelector('[data-slot="state-dot"]');
    expect(dot).toHaveClass('animate-pulse', 'motion-reduce:animate-none');
  });
});
