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
// Removed in the next tasks, together with these mocks.
vi.mock('@/components/settings/AgentConfigsDialog', () => ({ AgentConfigsDialog: () => null }));

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

  it('keeps the AI Agents button', () => {
    render(<TitleBar />);
    expect(screen.getByRole('button', { name: 'AI Agent configurations' })).toBeInTheDocument();
  });

  it('has no Secret Manager button, since Secret Managers live in Settings', () => {
    render(<TitleBar />);
    expect(screen.queryByRole('button', { name: 'Secret Manager connections' })).toBeNull();
  });

  it('has no Proxy button, since Proxy lives in Settings', () => {
    render(<TitleBar />);
    expect(screen.queryByRole('button', { name: 'Proxy settings' })).toBeNull();
  });
});
