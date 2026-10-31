import { render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { MainArea } from '@/components/layout/MainArea';
import { useSettingsStore } from '@/stores/settings-store';

vi.mock('@/components/settings/SettingsPage', () => ({
  SettingsPage: () => <section aria-label='Settings' />,
}));

describe('MainArea stacking', () => {
  beforeEach(() => useSettingsStore.setState({ open: true }));
  afterEach(() => useSettingsStore.getState().reset());

  it('keeps the Settings overlay below the sidebar so its shadow stays visible', () => {
    render(
      <MainArea>
        <p>tabs</p>
      </MainArea>,
    );
    const overlay = screen.getByRole('region', { name: 'Settings' }).parentElement as HTMLElement;
    expect(overlay.className).toContain('z-[1]');
    expect(overlay.className).not.toContain('z-20');
  });

  it('keeps the tabs inert and isolated while Settings is open', () => {
    const { container } = render(
      <MainArea>
        <p>tabs</p>
      </MainArea>,
    );
    const tabs = container.querySelector('.isolate') as HTMLElement;
    expect(tabs.hasAttribute('inert')).toBe(true);
  });

  it('renders no overlay when Settings is closed', () => {
    useSettingsStore.getState().closeSettings();
    render(
      <MainArea>
        <p>tabs</p>
      </MainArea>,
    );
    expect(screen.queryByRole('region', { name: 'Settings' })).toBeNull();
  });
});
