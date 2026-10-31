import { act, render, screen } from '@testing-library/react';
import { useEffect } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { useSettingsStore } from '@/stores/settings-store';
import { MainArea } from '../MainArea';

let settingsThrows = false;
vi.mock('@/components/settings/SettingsPage', () => ({
  SettingsPage: () => {
    if (settingsThrows) throw new Error('boom');
    return <section aria-label='Settings' />;
  },
}));

let mounts = 0;
function Child() {
  useEffect(() => {
    mounts += 1;
  }, []);
  return <p>tab content</p>;
}

describe('MainArea', () => {
  beforeEach(() => {
    mounts = 0;
    settingsThrows = false;
    useSettingsStore.getState().reset();
  });

  it('shows only its children while Settings is closed', () => {
    render(
      <MainArea>
        <Child />
      </MainArea>,
    );
    expect(screen.getByText('tab content')).toBeInTheDocument();
    expect(screen.queryByRole('region', { name: 'Settings' })).toBeNull();
  });

  it('keeps the children mounted while Settings opens and closes', () => {
    render(
      <MainArea>
        <Child />
      </MainArea>,
    );
    act(() => useSettingsStore.getState().openSettings());
    expect(screen.getByRole('region', { name: 'Settings' })).toBeInTheDocument();
    expect(screen.getByText('tab content')).toBeInTheDocument();
    act(() => useSettingsStore.getState().closeSettings());
    expect(screen.queryByRole('region', { name: 'Settings' })).toBeNull();
    expect(mounts).toBe(1);
  });

  it('makes the tabs inert while Settings is open', () => {
    render(
      <MainArea>
        <Child />
      </MainArea>,
    );
    const wrapper = screen.getByText('tab content').parentElement as HTMLElement;
    expect(wrapper).not.toHaveAttribute('inert');
    expect(wrapper).toHaveClass('isolate');
    act(() => useSettingsStore.getState().openSettings());
    expect(wrapper).toHaveAttribute('inert');
  });

  it('keeps the app alive when the Settings page throws', () => {
    vi.spyOn(console, 'error').mockImplementation(() => {});
    settingsThrows = true;
    useSettingsStore.getState().openSettings();
    render(
      <MainArea>
        <Child />
      </MainArea>,
    );
    expect(screen.getByText('tab content')).toBeInTheDocument();
  });
});
