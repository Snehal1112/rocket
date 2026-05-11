import { render, screen, waitFor } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { describe, expect, it, vi } from 'vitest';
import { useLayoutStore } from '@/stores/layout-store';
import { StatusBar } from '../StatusBar';

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn().mockResolvedValue('0.4.0'),
}));

vi.mock('@/components/status-bar/ContractsStatusItem', () => ({
  ContractsStatusItem: () => null,
}));

vi.mock('@/hooks/useTheme', () => ({
  useTheme: () => ({ isDark: false, toggleTheme: vi.fn() }),
}));

vi.mock('@/stores/console-store', () => ({
  useConsoleStore: (selector: (s: { entries: unknown[] }) => unknown) => selector({ entries: [] }),
}));

describe('StatusBar', () => {
  it('displays the app version in the bottom-right corner', async () => {
    render(<StatusBar />);
    await waitFor(() => {
      expect(screen.getByText('v0.4.0')).toBeDefined();
    });
  });

  it('renders "Side by side" button when layout is stacked', () => {
    useLayoutStore.setState({ requestLayout: 'stacked' });
    render(<StatusBar />);
    expect(screen.getByRole('button', { name: /side by side/i })).toBeInTheDocument();
  });

  it('renders "Stack" button when layout is side-by-side', () => {
    useLayoutStore.setState({ requestLayout: 'side-by-side' });
    render(<StatusBar />);
    expect(screen.getByRole('button', { name: /stack/i })).toBeInTheDocument();
  });

  it('clicking layout button toggles the store', async () => {
    useLayoutStore.setState({ requestLayout: 'stacked' });
    render(<StatusBar />);
    await userEvent.click(screen.getByRole('button', { name: /side by side/i }));
    expect(useLayoutStore.getState().requestLayout).toBe('side-by-side');
  });
});
