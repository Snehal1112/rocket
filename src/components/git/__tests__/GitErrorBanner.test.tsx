import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { GitErrorBanner } from '@/components/git/GitErrorBanner';

describe('GitErrorBanner', () => {
  it('renders the message in an alert region', () => {
    render(<GitErrorBanner message='something failed' />);

    expect(screen.getByRole('alert')).toHaveTextContent('something failed');
  });

  it('has no dismiss button when onDismiss is not provided', () => {
    render(<GitErrorBanner message='something failed' />);

    expect(screen.queryByRole('button', { name: /dismiss error/i })).not.toBeInTheDocument();
  });

  it('calls onDismiss when the dismiss button is clicked', async () => {
    const onDismiss = vi.fn();
    render(<GitErrorBanner message='something failed' onDismiss={onDismiss} />);
    const user = userEvent.setup();

    await user.click(screen.getByRole('button', { name: /dismiss error/i }));

    expect(onDismiss).toHaveBeenCalledOnce();
  });
});
