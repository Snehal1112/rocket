import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { renderHook } from '@testing-library/react';
import type { ReactNode } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
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
});
