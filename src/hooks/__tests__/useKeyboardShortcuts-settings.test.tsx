import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook } from '@testing-library/react';
import type { ReactNode } from 'react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { sendRequest } from '@/lib/execute-request';
import { usePaneStore } from '@/stores/pane-store';
import { useSettingsStore } from '@/stores/settings-store';
import { useKeyboardShortcuts } from '../useKeyboardShortcuts';

vi.mock('@/lib/auto-save', () => ({ scheduleAutoSave: vi.fn(), cancelAutoSave: vi.fn() }));
vi.mock('@/lib/execute-request', () => ({ sendRequest: vi.fn() }));

const wrapper = ({ children }: { children: ReactNode }) => (
  <QueryClientProvider client={new QueryClient()}>{children}</QueryClientProvider>
);

function press(key: string) {
  window.dispatchEvent(new KeyboardEvent('keydown', { key, ctrlKey: true }));
}

describe('Settings shortcuts', () => {
  beforeEach(() => {
    usePaneStore.getState().reset();
    useSettingsStore.getState().reset();
  });

  it('toggles Settings on Ctrl+,', () => {
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    press(',');
    expect(useSettingsStore.getState().open).toBe(true);
    press(',');
    expect(useSettingsStore.getState().open).toBe(false);
  });

  it('does not toggle on a plain comma', () => {
    renderHook(() => useKeyboardShortcuts(), { wrapper });
    window.dispatchEvent(new KeyboardEvent('keydown', { key: ',' }));
    expect(useSettingsStore.getState().open).toBe(false);
  });

  describe('while Settings is open', () => {
    const realCloseTab = usePaneStore.getState().closeTab;
    let closeTab: ReturnType<typeof vi.fn>;
    let saveEvents: string[];
    const onSave = (e: Event) => saveEvents.push(e.type);

    beforeEach(() => {
      vi.mocked(sendRequest).mockClear();
      usePaneStore.getState().openEphemeralTab();
      closeTab = vi.fn();
      usePaneStore.setState({ closeTab } as never);
      saveEvents = [];
      window.addEventListener('rocket:save-to-collection', onSave);
      window.addEventListener('rocket:save-draft', onSave);
      useSettingsStore.getState().openSettings();
    });
    afterEach(() => {
      window.removeEventListener('rocket:save-to-collection', onSave);
      window.removeEventListener('rocket:save-draft', onSave);
      usePaneStore.setState({ closeTab: realCloseTab });
    });

    it('does not send the hidden request on Ctrl+Enter', () => {
      renderHook(() => useKeyboardShortcuts(), { wrapper });
      press('Enter');
      expect(sendRequest).not.toHaveBeenCalled();
    });

    it('does not save the hidden tab on Ctrl+S', () => {
      renderHook(() => useKeyboardShortcuts(), { wrapper });
      press('s');
      expect(saveEvents).toEqual([]);
    });

    it('closes Settings instead of the tab on Ctrl+W', () => {
      renderHook(() => useKeyboardShortcuts(), { wrapper });
      press('w');
      expect(useSettingsStore.getState().open).toBe(false);
      expect(closeTab).not.toHaveBeenCalled();
    });

    it('keeps Settings and the tab on Ctrl+W while locked', () => {
      useSettingsStore.getState().setLocked(true);
      renderHook(() => useKeyboardShortcuts(), { wrapper });
      press('w');
      expect(useSettingsStore.getState().open).toBe(true);
      expect(closeTab).not.toHaveBeenCalled();
    });

    it('sends the request again once Settings is closed', () => {
      renderHook(() => useKeyboardShortcuts(), { wrapper });
      useSettingsStore.getState().closeSettings();
      press('Enter');
      expect(sendRequest).toHaveBeenCalledTimes(1);
    });
  });
});
