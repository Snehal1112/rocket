import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { EmptyState } from '@/components/ui/empty-state';

describe('EmptyState', () => {
  it('shows the title and the reason', () => {
    render(<EmptyState cause='none-yet' title='No history yet' reason='Requests appear here.' />);
    expect(screen.getByRole('status')).toHaveTextContent('No history yet');
    expect(screen.getByRole('status')).toHaveTextContent('Requests appear here.');
  });

  it('records the cause and uses role alert only when unavailable', () => {
    const { rerender } = render(<EmptyState cause='filtered' title='No matches' />);
    expect(screen.getByRole('status')).toHaveAttribute('data-cause', 'filtered');
    rerender(<EmptyState cause='needs-setup' title='Connect a server' />);
    expect(screen.getByRole('status')).toHaveAttribute('data-cause', 'needs-setup');
    rerender(<EmptyState cause='unavailable' title='Could not load' />);
    expect(screen.getByRole('alert')).toHaveAttribute('data-cause', 'unavailable');
  });

  it('runs the action when its button is clicked', async () => {
    const onClick = vi.fn();
    render(
      <EmptyState cause='filtered' title='No matches' action={{ label: 'Clear filters', onClick }} />,
    );
    await userEvent.click(screen.getByRole('button', { name: 'Clear filters' }));
    expect(onClick).toHaveBeenCalledTimes(1);
  });

  it('renders no button and no reason when none are given', () => {
    render(<EmptyState cause='none-yet' title='Nothing here' />);
    expect(screen.queryByRole('button')).toBeNull();
    expect(screen.getByRole('status').querySelectorAll('p')).toHaveLength(1);
  });

  it('hides the illustration from assistive tech and lets a surface replace it', () => {
    render(
      <EmptyState
        cause='none-yet'
        title='No collections yet'
        illustration={<svg data-testid='custom-art' />}
      />,
    );
    const art = screen.getByTestId('custom-art');
    expect(art.parentElement).toHaveAttribute('aria-hidden', 'true');
  });

  it('omits the illustration when it is set to false', () => {
    render(<EmptyState cause='none-yet' title='Empty' illustration={false} />);
    expect(screen.getByRole('status').querySelector('svg')).toBeNull();
  });

  it('wraps a very long unbroken reason', () => {
    render(<EmptyState cause='unavailable' title='Failed' reason={'x'.repeat(400)} />);
    expect(screen.getByText('x'.repeat(400))).toHaveClass('break-words');
  });

  it('uses a smaller illustration and tighter spacing when compact', () => {
    render(<EmptyState cause='none-yet' title='Empty' size='compact' />);
    const root = screen.getByRole('status');
    expect(root).toHaveClass('gap-2');
    expect(root.querySelector('svg')).toHaveClass('h-16', 'w-16');
  });
});
