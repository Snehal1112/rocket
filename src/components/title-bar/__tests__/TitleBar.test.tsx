import { render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useSettingsStore } from '@/stores/settings-store';
import { TitleBar } from '../TitleBar';

vi.mock('@tauri-apps/plugin-os', () => ({ type: () => 'linux' }));
vi.mock('../WorkspaceSwitcher', () => ({ WorkspaceSwitcher: () => null }));
vi.mock('../WindowControls', () => ({ WindowControls: () => null }));
vi.mock('@/components/assistant/AssistantToggleButton', () => ({
  AssistantToggleButton: () => null,
}));

describe('TitleBar', () => {
  beforeEach(() => {
    useSettingsStore.getState().reset();
  });

  it('opens and closes Settings with the gear', async () => {
    render(<TitleBar />);
    const gear = screen.getByRole('button', { name: 'Settings' });
    await userEvent.click(gear);
    expect(useSettingsStore.getState().open).toBe(true);
    await userEvent.click(gear);
    expect(useSettingsStore.getState().open).toBe(false);
  });

  it('has only the Settings gear for settings', () => {
    render(<TitleBar />);
    expect(screen.getByRole('button', { name: 'Settings' })).toBeInTheDocument();
    for (const name of [
      'Proxy settings',
      'Secret Manager connections',
      'AI Agent configurations',
    ]) {
      expect(screen.queryByRole('button', { name })).toBeNull();
    }
  });
});
