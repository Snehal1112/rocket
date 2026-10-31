import { openUrl } from '@tauri-apps/plugin-opener';
import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { toast } from 'sonner';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { SupportSection } from '../SupportSection';

vi.mock('@tauri-apps/plugin-opener', () => ({ openUrl: vi.fn() }));
vi.mock('sonner', () => ({ toast: { success: vi.fn(), error: vi.fn() } }));

describe('SupportSection', () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.mocked(openUrl).mockResolvedValue(undefined);
  });

  it('has a Support heading', () => {
    render(<SupportSection />);
    expect(screen.getByRole('heading', { name: 'Support' })).toBeInTheDocument();
  });

  it('opens the issues page', async () => {
    render(<SupportSection />);
    await userEvent.click(screen.getByRole('button', { name: /report issues/i }));
    expect(openUrl).toHaveBeenCalledWith('https://github.com/Snehal1112/rocket/issues');
  });

  it('opens the repository', async () => {
    render(<SupportSection />);
    await userEvent.click(screen.getByRole('button', { name: /github/i }));
    expect(openUrl).toHaveBeenCalledWith('https://github.com/Snehal1112/rocket');
  });

  it('shows the URL in a toast when the browser cannot be opened', async () => {
    vi.mocked(openUrl).mockRejectedValue(new Error('denied'));
    render(<SupportSection />);
    await userEvent.click(screen.getByRole('button', { name: /report issues/i }));
    await waitFor(() =>
      expect(toast.error).toHaveBeenCalledWith(
        expect.stringContaining('https://github.com/Snehal1112/rocket/issues'),
      ),
    );
  });
});
