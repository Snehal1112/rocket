import { render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { StatusBar } from '../StatusBar';

vi.mock('@tauri-apps/api/app', () => ({
  getVersion: vi.fn().mockResolvedValue('0.4.0'),
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
});
