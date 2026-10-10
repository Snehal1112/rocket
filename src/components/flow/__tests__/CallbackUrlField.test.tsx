import { act, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { copyTextAsync } from '@/lib/clipboard';
import { CallbackUrlField } from '../CallbackUrlField';

vi.mock('@/lib/clipboard', () => ({ copyTextAsync: vi.fn(async () => undefined) }));

describe('CallbackUrlField', () => {
  beforeEach(() => vi.mocked(copyTextAsync).mockClear());

  describe('copy announcement', () => {
    afterEach(() => {
      vi.useRealTimers();
    });

    it('announces the copy politely, then clears the message', async () => {
      vi.useFakeTimers();
      render(<CallbackUrlField url='http://10.0.0.5:4000/cb/tok' />);
      const status = screen.getByRole('status');
      expect(status).toHaveAttribute('aria-live', 'polite');
      expect(status).toBeEmptyDOMElement();
      await act(async () => {
        fireEvent.click(screen.getByRole('button', { name: 'Copy callback URL' }));
      });
      expect(screen.getByRole('status')).toHaveTextContent('Callback URL copied');
      act(() => {
        vi.advanceTimersByTime(1500);
      });
      expect(screen.getByRole('status')).toBeEmptyDOMElement();
    });
  });

  it('shows the URL and says how long it works', () => {
    render(<CallbackUrlField url='http://10.0.0.5:4000/cb/tok' />);
    expect(screen.getByTestId('callback-url')).toHaveTextContent('http://10.0.0.5:4000/cb/tok');
    expect(screen.getByText('Valid while the run is active.')).toBeInTheDocument();
  });

  it('copies the URL through the native-first clipboard helper', async () => {
    render(<CallbackUrlField url='http://10.0.0.5:4000/cb/tok' />);
    await userEvent.click(screen.getByRole('button', { name: 'Copy callback URL' }));
    expect(copyTextAsync).toHaveBeenCalledTimes(1);
    await expect(vi.mocked(copyTextAsync).mock.calls[0][0]).resolves.toBe(
      'http://10.0.0.5:4000/cb/tok',
    );
  });
});
